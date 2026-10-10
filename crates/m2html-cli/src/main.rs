use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use m2html_core::{PageStyle, RenderOptions, render};
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(author, version, about = "Fast, safe Markdown to HTML rendering.")]
struct Args {
    /// Markdown file or directory to render.
    input: PathBuf,

    /// Output file or directory. Defaults to stdout for files and ./site for directories.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Emit a fragment instead of a complete HTML page.
    #[arg(long, conflicts_with = "page")]
    fragment: bool,

    /// Emit a complete HTML page.
    #[arg(long)]
    page: bool,

    /// Preserve raw HTML from Markdown input. This can expose XSS when serving output.
    #[arg(long)]
    unsafe_html: bool,

    /// Disable syntax highlighting for fenced code blocks.
    #[arg(long)]
    no_highlight: bool,

    /// Page style used when rendering a complete HTML document.
    #[arg(long, value_enum, default_value_t = StyleArg::Github)]
    style: StyleArg,

    /// Document title for single-file page output.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StyleArg {
    Github,
    Minimal,
    None,
}

impl From<StyleArg> for PageStyle {
    fn from(value: StyleArg) -> Self {
        match value {
            StyleArg::Github => Self::Github,
            StyleArg::Minimal => Self::Minimal,
            StyleArg::None => Self::None,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(args)
}

fn run(args: Args) -> Result<()> {
    if args.input.is_dir() {
        render_directory(&args)
    } else if args.input.is_file() {
        render_file_command(&args)
    } else {
        bail!("input path does not exist: {}", args.input.display());
    }
}

fn render_file_command(args: &Args) -> Result<()> {
    let output = render_file_to_string(&args.input, args)?;

    if let Some(path) = &args.output {
        ensure_parent(path)?;
        fs::write(path, output).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{output}");
    }

    Ok(())
}

fn render_directory(args: &Args) -> Result<()> {
    let output_dir = args.output.clone().unwrap_or_else(|| PathBuf::from("site"));

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "directory input requires an output directory, got file: {}",
            output_dir.display()
        );
    }

    let mut rendered = 0usize;
    let mut output_paths = HashSet::new();

    for entry in WalkDir::new(&args.input) {
        let entry = entry.with_context(|| format!("failed to walk {}", args.input.display()))?;
        let input_path = entry.path();

        if !entry.file_type().is_file() || !is_markdown(input_path) {
            continue;
        }

        let relative = input_path.strip_prefix(&args.input).with_context(|| {
            format!(
                "failed to compute relative path for {}",
                input_path.display()
            )
        })?;
        let output_path = output_dir.join(relative).with_extension("html");
        if !output_paths.insert(output_path.clone()) {
            bail!(
                "multiple Markdown files map to the same output: {}",
                output_path.display()
            );
        }
        let output = render_file_to_string(input_path, args)?;

        ensure_parent(&output_path)?;
        fs::write(&output_path, output)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        rendered += 1;
    }

    if rendered == 0 {
        bail!("no markdown files found under {}", args.input.display());
    }

    Ok(())
}

fn render_file_to_string(path: &Path, args: &Args) -> Result<String> {
    let markdown =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let options = render_options(args, Some(path));

    render(&markdown, &options).map_err(|err| anyhow!(err))
}

fn render_options(args: &Args, input_path: Option<&Path>) -> RenderOptions {
    let page = !args.fragment;
    let title = args
        .title
        .clone()
        .or_else(|| input_path.and_then(inferred_title));

    RenderOptions {
        page,
        unsafe_html: args.unsafe_html,
        highlight: !args.no_highlight,
        style: args.style.into(),
        title,
    }
}

fn inferred_title(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.replace(['-', '_'], " "))
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    Ok(())
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_markdown_extensions() {
        assert!(is_markdown(Path::new("README.md")));
        assert!(is_markdown(Path::new("guide.markdown")));
        assert!(!is_markdown(Path::new("index.html")));
    }

    #[test]
    fn infers_title_from_file_stem() {
        assert_eq!(
            inferred_title(Path::new("docs/getting-started.md")),
            Some("getting started".to_owned())
        );
    }
}

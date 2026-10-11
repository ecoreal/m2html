use std::{
    collections::HashSet,
    fs::{self, File},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use m2html_core::{MAX_MARKDOWN_BYTES, PageStyle, RenderOptions, render};
use tempfile::NamedTempFile;
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
        reject_symlink_components(path)?;
        write_atomic(path, &output)?;
    } else {
        print!("{output}");
    }

    Ok(())
}

fn render_directory(args: &Args) -> Result<()> {
    let output_dir = args.output.clone().unwrap_or_else(|| PathBuf::from("site"));
    reject_symlink_components(&output_dir)?;

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "directory input requires an output directory, got file: {}",
            output_dir.display()
        );
    }

    let mut rendered = 0usize;
    let mut output_paths: HashSet<String> = HashSet::new();

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
        if !output_paths.insert(output_collision_key(&output_path)) {
            bail!(
                "multiple Markdown files map to the same output: {}",
                output_path.display()
            );
        }
        let output = render_file_to_string(input_path, args)?;

        reject_symlink_components(&output_path)?;
        write_atomic(&output_path, &output)?;
        rendered += 1;
    }

    if rendered == 0 {
        bail!("no markdown files found under {}", args.input.display());
    }

    Ok(())
}

fn render_file_to_string(path: &Path, args: &Args) -> Result<String> {
    let markdown = read_markdown(path)?;
    let options = render_options(args, Some(path));

    render(&markdown, &options).map_err(|err| anyhow!(err))
}

fn read_markdown(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((MAX_MARKDOWN_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {}", path.display()))?;

    if bytes.len() > MAX_MARKDOWN_BYTES {
        bail!(
            "Markdown input {} is too large: maximum is {} bytes",
            path.display(),
            MAX_MARKDOWN_BYTES
        );
    }

    String::from_utf8(bytes)
        .map_err(|_| anyhow!("Markdown input {} is not valid UTF-8", path.display()))
}

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    ensure_parent(path)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create temporary output beside {}",
            path.display()
        )
    })?;
    temporary
        .write_all(contents.as_bytes())
        .with_context(|| format!("failed to write temporary output for {}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to flush temporary output for {}", path.display()))?;
    temporary.persist(path).map_err(|error| {
        anyhow!(error.error).context(format!("failed to atomically replace {}", path.display()))
    })?;
    Ok(())
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

fn reject_symlink_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "refusing to write through symlink output path: {}",
                    current.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", current.display()));
            }
        }
    }
    Ok(())
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

fn output_collision_key(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
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
    fn output_collision_key_is_case_insensitive() {
        assert_eq!(
            output_collision_key(Path::new("site/Guide.html")),
            output_collision_key(Path::new("site/guide.html"))
        );
    }

    #[test]
    fn infers_title_from_file_stem() {
        assert_eq!(
            inferred_title(Path::new("docs/getting-started.md")),
            Some("getting started".to_owned())
        );
    }
}

use std::fmt;

use ammonia::Builder;
use html_escape::{encode_double_quoted_attribute, encode_safe};
use once_cell::sync::Lazy;
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html::push_html};
use syntect::{
    highlighting::ThemeSet,
    html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use thiserror::Error;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);
const HIGHLIGHT_CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "syntect-" };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    pub page: bool,
    pub unsafe_html: bool,
    pub highlight: bool,
    pub style: PageStyle,
    pub title: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            page: false,
            unsafe_html: false,
            highlight: false,
            style: PageStyle::None,
            title: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStyle {
    Github,
    Minimal,
    None,
}

impl PageStyle {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "github" => Some(Self::Github),
            "minimal" => Some(Self::Minimal),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

impl fmt::Display for PageStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github => f.write_str("github"),
            Self::Minimal => f.write_str("minimal"),
            Self::None => f.write_str("none"),
        }
    }
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("syntax highlighting failed: {0}")]
    Highlight(#[from] syntect::Error),
}

pub fn render(markdown: &str, options: &RenderOptions) -> Result<String, RenderError> {
    let fragment = render_fragment(markdown, options)?;

    if options.page {
        Ok(wrap_page(&fragment, options))
    } else {
        Ok(fragment)
    }
}

pub fn render_fragment(markdown: &str, options: &RenderOptions) -> Result<String, RenderError> {
    let mut html = if options.highlight {
        render_with_highlighting(markdown)?
    } else {
        render_without_highlighting(markdown)
    };

    if !options.unsafe_html {
        html = sanitize_fragment(&html);
    }

    Ok(html)
}

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
}

fn render_without_highlighting(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, markdown_options());
    let mut html = String::new();
    push_html(&mut html, parser);
    html
}

fn render_with_highlighting(markdown: &str) -> Result<String, RenderError> {
    let parser = Parser::new_ext(markdown, markdown_options());
    let mut events = Vec::new();
    let mut iter = parser.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = code_block_language(&kind).map(str::to_owned);
                let mut code = String::new();

                for code_event in iter.by_ref() {
                    match code_event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text)
                        | Event::Code(text)
                        | Event::Html(text)
                        | Event::InlineHtml(text) => code.push_str(&text),
                        Event::SoftBreak | Event::HardBreak => code.push('\n'),
                        _ => {}
                    }
                }

                let highlighted = highlight_code(&code, language.as_deref())?;
                events.push(Event::Html(CowStr::from(highlighted)));
            }
            _ => events.push(event.into_static()),
        }
    }

    let mut html = String::new();
    push_html(&mut html, events.into_iter());
    Ok(html)
}

fn code_block_language<'a>(kind: &'a CodeBlockKind<'a>) -> Option<&'a str> {
    match kind {
        CodeBlockKind::Fenced(info) => info.split_whitespace().next().filter(|s| !s.is_empty()),
        CodeBlockKind::Indented => None,
    }
}

fn highlight_code(code: &str, language: Option<&str>) -> Result<String, RenderError> {
    let syntax = language
        .and_then(|lang| SYNTAX_SET.find_syntax_by_token(lang))
        .or_else(|| language.and_then(|lang| SYNTAX_SET.find_syntax_by_extension(lang)))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let mut generator =
        ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAX_SET, HIGHLIGHT_CLASS_STYLE);

    for line in LinesWithEndings::from(code) {
        if line.ends_with('\n') {
            generator.parse_html_for_line_which_includes_newline(line)?;
        } else {
            let mut owned = String::from(line);
            owned.push('\n');
            generator.parse_html_for_line_which_includes_newline(&owned)?;
        }
    }

    let highlighted = generator.finalize();
    let class_attr = language
        .map(|lang| {
            format!(
                " class=\"language-{}\"",
                encode_double_quoted_attribute(lang)
            )
        })
        .unwrap_or_default();

    Ok(format!(
        "<pre><code{class_attr}>{highlighted}</code></pre>\n"
    ))
}

fn sanitize_fragment(html: &str) -> String {
    Builder::default()
        .add_generic_attributes(&["class"])
        .add_tags(&["input"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attribute_values("input", "type", &["checkbox"])
        .clean(html)
        .to_string()
}

fn wrap_page(fragment: &str, options: &RenderOptions) -> String {
    let title = options.title.as_deref().unwrap_or("Document");
    let css = page_css(options);

    format!(
        concat!(
            "<!doctype html>\n",
            "<html lang=\"en\">\n",
            "<head>\n",
            "<meta charset=\"utf-8\">\n",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
            "<title>{title}</title>\n",
            "{style_block}",
            "</head>\n",
            "<body>\n",
            "<article class=\"markdown-body\">\n",
            "{fragment}",
            "</article>\n",
            "</body>\n",
            "</html>\n"
        ),
        title = encode_safe(title),
        style_block = if css.is_empty() {
            String::new()
        } else {
            format!("<style>\n{css}\n</style>\n")
        },
        fragment = fragment
    )
}

fn page_css(options: &RenderOptions) -> String {
    let mut css = match options.style {
        PageStyle::Github => github_css().to_owned(),
        PageStyle::Minimal => minimal_css().to_owned(),
        PageStyle::None => String::new(),
    };

    if options.highlight
        && let Some(highlight_css) = highlight_css()
    {
        if !css.is_empty() {
            css.push('\n');
        }
        css.push_str(&highlight_css);
    }

    css
}

fn highlight_css() -> Option<String> {
    let theme = THEME_SET
        .themes
        .get("InspiredGitHub")
        .or_else(|| THEME_SET.themes.values().next())?;

    css_for_theme_with_class_style(theme, HIGHLIGHT_CLASS_STYLE).ok()
}

fn minimal_css() -> &'static str {
    r#"body {
  margin: 0;
  color: #24292f;
  background: #ffffff;
  font: 16px/1.6 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

.markdown-body {
  box-sizing: border-box;
  max-width: 900px;
  margin: 0 auto;
  padding: 40px 24px;
}

pre {
  overflow: auto;
}"#
}

fn github_css() -> &'static str {
    r#"body {
  margin: 0;
  color: #24292f;
  background: #ffffff;
  font: 16px/1.5 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

.markdown-body {
  box-sizing: border-box;
  max-width: 980px;
  margin: 0 auto;
  padding: 45px;
}

.markdown-body h1,
.markdown-body h2,
.markdown-body h3 {
  margin-top: 24px;
  margin-bottom: 16px;
  font-weight: 600;
  line-height: 1.25;
}

.markdown-body h1 {
  padding-bottom: .3em;
  font-size: 2em;
  border-bottom: 1px solid #d0d7de;
}

.markdown-body h2 {
  padding-bottom: .3em;
  font-size: 1.5em;
  border-bottom: 1px solid #d0d7de;
}

.markdown-body h3 {
  font-size: 1.25em;
}

.markdown-body p,
.markdown-body blockquote,
.markdown-body ul,
.markdown-body ol,
.markdown-body dl,
.markdown-body table,
.markdown-body pre {
  margin-top: 0;
  margin-bottom: 16px;
}

.markdown-body a {
  color: #0969da;
  text-decoration: none;
}

.markdown-body a:hover {
  text-decoration: underline;
}

.markdown-body blockquote {
  padding: 0 1em;
  color: #57606a;
  border-left: .25em solid #d0d7de;
}

.markdown-body code,
.markdown-body kbd,
.markdown-body samp {
  padding: .2em .4em;
  margin: 0;
  font-size: 85%;
  background-color: rgba(175, 184, 193, .2);
  border-radius: 6px;
}

.markdown-body pre {
  padding: 16px;
  overflow: auto;
  font-size: 85%;
  line-height: 1.45;
  background-color: #f6f8fa;
  border-radius: 6px;
}

.markdown-body pre code {
  display: inline;
  max-width: auto;
  padding: 0;
  margin: 0;
  overflow: visible;
  line-height: inherit;
  background-color: transparent;
  border: 0;
}

.markdown-body table {
  display: block;
  width: max-content;
  max-width: 100%;
  overflow: auto;
  border-spacing: 0;
  border-collapse: collapse;
}

.markdown-body table th,
.markdown-body table td {
  padding: 6px 13px;
  border: 1px solid #d0d7de;
}

.markdown-body table tr {
  background-color: #ffffff;
  border-top: 1px solid #d8dee4;
}

.markdown-body table tr:nth-child(2n) {
  background-color: #f6f8fa;
}

.markdown-body img {
  max-width: 100%;
}

@media (max-width: 767px) {
  .markdown-body {
    padding: 24px 16px;
  }
}"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_safe_fragment_by_default() {
        let html = render(
            "# Hello\n\n<script>alert(1)</script>\n\n<a href=\"javascript:alert(1)\">bad</a>",
            &RenderOptions::default(),
        )
        .unwrap();

        assert!(html.contains("<h1>Hello</h1>"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn can_keep_raw_html_when_explicitly_unsafe() {
        let options = RenderOptions {
            unsafe_html: true,
            ..RenderOptions::default()
        };

        let html = render("<section>raw</section>", &options).unwrap();

        assert!(html.contains("<section>raw</section>"));
    }

    #[test]
    fn wraps_complete_page() {
        let options = RenderOptions {
            page: true,
            style: PageStyle::Github,
            title: Some("Example".to_owned()),
            ..RenderOptions::default()
        };

        let html = render("# Example", &options).unwrap();

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>Example</title>"));
        assert!(html.contains("markdown-body"));
    }

    #[test]
    fn highlights_code_blocks_with_classes() {
        let options = RenderOptions {
            highlight: true,
            ..RenderOptions::default()
        };

        let html = render("```rust\nfn main() {}\n```", &options).unwrap();

        assert!(html.contains("<pre><code class=\"language-rust\">"));
        assert!(html.contains("syntect-"));
    }

    #[test]
    fn renders_task_list_checkbox_after_sanitizing() {
        let html = render("- [x] done", &RenderOptions::default()).unwrap();

        assert!(html.contains("<input"));
        assert!(html.contains("checkbox"));
    }
}

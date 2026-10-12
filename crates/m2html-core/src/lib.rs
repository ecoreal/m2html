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

/// Maximum Markdown input accepted by the renderer, in bytes.
///
/// The emitted HTML can be larger than the Markdown it came from, so the
/// derived [`MAX_HTML_BYTES`] and [`MAX_NESTING_DEPTH`] limits are checked
/// against the rendered HTML as well.
pub const MAX_MARKDOWN_BYTES: usize = 16 * 1024 * 1024;

/// Maximum HTML nesting depth accepted by the renderer.
///
/// Without a depth limit a single 8 MiB line of blank-quote markers (`> `)
/// expands to 4 million nested `<blockquote>` elements, which exhausts memory
/// or takes minutes to process in `html5ever`'s tree builder. Capping the depth
/// keeps the renderer bounded on untrusted input while staying far above
/// anything real Markdown reaches. The limit applies to both the sanitized
/// default path and the `--unsafe-html` path, so downstream consumers can rely
/// on bounded nesting either way.
pub const MAX_NESTING_DEPTH: usize = 512;

/// Maximum size of the rendered HTML, in bytes.
///
/// Markdown expands as it is rendered (escaping, generated markup), and with
/// syntax highlighting a small fenced block becomes a large span tree. Bounding
/// the output keeps expensive downstream work — most notably the HTML
/// sanitizer, whose cost grows with the size of its input — proportional to
/// [`MAX_MARKDOWN_BYTES`] even when the expansion factor is at its worst.
pub const MAX_HTML_BYTES: usize = 64 * 1024 * 1024;

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
    #[error("Markdown input is too large: {actual} bytes exceeds the maximum of {max} bytes")]
    InputTooLarge { actual: usize, max: usize },
    #[error("rendered HTML is too large: {actual} bytes exceeds the maximum of {max} bytes")]
    OutputTooLarge { actual: usize, max: usize },
    #[error("Markdown content nests deeper than the maximum depth of {max}")]
    TooDeeplyNested { max: usize },
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
    ensure_input_size(markdown)?;

    // Markdown rendering is bounded by the input size, but the emitted HTML can
    // grow several times larger than the Markdown it came from (`> > > a`
    // becomes nested `<blockquote><p>` pairs, a highlighted fence becomes a
    // span tree). The sanitizer's cost grows with the size and nesting depth of
    // its input, so both are checked against the rendered HTML before any
    // expensive downstream step runs.
    let limits_html = render_without_highlighting(markdown);
    if limits_html.len() > MAX_HTML_BYTES {
        return Err(RenderError::OutputTooLarge {
            actual: limits_html.len(),
            max: MAX_HTML_BYTES,
        });
    }
    ensure_nesting_depth(&limits_html)?;

    let mut html = if options.highlight {
        render_with_highlighting(markdown)?
    } else {
        limits_html
    };

    if !options.unsafe_html {
        html = sanitize_fragment(&html);
    }

    Ok(html)
}

fn ensure_input_size(markdown: &str) -> Result<(), RenderError> {
    if markdown.len() > MAX_MARKDOWN_BYTES {
        return Err(RenderError::InputTooLarge {
            actual: markdown.len(),
            max: MAX_MARKDOWN_BYTES,
        });
    }

    Ok(())
}

fn ensure_nesting_depth(html: &str) -> Result<(), RenderError> {
    let mut depth: usize = 0;
    let mut rest = html;

    while let Some(index) = rest.find('<') {
        rest = &rest[index + 1..];

        let Some(end) = rest.find('>') else {
            // Not a tag; scan the remaining text for the next `<`.
            continue;
        };
        let tag = &rest[..end];

        if tag.starts_with('/') {
            depth = depth.saturating_sub(1);
        } else if !tag.starts_with(['!', '?']) && !tag.ends_with('/') {
            depth += 1;
            if depth > MAX_NESTING_DEPTH {
                return Err(RenderError::TooDeeplyNested {
                    max: MAX_NESTING_DEPTH,
                });
            }
        }

        rest = &rest[end + 1..];
    }

    Ok(())
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
    fn rejects_markdown_above_the_input_limit() {
        let markdown = "x".repeat(MAX_MARKDOWN_BYTES + 1);
        let error = render(&markdown, &RenderOptions::default()).unwrap_err();

        match error {
            RenderError::InputTooLarge { actual, max } => {
                assert_eq!(actual, MAX_MARKDOWN_BYTES + 1);
                assert_eq!(max, MAX_MARKDOWN_BYTES);
            }
            other => panic!("unexpected render error: {other:?}"),
        }
    }

    #[test]
    fn rejects_deeply_nested_block_quotes() {
        let markdown = format!("{}x\n", "> ".repeat(MAX_NESTING_DEPTH + 1));

        // Unbounded nesting reaches html5ever's superlinear tree builder, so
        // this input must be rejected before rendering and sanitizing.
        let error = render(&markdown, &RenderOptions::default()).unwrap_err();

        match error {
            RenderError::TooDeeplyNested { max } => assert_eq!(max, MAX_NESTING_DEPTH),
            other => panic!("unexpected render error: {other:?}"),
        }
    }

    #[test]
    fn rejects_fragments_whose_sanitized_output_expands_past_the_limit() {
        // A blank quoted line ends the block quote, so each unit renders one
        // `<blockquote>` plus one `<p>` per level (~10x). The rendered fragment
        // stays shallow but grows past the output limit, which must reject it
        // before the sanitizer sees the fragment.
        let unit = format!("{}a\n>\n", "> ".repeat(10));
        let markdown = unit.repeat(MAX_MARKDOWN_BYTES / 2 / unit.len());

        let error = render(&markdown, &RenderOptions::default()).unwrap_err();

        assert!(
            matches!(error, RenderError::OutputTooLarge { .. }),
            "unexpected render error: {error:?}"
        );
    }

    #[test]
    fn counts_only_start_tags_toward_the_nesting_depth() {
        // Self-closing and void tags must not increase the depth, and closing
        // tags must not underflow.
        let html = "</div>".repeat(MAX_NESTING_DEPTH + 1);
        assert!(ensure_nesting_depth(&html).is_ok());

        let html = format!("{}<br/>x", "<div>".repeat(MAX_NESTING_DEPTH));
        assert!(ensure_nesting_depth(&html).is_ok());

        let html = format!("{}<img src=\"x\">", "<div>".repeat(MAX_NESTING_DEPTH));
        assert!(ensure_nesting_depth(&html).is_err());
    }

    #[test]
    fn ignores_unterminated_angle_brackets_when_counting_depth() {
        // A literal `<` in text must not swallow the rest of the document.
        let html = format!("{}a < b", "<div>".repeat(MAX_NESTING_DEPTH));
        assert!(ensure_nesting_depth(&html).is_ok());

        let html = format!("{}<div>", "<div>".repeat(MAX_NESTING_DEPTH));
        assert!(ensure_nesting_depth(&html).is_err());
    }

    #[test]
    fn rejects_deeply_nested_literal_html() {
        let markdown = format!(
            "{}x{}\n",
            "<div>".repeat(MAX_NESTING_DEPTH + 1),
            "</div>".repeat(MAX_NESTING_DEPTH + 1)
        );

        let error = render(&markdown, &RenderOptions::default()).unwrap_err();

        assert!(matches!(error, RenderError::TooDeeplyNested { .. }));
    }

    #[test]
    fn accepts_nesting_at_the_depth_limit() {
        // The limit applies to the rendered HTML, where each nesting level also
        // opens a paragraph, so the deepest accepted Markdown block nesting is
        // one level below `MAX_NESTING_DEPTH`.
        let quotes = MAX_NESTING_DEPTH - 1;
        let markdown = format!("{}x\n", "> ".repeat(quotes));

        let html = render(&markdown, &RenderOptions::default()).unwrap();

        assert_eq!(html.matches("<blockquote>").count(), quotes);
    }

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

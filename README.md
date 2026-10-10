<!-- ecoreal-repo-header: m2html -->
<div align="center">

<img src="assets/repo-icon.png" width="160" alt="M2HTML icon">

# M2HTML

Rust-based Markdown to HTML renderer and CLI.

</div>

# m2html

Fast, safe Markdown to HTML rendering powered by Rust.

`m2html` is a developer-focused Markdown exporter with a Rust core, a CLI for
batch conversion, and Python bindings for application code. The v1 goal is a
small, sharp tool: fast rendering, safe defaults, pleasant standalone HTML, and
simple installation.

## Features

- Rust rendering core built on `pulldown-cmark`.
- CLI defaults to complete standalone HTML pages.
- Python API defaults to safe embeddable HTML fragments.
- Raw Markdown HTML is sanitized by default.
- GitHub-style page CSS for exported documents.
- Syntax highlighting for fenced code blocks in CLI page output.
- Recursive directory export that preserves relative paths.

## Install

```sh
cargo install m2html
```

```sh
pip install m2html
```

The Python package is built with PyO3 and maturin.

## CLI

Render a single Markdown file to stdout:

```sh
m2html README.md
```

Render a single file to a standalone page:

```sh
m2html README.md -o README.html
```

Render a directory into `site/`:

```sh
m2html docs/ -o site/
```

Directory export preserves relative paths and refuses to overwrite an output when multiple Markdown extensions map to the same `.html` path.

Emit an embeddable fragment instead of a complete page:

```sh
m2html README.md --fragment
```

Allow raw HTML only when you trust the input:

```sh
m2html README.md --unsafe-html
```

Useful options:

```sh
m2html --help
```

## Python

```python
import m2html

html = m2html.render("# Hello")
assert html == "<h1>Hello</h1>\n"

page = m2html.render("# Hello", page=True, style="github", highlight=True)
```

Render a file:

```python
import m2html

m2html.render_file("README.md", "README.html", page=True, style="github", highlight=True)
```

Python defaults are intentionally conservative:

- `page=False`
- `unsafe_html=False`
- `highlight=False`
- `style="none"`

## Security

`m2html` sanitizes rendered HTML by default. This protects common cases where
Markdown output is inserted into a website or served to other users.

Use `--unsafe-html` or `unsafe_html=True` only for trusted Markdown. That mode
preserves raw HTML from the input and can expose XSS vulnerabilities if the
output is served to browsers.

## Benchmarks

The intended benchmark set compares Python-facing rendering against:

- `markdown-it-py`
- `mistune`
- `Python-Markdown`

Benchmarks should report machine details, document size, throughput, and
latency. The first public benchmark should include small README files, medium
documentation pages, and large generated Markdown files.

## Development

Build everything:

```sh
cargo build --workspace
```

Run tests:

```sh
cargo test --workspace
```

Build the Python extension locally:

```sh
python -m pip install maturin
maturin develop
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license

at your option.

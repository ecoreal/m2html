"""Fast, safe Markdown to HTML rendering powered by Rust."""

from ._native import __version__, render, render_file

__all__ = ["__version__", "render", "render_file"]

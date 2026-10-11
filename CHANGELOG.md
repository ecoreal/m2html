# Changelog

## 0.1.0

- Initial Rust core, CLI, and Python binding.
- Reject symlinked output paths during directory and single-file export.
- Detect output collisions case-insensitively for cross-platform safety.
- Cap Markdown input at 16 MiB and reject oversized or non-UTF-8 files before rendering.
- Write CLI and Python outputs atomically via a temporary file to avoid partial results.

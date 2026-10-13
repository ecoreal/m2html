# Contributing

Thanks for improving `m2html`.

## Development

```sh
cargo build --workspace
cargo test --workspace
```

For Python binding work:

```sh
python -m pip install maturin
maturin develop
python -c "import m2html; print(m2html.render('# ok'))"
```

## Pull Requests

- Keep changes focused.
- Add tests for behavior changes.
- Run `cargo test --workspace` before opening a PR.
- Update README examples when public behavior changes.

The [pull request template](.github/PULL_REQUEST_TEMPLATE.md) lists the full
checklist; sanitizer changes additionally go through
[SECURITY.md](SECURITY.md) for private disclosure rules. Release mechanics
live in [docs/releasing.md](docs/releasing.md).

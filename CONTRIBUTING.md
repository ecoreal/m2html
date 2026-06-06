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

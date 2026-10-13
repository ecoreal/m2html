## Summary

## Type

- [ ] Bug fix
- [ ] Feature
- [ ] Sanitizer / security fix
- [ ] Performance
- [ ] Docs

## Checklist

- [ ] `cargo test --workspace` passes locally.
- [ ] `cargo clippy --workspace --all-targets` is clean for the touched crates.
- [ ] Behavior changes include tests.
- [ ] README / CHANGELOG updated when public behavior changes.
- [ ] Sanitizer changes stay on the allowlist path (no new pass-through of raw HTML in the default mode).

## Verification

Paste the commands you ran and their result, e.g.:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

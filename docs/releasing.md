# Releasing

`m2html` publishes to crates.io with GitHub Actions and crates.io Trusted
Publishing (OIDC). No API token is stored in the repository or in GitHub
secrets; the workflow exchanges a short-lived OIDC token at publish time.

Two crates are published, in dependency order: `m2html-core` first, then the
`m2html` CLI (which depends on the core at the same version). The
`m2html-python` crate is `publish = false` — it ships through maturin wheels,
not crates.io.

## One-Time Setup (maintainer)

1. On crates.io, open each crate's Trusted Publishers page (after the first
   manual publish, or via a Trusted Publisher registration on
   https://crates.io/settings/tokens):
   - Crate: `m2html-core` — Repo: `ecoreal/m2html`, workflow `publish.yml`,
     environment `publish`.
   - Crate: `m2html` — same values.
2. In the GitHub repository, create an environment named `publish`
   (Settings → Environments). Optionally add required reviewers so a human
   approves each publish.

## Cutting a Release

1. Bump `version` in the workspace `Cargo.toml` (`[workspace.package]`).
   Every crate inherits it, so both published crates move together.
2. Add a matching section to `CHANGELOG.md`.
3. Verify packaging locally before tagging:

   ```bash
   cargo test --workspace
   cargo publish -p m2html-core --dry-run
   ```

   (The `m2html` dry-run only succeeds after the matching `m2html-core`
   version exists on crates.io; the workflow handles this ordering.)
4. Tag and push the tag, then publish a GitHub Release for that tag.
   Publishing the release triggers `publish.yml`.

## After Publishing

- Confirm the crates appear on crates.io and the docs build on docs.rs.
- `cargo install m2html` should install the release build of the CLI.

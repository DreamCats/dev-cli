# dev-cli Agent Guide

`dev-cli` is the Rust implementation of the `dev` remote development CLI. Its
project shape follows the sibling `../cg-cli`; compatibility with the retired
Go implementation is encoded in tests and public command contracts.

## Boundaries

- One Cargo package and one binary, `dev`. Do not add a workspace, daemon,
  async runtime, plugin system, or transport abstraction without measured need.
- `src/main.rs` is a thin process shell. `src/lib.rs` owns lifecycle and local
  history. `src/cli.rs` only parses Clap arguments. Domain behavior stays in
  `src/commands`, `config`, `transport`, and `stats`.
- Preserve `~/.config/dev-cli/config.yaml`, JSON/NDJSON schemas, exit codes,
  stdout/stderr separation, SSH/SCP arguments, and the Go command surface.
- The repository has no runtime or source dependency on the retired Go
  implementation. A command is not fully compatible until a local fixture or
  differential test covers its public contract.
- Never record command arguments, paths, file contents, stdout, stderr, tokens,
  or credentials in history.

## Required checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```

## Releases

- `Cargo.toml` is the single source of truth for the application version.
- A push to `main` or a pull request runs CI only. A release is created only by
  pushing a matching `vX.Y.Z` tag; the release workflow rejects a tag whose
  version differs from `Cargo.toml`.
- Run the required checks before creating a release tag. Do not force-move or
  re-push a published release tag.
- Keep the `dev-<target>.tar.gz` asset names and `SHA256SUMS` manifest stable:
  `scripts/install.sh` and `dev update` verify and consume that contract.

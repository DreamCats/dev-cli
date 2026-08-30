# dev-cli Agent Guide

`dev-cli` is the Rust implementation of the `dev` remote development CLI. Its
behavioral reference is the sibling Go repository `../dev-connect`; its project
shape follows `../cg-cli`.

## Boundaries

- One Cargo package and one binary, `dev`. Do not add a workspace, daemon,
  async runtime, plugin system, or transport abstraction without measured need.
- `src/main.rs` is a thin process shell. `src/lib.rs` owns lifecycle and local
  history. `src/cli.rs` only parses Clap arguments. Domain behavior stays in
  `src/commands`, `config`, `transport`, and `stats`.
- Preserve `~/.config/dev-connect/config.yaml`, JSON/NDJSON schemas, exit codes,
  stdout/stderr separation, SSH/SCP arguments, and the Go command surface.
- The Go implementation is the behavior oracle. A command is not fully
  compatible until a local fixture or differential test covers it.
- Never record command arguments, paths, file contents, stdout, stderr, tokens,
  or credentials in history.

## Required checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```


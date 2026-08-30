# dev-cli

Rust implementation of the `dev` remote development CLI. The binary remains
`dev` and reuses the Go/Python-compatible configuration at
`~/.config/dev-connect/config.yaml`.

## Compatibility scope

- Remote reads: `ls`, `cat`, `slice`, `grep`, `find`, `tree`, `head`, `tail`.
- Transfer and execution: `push`, `pull`, `exec`, `exec --watch`, `exec-watch`.
- Writes: `write`, `edit`, `diff`, and Codex structured `patch`.
- Repository helpers: `repo-status`, `repo-diff`, `git-snapshot`,
  `repo resolve`, and `verify go --changed`.
- Remote code graph: `cg install` and transparent `cg` proxying.
- Local state: `config`, `stats`, and redacted `history`.
- POSIX plus the Go baseline's Windows subset (`exec`, `ls`, `cat`, `head`,
  `tail`, `grep`, and `write`); unsupported Windows commands fail explicitly.

The Go repository `../dev-connect` remains the behavioral reference. Current
local checks cover the command surface, config serialization, SSH-backed exec
and grep contracts, PowerShell builders, patch application, cg path injection,
history privacy, formatting, Clippy, tests, and release compilation. Real-host
smoke tests and release publication are separate acceptance states.

## Architecture

```text
src/main.rs              process exit/error shell
src/lib.rs               dispatch lifecycle and redacted history
src/cli.rs               Clap command/flag model
src/config.rs            compatible YAML config
src/transport.rs         SSH/SCP, timeout, quoting, PowerShell encoding
src/commands/            command behavior and stable remote scripts
src/stats.rs             local usage counters and private JSONL history
tests/cli.rs             CLI and fake-SSH acceptance tests
```

Like `cg-cli`, this is one Cargo package with a thin `main.rs`, deterministic
machine output, domain modules, and no daemon or async runtime.

## Develop and install

```bash
make check
make build
./target/release/dev --help
make install
make install-skill
```

`./install.sh` runs all checks and installs both the binary and Skill locally.
It does not publish a release or modify remote hosts.


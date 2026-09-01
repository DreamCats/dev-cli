# dev-cli

`dev-cli` is a Rust remote-development CLI built for humans and coding agents.
The binary remains `dev`; it preserves the Go `dev-connect` command surface,
configuration format, JSON/NDJSON contracts, and SSH/SCP behavior while running
as a single local process with no daemon.

## Features

- Remote reads and search: `ls`, `cat`, `slice`, `grep`, `find`, `tree`,
  `head`, and `tail`.
- Transfer and execution: `push`, `pull`, `exec`, `exec --watch`, and
  `exec-watch`.
- Precise writes: `write`, `edit`, `diff`, and structured `patch`.
- Repository helpers: `repo-status`, `repo-diff`, `git-snapshot`,
  `repo resolve`, and `verify go --changed`.
- Remote code graph installation and transparent `cg` proxying.
- Existing YAML configuration compatibility plus local, redacted `stats` and
  `history`.
- POSIX hosts and the Go baseline's explicit Windows command subset.
- Deterministic JSON output for structured commands and fail-loud remote errors.

## Install

Install the latest verified release and the `dev-connect` Skill:

```bash
curl -fsSL https://raw.githubusercontent.com/DreamCats/dev-cli/main/scripts/install.sh | sh
```

This installs:

```text
~/.local/bin/dev
~/.agents/skills/dev-connect/SKILL.md
```

Override either location with `DEV_INSTALL_DIR` or `DEV_SKILL_DIR`. The
installer supports Apple Silicon macOS, Intel macOS, and x86_64 Linux. It
downloads the matching GitHub Release archive and verifies it against the
published `SHA256SUMS` before installing.

Configuration and private local history live under `~/.config/dev-cli/`.

Install from source with Rust 1.85+:

```bash
git clone git@github.com:DreamCats/dev-cli.git
cd dev-cli
make check
make install
make install-skill
```

## Update

```bash
dev update --check
dev update
```

`dev update` resolves GitHub's public latest-release redirect, downloads the
matching archive, verifies `SHA256SUMS`, and replaces an installed release
binary. It refuses to replace a development binary under a Cargo `target`
directory.

Check the installed version with either `dev --version` or `dev version`.

## Quick start

List configured hosts and inspect a remote directory:

```bash
dev config show
dev ls --host sgdev --cwd ~/repo .
dev cat --host sgdev --cwd ~/repo README.md
dev grep --host sgdev --cwd ~/repo 'handle_request' src
```

Run a command or watch a longer operation:

```bash
dev exec --host sgdev --cwd ~/repo -- git status --short
dev exec --host sgdev --cwd ~/repo --watch --timeout 300 -- cargo test
```

Use `--json` for agent-readable output when the command supports structured
results:

```bash
dev --json config show
dev --json grep --host sgdev --cwd ~/repo 'TODO' src
dev --json history --limit 20
```

Run `dev <command> --help` for command-specific flags.

`dev --json git-snapshot --cwd REPO` preserves the existing short `head` field
and also returns `head_full` for the complete commit SHA. `origin_url` contains
the configured `origin` URL; when the remote is unavailable it is `null` and
`origin_error` contains the Git error instead of silently treating the lookup
as an empty remote.

## Commands

| Commands | Purpose |
| --- | --- |
| `ls`, `cat`, `slice`, `head`, `tail`, `tree` | Read bounded remote filesystem content. |
| `grep`, `find` | Search remote contents or paths. |
| `push`, `pull` | Transfer files through SCP. |
| `exec`, `exec-watch` | Run or observe remote commands. |
| `write`, `edit`, `diff`, `patch` | Apply explicit remote file changes. |
| `repo-status`, `repo-diff`, `git-snapshot` | Inspect remote Git state. |
| `repo resolve`, `verify` | Resolve repositories and run scoped checks. |
| `cg` | Install or proxy the remote Rust code-graph CLI. |
| `config` | Manage compatible host configuration. |
| `stats`, `history` | Inspect private local usage metadata. |
| `version`, `update [--check]` | Inspect or update the installed CLI. |

## Configuration

`dev-cli` uses its own configuration root:

```text
~/.config/dev-cli/
├── config.yaml
├── stats.json
└── history.jsonl
```

Set `XDG_CONFIG_HOME` to relocate the root. History stores only redacted command
names, success state, duration, timestamp, and optional session ID. It never
records arguments, paths, file contents, stdout, stderr, tokens, or credentials.

## Architecture

```text
src/main.rs              process exit/error shell
src/lib.rs               dispatch lifecycle and redacted history
src/cli.rs               Clap command/flag model
src/config.rs            compatible YAML configuration
src/transport.rs         SSH/SCP, timeout, quoting, PowerShell encoding
src/commands/            command behavior and stable remote scripts
src/update.rs            verified GitHub Release self-update
src/stats.rs             local counters and private JSONL history
tests/cli.rs             CLI and fake-SSH acceptance tests
```

Compatibility with the retired Go implementation is preserved through public
command contracts, local fixtures, and differential tests. The repository has
no runtime or source dependency on the former Go source tree.

## Release model

`Cargo.toml` is the single source of truth for the version. CI runs formatting,
Clippy, tests, and a release build on pushes and pull requests to `main`.

A GitHub Release is created only when a matching `vX.Y.Z` tag is pushed. The
release workflow rejects a tag that does not match `Cargo.toml`, builds these
archives, and publishes a combined `SHA256SUMS`:

```text
dev-aarch64-apple-darwin.tar.gz
dev-x86_64-apple-darwin.tar.gz
dev-x86_64-unknown-linux-musl.tar.gz
```

The Linux binary is built with the musl target and must pass a static-link gate
before publication.

## Development

```bash
make check
make build
./target/release/dev --help
```

Required checks are formatting, Clippy with warnings denied, all tests, and a
release build. See [AGENTS.md](AGENTS.md) for repository rules and
[skills/dev-connect/SKILL.md](skills/dev-connect/SKILL.md) for agent usage.

#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$script_dir"

command -v cargo >/dev/null 2>&1 || { echo "error: cargo is not installed" >&2; exit 1; }
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
make install
make install-skill

echo "installed dev to ${PREFIX:-$HOME/.local}/bin/dev"
echo "existing ~/.config/dev-connect/config.yaml remains compatible"


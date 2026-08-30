#!/usr/bin/env sh
set -eu

repo="DreamCats/dev-cli"
bin_dir="${DEV_INSTALL_DIR:-$HOME/.local/bin}"
skill_dir="${DEV_SKILL_DIR:-$HOME/.agents/skills/dev-connect}"
os="$(uname -s)"
arch="$(uname -m)"

case "$os/$arch" in
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  Linux/x86_64) target="x86_64-unknown-linux-musl" ;;
  *) echo "unsupported platform: $os/$arch" >&2; exit 1 ;;
esac

archive="dev-$target.tar.gz"
base="https://github.com/$repo/releases/latest/download"
skill_url="https://raw.githubusercontent.com/$repo/main/skills/dev-connect/SKILL.md"
temp="$(mktemp -d)"
trap 'rm -rf "$temp"' EXIT

curl --fail --location --silent --show-error "$base/$archive" -o "$temp/$archive"
curl --fail --location --silent --show-error "$base/SHA256SUMS" -o "$temp/SHA256SUMS"
curl --fail --location --silent --show-error "$skill_url" -o "$temp/SKILL.md"
expected="$(awk -v file="$archive" '$2 == file { print $1 }' "$temp/SHA256SUMS")"
test -n "$expected" || { echo "missing checksum for $archive" >&2; exit 1; }
if command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$temp/$archive" | awk '{print $1}')"
else
  actual="$(sha256sum "$temp/$archive" | awk '{print $1}')"
fi
test "$actual" = "$expected" || { echo "checksum verification failed" >&2; exit 1; }
tar -xzf "$temp/$archive" -C "$temp"
mkdir -p "$bin_dir"
install -m 755 "$temp/dev" "$bin_dir/dev"
mkdir -p "$skill_dir"
install -m 644 "$temp/SKILL.md" "$skill_dir/SKILL.md"
echo "installed dev to $bin_dir/dev"
echo "installed dev-connect Skill to $skill_dir/SKILL.md"
echo "existing ~/.config/dev-connect/config.yaml remains compatible"

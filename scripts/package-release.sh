#!/usr/bin/env bash
# Build an archive for the current host; GTK4 remains a system dependency.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
arch=$(uname -m)
name="archeasy-${version}-${arch}-linux"
cargo build --release --locked --offline
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
mkdir -p "$stage/$name/assets" "$stage/$name/scripts"
install -m755 target/release/archeasy "$stage/$name/archeasy"
install -m644 LICENSE README.md "$stage/$name/"
install -m644 assets/archeasy.desktop assets/archeasy.svg assets/AppCleaner-LICENSE assets/Octicons-LICENSE "$stage/$name/assets/"
mkdir -p "$stage/$name/assets/fonts"
install -m644 assets/fonts/Ribeye-Regular.ttf assets/fonts/Ribeye-OFL.txt "$stage/$name/assets/fonts/"
install -m755 scripts/install-desktop.sh "$stage/$name/scripts/"
tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
printf 'Release archive: %s/dist/%s.tar.gz\n' "$root" "$name"

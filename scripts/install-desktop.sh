#!/usr/bin/env bash
# Run after cargo install --path . --locked (or after installing the release binary).
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 "$root/assets/archeasy.desktop" "$data_home/applications/archeasy.desktop"
install -Dm644 "$root/assets/archeasy.svg" "$data_home/icons/hicolor/scalable/apps/archeasy.svg"
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$data_home/applications"
fi
printf 'ArchEasy launcher installed. Ensure the archeasy binary is on your desktop session PATH.\n'

# ArchEasy first-run setup. Run as the desktop user; only pacman uses sudo.
set -euo pipefail
finish() {
    code=$?
    if (( code == 0 )); then printf '\nArchEasy setup complete. Search again to refresh results.\n';
    else printf '\nSetup stopped (code %s). Review the error above and retry.\n' "$code"; fi
    if [[ -t 0 ]]; then read -r -p 'Press Enter to close…' _ || true; fi
}
trap finish EXIT
[[ $EUID != 0 ]] || { printf 'Run ArchEasy as your normal desktop user.\n'; exit 1; }
printf 'ArchEasy setup: refresh and upgrade Arch, install Flatpak and build tools, then set up paru and Flathub.\n'
sudo pacman -Syu --needed -- base-devel git flatpak python
if ! command -v paru >/dev/null 2>&1; then
    build=$(mktemp -d -- "${TMPDIR:-/tmp}/archeasy-paru.XXXXXXXX")
    git clone -- https://aur.archlinux.org/paru.git "$build/paru"
    cd -- "$build/paru"
    printf '\nReview the paru build script before continuing:\n'
    if command -v less >/dev/null 2>&1; then less PKGBUILD; else cat PKGBUILD; fi
    read -r -p 'Build and install paru? [y/N] ' answer
    [[ $answer == y || $answer == Y ]] || exit 1
    makepkg -si
    printf 'Build files retained at %s\n' "$build"
fi
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

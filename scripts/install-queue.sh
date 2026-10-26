set -euo pipefail
finish() {
    code=$?
    if (( code == 0 )); then printf '\nAll queued packages completed.\n';
    else printf '\nQueue stopped (code %s). Earlier completed installs are kept.\n' "$code"; fi
    if [[ -t 0 ]]; then read -r -p 'Press Enter to close…' _ || true; fi
}
trap finish EXIT
(( $# > 0 && $# % 3 == 0 )) || exit 1
# Validate the complete queue before performing a transaction.
items=("$@")
for ((i=0; i<${#items[@]}; i+=3)); do
    [[ ${items[i+2]} =~ ^[a-zA-Z0-9@._+][a-zA-Z0-9@._+/-]*$ ]] || exit 1
    case ${items[i]} in
        Pacman) ;;
        AUR) command -v paru >/dev/null 2>&1 || command -v yay >/dev/null 2>&1 || { printf 'Run Setup ArchEasy to install paru first.\n'; exit 1; } ;;
        Flatpak) [[ ${items[i+1]} =~ ^[a-zA-Z0-9._][a-zA-Z0-9._-]*$ ]] || exit 1 ;;
        *) exit 1 ;;
    esac
done
updated=false
while (( $# )); do
    source=$1 remote=$2 package=$3; shift 3
    printf '\nInstalling %s (%s)\n' "$package" "$source"
    case $source in
        Pacman)
            if [[ $updated == false ]]; then sudo pacman -Syu --needed -- "$package"; updated=true;
            else sudo pacman -S --needed -- "$package"; fi ;;
        AUR)
            if command -v paru >/dev/null 2>&1; then paru -Syu --needed -- "$package";
            else yay -Syu --needed -- "$package"; fi ;;
        Flatpak) flatpak install --user -- "$remote" "$package" ;;
    esac
done

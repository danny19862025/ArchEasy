# Embedded in ArchEasy. All package names arrive as positional arguments.
set -euo pipefail
(( $# )) || { printf 'No packages selected.\n'; exit 1; }
for package in "$@"; do
    [[ $package != -* && $package != *[!a-zA-Z0-9@._+-]* && -n $package ]] || exit 1
done
current=$(pacman -Qdtq) || {
    code=$?
    if [[ $code == 1 && -z $current ]]; then
        printf 'No orphans remain. Refresh the list in ArchEasy.\n'
    else
        printf 'Could not recheck orphan packages.\n'
    fi
    exit 1
}
for package in "$@"; do
    found=false
    while IFS= read -r orphan; do [[ $orphan != "$package" ]] || found=true; done <<< "$current"
    if [[ $found != true ]]; then
        printf '%s is no longer an orphan. Refresh the list; nothing was removed.\n' "$package"
        exit 1
    fi
done
# Pacman shows the final transaction (including any unused dependencies) and
# asks for confirmation. Keep pacsave backups by omitting --nosave.
sudo pacman -Rs -- "$@"

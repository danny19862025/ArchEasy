set -euo pipefail
finish() {
    code=$?
    if (( code != 0 )); then printf '\nGitHub build stopped (code %s).\n' "$code"; fi
    if [[ -t 0 ]]; then read -r -p 'Press Enter to close…' _ || true; fi
}
trap finish EXIT
repo=${1:-}
[[ $repo =~ ^[a-zA-Z0-9._][a-zA-Z0-9._-]*/[a-zA-Z0-9._][a-zA-Z0-9._-]*$ ]] || exit 1
owner=${repo%/*} name=${repo#*/}
[[ $owner != . && $owner != .. && $name != . && $name != .. ]] || exit 1
(( EUID != 0 )) || { printf 'Run GitHub builds as your desktop user.\n'; exit 1; }
command -v git >/dev/null
command -v makepkg >/dev/null
# A private, unique workspace avoids stale clones, collisions and /tmp symlinks.
umask 077
workspace=$(mktemp -d "/tmp/archeasy-github-${owner}-${name}.XXXXXX")
printf 'Cloning https://github.com/%s into %s\n' "$repo" "$workspace/$name"
git clone -- "https://github.com/$repo.git" "$workspace/$name"
cd -- "$workspace/$name"
[[ -f PKGBUILD && ! -L PKGBUILD ]] || { printf 'This repository has no regular root PKGBUILD.\n'; exit 1; }
printf '\nBuild files: %s\nReview PKGBUILD and any referenced install scripts before continuing.\n\n' "$PWD"
cat -- PKGBUILD
read -r -p 'Build and install this PKGBUILD? [y/N] ' answer
[[ $answer == y || $answer == Y ]] || exit 0
makepkg -si

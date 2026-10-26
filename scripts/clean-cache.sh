set -euo pipefail
command -v paccache >/dev/null || { printf 'Install pacman-contrib to use the cache cleaner.\n'; exit 1; }
printf 'Pacman cache: '
du -sh /var/cache/pacman/pkg
printf 'Remove older cached package versions, keeping the newest three versions of each package.\n'
read -r -p 'Clean the cache? [y/N] ' answer
[[ $answer == y || $answer == Y ]] || exit 0
sudo paccache --remove --keep 3

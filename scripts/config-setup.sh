# Embedded in ArchEasy and invoked with bash; arguments are never evaluated as shell code.
set -u
binary=$1
job=$2
make_default=$3
install_missing=$4
shift 4
dependencies=("$@")
result=0
summary=()
finish() {
    local code=$1
    printf '\n──── Setup summary ────\n'
    printf '%s\n' "${summary[@]}"
    if [[ "$job" != '-' ]]; then printf '%s\n' "$code" > "$job.done"; fi
    flock -u 9 2>/dev/null || true
    exec 9>&-
    if [[ -t 0 ]]; then printf '\nPress Enter to close…'; read -r _ || true; fi
    exit "$code"
}
trap 'summary+=("Setup interrupted. Review any package-manager messages before retrying."); finish 130' HUP INT TERM
lock_dir=${XDG_RUNTIME_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/archeasy}
mkdir -p -- "$lock_dir" || exit 1
exec 9>"$lock_dir/archeasy-config-setup.lock"
if ! flock -n 9; then summary+=('Another config setup is running. Finish it first.'); finish 1; fi
printf 'ArchEasy — Config setup\nReview package and password prompts in this terminal.\n'
dependency_met() {
    command -v "$1" >/dev/null 2>&1 && return 0
    case "$1" in xdg-desktop-portal-hyprland|hyprpolkitagent) pacman -Q "$1" >/dev/null 2>&1 ;; *) return 1 ;; esac
}
missing=()
for dependency in "${dependencies[@]}"; do
    case "$dependency" in fish|starship|ghostty|fastfetch|kitty|Hyprland|nvim|tmux|waybar|rofi|wofi|btop|thunar|swaync|xdg-desktop-portal-hyprland|hyprpolkitagent|wl-copy|cliphist|grim|slurp|wpctl|brightnessctl) ;; *) summary+=('Invalid setup dependency.'); finish 1 ;; esac
    if ! dependency_met "$dependency"; then
        case "$dependency" in Hyprland) package=hyprland ;; nvim) package=neovim ;; wl-copy) package=wl-clipboard ;; wpctl) package=wireplumber ;; *) package=$dependency ;; esac
        missing+=("$package")
    fi
done
if (( ${#missing[@]} > 0 )); then
    if [[ "$install_missing" == true ]]; then
        printf '\nInstalling missing apps (includes Arch system update): %s\n' "${missing[*]}"
        if sudo pacman -Syu --needed -- "${missing[@]}" 9>&-; then
            for dependency in "${dependencies[@]}"; do
                if ! dependency_met "$dependency"; then summary+=("Dependency still unavailable: $dependency; configs were not changed"); finish 1; fi
            done
            summary+=('Dependencies: installation completed');
        else summary+=('Dependencies: failed or cancelled; configs and login shell were not changed'); finish 1; fi
    else summary+=("Config-only mode. Missing apps: ${missing[*]}"); fi
else summary+=('Dependencies: already installed'); fi
if [[ "$job" != '-' ]]; then
    if "$binary" --config-apply-job "$job" 9>&-; then summary+=('Configs: applied with one complete backup');
    else summary+=('Configs: apply failed; default shell step skipped. Review the error above.'); finish 1; fi
fi
if [[ "$make_default" == true ]]; then
    allowed=false
    while IFS= read -r line; do [[ "$line" == /usr/bin/fish ]] && allowed=true; done < /etc/shells
    if [[ -x /usr/bin/fish && "$allowed" == true ]]; then
        current=$(getent passwd "$(id -un)" | cut -d: -f7)
        if [[ "$current" == /usr/bin/fish ]]; then summary+=('Default shell: Fish was already selected');
        elif chsh -s /usr/bin/fish 9>&-; then summary+=('Default shell: Fish selected. Takes effect at your next login; no logout is forced.');
        else summary+=('Default shell: failed or cancelled; applied configs and backups are kept'); result=1; fi
    else summary+=('Default shell: skipped — /usr/bin/fish must be installed and listed in /etc/shells'); result=1; fi
else summary+=('Default shell: unchanged (not selected)'); fi
summary+=('Use Show result in ArchEasy to open your configured app or Fish shell.')
finish "$result"

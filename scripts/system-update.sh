# Embedded in ArchEasy; invoked with bash. $1 is a detected AUR helper or empty.
set -u
helper=${1:-}
snapshot=${2:-none}
case "$snapshot" in none|timeshift|snapper) ;; *) printf 'Invalid snapshot provider.\n'; exit 2 ;; esac
case "$helper" in paru|yay|'') ;; *) printf 'Invalid AUR helper.\n'; exit 2 ;; esac

finish() {
    local result=$1
    if [[ -t 0 ]]; then
        printf '\nPress Enter to close this update session…'
        read -r _ || true
    fi
    exit "$result"
}

lock_dir=${XDG_RUNTIME_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/archeasy}
if ! mkdir -p -- "$lock_dir"; then
    printf 'Cannot create the update lock folder.\n'
    finish 1
fi
exec 9>"$lock_dir/archeasy-update.lock"
if ! flock -n 9; then
    printf 'An ArchEasy system update is already running. Check its terminal window.\n'
    finish 1
fi

printf 'ArchEasy — System update\n'
printf 'Updates official Arch packages, AUR packages, and user/system Flatpaks.\n'
printf 'Review the prompts. Pacman and AUR may ask for your password.\n'
printf 'Arch news: https://archlinux.org/news/\n\n'
summary=()
result=0
run_step() {
    local title=$1
    shift
    printf '\n──── %s ────\n' "$title"
    "$@" 9>&-
    local code=$?
    if (( code == 0 )); then
        summary+=("$title: completed")
    else
        summary+=("$title: failed or cancelled (exit $code)")
        result=1
    fi
    return "$code"
}

# A requested snapshot is a prerequisite: failure stops every update step.
case "$snapshot" in
    timeshift) if ! run_step 'Timeshift · before update' sudo timeshift --create --comments 'ArchEasy: before system update' --tags O; then printf '\nSnapshot failed; no updates started.\n'; finish 1; fi ;;
    snapper) if ! run_step 'Snapper · before update' sudo snapper -c root create --type single --description 'ArchEasy: before system update'; then printf '\nSnapshot failed; no updates started.\n'; finish 1; fi ;;
esac

if run_step 'Pacman · official packages' sudo pacman -Syu; then
    if [[ -n "$helper" ]]; then
        # Only AUR packages: the official repositories were already updated.
        run_step 'AUR · community packages' "$helper" -Sua || true
    else
        summary+=('AUR: skipped — install paru or yay to update community packages')
        result=1
    fi
else
    # Do not build AUR packages against an incompletely updated Arch system.
    summary+=('AUR: skipped — resolve the Pacman update error first')
fi

if command -v flatpak >/dev/null 2>&1; then
    run_step 'Flatpak · your account' flatpak update --user || true
    run_step 'Flatpak · system-wide' flatpak update --system || true
else
    summary+=('Flatpak: skipped — flatpak is not installed')
    result=1
fi

printf '\n──── Update summary ────\n'
printf '%s\n' "${summary[@]}"
if (( result == 0 )); then
    printf '\nAll update steps completed successfully.\n'
else
    printf '\nSome updates failed, were cancelled, or were skipped. Review the messages above.\n'
fi
# Release the lock before holding the terminal open for the summary.
flock -u 9
exec 9>&-
finish "$result"

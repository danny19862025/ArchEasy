# Command and package names arrive as separate arguments, never shell source.
printf 'ArchEasy · AppCleaner\n\nCommand: '
printf '%q ' "$@"
printf '\n\n'
"$@"
code=$?
if (( code == 0 )); then
    printf '\nUninstall command completed. Reopen AppCleaner to refresh installed applications.\n'
else
    printf '\nUninstall command stopped (exit %s). Review the messages above.\n' "$code"
fi
if [[ -t 0 ]]; then read -r -p 'Press Enter to close…' _ || true; fi
exit "$code"

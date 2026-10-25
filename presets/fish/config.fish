# Interactive terminal setup for ArchEasy.
if status is-interactive
    if set -q ALACRITTY_WINDOW_ID; and type -q hyfetch
        hyfetch
    else if type -q fastfetch
        fastfetch
    end
    if type -q zoxide
        zoxide init fish | source
    end
end

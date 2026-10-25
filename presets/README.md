# Personal defaults

Main source: `~/Projects/python/notes_configs/nix_os_config`, as requested. Snapshots are embedded in Rust, so the original folders do not need to remain on disk.

- Fastfetch default: `fastfetch2` (ASCII logo, hardware/system/terminal sections). Image option: `fastfetch3`, with the machine-specific path rewritten at apply time. `arch.png` comes from the live config because the notes reference the image but do not include it. Image mode needs a Kitty-compatible terminal.
- Fish: interactive behavior from the third variant in `fish`, with availability guards and the machine-specific LM Studio path removed. Prompt/functions/colors come from the live `~/.config/fish`. No histories, universal variables, or unrelated configs are copied.
- Starship: the complete username/hostname example in `starship`, with shell transcript lines removed. `starship1` is incomplete and is not used.
- Ghostty: `ghostty`, without its shell transcript header.
- Kitty: `kitty`, without its transcript header, external `theme.conf` include, or remote-control socket settings. `current-theme.conf` is supplied from the dotfiles folder because the notes do not contain it.

NixOS configuration supplies context. ArchEasy installs application config files on Arch; it does not evaluate Nix or change the system/login shell. A Nerd Font improves icon rendering. Backups are stored per application before changes, and Restore reverts the latest application backup.

- Hyprland: `Projects/dotfiles/dotfiles/hypr/hyprland.conf`, adapted to modern Lua using the upstream 0.56.2 example. Preserves personal appearance and core shortcuts, removes duplicate bindings/external cache/widget paths, and guards optional autostart tools. Only `hypr/hyprland.lua` is written; the legacy `.conf` stays unchanged. Setup instructions are part of the app, not copied personal notes.

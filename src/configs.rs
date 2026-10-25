use gtk::{glib, prelude::*};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Preset {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    description: &'static str,
    instructions: &'static str,
    source: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}
const PRESETS: &[Preset] = &[
    Preset {
        id: "fastfetch",
        name: "Fastfetch · Arch dashboard",
        command: "fastfetch",
        description: "Your saved hardware, system, and terminal layout with a portable ASCII logo.",
        instructions: "Run fastfetch to see the result. Edit fastfetch/config.jsonc to change modules, labels, and colors. A Nerd Font displays the icons correctly.",
        source: "notes_configs/nix_os_config/fastfetch2",
        files: &[(
            "fastfetch/config.jsonc",
            include_bytes!("../presets/fastfetch/config.jsonc"),
        )],
    },
    Preset {
        id: "fastfetch",
        name: "Fastfetch · Arch image",
        command: "fastfetch",
        description: "The image-logo variant from your notes, with your Arch PNG included.",
        instructions: "Run fastfetch in Kitty to see the image. Other terminals may show text only. The image path is set automatically for your config folder. This replaces the same config as the ASCII preset.",
        source: "notes_configs/nix_os_config/fastfetch3 + ~/.config/fastfetch/arch.png",
        files: &[
            (
                "fastfetch/config.jsonc",
                include_bytes!("../presets/fastfetch/image.jsonc"),
            ),
            (
                "fastfetch/arch.png",
                include_bytes!("../presets/fastfetch/arch.png"),
            ),
        ],
    },
    Preset {
        id: "fish",
        name: "Fish · Danny’s shell",
        command: "fish",
        description: "Your custom Git-aware prompt, shell colors, startup fetch, and optional zoxide integration.",
        instructions: "Open a new Fish shell to see the prompt. Edit fish/config.fish for startup commands, fish/functions/fish_prompt.fish for the prompt, and fish/conf.d/fish_frozen_theme.fish for colors. This does not change your login shell. Fastfetch, hyfetch, and zoxide are optional and checked before use.",
        source: "notes_configs/nix_os_config/fish (third variant) + current Fish functions/colors",
        files: &[
            (
                "fish/config.fish",
                include_bytes!("../presets/fish/config.fish"),
            ),
            (
                "fish/functions/fish_prompt.fish",
                include_bytes!("../presets/fish/functions/fish_prompt.fish"),
            ),
            (
                "fish/functions/fish_greeting.fish",
                include_bytes!("../presets/fish/functions/fish_greeting.fish"),
            ),
            (
                "fish/conf.d/fish_frozen_theme.fish",
                include_bytes!("../presets/fish/conf.d/fish_frozen_theme.fish"),
            ),
        ],
    },
    Preset {
        id: "starship",
        name: "Starship · User & host",
        command: "starship",
        description: "Your saved green username and blue hostname prompt settings.",
        instructions: "Edit starship.toml to customize the prompt. To enable in Fish, add `starship init fish | source` inside the interactive block in fish/config.fish. For Zsh, add `eval \"$(starship init zsh)\"` to ~/.zshrc. Apply & show previews Starship without changing your shell integration.",
        source: "notes_configs/nix_os_config/starship",
        files: &[("starship.toml", include_bytes!("../presets/starship.toml"))],
    },
    Preset {
        id: "ghostty",
        name: "Ghostty · TokyoNight Moon",
        command: "ghostty",
        description: "Your saved TokyoNight theme, 75% opacity, 14-point text, and 120 × 35 window.",
        instructions: "A new Ghostty window opens after applying. Edit ghostty/config to change theme, opacity, font size, and dimensions. Existing windows may need to reload their config.",
        source: "notes_configs/nix_os_config/ghostty",
        files: &[(
            "ghostty/config",
            include_bytes!("../presets/ghostty/config"),
        )],
    },
    Preset {
        id: "kitty",
        name: "Kitty · Personal terminal",
        command: "kitty",
        description: "Your saved 14-point font, transparent background, cursor trail, and bundled color theme.",
        instructions: "A new Kitty window opens after applying. Edit kitty/kitty.conf for terminal settings and kitty/current-theme.conf for colors. FantasqueSansM Nerd Font is recommended; Kitty falls back if it is unavailable.",
        source: "notes_configs/nix_os_config/kitty + dotfiles/.config/kitty/current-theme.conf",
        files: &[
            (
                "kitty/kitty.conf",
                include_bytes!("../presets/kitty/kitty.conf"),
            ),
            (
                "kitty/current-theme.conf",
                include_bytes!("../presets/kitty/current-theme.conf"),
            ),
        ],
    },
    Preset {
        id: "hyprland",
        name: "Hyprland · Danny’s desktop",
        command: "Hyprland",
        description: "Your rounded windows, blue/pink borders, blur, smooth animations, natural scrolling, and Super-key shortcuts, migrated to Lua.",
        instructions: "For Hyprland 0.55+ (Arch currently provides 0.56). Apply config saves hypr/hyprland.lua with a backup. If Hyprland is already running it may reload automatically. Otherwise, save your work, log out, and choose Hyprland from your login screen. This app does not log you out or start a nested desktop. Super+Q opens a terminal, Super+R the launcher, Super+E files, Super+C closes a window; Super+1–0 switches workspaces. Use Setup guide for dependencies and customization.",
        source: "Projects/dotfiles/dotfiles/hypr/hyprland.conf, migrated using the upstream Hyprland 0.56.2 example",
        files: &[(
            "hypr/hyprland.lua",
            include_bytes!("../presets/hypr/hyprland.lua"),
        )],
    },
];
fn stamp() -> String {
    format!(
        "{:020}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn safe_parent(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Invalid preset path".into());
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut parent = root.to_path_buf();
    for part in rel.parent().unwrap_or(Path::new("")).components() {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(m) if m.file_type().is_symlink() || !m.is_dir() => {
                return Err(format!(
                    "{} is a linked or non-directory config folder. Manage it with its original config manager.",
                    parent.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&parent).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(root.join(relative))
}
fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let temporary = path.with_file_name(format!(".archeasy-{}", stamp()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|e: std::io::Error| e.to_string())
}
fn apply(preset: &Preset, root: &Path, backups: &Path) -> Result<PathBuf, String> {
    // Validate every destination before saving a complete rollback snapshot.
    for (name, _) in preset.files {
        safe_parent(root, name)?;
    }
    let backup = backups.join(preset.id).join(stamp());
    fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (name, _) in preset.files {
        let target = root.join(name);
        let metadata = match fs::symlink_metadata(&target) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let mut entry =
            serde_json::json!({"path": name, "existed": metadata.is_some(), "mode": 384});
        if let Some(metadata) = metadata {
            if metadata.is_dir() {
                return Err(format!(
                    "{} is a directory, expected a config file",
                    target.display()
                ));
            }
            if metadata.file_type().is_symlink() {
                let link = fs::read_link(&target).map_err(|e| e.to_string())?;
                entry["symlink"] =
                    serde_json::json!(link.to_str().ok_or("Non-UTF8 symlink target")?);
            } else if metadata.is_file() {
                entry["mode"] = serde_json::json!(metadata.permissions().mode() & 0o777);
                let saved = backup.join("files").join(name);
                fs::create_dir_all(saved.parent().unwrap()).map_err(|e| e.to_string())?;
                // Backups are private even when original files were world-readable.
                write_atomic(
                    &saved,
                    &fs::read(&target).map_err(|e| e.to_string())?,
                    0o600,
                )?;
            } else {
                return Err(format!("{} is not a regular file", target.display()));
            }
        }
        entries.push(entry);
    }
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"root": root, "entries": entries});
    write_atomic(
        &backup.join("manifest.json"),
        manifest.to_string().as_bytes(),
        0o600,
    )?;
    for (name, bytes) in preset.files {
        let contents = if name.ends_with(".png") {
            bytes.to_vec()
        } else {
            String::from_utf8_lossy(bytes)
                .replace("__CONFIG_DIR__", &root.to_string_lossy())
                .into_bytes()
        };
        if let Err(error) = write_atomic(&root.join(name), &contents, 0o600) {
            let rollback = restore_snapshot(&backup, root);
            return Err(format!(
                "Apply failed: {error}. Rollback: {}. Backup: {}",
                if rollback.is_ok() {
                    "completed"
                } else {
                    "failed; keep this backup"
                },
                backup.display()
            ));
        }
    }
    Ok(backup)
}
fn restore_snapshot(backup: &Path, root: &Path) -> Result<(), String> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest["root"].as_str() != root.to_str() {
        return Err("Backup belongs to a different config folder".into());
    }
    let entries = manifest["entries"]
        .as_array()
        .ok_or("Invalid backup manifest")?;
    // Preflight the entire snapshot before modifying anything.
    for entry in entries {
        let name = entry["path"].as_str().ok_or("Invalid backup path")?;
        if !PRESETS
            .iter()
            .any(|p| p.files.iter().any(|(n, _)| *n == name))
        {
            return Err("Unknown backup path".into());
        }
        safe_parent(root, name)?;
        if entry["existed"] == true
            && entry["symlink"].is_null()
            && !backup.join("files").join(name).is_file()
        {
            return Err(format!("Backup file missing: {name}"));
        }
    }
    for entry in entries {
        let name = entry["path"].as_str().unwrap();
        let target = root.join(name);
        if entry["existed"] != true {
            match fs::remove_file(&target) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        } else if let Some(link) = entry["symlink"].as_str() {
            let temporary = target.with_file_name(format!(".archeasy-{}", stamp()));
            std::os::unix::fs::symlink(link, &temporary).map_err(|e| e.to_string())?;
            if let Err(error) = fs::rename(&temporary, &target) {
                let _ = fs::remove_file(temporary);
                return Err(error.to_string());
            }
        } else {
            write_atomic(
                &target,
                &fs::read(backup.join("files").join(name)).map_err(|e| e.to_string())?,
                entry["mode"].as_u64().unwrap_or(0o600) as u32,
            )?;
        }
    }
    write_atomic(&backup.join("restored"), b"restored\n", 0o600)
}
fn latest_backup(id: &str, backups: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<_> = fs::read_dir(backups.join(id))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("manifest.json").is_file() && !p.join("restored").exists())
        .collect();
    candidates.sort();
    candidates.pop()
}
fn backups_dir() -> PathBuf {
    glib::user_data_dir().join("archeasy/backups")
}
fn text_window(parent: &gtk::ApplicationWindow, title: &str, text: &str) {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(16)
        .bottom_margin(16)
        .left_margin(16)
        .right_margin(16)
        .build();
    view.buffer().set_text(text);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .vexpand(true)
        .build();
    let window = gtk::Window::builder()
        .transient_for(parent)
        .title(title)
        .default_width(780)
        .default_height(600)
        .child(&scroll)
        .build();
    window.present();
}
fn notice(parent: &gtk::ApplicationWindow, title: &str, text: &str) {
    gtk::AlertDialog::builder()
        .message(title)
        .detail(text)
        .buttons(["Close"])
        .build()
        .show(Some(parent));
}
fn preview(preset: &Preset) -> Result<(), String> {
    if preset.files.iter().any(|(name, _)| name.ends_with(".png")) && super::available("kitty") {
        std::process::Command::new("kitty")
            .args(["--hold", "fastfetch"])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    match preset.command {
        "Hyprland" => {
            Err("Use the Hyprland setup guide to start a session from your login screen.".into())
        }
        "ghostty" | "kitty" => {
            std::process::Command::new(preset.command)
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "fish" => super::terminal(vec!["fish".into(), "-i".into()]),
        "starship" => super::terminal(vec!["starship".into(), "prompt".into()]),
        _ => super::terminal(vec!["fastfetch".into()]),
    }
}
fn show_result(preset: &Preset, parent: &gtk::ApplicationWindow) -> Result<(), String> {
    if preset.id == "hyprland" {
        text_window(
            parent,
            "Hyprland setup guide",
            "Hyprland · Danny’s desktop\n\n1. Click Install Hyprland & tools if needed. Confirm the package transaction in the terminal.\n2. Click Apply config. Your previous hypr/hyprland.lua is backed up. An existing legacy hyprland.conf is left in place; modern Hyprland uses the Lua file.\n3. Save your work, log out, and select Hyprland on your login screen. ArchEasy does not switch desktops automatically. On an active Hyprland session, config changes may reload immediately.\n\nShortcuts\nSuper+Q: Ghostty, with Kitty fallback\nSuper+C: close window\nSuper+E: Thunar, with Dolphin fallback\nSuper+R or Super+Space: Wofi launcher\nSuper+V: toggle floating window\nSuper+Shift+V: clipboard history\nSuper+arrows: focus; Super+Shift+arrows: move window\nSuper+1–0: workspace; add Shift to move a window there\nSuper+M: leave the Hyprland session\nPrint: capture a selected region to the clipboard\n\nCustomize ~/.config/hypr/hyprland.lua. Terminal, file manager and launcher commands are at the top; gaps, colors, blur, animations and touchpad settings follow. Change monitor scale if needed. Optional tools start only when installed. No wallpaper, AGS widgets, or pywal cache paths are required. Existing Waybar configuration is used if present.\n\nInstall tools includes Hyprland, Kitty, Wofi, Thunar, Waybar, SwayNC, the Hyprland portal and polkit agent, clipboard/screenshot tools, WirePlumber and brightnessctl. Installation may update your Arch system.\n\nCheck an installed version with Hyprland --version. See https://wiki.hypr.land/configuring/ for current configuration documentation. Use Restore backup to revert the saved Lua config. A full Hyprland session is needed to test compositor behavior.",
        );
        Ok(())
    } else {
        preview(preset)
    }
}
pub fn page(parent: &gtk::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let intro = super::label(
        "Your personal defaults, ready to use. Apply saves a backup first, then opens the result. Missing apps can be installed separately.",
        "subtitle",
    );
    intro.set_wrap(true);
    page.append(&intro);
    let status = super::label(
        &format!("Config folder: {}", glib::user_config_dir().display()),
        "subtitle",
    );
    status.set_wrap(true);
    status.set_selectable(true);
    page.append(&status);
    let folders = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    if let Ok(uri) = glib::filename_to_uri(glib::user_config_dir(), None) {
        folders.append(&gtk::LinkButton::with_label(&uri, "Open config folder ↗"));
    }
    page.append(&folders);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for preset in PRESETS {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("card");
        card.append(&super::label(preset.name, "name"));
        let description = super::label(preset.description, "subtitle");
        description.set_wrap(true);
        card.append(&description);
        let instructions = super::label(preset.instructions, "");
        instructions.set_wrap(true);
        instructions.set_selectable(true);
        card.append(&instructions);
        let source = super::label(&format!("Source: {}", preset.source), "subtitle");
        source.set_wrap(true);
        card.append(&source);
        let destinations = super::label(
            &format!(
                "Writes: {}",
                preset
                    .files
                    .iter()
                    .map(|(n, _)| format!("~/.config/{n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "subtitle",
        );
        destinations.set_wrap(true);
        card.append(&destinations);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let view = gtk::Button::with_label("View config");
        let weak = parent.downgrade();
        view.connect_clicked(move |_| {
            if let Some(parent) = weak.upgrade() {
                let mut text = format!(
                    "{}\nSource: {}\n\n{}\n",
                    preset.name, preset.source, preset.instructions
                );
                for (name, bytes) in preset.files {
                    text.push_str(&format!("\n──── {name} ────\n"));
                    if name.ends_with(".png") {
                        text.push_str("Bundled Arch logo image\n");
                    } else {
                        text.push_str(
                            &String::from_utf8_lossy(bytes).replace(
                                "__CONFIG_DIR__",
                                &glib::user_config_dir().to_string_lossy(),
                            ),
                        );
                    }
                }
                text_window(&parent, preset.name, &text);
            }
        });
        actions.append(&view);
        let apply_button = gtk::Button::with_label(if preset.id == "hyprland" {
            "Apply config"
        } else {
            "Apply & show"
        });
        apply_button.add_css_class("suggested-action");
        let s = status.clone();
        let weak = parent.downgrade();
        apply_button.connect_clicked(move |_| {
            let Some(parent) = weak.upgrade() else { return; };
            match apply(preset, &glib::user_config_dir(), &backups_dir()) {
                Ok(backup) => {
                    s.set_text(&format!("Applied {}. Backup: {}", preset.name, backup.display()));
                    if preset.id == "hyprland" { let _ = show_result(preset, &parent); }
                    else if !super::available(preset.command) { notice(&parent, "Config applied — app is not installed", &format!("Your config is ready. Click Install app on this card to install {}, then use Show result.", preset.command)); }
                    else if let Err(error) = show_result(preset, &parent) { notice(&parent, "Config applied — preview could not open", &error); }
                },
                Err(error) => { s.set_text(&error); notice(&parent, "Could not apply config", &error); }
            }
        });
        actions.append(&apply_button);
        let show = gtk::Button::with_label(if preset.id == "hyprland" {
            "Setup guide"
        } else {
            "Show result"
        });
        let weak = parent.downgrade();
        show.connect_clicked(move |_| {
            if let Some(parent) = weak.upgrade()
                && let Err(error) = show_result(preset, &parent)
            {
                notice(&parent, "Could not show result", &error);
            }
        });
        actions.append(&show);
        let restore = gtk::Button::with_label("Restore backup");
        let s = status.clone();
        let weak = parent.downgrade();
        restore.connect_clicked(move |_| {
            let Some(parent) = weak.upgrade() else {
                return;
            };
            let Some(backup) = latest_backup(preset.id, &backups_dir()) else {
                notice(
                    &parent,
                    "No backup yet",
                    "Apply a preset first. Backups are created before changing your config files.",
                );
                return;
            };
            match restore_snapshot(&backup, &glib::user_config_dir()) {
                Ok(()) => s.set_text(&format!(
                    "Restored previous {} config. Open a new app/shell to see it. Backup: {}",
                    preset.command,
                    backup.display()
                )),
                Err(e) => notice(&parent, "Could not restore backup", &e),
            }
        });
        actions.append(&restore);
        if !super::available(preset.command) || preset.id == "hyprland" {
            let install = gtk::Button::with_label(if preset.id == "hyprland" {
                "Install Hyprland & tools"
            } else {
                "Install app"
            });
            let weak = parent.downgrade();
            install.connect_clicked(move |_| {
                if preset.id == "hyprland" {
                    let args = [
                        "sudo",
                        "pacman",
                        "-Syu",
                        "--needed",
                        "hyprland",
                        "kitty",
                        "wofi",
                        "thunar",
                        "waybar",
                        "swaync",
                        "xdg-desktop-portal-hyprland",
                        "hyprpolkitagent",
                        "wl-clipboard",
                        "cliphist",
                        "grim",
                        "slurp",
                        "wireplumber",
                        "brightnessctl",
                    ]
                    .map(String::from)
                    .to_vec();
                    if let Err(error) = super::terminal(args)
                        && let Some(parent) = weak.upgrade()
                    {
                        notice(&parent, "Could not start installation", &error);
                    }
                    return;
                }
                let p = super::Package {
                    name: preset.command.into(),
                    version: String::new(),
                    description: String::new(),
                    source: "Pacman".into(),
                    remote: String::new(),
                    installed: false,
                };
                if let Err(e) = super::install(&p)
                    && let Some(parent) = weak.upgrade()
                {
                    notice(&parent, "Could not start installation", &e);
                }
            });
            actions.append(&install);
        }
        card.append(&actions);
        list.append(&card);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    page.append(&scroll);
    page
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("archeasy-config-test-{}", stamp()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn hyprland_apply_preserves_legacy_config_and_restores_previous_lua() {
        let preset = PRESETS.iter().find(|p| p.id == "hyprland").unwrap();
        let temp = Temp::new();
        let root = temp.0.join("config");
        fs::create_dir_all(root.join("hypr")).unwrap();
        fs::write(root.join("hypr/hyprland.conf"), "legacy config").unwrap();
        fs::write(root.join("hypr/hyprland.lua"), "previous lua").unwrap();
        let backup = apply(preset, &root, &temp.0.join("backups")).unwrap();
        let lua = fs::read_to_string(root.join("hypr/hyprland.lua")).unwrap();
        assert!(lua.contains("hl.config"));
        assert!(!lua.contains("/home/danny"));
        assert_eq!(
            fs::read_to_string(root.join("hypr/hyprland.conf")).unwrap(),
            "legacy config"
        );
        restore_snapshot(&backup, &root).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("hypr/hyprland.lua")).unwrap(),
            "previous lua"
        );
    }
    #[test]
    fn hyprland_preview_never_launches_a_nested_compositor() {
        let preset = PRESETS.iter().find(|p| p.id == "hyprland").unwrap();
        assert!(preview(preset).unwrap_err().contains("login screen"));
    }
    #[test]
    fn applying_and_restoring_preserves_existing_content_and_permissions() {
        let temp = Temp::new();
        let root = temp.0.join("config");
        let backups = temp.0.join("backups");
        fs::create_dir_all(root.join("fastfetch")).unwrap();
        let target = root.join("fastfetch/config.jsonc");
        fs::write(&target, "original config").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let backup = apply(&PRESETS[0], &root, &backups).unwrap();
        assert_ne!(fs::read_to_string(&target).unwrap(), "original config");
        assert_eq!(latest_backup("fastfetch", &backups).unwrap(), backup);
        restore_snapshot(&backup, &root).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "original config");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(latest_backup("fastfetch", &backups).is_none());
    }
    #[test]
    fn restore_removes_new_files_and_preserves_unrelated_files() {
        let temp = Temp::new();
        let root = temp.0.join("config");
        fs::create_dir_all(root.join("fish")).unwrap();
        fs::write(root.join("fish/unrelated"), "keep").unwrap();
        let backup = apply(&PRESETS[2], &root, &temp.0.join("backups")).unwrap();
        assert!(root.join("fish/functions/fish_prompt.fish").exists());
        restore_snapshot(&backup, &root).unwrap();
        assert!(!root.join("fish/config.fish").exists());
        assert!(!root.join("fish/functions/fish_prompt.fish").exists());
        assert_eq!(
            fs::read_to_string(root.join("fish/unrelated")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn linked_config_file_is_replaced_without_writing_through_and_link_is_restored() {
        let temp = Temp::new();
        let root = temp.0.join("config");
        fs::create_dir(&root).unwrap();
        let original = temp.0.join("managed-config");
        fs::write(&original, "managed").unwrap();
        let target = root.join("starship.toml");
        std::os::unix::fs::symlink(&original, &target).unwrap();
        let backup = apply(&PRESETS[3], &root, &temp.0.join("backups")).unwrap();
        assert!(
            !fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&original).unwrap(), "managed");
        restore_snapshot(&backup, &root).unwrap();
        assert_eq!(fs::read_link(&target).unwrap(), original);
    }
    #[test]
    fn refuses_linked_parent_folders_and_path_traversal() {
        let temp = Temp::new();
        let root = temp.0.join("config");
        let outside = temp.0.join("managed");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("fish")).unwrap();
        assert!(apply(&PRESETS[2], &root, &temp.0.join("backups")).is_err());
        assert!(!outside.join("config.fish").exists());
        assert!(safe_parent(&root, "../outside").is_err());
        assert!(safe_parent(&root, "/etc/fish").is_err());
    }
    #[test]
    fn image_paths_are_portable_and_variants_share_backups() {
        let temp = Temp::new();
        let root = temp.0.join("config");
        let backups = temp.0.join("backups");
        let first = apply(&PRESETS[0], &root, &backups).unwrap();
        let ascii = fs::read(root.join("fastfetch/config.jsonc")).unwrap();
        let second = apply(&PRESETS[1], &root, &backups).unwrap();
        let config = fs::read_to_string(root.join("fastfetch/config.jsonc")).unwrap();
        assert!(config.contains(&root.to_string_lossy().to_string()));
        assert!(!config.contains("/home/danny"));
        assert_eq!(latest_backup("fastfetch", &backups).unwrap(), second);
        restore_snapshot(&second, &root).unwrap();
        assert_eq!(
            fs::read(root.join("fastfetch/config.jsonc")).unwrap(),
            ascii
        );
        assert!(!root.join("fastfetch/arch.png").exists());
        assert_eq!(latest_backup("fastfetch", &backups).unwrap(), first);
    }
}

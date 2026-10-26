mod about;
mod appcleaner;
mod appearance;
mod cloud;
mod configs;
mod custom_theme;
mod flatpak_permissions;
mod maintenance;
mod search_history;
mod snapshots;
mod store;
mod system_export;
mod system_tools;
mod task_search;
mod updates;
mod ux;

use adw::prelude::*;
use gtk::glib;
use std::{
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
struct Package {
    name: String,
    version: String,
    description: String,
    source: String,
    remote: String,
    installed: bool,
}
fn package_page(package: &Package) -> Option<(String, &'static str)> {
    let name = glib::uri_escape_string(&package.name, None::<&str>, false);
    match package.source.as_str() {
        "Pacman" => Some((
            format!("https://archlinux.org/packages/?name={name}"),
            "Arch package ↗",
        )),
        "AUR" => Some((
            format!("https://aur.archlinux.org/packages/{name}"),
            "AUR page ↗",
        )),
        "Flatpak" if package.remote == "flathub" => {
            Some((format!("https://flathub.org/apps/{name}"), "Flathub page ↗"))
        }
        _ => None,
    }
}
fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
fn available(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"@._+-/".contains(&c))
}
fn search(query: &str, source: &str) -> Result<Vec<Package>, String> {
    let terms = task_search::terms(query);
    let mut results = vec![];
    let mut errors = vec![];
    for term in terms {
        match search_exact(&term, source) {
            Ok(packages) => {
                for p in packages {
                    if !results.iter().any(|r: &Package| {
                        r.source == p.source && r.name == p.name && r.remote == p.remote
                    }) {
                        results.push(p);
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if results.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        Ok(results)
    }
}
fn search_exact(query: &str, source: &str) -> Result<Vec<Package>, String> {
    match source {
        "Pacman" => {
            // Search literal terms, rather than exposing pacman's regex syntax.
            let query = query
                .chars()
                .map(|c| {
                    if ".^$*+?()[]{}|\\".contains(c) {
                        format!("\\{c}")
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>();
            let result = output("pacman", &["-Ss", "--color", "never", "--", &query]);
            let data = match result {
                Err(e) if e.is_empty() => String::new(),
                other => other?,
            };
            Ok(parse_pacman(&data))
        }
        "AUR" => {
            let data = output(
                "curl",
                &[
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--max-time",
                    "25",
                    "--get",
                    "--data-urlencode",
                    &format!("arg={query}"),
                    "--data",
                    "v=5&type=search&by=name-desc",
                    "https://aur.archlinux.org/rpc/",
                ],
            )?;
            let json: serde_json::Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
            if let Some(error) = json["error"].as_str() {
                return Err(error.into());
            }
            let installed = output("pacman", &["-Qq"]).unwrap_or_default();
            Ok(json["results"]
                .as_array()
                .ok_or("Unexpected AUR response")?
                .iter()
                .map(|p| Package {
                    name: p["Name"].as_str().unwrap_or_default().into(),
                    version: p["Version"].as_str().unwrap_or_default().into(),
                    description: p["Description"].as_str().unwrap_or("No description").into(),
                    source: "AUR".into(),
                    remote: String::new(),
                    installed: installed.lines().any(|n| Some(n) == p["Name"].as_str()),
                })
                .collect())
        }
        _ => {
            let data = output(
                "flatpak",
                &[
                    "search",
                    "--columns=application,name,description,version,remotes",
                    query,
                ],
            )?;
            let installed =
                output("flatpak", &["list", "--app", "--columns=application"]).unwrap_or_default();
            Ok(data
                .lines()
                .filter_map(|line| {
                    let c: Vec<_> = line.split('\t').collect();
                    if c.len() < 5 || !valid_id(c[0]) {
                        return None;
                    }
                    Some(Package {
                        name: c[0].into(),
                        version: c[3].into(),
                        description: format!("{} — {}", c[1], c[2]),
                        source: "Flatpak".into(),
                        remote: c[4].split(',').next().unwrap_or("flathub").trim().into(),
                        installed: installed.lines().any(|n| n == c[0]),
                    })
                })
                .collect())
        }
    }
}
fn parse_pacman(data: &str) -> Vec<Package> {
    let mut packages: Vec<Package> = vec![];
    for line in data.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(p) = packages.last_mut() {
                p.description.push_str(line.trim());
            }
        } else {
            let mut words = line.split_whitespace();
            if let Some((repo, name)) = words.next().and_then(|s| s.split_once('/')) {
                packages.push(Package {
                    name: name.into(),
                    version: words.next().unwrap_or_default().into(),
                    description: String::new(),
                    source: "Pacman".into(),
                    remote: repo.into(),
                    installed: line.contains("[installed"),
                });
            }
        }
    }
    packages
}
fn show_error(message: &str) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message("Could not start installation")
        .detail(message)
        .buttons(["Close"])
        .build();
    let parent = gtk::gio::Application::default()
        .and_downcast::<gtk::Application>()
        .and_then(|app| app.active_window());
    dialog.show(parent.as_ref());
}
fn terminal_options(name: &str) -> &[&str] {
    match name {
        "konsole" => &["--separate", "--hold", "-e"],
        "kitty" => &["--hold"],
        "gnome-terminal" => &["--"],
        "xfce4-terminal" => &["--hold", "-x"],
        _ => &["-hold", "-e"],
    }
}
// A successful spawn does not mean a terminal window actually opened. Check
// early exits off the GTK thread and retry another installed terminal.
fn launch_terminal(args: &[String], terminals: &[&str]) -> Result<(), String> {
    use std::io::Read;
    let mut errors = Vec::new();
    for name in terminals {
        let mut child = match Command::new(name)
            .args(terminal_options(name))
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                errors.push(format!("{name}: {error}"));
                continue;
            }
        };
        let stderr = child.stderr.take();
        let reader = thread::spawn(move || {
            let mut text = String::new();
            if let Some(stderr) = stderr {
                let _ = stderr.take(65536).read_to_string(&mut text);
            }
            text
        });
        let mut failed = false;
        for _ in 0..15 {
            thread::sleep(Duration::from_millis(100));
            match child.try_wait() {
                Ok(Some(status)) if !status.success() => {
                    // Do not wait for a pipe held open by descendant processes.
                    let detail = if reader.is_finished() {
                        reader.join().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    errors.push(format!("{name} exited with {status}. {detail}"));
                    failed = true;
                    break;
                }
                Ok(Some(_)) => return Ok(()), // Some terminals hand off to a server.
                Ok(None) => {}
                Err(error) => return Err(format!("Cannot monitor {name}: {error}")),
            }
        }
        if !failed {
            thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
    }
    Err(format!(
        "No terminal could open.\n\n{}\n\nTry launching ArchEasy from your desktop terminal: ~/.local/bin/archeasy. Search again to retry installation.",
        errors.join("\n")
    ))
}
fn terminal(args: Vec<String>) -> Result<(), String> {
    let terminals: Vec<_> = [
        "konsole",
        "kitty",
        "gnome-terminal",
        "xfce4-terminal",
        "xterm",
    ]
    .into_iter()
    .filter(|p| available(p))
    .collect();
    if terminals.is_empty() {
        return Err("Install a supported terminal: Konsole, Kitty, GNOME Terminal, Xfce Terminal, or xterm.".into());
    }
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(launch_terminal(&args, &terminals));
    });
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok(Err(error)) => {
            show_error(&error);
            glib::ControlFlow::Break
        }
        Ok(Ok(())) | Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
    });
    Ok(())
}
const SYSTEM_UPDATE_SCRIPT: &str = include_str!("../scripts/system-update.sh");
fn system_update(snapshot: &str) -> Result<(), String> {
    if !available("pacman") || !available("sudo") || !available("flock") {
        return Err("System updates require pacman, sudo, and flock (util-linux).".into());
    }
    let helper = ["paru", "yay"]
        .into_iter()
        .find(|p| available(p))
        .unwrap_or("");
    terminal(vec![
        "/usr/bin/bash".into(),
        "-c".into(),
        SYSTEM_UPDATE_SCRIPT.into(),
        "archeasy-system-update".into(),
        helper.into(),
        snapshot.into(),
    ])
}
fn install(p: &Package) -> Result<(), String> {
    if !valid_id(&p.name) || (!p.remote.is_empty() && !valid_id(&p.remote)) {
        return Err("Invalid package identifier".into());
    }
    let args: Vec<String> = match p.source.as_str() {
        "Pacman" => vec!["sudo", "pacman", "-Syu", "--needed", "--", &p.name],
        "AUR" => { let helper = ["paru", "yay"].into_iter().find(|p| available(p)).ok_or("AUR needs paru or yay. Install a helper using the ArchWiki instructions, then reopen ArchEasy.")?; vec![helper, "-S", "--needed", "--", &p.name] },
        _ => vec!["flatpak", "install", "--user", &p.remote, &p.name],
    }.into_iter().map(String::from).collect();
    terminal(args)
}
fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l
}
fn main() -> glib::ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--config-apply-job") {
        return if let Some(path) = args.get(2) {
            configs::cli_apply(std::path::Path::new(path))
        } else {
            glib::ExitCode::FAILURE
        };
    }
    if let Err(error) = appearance::load_brand_font() {
        eprintln!("ArchEasy wordmark font: {error}");
    }
    adw::init().expect("Libadwaita initialization");
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.ArchEasy")
        .build();
    app.connect_activate(build_ui);
    app.run()
}
const THEME_CSS: &str = r#"
window.archeasy { background: @theme_bg_color; color: @theme_fg_color; }
.archeasy label { color: inherit; }
.archeasy headerbar { background: transparent; color: inherit; }
.archeasy .about-close { font-size: 22px; min-width: 24px; padding: 2px 10px; }
.archeasy row:selected label { color: inherit; }
.archeasy .title { font-size: 32px; font-weight: 800; }
.archeasy .brand-wordmark {
    font-family: "Ribeye", serif;
    font-size: 36px;
    font-weight: 400;
    letter-spacing: 0;
    opacity: 0.9;
}
.archeasy .subtitle { opacity: 0.8; }
.archeasy .card { background: alpha(@theme_fg_color, 0.06); border-radius: 14px; padding: 18px; }
.archeasy .name { font-weight: 700; font-size: 17px; }
.archeasy .heading { font-weight: 700; font-size: 18px; }
.archeasy .badge { font-weight: 700; }
.archeasy .card label.deps-met { color: #16845f; }
.archeasy .card label.deps-missing { color: #967000; }
.archeasy button {
    border-radius: 12px;
    padding: 9px 16px;
    border: 1px solid alpha(@theme_fg_color, 0.14);
    box-shadow: 0 2px 4px alpha(black, 0.08), inset 0 1px alpha(white, 0.06);
    font-weight: 600;
    transition: box-shadow 180ms ease, border-color 180ms ease, background-color 180ms ease;
}
.archeasy button:hover {
    border-color: alpha(#1793d1, 0.75);
    background-image: linear-gradient(to bottom, alpha(#1793d1, 0.18), alpha(#1793d1, 0.06));
    box-shadow: 0 0 16px 2px alpha(#1793d1, 0.30), inset 0 1px alpha(white, 0.16);
}
.archeasy button:active {
    border-color: #55cbb6;
    box-shadow: inset 0 2px 5px alpha(black, 0.24), 0 0 10px alpha(#55cbb6, 0.40);
}
.archeasy button:focus-visible { outline: 2px solid alpha(#1793d1, 0.85); outline-offset: 3px; }
.archeasy button:checked { border-color: alpha(#1793d1, 0.65); background-image: linear-gradient(to bottom, alpha(#1793d1, 0.22), alpha(#1793d1, 0.08)); }
.archeasy button:disabled { background-image: none; box-shadow: none; }
.archeasy .main-navigation > button { border-radius: 12px; }
.archeasy .main-navigation > button:not(:last-child) { margin-right: 8px; }
.archeasy dropdown { border-radius: 12px; }
.archeasy dropdown > button { padding: 9px 12px; }
.archeasy dropdown label { color: inherit; }
.archeasy .card { border: 1px solid alpha(@theme_fg_color, 0.08); }
.archeasy .suggested-action:hover { box-shadow: 0 0 20px 2px alpha(#55cbb6, 0.45); }
.archeasy entry { padding: 10px; border-radius: 10px; }
.archeasy .filter-chip:checked { background: #3584e4; color: white; }
.archeasy row:selected { background: alpha(#3584e4, 0.18); color: inherit; }
.dark button:hover { box-shadow: 0 0 12px alpha(#78a9df, 0.16); border-color: alpha(#78a9df, 0.45); }
.dark .suggested-action:hover { box-shadow: 0 0 12px alpha(#78a9df, 0.20); }
window.archeasy.dark { background: #101722; color: #e7eef8; }
.dark .subtitle { color: #aebdd0; opacity: 1; }
.dark .card { background: #1b2738; }
.dark .badge, .dark link { color: #6ddbc5; }
.dark button, .dark dropdown { background: #26364b; color: #e7eef8; }
.dark entry { background: #1b2738; color: #e7eef8; }
window.archeasy.light { background: #f3f6fb; color: #182536; }
.light .subtitle { color: #506176; opacity: 1; }
.light .card { background: #ffffff; }
.light .badge, .light link { color: #087564; }
.light button, .light dropdown { background: #e2e9f2; color: #182536; }
.light entry { background: #ffffff; color: #182536; }
.archeasy .suggested-action { background: #55cbb6; color: #09251f; }
.archeasy button:disabled { opacity: 0.55; }
.archeasy .opening-title { color: #f0f7ff; font-size: 42px; font-weight: 800; letter-spacing: 1px; }
.archeasy .opening-subtitle { color: #9cc8e3; font-size: 16px; letter-spacing: 2px; }
"#;
fn theme_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy").join("theme")
}
// Missing, unreadable or corrupt preferences always use the desktop appearance.
fn load_theme(path: &std::path::Path, palette_path: &std::path::Path) -> u32 {
    match std::fs::read_to_string(path).unwrap_or_default().trim() {
        "light" => 1,
        "dark" => 2,
        "custom" if custom_theme::Palette::try_load(palette_path).is_some() => 3,
        _ => 0,
    }
}
fn apply_theme(window: &gtk::ApplicationWindow, selected: u32, system_dark: bool) {
    window.remove_css_class("light");
    window.remove_css_class("dark");
    window.remove_css_class("custom");
    match selected {
        1 => window.add_css_class("light"),
        2 => window.add_css_class("dark"),
        3 => window.add_css_class("custom"),
        _ => {}
    }
    adw::StyleManager::default().set_color_scheme(match selected {
        1 => adw::ColorScheme::ForceLight,
        2 => adw::ColorScheme::ForceDark,
        3 if custom_theme::Palette::load(&custom_theme_path()).is_light() => {
            adw::ColorScheme::ForceLight
        }
        3 => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
    let _ = system_dark;
    if selected == 0 && adw::StyleManager::default().is_dark() {
        window.add_css_class("dark");
    }
}
fn custom_theme_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy/custom-theme.json")
}
fn build_ui(app: &gtk::Application) {
    let css = gtk::CssProvider::new();
    css.load_from_data(&format!("{THEME_CSS}\n{}", appcleaner::css()));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Desktop display required"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("ArchEasy — Software made simple")
        .icon_name("archeasy")
        .default_width(1000)
        .default_height(760)
        .build();
    win.add_css_class("archeasy");
    let system_dark = gtk::Settings::default()
        .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme());
    let selected = load_theme(&theme_path(), &custom_theme_path());
    let custom_css = gtk::CssProvider::new();
    custom_css.load_from_data(&custom_theme::Palette::load(&custom_theme_path()).arch_css());
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(&win),
        &custom_css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    apply_theme(&win, selected, system_dark);
    let weak = win.downgrade();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if let Some(window) = weak.upgrade()
            && !window.has_css_class("light")
            && !window.has_css_class("custom")
        {
            if adw::StyleManager::default().is_dark() {
                window.add_css_class("dark");
            } else {
                window.remove_css_class("dark");
            }
        }
    });
    let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
    root.set_margin_top(28);
    root.set_margin_bottom(22);
    root.set_margin_start(30);
    root.set_margin_end(30);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    top.set_halign(gtk::Align::End);
    let title = label("ArchEasy", "brand-wordmark");
    title.set_halign(gtk::Align::Start);
    title.set_width_chars(9);
    let update = gtk::Button::with_label("System update");
    update.add_css_class("suggested-action");
    update.set_valign(gtk::Align::Center);
    update.set_tooltip_text(Some(
        "Update Pacman, AUR, and user/system Flatpaks. Review and confirm prompts in the terminal.",
    ));

    let setup_arch = gtk::Button::with_label("Setup ArchEasy");
    setup_arch.set_widget_name("setup-archeasy");
    setup_arch.set_tooltip_text(Some(
        "Upgrade Arch, install paru if missing, install Flatpak and add Flathub for your account.",
    ));
    setup_arch.connect_clicked(move |_| {
        if let Err(e) = terminal(vec![
            "/usr/bin/bash".into(),
            "-c".into(),
            include_str!("../scripts/first-run.sh").into(),
            "archeasy-first-run".into(),
        ]) {
            show_error(&e);
        }
    });

    top.append(&label("Theme", "subtitle"));
    let theme = gtk::DropDown::from_strings(&["System", "Light", "Dark", "Custom"]);
    theme.set_widget_name("archeasy-theme");
    theme.set_size_request(120, -1);
    theme.set_selected(selected);
    theme.set_valign(gtk::Align::Center);
    theme.set_tooltip_text(Some(
        "Choose the app appearance. Your choice is saved automatically.",
    ));
    let weak_window = win.downgrade();
    theme.connect_selected_notify(move |theme| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let selected = theme.selected();
        apply_theme(&window, selected, system_dark);
        let name = match selected {
            1 => "light",
            2 => "dark",
            3 => "custom",
            _ => "system",
        };
        let path = theme_path();
        let result = std::fs::create_dir_all(path.parent().expect("Theme directory"))
            .and_then(|()| std::fs::write(&path, name));
        if let Err(error) = result {
            gtk::AlertDialog::builder()
                .message("Theme changed, but could not be saved")
                .detail(error.to_string())
                .buttons(["Close"])
                .build()
                .show(Some(&window));
        }
    });
    top.append(&theme);
    let customize = gtk::Button::with_label("Colours…");
    customize.set_widget_name("archeasy-customize-theme");
    customize.set_tooltip_text(Some("Customize theme colours"));
    customize.set_valign(gtk::Align::Center);
    let weak_window = win.downgrade();
    let weak_theme = theme.downgrade();
    customize.connect_clicked(move |_| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let weak_window = window.downgrade();
        let weak_theme = weak_theme.clone();
        let css = custom_css.clone();
        custom_theme::editor(
            &window,
            "Customize ArchEasy theme",
            custom_theme_path(),
            move |palette| {
                css.load_from_data(&palette.arch_css());
                if let Some(theme) = weak_theme.upgrade() {
                    theme.set_selected(3);
                }
                if let Some(window) = weak_window.upgrade() {
                    apply_theme(&window, 3, system_dark);
                }
            },
        );
    });
    top.append(&customize);
    top.append(&appearance::install_sound(&win));
    let about = gtk::Button::with_label("About");
    about.set_valign(gtk::Align::Center);
    about.set_widget_name("archeasy-about");
    let dialog = about::window(
        &win,
        "ArchEasy",
        "system-software-install",
        "A beginner-friendly software browser for Arch Linux.\n\nSearch Pacman, AUR, and Flatpak in one place. Open package pages and start installations with your usual package manager.\n\nPacman installations include system updates. Review community AUR build scripts before installing. Password and confirmation prompts appear in your terminal.\n\nBuilt with Rust, GTK4, and Libadwaita.",
    );
    about.connect_clicked(move |_| dialog.present());
    top.append(&about);
    root.append(&top);
    let maintenance = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    maintenance.append(&setup_arch);
    maintenance.append(&update);
    snapshots::button(&maintenance, &win);
    root.append(&maintenance);
    let update_status = label("", "subtitle");
    update_status.set_wrap(true);
    update_status.set_visible(false);
    root.append(&update_status);
    let weak = win.downgrade();
    update.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            system_tools::update_dialog(&window, &update_status);
        }
    });
    root.append(&label(
        "Find your next app. One search, three software sources.",
        "subtitle",
    ));
    let software = gtk::Box::new(gtk::Orientation::Vertical, 14);
    software.set_vexpand(true);
    let stack = gtk::Stack::builder().vexpand(true).build();
    stack.set_widget_name("archeasy-tabs");
    stack.add_titled(&software, Some("software"), "Software");
    stack.add_titled(&store::installed_page(), Some("installed"), "Installed");
    stack.add_titled(&configs::page(&win), Some("configs"), "Configs");
    let cleaner = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_titled(&cleaner, Some("appcleaner"), "AppCleaner");
    let weak_window = win.downgrade();
    stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("appcleaner")
            && cleaner.first_child().is_none()
            && let Some(window) = weak_window.upgrade()
        {
            cleaner.append(&appcleaner::page(&window));
        }
    });
    let cloud = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_titled(&cloud, Some("cloud"), "Cloud Backup");
    let weak_window = win.downgrade();
    stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("cloud")
            && cloud.first_child().is_none()
            && let Some(window) = weak_window.upgrade()
        {
            cloud.append(&cloud::page(&window));
        }
    });
    stack.add_titled(&system_tools::page(&win), Some("system"), "System");
    stack.add_titled(
        &flatpak_permissions::page(&win),
        Some("permissions"),
        "Permissions",
    );
    let navigation = gtk::StackSwitcher::builder()
        .stack(&stack)
        .halign(gtk::Align::Start)
        .build();
    navigation.remove_css_class("linked");
    navigation.add_css_class("main-navigation");
    root.append(&navigation);
    root.append(&stack);
    root.append(&title);
    ux::software(&software, &win, &stack);

    let background = appearance::background(&root);
    appearance::opening_animation(&background, &title);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&background));
    win.set_child(Some(&toasts));
    ux::register_toasts(&toasts);
    win.present();
    ux::onboarding(&win, &stack, &setup_arch, &update);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_theme_preferences_use_system() {
        let root = std::env::temp_dir().join(format!("archeasy-theme-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let theme = root.join("theme");
        let palette = root.join("custom-theme.json");
        assert_eq!(load_theme(&theme, &palette), 0);
        assert_eq!(load_theme(&root, &palette), 0); // unreadable as a file
        for bytes in [b"garbage".as_slice(), b"", b"\xff\xfe", b"custom"] {
            std::fs::write(&theme, bytes).unwrap();
            assert_eq!(load_theme(&theme, &palette), 0);
        }
        for (value, expected) in [("system", 0), ("light\n", 1), ("dark", 2)] {
            std::fs::write(&theme, value).unwrap();
            assert_eq!(load_theme(&theme, &palette), expected);
        }
        std::fs::write(&theme, "custom").unwrap();
        for value in [
            "invalid JSON",
            r##"["#bad", "#000000", "#000000", "#000000", "#000000", "#000000"]"##,
        ] {
            std::fs::write(&palette, value).unwrap();
            assert_eq!(load_theme(&theme, &palette), 0);
        }
        custom_theme::Palette::default().save(&palette).unwrap();
        assert_eq!(load_theme(&theme, &palette), 3);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn terminal_failure_is_reported() {
        let error = launch_terminal(&[], &["/usr/bin/false"]).unwrap_err();
        assert!(error.contains("exited with"));
    }
    #[test]
    fn terminal_failure_falls_back() {
        assert!(launch_terminal(&[], &["/usr/bin/false", "/usr/bin/true"]).is_ok());
    }
    #[test]
    fn parses_repository_results() {
        let p = parse_pacman(
            "extra/firefox 145.0-1 [installed]\n    Fast web browser\nextra/vlc 3.0-1\n    Media player\n",
        );
        assert_eq!(p.len(), 2);
        assert!(p[0].installed);
        assert_eq!(p[0].name, "firefox");
        assert_eq!(p[1].description, "Media player");
    }
    #[test]
    fn rejects_options_and_shell_payloads() {
        for id in ["--help", "x;reboot", "$(id)", "", "hello world"] {
            assert!(!valid_id(id));
        }
        for id in ["firefox", "org.mozilla.firefox", "lib32-mesa", "c++"] {
            assert!(valid_id(id));
        }
    }
}

#[cfg(test)]
mod update_tests;

#[cfg(test)]
mod ux_tests;

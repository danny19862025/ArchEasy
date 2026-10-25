mod appearance;
mod configs;

use gtk::{glib, prelude::*};
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
fn system_update() -> Result<(), String> {
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
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.ArchEasy")
        .build();
    app.connect_activate(build_ui);
    app.run()
}
const THEME_CSS: &str = r#"
window.archeasy { background: @theme_bg_color; color: @theme_fg_color; }
.archeasy .title { font-size: 32px; font-weight: 800; }
.archeasy .subtitle { opacity: 0.8; }
.archeasy .card { background: alpha(@theme_fg_color, 0.06); border-radius: 14px; padding: 18px; }
.archeasy .name { font-weight: 700; font-size: 17px; }
.archeasy .badge { font-weight: 700; }
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
.archeasy .card { border: 1px solid alpha(@theme_fg_color, 0.08); }
.archeasy .suggested-action:hover { box-shadow: 0 0 20px 2px alpha(#55cbb6, 0.45); }
.archeasy entry { padding: 10px; border-radius: 10px; }
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
"#;
fn theme_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy").join("theme")
}
fn apply_theme(window: &gtk::ApplicationWindow, selected: u32, system_dark: bool) {
    window.remove_css_class("light");
    window.remove_css_class("dark");
    match selected {
        1 => window.add_css_class("light"),
        2 => window.add_css_class("dark"),
        _ => {}
    }
    if let Some(settings) = gtk::Settings::default() {
        // GTK 4.20+ follows the desktop color scheme natively. Keep the
        // older setting as a fallback for systems running GTK 4.10–4.18.
        if let Some(property) = settings.find_property("gtk-interface-color-scheme") {
            let nick = match selected {
                1 => "light",
                2 => "dark",
                _ => "default",
            };
            if let Some(value) = glib::EnumClass::with_type(property.value_type())
                .and_then(|class| class.to_value_by_nick(nick))
            {
                settings.set_property_from_value("gtk-interface-color-scheme", &value);
            }
        } else {
            settings.set_gtk_application_prefer_dark_theme(match selected {
                1 => false,
                2 => true,
                _ => system_dark,
            });
        }
    }
}
fn build_ui(app: &gtk::Application) {
    let css = gtk::CssProvider::new();
    css.load_from_data(THEME_CSS);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Desktop display required"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("ArchEasy — Software made simple")
        .default_width(1000)
        .default_height(760)
        .build();
    win.add_css_class("archeasy");
    let system_dark = gtk::Settings::default()
        .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme());
    let saved_theme = std::fs::read_to_string(theme_path()).unwrap_or_default();
    let selected = match saved_theme.trim() {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    };
    apply_theme(&win, selected, system_dark);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
    root.set_margin_top(28);
    root.set_margin_bottom(22);
    root.set_margin_start(30);
    root.set_margin_end(30);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = label("ArchEasy", "title");
    title.set_hexpand(true);
    top.append(&title);
    let update = gtk::Button::with_label("System update");
    update.add_css_class("suggested-action");
    update.set_valign(gtk::Align::Center);
    update.set_tooltip_text(Some(
        "Update Pacman, AUR, and user/system Flatpaks. Review and confirm prompts in the terminal.",
    ));
    top.append(&update);
    top.append(&label("Theme", "subtitle"));
    let theme = gtk::DropDown::from_strings(&["System", "Light", "Dark"]);
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
    top.append(&appearance::install_sound(&win));
    let about = gtk::Button::with_label("About");
    about.set_valign(gtk::Align::Center);
    let dialog = gtk::AboutDialog::builder()
        .transient_for(&win)
        .modal(true)
        .hide_on_close(true)
        .program_name("ArchEasy")
        .version(env!("CARGO_PKG_VERSION"))
        .logo_icon_name("system-software-install")
        .comments("A beginner-friendly software browser for Arch Linux.\n\nSearch Pacman, AUR, and Flatpak in one place. Open package pages and start installations with your usual package manager.\n\nPacman installations include system updates. AUR requires paru or yay; review community build scripts before installing. Password and confirmation prompts appear in your terminal.\n\nBuilt with Rust and GTK4.")
        .build();
    about.connect_clicked(move |_| dialog.present());
    top.append(&about);
    root.append(&top);
    let update_status = label("", "subtitle");
    update_status.set_wrap(true);
    update_status.set_visible(false);
    root.append(&update_status);
    update.connect_clicked(move |_| {
        match system_update() {
            Ok(()) => {
                update_status.set_text("Update terminal requested. Enter your password and review each step there. The terminal shows the final summary.");
                update_status.set_visible(true);
            },
            Err(error) => show_error(&error),
        }
    });
    root.append(&label(
        "Find your next app. One search, three software sources.",
        "subtitle",
    ));
    let software = gtk::Box::new(gtk::Orientation::Vertical, 14);
    software.set_vexpand(true);
    let stack = gtk::Stack::builder().vexpand(true).build();
    stack.add_titled(&software, Some("software"), "Software");
    stack.add_titled(&configs::page(&win), Some("configs"), "Configs");
    let navigation = gtk::StackSwitcher::builder()
        .stack(&stack)
        .halign(gtk::Align::Start)
        .build();
    root.append(&navigation);
    root.append(&stack);
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let entry = gtk::Entry::builder()
        .placeholder_text("Search apps and packages, e.g. firefox or vlc")
        .hexpand(true)
        .build();
    let filter = gtk::DropDown::from_strings(&["All sources", "Pacman", "AUR", "Flatpak"]);
    let button = gtk::Button::with_label("Search");
    button.add_css_class("suggested-action");
    bar.append(&entry);
    bar.append(&filter);
    bar.append(&button);
    software.append(&bar);
    let hint = label(
        "Pacman: official packages   •   AUR: community build scripts   •   Flatpak: sandboxed apps",
        "subtitle",
    );
    hint.set_wrap(true);
    software.append(&hint);
    let status = label(
        "Search to get started. Installation prompts open in your terminal.",
        "subtitle",
    );
    status.set_wrap(true);
    software.append(&status);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    software.append(&scroll);
    let footer = label(
        "Pacman installs include system updates. Review terminal prompts before continuing.\nAUR packages are community maintained: review their PKGBUILD before installing. Search again after installation to refresh status.",
        "subtitle",
    );
    footer.set_wrap(true);
    software.append(&footer);
    let setup = gtk::Button::with_label("Set up Flathub for my account");
    setup.set_halign(gtk::Align::Start);
    let s = status.clone();
    setup.connect_clicked(move |_| {
        let r = terminal(
            [
                "flatpak",
                "remote-add",
                "--user",
                "--if-not-exists",
                "flathub",
                "https://flathub.org/repo/flathub.flatpakrepo",
            ]
            .map(String::from)
            .to_vec(),
        );
        s.set_text(&r.err().unwrap_or(
            "Flathub setup opened in your terminal. Search again when it finishes.".into(),
        ));
    });
    software.append(&setup);
    let help = gtk::LinkButton::with_label(
        "https://wiki.archlinux.org/title/AUR_helpers",
        "AUR helper setup instructions",
    );
    help.set_halign(gtk::Align::Start);
    software.append(&help);
    let e = entry.clone();
    let f = filter.clone();
    let b = button.clone();
    let l = list.clone();
    let s = status.clone();
    button.connect_clicked(move |_| {
        let query = e.text().trim().to_string(); if query.len() < 2 { s.set_text("Type at least two characters to search."); return; }
        while let Some(child) = l.first_child() { l.remove(&child); } b.set_sensitive(false); s.set_text("Searching your software sources…");
        let selected = f.selected(); let (tx, rx) = mpsc::channel();
        thread::spawn(move || { let sources = ["Pacman", "AUR", "Flatpak"]; let mut jobs = vec![];
            for (i, source) in sources.into_iter().enumerate() { if selected == 0 || selected == i as u32 + 1 { let q = query.clone(); jobs.push((source, thread::spawn(move || search(&q, source)))); } }
            let mut packages = vec![]; let mut errors = vec![];
            for (source, job) in jobs { match job.join() { Ok(Ok(mut results)) => packages.append(&mut results), Ok(Err(e)) => errors.push(format!("{source}: {e}")), Err(_) => errors.push(format!("{source}: search failed")) } }
            let _ = tx.send((packages, errors));
        });
        let l = l.clone(); let s = s.clone(); let b = b.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let Ok((packages, errors)) = rx.try_recv() else { return glib::ControlFlow::Continue; };
            b.set_sensitive(true); let count = packages.len();
            s.set_text(&format!("{count} results{}. {}", if count > 150 { " (showing the first 150; narrow your search)" } else { "" }, errors.join(" • ")));
            for p in packages.into_iter().take(150) {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 16); row.add_css_class("card");
                let info = gtk::Box::new(gtk::Orientation::Vertical, 6); info.set_hexpand(true);
                let title = label(&p.name, "name"); title.set_selectable(true); info.append(&title);
                info.append(&label(&format!("{}   {}   {}", p.source, p.remote, p.version), "badge"));
                let desc = label(&p.description, "subtitle"); desc.set_wrap(true); desc.set_max_width_chars(70); info.append(&desc); row.append(&info);
                if let Some((url, title)) = package_page(&p) {
                    let page = gtk::LinkButton::with_label(&url, title);
                    page.set_valign(gtk::Align::Center);
                    page.set_tooltip_text(Some(&url));
                    row.append(&page);
                }
                let install_button = gtk::Button::with_label(if p.installed { "Installed ✓" } else { "Install" }); install_button.set_valign(gtk::Align::Center); install_button.set_sensitive(!p.installed); install_button.add_css_class("suggested-action");
                let s = s.clone(); install_button.connect_clicked(move |button| { match install(&p) { Ok(()) => { button.set_sensitive(false); button.set_label("Check terminal"); s.set_text(&format!("Installation requested for {}. Enter your password and confirm in the terminal. Search again when finished.", p.name)); }, Err(e) => { s.set_text(&e); show_error(&e); } } });
                row.append(&install_button); l.append(&row);
            }
            glib::ControlFlow::Break
        });
    });
    let b = button.clone();
    entry.connect_activate(move |_| {
        b.emit_clicked();
    });
    let background = appearance::background(&root);
    win.set_child(Some(&background));
    win.present();
}
#[cfg(test)]
mod tests {
    use super::*;
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

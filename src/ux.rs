use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
thread_local! { static TOASTS: RefCell<Option<glib::WeakRef<adw::ToastOverlay>>> = const { RefCell::new(None) }; }
pub fn register_toasts(overlay: &adw::ToastOverlay) {
    TOASTS.with(|slot| *slot.borrow_mut() = Some(overlay.downgrade()));
}
pub fn toast(title: &str) {
    TOASTS.with(|slot| {
        if let Some(overlay) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            overlay.add_toast(adw::Toast::new(title));
        }
    });
}
pub fn undo_toast(title: &str, undo: impl Fn() + 'static) {
    TOASTS.with(|slot| {
        if let Some(overlay) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            let toast = adw::Toast::builder()
                .title(title)
                .button_label("Undo")
                .timeout(8)
                .build();
            toast.connect_button_clicked(move |_| undo());
            overlay.add_toast(toast);
        }
    });
}
pub fn icon(name: &str) -> gtk::Image {
    if name.eq_ignore_ascii_case(github::FEATURED_REPO) {
        let bytes = glib::Bytes::from_static(include_bytes!("../assets/archeasy.svg"));
        let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
        if let Ok(pixbuf) =
            gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE)
            && let Some(pixbuf) = pixbuf.scale_simple(32, 32, gtk::gdk_pixbuf::InterpType::Bilinear)
        {
            let image = gtk::Image::from_pixbuf(Some(&pixbuf));
            image.set_widget_name("featured-archeasy-icon");
            return image;
        }
    }
    let candidates: &[&str] = match name.to_lowercase().as_str() {
        "fish" => &["fish", "utilities-terminal-symbolic"],
        "ghostty" => &["com.mitchellh.ghostty", "utilities-terminal-symbolic"],
        "starship" => &["starship", "starred-symbolic"],
        "hyprland" => &["hyprland", "preferences-desktop-display-symbolic"],
        "neovim" | "nvim" => &["nvim", "accessories-text-editor-symbolic"],
        "fastfetch" | "btop" => &[
            "utilities-system-monitor",
            "utilities-system-monitor-symbolic",
        ],
        "firefox" => &["firefox", "web-browser-symbolic"],
        "kitty" | "tmux" => &["kitty", "utilities-terminal-symbolic"],
        "waybar" => &["preferences-desktop", "preferences-system-symbolic"],
        "wofi" | "rofi" => &["system-run", "system-run-symbolic"],
        _ => &[name, "application-x-executable-symbolic"],
    };
    let theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
    let chosen = candidates
        .iter()
        .find(|n| theme.has_icon(n))
        .copied()
        .unwrap_or("application-x-executable-symbolic");
    let image = gtk::Image::from_icon_name(chosen);
    image.set_pixel_size(32);
    image
}
fn empty(entry: &gtk::Entry) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_valign(gtk::Align::Center);
    body.set_vexpand(true);
    body.set_margin_top(28);
    body.set_margin_bottom(28);
    let bytes = glib::Bytes::from_static(include_bytes!("../assets/archlinux-logo.svg"));
    let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
    if let Ok(pixbuf) =
        gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, None::<&gtk::gio::Cancellable>)
    {
        let pixbuf = pixbuf
            .scale_simple(110, 110, gtk::gdk_pixbuf::InterpType::Bilinear)
            .unwrap_or(pixbuf);
        let picture = gtk::Picture::for_pixbuf(&pixbuf);
        picture.set_size_request(110, 110);
        picture.set_can_shrink(true);
        picture.set_vexpand(false);
        picture.set_valign(gtk::Align::Center);
        picture.set_halign(gtk::Align::Center);
        body.append(&picture);
    }
    let title = gtk::Label::new(Some("Search for apps, configs, drivers…"));
    title.add_css_class("title-2");
    body.append(&title);
    body.append(&gtk::Label::new(Some("Discover your next Arch essential")));
    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    chips.set_halign(gtk::Align::Center);
    for name in ["Firefox", "Hyprland", "Ghostty"] {
        let chip = gtk::Button::with_label(name);
        chip.add_css_class("pill");
        let entry = entry.clone();
        chip.connect_clicked(move |_| entry.set_text(name));
        chips.append(&chip);
    }
    body.append(&chips);
    body
}
/// pacman returns download sizes in the repository metadata; never invent missing sizes.
fn sizes(packages: &[Package]) -> std::collections::HashMap<String, String> {
    let names: Vec<_> = packages
        .iter()
        .filter(|p| p.source == "Pacman")
        .take(150)
        .map(|p| p.name.as_str())
        .collect();
    let mut sizes = std::collections::HashMap::new();
    if names.is_empty() {
        return sizes;
    }
    let mut args = vec!["-Si", "--"];
    args.extend(names);
    if let Ok(data) = output("pacman", &args) {
        let mut name = String::new();
        for line in data.lines() {
            if let Some((key, value)) = line.split_once(':') {
                match key.trim() {
                    "Name" => name = value.trim().into(),
                    "Download Size" => {
                        sizes.insert(name.clone(), value.trim().into());
                    }
                    _ => {}
                }
            }
        }
    }
    sizes
}
/// Shared result renderer keeps source-specific actions consistent with keyboard activation.
pub(super) fn result_row(
    p: Package,
    size: &str,
    queue: &store::Queue,
    queue_run: &gtk::Button,
    status: &gtk::Label,
) -> (adw::ActionRow, gtk::Button) {
    let row = adw::ActionRow::builder()
        .title(&p.name)
        .use_markup(false)
        .activatable(true)
        .build();
    row.set_title_lines(1);
    row.set_subtitle_lines(3);
    row.set_tooltip_text(Some(&format!("{} • {} • {}", p.name, p.version, p.remote)));
    let origin = if p.source == "Pacman" {
        "Official"
    } else {
        &p.source
    };
    if github::is_featured(&p) {
        row.set_widget_name("featured-archeasy-repository");
        row.add_css_class("featured-repository");
        row.set_subtitle(&format!(
            "{}\nGitHub • Featured • Danny’s repository",
            p.description
        ));
    } else if p.source == "GitHub" {
        row.set_subtitle(&format!(
            "{}\nGitHub • {} • Root PKGBUILD",
            p.description, p.version
        ));
    } else {
        row.set_subtitle(&format!(
            "{}\n{origin} • {} • {}",
            p.description,
            size,
            if p.installed {
                "Installed"
            } else {
                "Not installed"
            }
        ));
    }
    row.add_prefix(&icon(&p.name));
    if p.source != "GitHub" {
        row.add_suffix(&store::checkbox(&p, queue, queue_run));
    }
    if let Some((url, title)) = package_page(&p) {
        let link = gtk::LinkButton::with_label(
            &url,
            if p.source == "GitHub" {
                "GitHub ↗"
            } else {
                "Details ↗"
            },
        );
        link.set_valign(gtk::Align::Center);
        link.set_tooltip_text(Some(title));
        row.add_suffix(&link);
    }
    if p.source == "Flatpak" {
        let permissions = gtk::Button::from_icon_name("security-high-symbolic");
        permissions.set_tooltip_text(Some("View Flatpak permissions"));
        permissions.set_valign(gtk::Align::Center);
        let id = p.name.clone();
        permissions.connect_clicked(move |_| flatpak_permissions::open(&id, "--user"));
        row.add_suffix(&permissions);
    }
    let button = gtk::Button::with_label(if github::is_featured(&p) {
        "Open repository"
    } else if p.installed {
        "Installed"
    } else if p.source == "GitHub" {
        "Clone & Build"
    } else {
        "Install"
    });
    button.set_valign(gtk::Align::Center);
    button.set_sensitive(!p.installed);
    button.add_css_class("suggested-action");
    let status = status.clone();
    button.connect_clicked(move |button| {
        if github::is_featured(&p) {
            if let Err(error) = gtk::gio::AppInfo::launch_default_for_uri(
                "https://github.com/danny19862025/ArchEasy",
                None::<&gtk::gio::AppLaunchContext>,
            ) {
                status.set_text(&format!("Could not open repository: {error}"));
            }
            return;
        }
        let button = button.clone();
        let p = p.clone();
        let status = status.clone();
        confirm_upgrade(p.source == "Pacman", move || match install(&p) {
            Ok(()) => {
                button.set_sensitive(false);
                button.set_label("Check terminal");
                toast(&format!("{} installation opened in terminal", p.name));
                status.set_text(
                "Review package and password prompts in your terminal. Search again when finished.",
            );
            }
            Err(e) => {
                status.set_text(&e);
                show_error(&e);
            }
        });
    });
    row.add_suffix(&button);
    (row, button)
}

fn source_content(name: &str) -> gtk::Box {
    let svg: Option<&'static [u8]> = match name {
        "GitHub" | "danny19862025" => Some(include_bytes!("../assets/github.svg")),
        "Pacman" => Some(include_bytes!("../assets/archlinux-logo.svg")),
        _ => None,
    };
    let image = svg
        .and_then(|svg| {
            let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(svg));
            gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE)
                .ok()
                .and_then(|p| p.scale_simple(16, 16, gtk::gdk_pixbuf::InterpType::Bilinear))
                .map(|p| gtk::Image::from_pixbuf(Some(&p)))
        })
        .unwrap_or_else(|| {
            gtk::Image::from_icon_name(match name {
                "All" => "view-grid-symbolic",
                "AUR" => "system-software-install-symbolic",
                "Flatpak" => "package-x-generic-symbolic",
                "Installed" => "emblem-ok-symbolic",
                _ => "application-x-executable-symbolic",
            })
        });
    image.set_pixel_size(16);
    image.set_widget_name(&format!("source-icon-{}", name.to_lowercase()));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.set_halign(gtk::Align::Center);
    content.append(&image);
    content.append(&gtk::Label::new(Some(name)));
    content
}

pub fn software(page: &gtk::Box, window: &gtk::ApplicationWindow, tabs: &gtk::Stack) {
    let entry = gtk::Entry::builder()
        .placeholder_text("Search apps or tasks…   / to focus")
        .hexpand(true)
        .build();
    entry.set_widget_name("software-search");
    entry.set_primary_icon_name(Some("system-search-symbolic"));
    page.append(&entry);
    let history = gtk::FlowBox::new();
    history.set_widget_name("search-history");
    history.set_selection_mode(gtk::SelectionMode::None);
    history.set_homogeneous(false);
    history.set_halign(gtk::Align::Start);
    history.set_column_spacing(6);
    history.set_row_spacing(6);
    history.set_max_children_per_line(5);
    history.set_min_children_per_line(1);
    search_history::refresh(&history, &entry);
    page.append(&history);
    let filters = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let selected = Rc::new(Cell::new(0usize));
    let generation = Rc::new(Cell::new(0u64));
    let run = gtk::Button::with_label("Search");
    run.set_visible(false);
    page.append(&run);
    let mut first: Option<gtk::ToggleButton> = None;
    for (i, name) in ["All", "Pacman", "AUR", "Flatpak", "GitHub", "Installed"]
        .iter()
        .enumerate()
    {
        let chip = gtk::ToggleButton::with_label(name);
        chip.set_child(Some(&source_content(name)));
        chip.set_widget_name(&if *name == "GitHub" {
            "github-source".into()
        } else {
            format!("source-{}", name.to_lowercase())
        });
        chip.set_tooltip_text(Some(&format!("Search {name}")));
        chip.update_property(&[gtk::accessible::Property::Label(name)]);
        chip.add_css_class("pill");
        chip.add_css_class("filter-chip");
        if let Some(first) = &first {
            chip.set_group(Some(first));
        } else {
            chip.set_active(true);
            first = Some(chip.clone());
        }
        let selected = selected.clone();
        let run = run.clone();
        chip.connect_toggled(move |chip| {
            if chip.is_active() {
                selected.set(i);
                run.emit_clicked();
            }
        });
        filters.append(&chip);
    }
    let profile = gtk::LinkButton::with_label(
        "https://github.com/danny19862025?tab=repositories",
        "danny19862025",
    );
    profile.set_child(Some(&source_content("danny19862025")));
    profile.update_property(&[gtk::accessible::Property::Label(
        "danny19862025 GitHub repositories",
    )]);
    profile.set_widget_name("danny-github-repositories");
    profile.set_tooltip_text(Some("Open Danny’s GitHub repositories"));
    filters.append(&profile);
    page.append(&filters);
    let status = label(
        "/ Search   •   Esc Clear   •   ↑ ↓ Navigate   •   Enter Install",
        "subtitle",
    );
    status.set_wrap(true);
    page.append(&status);
    let (queue, queue_run) = store::queue_bar(page, &status);
    let results = gtk::ListBox::new();
    results.set_widget_name("software-results");
    results.set_selection_mode(gtk::SelectionMode::Single);
    results.add_css_class("boxed-list");
    results.set_activate_on_single_click(false);
    let group = adw::PreferencesGroup::new();
    group.set_title("Search results");
    group.add(&results);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&group)
        .build();
    let states = gtk::Stack::builder().vexpand(true).build();
    states.add_named(&empty(&entry), Some("empty"));
    states.add_named(&scroll, Some("results"));
    states.set_visible_child_name("empty");
    page.append(&states);
    let installs: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(vec![]));
    let actions = installs.clone();
    results.connect_row_activated(move |_, row| {
        if let Some(button) = actions.borrow().get(row.index() as usize)
            && button.is_sensitive()
        {
            button.emit_clicked();
        }
    });
    let e = entry.clone();
    let l = results.clone();
    let s = status.clone();
    let g = generation.clone();
    let st = states.clone();
    let actions = installs.clone();
    let debounce_source = selected.clone();
    run.connect_clicked(move |_| {
        let id = g.get().wrapping_add(1);
        g.set(id);
        let query = e.text().trim().to_owned();
        while let Some(child) = l.first_child() {
            l.remove(&child);
        }
        actions.borrow_mut().clear();
        if query.chars().count() < 2 {
            st.set_visible_child_name("empty");
            s.set_text("Type at least two characters • / Search • Esc Clear");
            return;
        }
        st.set_visible_child_name("results");
        s.set_text("Searching your software sources…");
        let selected = selected.get();
        let (tx, rx) = mpsc::channel();
        let sources: Vec<_> = ["Pacman", "AUR", "Flatpak", "GitHub"]
            .into_iter()
            .enumerate()
            .filter(|(i, source)| {
                (selected == 0 || selected == 5 || selected == i + 1)
                    && !(selected == 5 && *source == "GitHub")
            })
            .map(|(_, source)| source)
            .collect();
        let mut remaining = sources.len();
        for source in sources {
            let tx = tx.clone();
            let query = query.clone();
            thread::spawn(move || {
                let (mut packages, errors) = match search(&query, source) {
                    Ok(p) => (p, vec![]),
                    Err(e) => (vec![], vec![format!("{source}: {e}")]),
                };
                if selected == 5 {
                    packages.retain(|p| p.installed);
                }
                if source == "GitHub" {
                    github::add_featured(&query, &mut packages);
                }
                packages.sort_by_key(|p| {
                    (
                        !github::is_featured(p),
                        !p.name.eq_ignore_ascii_case(&query),
                        !p.name.to_lowercase().starts_with(&query.to_lowercase()),
                        p.name.clone(),
                    )
                });
                let sizes = sizes(&packages);
                let _ = tx.send((packages, errors, sizes));
            });
        }
        drop(tx);
        let mut total = 0usize;
        let mut shown = 0usize;
        let mut failures = Vec::new();
        let l = l.clone();
        let s = s.clone();
        let g = g.clone();
        let queue = queue.clone();
        let queue_run = queue_run.clone();
        let actions = actions.clone();
        let history = history.clone();
        let entry = e.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            if id != g.get() {
                return glib::ControlFlow::Break;
            }
            let (packages, errors, sizes) = match rx.try_recv() {
                Ok(v) => v,
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(_) => {
                    s.set_text("Search stopped. Try again.");
                    return glib::ControlFlow::Break;
                }
            };
            remaining -= 1;
            total += packages.len();
            failures.extend(errors);
            s.set_text(&format!(
                "{}{}{}{}",
                if total == 0 && remaining == 0 {
                    "No matches. Try a different search.".into()
                } else {
                    format!("{total} results")
                },
                if total > 150 { " (showing 150)" } else { "" },
                if remaining > 0 {
                    " • Searching other sources…"
                } else {
                    ""
                },
                if failures.is_empty() {
                    String::new()
                } else {
                    format!(" • {}", failures.join(" • "))
                }
            ));
            for p in packages.into_iter().take(150usize.saturating_sub(shown)) {
                shown += 1;
                let size = sizes
                    .get(&p.name)
                    .map(String::as_str)
                    .unwrap_or("Size unavailable");
                let (row, button) = result_row(p, size, &queue, &queue_run, &s);
                actions.borrow_mut().push(button);
                l.append(&row);
            }
            if remaining == 0 {
                search_history::record(&query, &history, &entry);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let b = run.clone();
    let g = generation.clone();
    entry.connect_changed(move |_| {
        g.set(g.get().wrapping_add(1));
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let b = b.clone();
        let pending_done = pending.clone();
        *pending.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(if matches!(debounce_source.get(), 0 | 4) {
                900
            } else {
                220
            }),
            move || {
                pending_done.borrow_mut().take();
                b.emit_clicked();
            },
        ));
    });
    let l = results.clone();
    entry.connect_activate(move |_| {
        if let Some(row) = l.row_at_index(0) {
            l.select_row(Some(&row));
            row.grab_focus();
        }
    });
    let keys = gtk::EventControllerKey::new();
    let e = entry.clone();
    let tabs = tabs.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if tabs.visible_child_name().as_deref() != Some("software") {
            return glib::Propagation::Proceed;
        }
        if !modifiers.is_empty() {
            return glib::Propagation::Proceed;
        }
        let editing = e.has_focus() || e.focus_child().is_some();
        match key {
            gtk::gdk::Key::slash if !editing => {
                e.grab_focus();
                glib::Propagation::Stop
            }
            gtk::gdk::Key::Escape => {
                e.set_text("");
                e.grab_focus();
                glib::Propagation::Stop
            }
            gtk::gdk::Key::Down if editing => {
                if let Some(row) = results.row_at_index(0) {
                    results.select_row(Some(&row));
                    row.grab_focus();
                }
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });
    window.add_controller(keys);
}
pub fn onboarding(
    parent: &gtk::ApplicationWindow,
    tabs: &gtk::Stack,
    setup: &gtk::Button,
    update: &gtk::Button,
) {
    let path = glib::user_config_dir().join("archeasy/onboarding-complete");
    if path.exists() {
        return;
    }
    let dialog = gtk::Window::builder()
        .title("Welcome to ArchEasy")
        .transient_for(parent)
        .modal(true)
        .default_width(560)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
    system_tools::margins(&body);
    body.append(&label("Make Arch feel like home", "title-1"));
    body.append(&label("Follow these steps at your own pace.", "subtitle"));
    let group = adw::PreferencesGroup::new();
    for (title, subtitle, icon_name) in [
        (
            "1. Setup paru / yay",
            "Install an AUR helper and enable Flathub",
            "system-software-install-symbolic",
        ),
        (
            "2. Apply Danny’s base configs",
            "Review the files and backups before applying",
            "preferences-system-symbolic",
        ),
        (
            "3. Update your system",
            "Review available updates and choose a snapshot",
            "software-update-available-symbolic",
        ),
    ] {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build();
        row.add_prefix(&icon(icon_name));
        group.add(&row);
    }
    body.append(&group);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for (name, index) in [
        ("Setup helper", 0),
        ("Browse configs", 1),
        ("Review updates", 2),
        ("Skip", 3),
    ] {
        let button = gtk::Button::with_label(name);
        let weak = dialog.downgrade();
        let tabs = tabs.clone();
        let setup = setup.clone();
        let update = update.clone();
        button.connect_clicked(move |_| {
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
            match index {
                0 => setup.emit_clicked(),
                1 => tabs.set_visible_child_name("configs"),
                2 => update.emit_clicked(),
                _ => {}
            }
        });
        actions.append(&button);
    }
    dialog.connect_close_request(move |_| {
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, "done"))
        {
            toast(&format!("Could not save onboarding preference: {e}"));
        }
        glib::Propagation::Proceed
    });
    body.append(&actions);
    dialog.set_child(Some(&body));
    dialog.present();
}

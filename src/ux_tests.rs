use super::*;
fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut all = vec![widget.as_ref().clone()];
    let mut child = widget.as_ref().first_child();
    while let Some(next) = child {
        all.extend(descendants(&next));
        child = next.next_sibling();
    }
    all
}
fn pump(duration: Duration) {
    let end = std::time::Instant::now() + duration;
    while std::time::Instant::now() < end {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
#[ignore = "Requires a desktop display and ARCHEASY_UI_FIXTURE=1 fake package tools"]
fn native_search_and_guided_flows() {
    assert_eq!(std::env::var("ARCHEASY_UI_FIXTURE").as_deref(), Ok("1"));
    adw::init().unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.UITest")
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let win = gtk::ApplicationWindow::builder()
        .application(&app)
        .default_width(1000)
        .default_height(760)
        .build();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&page);
    let tabs = gtk::Stack::new();
    tabs.add_named(&page, Some("software"));
    ux::software(&page, &win, &tabs);
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&tabs));
    ux::register_toasts(&overlay);
    win.set_child(Some(&overlay));
    win.present();
    pump(Duration::from_millis(300));
    let widgets = descendants(&page);
    let entry = widgets
        .iter()
        .find_map(|w| w.clone().downcast::<gtk::Entry>().ok())
        .unwrap();
    let list = widgets
        .iter()
        .filter(|w| w.widget_name() == "software-results")
        .find_map(|w| w.clone().downcast::<gtk::ListBox>().ok())
        .unwrap();
    let states = widgets
        .iter()
        .find_map(|w| w.clone().downcast::<gtk::Stack>().ok())
        .unwrap();
    assert_eq!(states.visible_child_name().as_deref(), Some("empty"));
    let pacman = widgets
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::ToggleButton>().ok())
        .find(|b| b.label().as_deref() == Some("Pacman"))
        .unwrap();
    pacman.set_active(true);
    entry.set_text("fire");
    pump(Duration::from_millis(300));
    entry.set_text("ghost");
    pump(Duration::from_millis(1000));
    let row = list
        .row_at_index(0)
        .unwrap()
        .downcast::<adw::ActionRow>()
        .unwrap();
    assert_eq!(row.title(), "ghostty");
    assert!(
        row.subtitle()
            .unwrap()
            .contains("Official • 120 KiB • Installed")
    );
    assert!(
        list.row_at_index(1).is_none(),
        "Old results must not be appended"
    );
    let installed = widgets
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::ToggleButton>().ok())
        .find(|b| b.label().as_deref() == Some("Installed"))
        .unwrap();
    installed.set_active(true);
    pump(Duration::from_millis(600));
    assert!(list.row_at_index(0).is_some());
    entry.set_text("");
    pump(Duration::from_millis(300));
    assert!(list.first_child().is_none());
    assert_eq!(states.visible_child_name().as_deref(), Some("empty"));
    let undo = std::rc::Rc::new(std::cell::Cell::new(false));
    let clicked = undo.clone();
    ux::undo_toast("Fish config applied • backup saved", move || {
        clicked.set(true)
    });
    pump(Duration::from_millis(200));
    let undo_button = descendants(&overlay)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Undo"))
        .unwrap();
    undo_button.emit_clicked();
    assert!(undo.get());
    let setup = gtk::Button::new();
    let update = gtk::Button::new();
    ux::onboarding(&win, &tabs, &setup, &update);
    pump(Duration::from_millis(200));
    let onboarding = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.title().as_deref() == Some("Welcome to ArchEasy"))
        .unwrap();
    let skip = descendants(&onboarding)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Skip"))
        .unwrap();
    skip.emit_clicked();
    pump(Duration::from_millis(100));
    assert!(
        glib::user_config_dir()
            .join("archeasy/onboarding-complete")
            .exists()
    );
    system_tools::update_dialog(&win, &gtk::Label::new(None));
    pump(Duration::from_millis(600));
    let dialog = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.title().as_deref() == Some("Review system update"))
        .unwrap();
    let titles: Vec<_> = descendants(&dialog)
        .iter()
        .filter_map(|w| w.clone().downcast::<adw::ActionRow>().ok())
        .map(|row| row.title().to_string())
        .collect();
    assert!(
        titles.iter().any(|s| s.contains("Found 2 Pacman + 1 AUR")),
        "{titles:?}"
    );
    dialog.close();
    win.close();
    build_ui(&app);
    pump(Duration::from_millis(2100));
    let win = app.active_window().unwrap();
    let widgets = descendants(&win);
    let sound_settings = widgets
        .iter()
        .find(|w| w.widget_name() == "sound-settings")
        .unwrap()
        .clone()
        .downcast::<gtk::MenuButton>()
        .unwrap();
    sound_settings.popup();
    pump(Duration::from_millis(100));
    let volume = descendants(&win)
        .into_iter()
        .find(|w| w.widget_name() == "sound-volume")
        .unwrap()
        .downcast::<gtk::Scale>()
        .unwrap();
    volume.set_value(42.0);
    assert_eq!(
        std::fs::read_to_string(glib::user_config_dir().join("archeasy/sound-volume")).unwrap(),
        "42"
    );
    sound_settings.popdown();
    let history = widgets
        .iter()
        .find(|w| w.widget_name() == "search-history")
        .unwrap();
    assert!(descendants(history).iter().any(|w| w.is::<gtk::Button>()));
    let about = widgets
        .iter()
        .find(|w| w.widget_name() == "archeasy-about")
        .unwrap()
        .clone()
        .downcast::<gtk::Button>()
        .unwrap();
    about.emit_clicked();
    pump(Duration::from_millis(100));
    let about_window = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "archeasy-about-window" && w.is_visible())
        .unwrap();
    descendants(&about_window)
        .into_iter()
        .find(|w| w.widget_name() == "check-app-updates")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    pump(Duration::from_millis(300));
    assert!(
        descendants(&about_window)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .any(|l| l.widget_name() == "app-update-status"
                && l.text().contains("v9.0.0 is available"))
    );
    about_window.downcast::<gtk::Window>().unwrap().close();
    widgets
        .iter()
        .find(|w| w.widget_name() == "find-orphans")
        .unwrap()
        .clone()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    pump(Duration::from_millis(300));
    let orphans = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "orphan-cleaner-window")
        .unwrap();
    assert!(
        !descendants(&orphans)
            .iter()
            .find(|w| w.widget_name() == "remove-selected-orphans")
            .unwrap()
            .is_sensitive()
    );
    orphans.downcast::<gtk::Window>().unwrap().close();
    let theme = widgets
        .iter()
        .filter(|w| w.widget_name() == "archeasy-theme")
        .find_map(|w| w.clone().downcast::<gtk::DropDown>().ok())
        .unwrap();
    theme.set_selected(1);
    pump(Duration::from_millis(100));
    assert!(!adw::StyleManager::default().is_dark());
    assert!(win.has_css_class("light"));
    theme.set_selected(2);
    pump(Duration::from_millis(100));
    assert!(adw::StyleManager::default().is_dark());
    assert!(win.has_css_class("dark"));
    let apply = widgets
        .iter()
        .filter(|w| w.widget_name() == "apply-terminal-stack")
        .find_map(|w| w.clone().downcast::<gtk::Button>().ok())
        .unwrap();
    apply.emit_clicked();
    pump(Duration::from_millis(150));
    let diff = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.widget_name() == "config-diff-dialog")
        .unwrap();
    let labels: Vec<_> = descendants(&diff)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.text().to_string())
        .collect();
    assert!(
        labels
            .iter()
            .any(|s| s.contains("Will write") && s.contains("Backup will be saved to"))
    );
    assert!(descendants(&diff).iter().any(|w| w.is::<gtk::Expander>()));
    assert!(
        !glib::user_config_dir().join("fish/config.fish").exists(),
        "Preview must not write configs"
    );
    diff.close();
    theme.set_selected(0);
    pump(Duration::from_millis(100));
    assert!(!win.has_css_class("light"));
    assert_eq!(
        win.has_css_class("dark"),
        adw::StyleManager::default().is_dark()
    );
    assert!(
        widgets
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::ToggleButton>().ok())
            .any(|b| b.label().as_deref() == Some("GitHub"))
    );
    let tabs = widgets
        .iter()
        .find(|w| w.widget_name() == "archeasy-tabs")
        .unwrap()
        .clone()
        .downcast::<gtk::Stack>()
        .unwrap();
    tabs.set_visible_child_name("installed");
    pump(Duration::from_millis(1600));
    let installed_widgets = descendants(&tabs.child_by_name("installed").unwrap());
    let ring = |name: &str| {
        installed_widgets
            .iter()
            .find(|w| w.widget_name() == name)
            .unwrap()
    };
    if std::env::var("ARCHEASY_BROKEN_REPO").as_deref() == Ok("1") {
        assert!(
            ring("installed-ring-pacman")
                .tooltip_text()
                .unwrap()
                .contains("3 of 3 installed entries (100%)")
        );
        assert!(
            ring("installed-ring-aur")
                .tooltip_text()
                .unwrap()
                .contains("included in the Pacman total")
        );
        assert!(
            installed_widgets
                .iter()
                .filter_map(|w| w.clone().downcast::<gtk::Label>().ok())
                .any(|l| l.text().contains("missing required signature"))
        );
    } else {
        assert!(
            ring("installed-ring-pacman")
                .tooltip_text()
                .unwrap()
                .contains("2 of 3 installed entries (67%)")
        );
        assert!(
            ring("installed-ring-aur")
                .tooltip_text()
                .unwrap()
                .contains("1 of 3 installed entries (33%)")
        );
    }
    assert!(
        installed_widgets
            .iter()
            .any(|w| w.widget_name().contains("local-tool"))
    );
    if let Ok(path) = std::env::var("ARCHEASY_INSTALLED_SCREENSHOT") {
        let paintable = gtk::WidgetPaintable::new(Some(&win));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, win.width() as f64, win.height() as f64);
        let node = snapshot.to_node().unwrap();
        let renderer = gtk::gsk::Renderer::for_surface(&win.surface().unwrap()).unwrap();
        renderer
            .render_texture(&node, None)
            .save_to_png(path)
            .unwrap();
        renderer.unrealize();
    }
    tabs.set_visible_child_name("software");
    pump(Duration::from_millis(200));
    let card_host = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let card_status = gtk::Label::new(None);
    let (queue, queue_run) = store::queue_bar(&card_host, &card_status);
    let (github_row, build) = ux::result_row(
        Package {
            name: "danny19862025/dotfiles".into(),
            version: "★ 12".into(),
            description: "fish config with starship".into(),
            source: "GitHub".into(),
            remote: String::new(),
            installed: false,
        },
        "Size unavailable",
        &queue,
        &queue_run,
        &card_status,
    );
    assert_eq!(build.label().as_deref(), Some("Clone & Build"));
    assert!(build.is_sensitive());
    assert_eq!(github_row.title(), "danny19862025/dotfiles");
    assert!(
        github_row
            .subtitle()
            .unwrap()
            .contains("GitHub • ★ 12 • Root PKGBUILD")
    );
    assert!(
        descendants(&github_row)
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::LinkButton>().ok())
            .any(|b| b.uri() == "https://github.com/danny19862025/dotfiles")
    );
    assert!(
        !descendants(&github_row)
            .iter()
            .any(|w| w.is::<gtk::CheckButton>())
    );
    let list = widgets
        .iter()
        .find(|w| w.widget_name() == "software-results")
        .unwrap()
        .clone()
        .downcast::<gtk::ListBox>()
        .unwrap();
    let states = widgets
        .iter()
        .filter_map(|w| w.clone().downcast::<gtk::Stack>().ok())
        .find(|stack| stack.child_by_name("results").is_some())
        .unwrap();
    list.append(&github_row);
    states.set_visible_child_name("results");
    pump(Duration::from_millis(150));
    if let Ok(path) = std::env::var("ARCHEASY_UI_SCREENSHOT") {
        assert!(
            win.measure(gtk::Orientation::Vertical, win.width()).0 < 1000,
            "Cache permission errors must not force the window off-screen"
        );
        let paintable = gtk::WidgetPaintable::new(Some(&win));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, win.width() as f64, win.height() as f64);
        let node = snapshot.to_node().unwrap();
        let renderer = gtk::gsk::Renderer::for_surface(&win.surface().unwrap()).unwrap();
        renderer
            .render_texture(&node, None)
            .save_to_png(path)
            .unwrap();
        renderer.unrealize();
    }
    win.close();
}

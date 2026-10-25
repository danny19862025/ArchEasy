use super::*;
use std::{fs, path::PathBuf, time::Instant};

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        result.extend(descendants(&current));
        child = current.next_sibling();
    }
    result
}
fn named(root: &gtk::Widget, name: &str) -> gtk::Widget {
    descendants(root)
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap()
}
fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        if condition() {
            return;
        }
        assert!(Instant::now() < deadline, "UI operation timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn settle() {
    let start = Instant::now();
    wait_until(|| start.elapsed() >= Duration::from_millis(250));
}
fn snapshot(window: &impl IsA<gtk::Window>, name: &str) {
    let window = window.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let node = snapshot.to_node().unwrap();
    window
        .renderer()
        .unwrap()
        .render_texture(&node, None)
        .save_to_png(format!("/tmp/archeasy-cleaner-{name}.png"))
        .unwrap();
}
#[allow(deprecated)]
fn confirmation() -> gtk::MessageDialog {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::MessageDialog>().ok())
        .find(|w| w.is_visible())
        .expect("Trash confirmation")
}

#[allow(deprecated)]
fn theme_editor() -> gtk::Dialog {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter(|w| w.widget_name() == "custom-theme-editor" && w.is_visible())
        .find_map(|w| w.downcast::<gtk::Dialog>().ok())
        .unwrap()
}

/// Run explicitly in a desktop session with isolated XDG_CONFIG_HOME/XDG_DATA_HOME.
#[test]
#[ignore = "requires a graphical session and isolated XDG directories"]
#[allow(deprecated)]
fn embedded_cleaner_workflow() {
    let isolated =
        PathBuf::from(std::env::var_os("ARCHEASY_CLEANER_TEST_ROOT").expect("isolated test root"));
    assert_eq!(
        isolated.parent(),
        Some(glib::home_dir().join(".cache").as_path())
    );
    assert!(
        isolated
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("archeasy-cleaner-smoke-")
    );
    assert_eq!(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        Some(isolated.join("data"))
    );
    gtk::init().unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.CleanerTest")
        .build();
    app.register(gtk::gio::Cancellable::NONE).unwrap();
    crate::build_ui(&app);
    let window = app
        .active_window()
        .unwrap()
        .downcast::<gtk::ApplicationWindow>()
        .unwrap();
    let stack = named(window.upcast_ref(), "archeasy-tabs")
        .downcast::<gtk::Stack>()
        .unwrap();
    let titles: Vec<_> = (0..stack.pages().n_items())
        .map(|i| {
            stack
                .pages()
                .item(i)
                .unwrap()
                .downcast::<gtk::StackPage>()
                .unwrap()
                .title()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        titles,
        ["Software", "Configs", "AppCleaner", "Cloud Backup"]
    );
    stack.set_visible_child_name("appcleaner");
    let intro_end = Instant::now();
    wait_until(|| intro_end.elapsed() > Duration::from_secs(2));
    let original_size = (window.width(), window.height());
    snapshot(&window, "initial");
    let settings = gtk::Settings::default().unwrap();
    let original_settings = format!(
        "{:?}",
        settings.property_value("gtk-interface-color-scheme")
    );
    let picker = descendants(window.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::ComboBoxText>().ok())
        .unwrap();
    for (id, name) in [("light", "light"), ("dark", "dark"), ("cyan", "cyan")] {
        picker.set_active_id(Some(id));
        settle();
        assert_eq!(
            format!(
                "{:?}",
                settings.property_value("gtk-interface-color-scheme")
            ),
            original_settings
        );
        snapshot(&window, name);
    }
    // Custom themes are independent, validated, persisted, and only applied on acceptance.
    let main_picker = named(window.upcast_ref(), "archeasy-theme")
        .downcast::<gtk::DropDown>()
        .unwrap();
    named(window.upcast_ref(), "archeasy-customize-theme")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let editor = theme_editor();
    let accent = named(editor.upcast_ref(), "custom-colour-4")
        .downcast::<gtk::Entry>()
        .unwrap();
    accent.set_text("invalid");
    editor.response(gtk::ResponseType::Accept);
    assert!(editor.is_visible());
    assert_ne!(main_picker.selected(), 3);
    editor.response(gtk::ResponseType::Other(1));
    assert_eq!(accent.text(), crate::custom_theme::Palette::default().0[4]);
    let colour_button = descendants(editor.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ColorDialogButton>().ok())
        .nth(4)
        .unwrap();
    colour_button.set_rgba(&gtk::gdk::RGBA::parse("#a855f7").unwrap());
    assert_eq!(accent.text(), "#a855f7");
    settle();
    snapshot(&editor, "custom-editor");
    editor.response(gtk::ResponseType::Accept);
    assert_eq!(main_picker.selected(), 3);
    assert_eq!(
        crate::custom_theme::Palette::load(&crate::custom_theme_path()).0[4],
        "#a855f7"
    );
    assert_eq!(fs::read_to_string(crate::theme_path()).unwrap(), "custom");
    named(window.upcast_ref(), "cleaner-customize-theme")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let editor = theme_editor();
    named(editor.upcast_ref(), "custom-colour-4")
        .downcast::<gtk::Entry>()
        .unwrap()
        .set_text("#ff9800");
    editor.response(gtk::ResponseType::Accept);
    assert_eq!(picker.active_id().as_deref(), Some("custom"));
    let cleaner_palette = crate::custom_theme::Palette::load(&theme::Theme::custom_path());
    assert_eq!(cleaner_palette.0[4], "#ff9800");
    assert_eq!(
        crate::custom_theme::Palette::load(&crate::custom_theme_path()).0[4],
        "#a855f7"
    );
    settle();
    snapshot(&window, "custom-cleaner");
    named(window.upcast_ref(), "cleaner-customize-theme")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let editor = theme_editor();
    editor.response(gtk::ResponseType::Other(1));
    editor.response(gtk::ResponseType::Cancel);
    assert_eq!(
        crate::custom_theme::Palette::load(&theme::Theme::custom_path()),
        cleaner_palette
    );
    main_picker.set_selected(0);
    main_picker.set_selected(3);
    stack.set_visible_child_name("configs");
    settle();
    assert_eq!(stack.visible_child_name().as_deref(), Some("configs"));
    stack.set_visible_child_name("software");
    settle();
    snapshot(&window, "software");

    // Use only disposable fixtures for selection, cancellation and actual Trash.
    let roots = Roots {
        home: isolated.join("fixture-home"),
        config: isolated.join("fixture-home/config"),
        cache: isolated.join("fixture-home/cache"),
        data: isolated.join("fixture-home/data"),
    };
    fs::create_dir_all(roots.data.join("applications")).unwrap();
    fs::create_dir_all(roots.cache.join("cleaner-fixture")).unwrap();
    fs::write(roots.cache.join("cleaner-fixture/example.txt"), b"fixture").unwrap();
    fs::write(roots.data.join("applications/cleaner-fixture.desktop"), "[Desktop Entry]\nType=Application\nName=Cleaner Fixture\nExec=example\nX-Flatpak=io.example.CleanerFixture\n").unwrap();
    let page = page_with_roots(&window, roots.clone());
    window.set_child(Some(&page));
    settle();
    let search = named(page.upcast_ref(), "cleaner-search")
        .downcast::<gtk::SearchEntry>()
        .unwrap();
    search.set_text("Cleaner Fixture");
    let list = named(page.upcast_ref(), "cleaner-apps")
        .downcast::<gtk::ListBox>()
        .unwrap();
    wait_until(|| {
        (0..1000)
            .filter_map(|i| list.row_at_index(i))
            .filter(|r| r.is_visible())
            .count()
            == 1
    });
    let row = (0..1000)
        .filter_map(|i| list.row_at_index(i))
        .find(|r| r.is_visible())
        .unwrap();
    list.select_row(Some(&row));
    let files = named(page.upcast_ref(), "cleaner-files");
    wait_until(|| {
        descendants(&files)
            .iter()
            .any(|w| w.is::<gtk::CheckButton>())
    });
    let check = descendants(&files)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::CheckButton>().ok())
        .unwrap();
    let clean = named(page.upcast_ref(), "cleaner-trash")
        .downcast::<gtk::Button>()
        .unwrap();
    assert!(!check.is_active());
    assert!(!clean.is_sensitive());
    let command = named(page.upcast_ref(), "cleaner-command")
        .downcast::<gtk::Entry>()
        .unwrap();
    assert_eq!(
        command.text(),
        "flatpak uninstall 'io.example.CleanerFixture'"
    );
    let copy = descendants(page.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Copy uninstall command"))
        .unwrap();
    assert!(copy.is_sensitive());
    window.present();
    wait_until(|| window.is_active());
    copy.emit_clicked();
    assert_eq!(
        glib::MainContext::default()
            .block_on(copy.clipboard().read_text_future())
            .unwrap()
            .unwrap(),
        command.text()
    );
    check.set_active(true);
    assert!(clean.is_sensitive());
    settle();
    snapshot(&window, "fixture-selected");
    clean.emit_clicked();
    let dialog = confirmation();
    assert!(
        dialog
            .secondary_text()
            .unwrap()
            .contains(roots.cache.join("cleaner-fixture").to_str().unwrap())
    );
    dialog.response(gtk::ResponseType::Cancel);
    assert!(roots.cache.join("cleaner-fixture/example.txt").exists());
    clean.emit_clicked();
    confirmation().response(gtk::ResponseType::Accept);
    assert!(
        !roots.cache.join("cleaner-fixture").exists(),
        "{}",
        descendants(page.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.text().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(!check.is_active());
    assert!(!check.is_sensitive());
    assert!(!clean.is_sensitive());
    assert!(
        isolated
            .join("data/Trash/files")
            .read_dir()
            .unwrap()
            .next()
            .is_some()
    );
    println!(
        "Tabs, themes, search, scan, selection, uninstall command, cancel and fixture Trash verified at {original_size:?}."
    );
    window.close();
}

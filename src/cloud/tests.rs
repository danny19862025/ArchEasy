use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "archeasy-cloud-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn client(&self) -> Client {
        fs::create_dir_all(self.0.join("remote")).unwrap();
        let config = self.0.join("rclone.conf");
        fs::write(
            &config,
            format!(
                "[fixture]\ntype = alias\nremote = {}\n",
                self.0.join("remote").display()
            ),
        )
        .unwrap();
        Client {
            config: Some(config),
        }
    }
    fn home(&self) -> PathBuf {
        let home = self.0.join("home");
        fs::create_dir_all(home.join(".config/example")).unwrap();
        fs::write(
            home.join(".config/example/settings.conf"),
            b"original config\n",
        )
        .unwrap();
        home
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn snapshot_rejects_external_paths_skips_links_and_preserves_originals() {
    let fixture = Fixture::new();
    let home = fixture.home();
    symlink("/etc/passwd", home.join(".config/example/external-link")).unwrap();
    let snapshot = core::snapshot(
        &home,
        &[
            home.join(".config/example"),
            home.join(".config/example/settings.conf"),
        ],
        &fixture.0.join("stage"),
    )
    .unwrap();
    assert_eq!(snapshot.files, 1);
    assert_eq!(snapshot.skipped, 1);
    assert_eq!(
        fs::read(snapshot.path.join("files/.config/example/settings.conf")).unwrap(),
        b"original config\n"
    );
    assert!(
        !snapshot
            .path
            .join("files/.config/example/external-link")
            .exists()
    );
    assert_eq!(
        fs::metadata(&snapshot.path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(core::validate_source(&home, &home).is_err());
    assert!(core::validate_source(&home, &home.join("../outside")).is_err());
    assert!(core::validate_source(&home, &PathBuf::from("/etc/passwd")).is_err());
    assert!(core::snapshot(&home, &[], &fixture.0.join("stage")).is_err());
    let path = snapshot.path.clone();
    drop(snapshot);
    assert!(!path.exists());
    assert_eq!(
        fs::read(home.join(".config/example/settings.conf")).unwrap(),
        b"original config\n"
    );
}
#[test]
fn local_cloud_roundtrip_preserves_backups_and_refuses_overwrites() {
    let fixture = Fixture::new();
    let client = fixture.client();
    let home = fixture.home();
    assert_eq!(client.remotes().unwrap()[0].name, "fixture");
    let first = core::snapshot(
        &home,
        &[home.join(".config/example")],
        &fixture.0.join("stage"),
    )
    .unwrap();
    let target = client
        .upload(&first, "fixture", "ArchEasy/ConfigBackups")
        .unwrap();
    assert!(target.contains(&first.name));
    fs::write(
        home.join(".config/example/settings.conf"),
        b"changed config",
    )
    .unwrap();
    let second = core::snapshot(
        &home,
        &[home.join(".config/example")],
        &fixture.0.join("stage"),
    )
    .unwrap();
    client
        .upload(&second, "fixture", "ArchEasy/ConfigBackups")
        .unwrap();
    let names = client.list("fixture", "ArchEasy/ConfigBackups").unwrap();
    assert_eq!(names.len(), 2);
    fs::create_dir(fixture.0.join("downloads")).unwrap();
    let downloaded = client
        .download(
            "fixture",
            "ArchEasy/ConfigBackups",
            &first.name,
            &fixture.0.join("downloads"),
        )
        .unwrap();
    assert_eq!(
        fs::read(downloaded.join("files/.config/example/settings.conf")).unwrap(),
        b"original config\n"
    );
    assert!(downloaded.join("manifest.json").is_file());
    assert!(
        client
            .download(
                "fixture",
                "ArchEasy/ConfigBackups",
                &first.name,
                &fixture.0.join("downloads")
            )
            .is_err()
    );
    assert!(client.list("fixture", "../bad").is_err());
    assert!(client.list("--help", "safe").is_err());
    assert!(
        client
            .download("fixture", "safe", "../bad", &fixture.0)
            .is_err()
    );
    assert_eq!(
        fs::read(home.join(".config/example/settings.conf")).unwrap(),
        b"changed config"
    );
}
fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(w) = child {
        out.extend(descendants(&w));
        child = w.next_sibling();
    }
    out
}
fn named(root: &gtk::Widget, name: &str) -> gtk::Widget {
    descendants(root)
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap()
}
fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        if condition() {
            return;
        }
        assert!(Instant::now() < deadline, "cloud UI timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
#[ignore = "requires a graphical session; uses only a local fixture cloud"]
fn cloud_tab_workflow() {
    gtk::init().unwrap();
    let fixture = Fixture::new();
    let client = fixture.client();
    let home = fixture.home();
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.CloudTest")
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
    let page = page_with_client(&window, client.clone(), home.clone(), home.join(".config"));
    stack
        .child_by_name("cloud")
        .unwrap()
        .downcast::<gtk::Box>()
        .unwrap()
        .append(&page);
    stack.set_visible_child_name("cloud");
    let start = Instant::now();
    wait_until(|| start.elapsed() > Duration::from_secs(2));
    let status = named(page.upcast_ref(), "cloud-status")
        .downcast::<gtk::Label>()
        .unwrap();
    wait_until(|| status.text().contains("1 cloud connections"));
    let picker = named(page.upcast_ref(), "cloud-account")
        .downcast::<gtk::ComboBoxText>()
        .unwrap();
    assert_eq!(picker.active_id().as_deref(), Some("fixture"));
    let save = named(page.upcast_ref(), "cloud-save")
        .downcast::<gtk::Button>()
        .unwrap();
    assert!(!save.is_sensitive());
    let source = named(page.upcast_ref(), "cloud-sources");
    let check = descendants(&source)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::CheckButton>().ok())
        .unwrap();
    assert!(!check.is_active());
    check.set_active(true);
    assert!(save.is_sensitive());
    save.emit_clicked();
    let button = || {
        gtk::Window::list_toplevels()
            .into_iter()
            .flat_map(|w| descendants(&w))
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.label().as_deref() == Some("Save backup"))
    };
    wait_until(|| button().is_some());
    let cancel = gtk::Window::list_toplevels()
        .into_iter()
        .flat_map(|w| descendants(&w))
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Cancel"))
        .unwrap();
    cancel.emit_clicked();
    assert!(!fixture.0.join("remote/ArchEasy").exists());
    save.emit_clicked();
    wait_until(|| button().is_some());
    button().unwrap().emit_clicked();
    wait_until(|| status.text().starts_with("Saved 1 files"));
    assert_eq!(
        client
            .list("fixture", "ArchEasy/ConfigBackups")
            .unwrap()
            .len(),
        1
    );
    let browse = descendants(page.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("List backups"))
        .unwrap();
    browse.emit_clicked();
    wait_until(|| status.text().contains("1 backups found"));
    let backups = named(page.upcast_ref(), "cloud-backups")
        .downcast::<gtk::ComboBoxText>()
        .unwrap();
    assert!(backups.active_id().unwrap().starts_with("config-"));
    let rendered = Instant::now();
    wait_until(|| rendered.elapsed() > Duration::from_millis(300));
    let paintable = gtk::WidgetPaintable::new(Some(&window));
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
        .save_to_png("/tmp/archeasy-cloud-tab.png")
        .unwrap();
    assert_eq!(
        fs::read(home.join(".config/example/settings.conf")).unwrap(),
        b"original config\n"
    );
    window.close();
}

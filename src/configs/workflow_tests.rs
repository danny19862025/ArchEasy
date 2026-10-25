use super::workflow::{Plan, apply_reviewed, reviewed};
use super::*;
use std::process::Command;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("archeasy-workflow-{}", stamp()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn stack_backs_up_everything_before_writes_and_restores_all_files() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    let backups = temp.0.join("backups");
    fs::create_dir_all(root.join("fish")).unwrap();
    fs::create_dir_all(root.join("fastfetch/config.jsonc")).unwrap();
    fs::write(root.join("fish/config.fish"), "original fish").unwrap();
    let plan = Plan::stack();
    assert!(apply_plan(&plan, &root, &backups).is_err());
    assert_eq!(
        fs::read_to_string(root.join("fish/config.fish")).unwrap(),
        "original fish"
    );
    assert!(!root.join("starship.toml").exists());
    fs::remove_dir(root.join("fastfetch/config.jsonc")).unwrap();
    let before = reviewed(&plan, &root).unwrap();
    let backup = apply_reviewed(&plan, &before, &root, &backups).unwrap();
    assert!(root.join("fish/conf.d/archeasy-starship.fish").is_file());
    assert!(root.join("ghostty/config").is_file());
    restore_snapshot(&backup, &root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("fish/config.fish")).unwrap(),
        "original fish"
    );
    assert!(!root.join("fish/conf.d/archeasy-starship.fish").exists());
    assert!(!root.join("ghostty/config").exists());
    assert!(!root.join("starship.toml").exists());
}
#[test]
fn diff_review_is_read_only_and_stale_reviews_are_rejected() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    let plan = Plan::builtin(&PRESETS[2]);
    let before = reviewed(&plan, &root).unwrap();
    assert!(!root.exists());
    fs::create_dir_all(root.join("fish")).unwrap();
    fs::write(root.join("fish/config.fish"), "changed while reviewing").unwrap();
    assert!(
        apply_reviewed(&plan, &before, &root, &temp.0.join("backups"))
            .unwrap_err()
            .contains("changed after")
    );
    assert_eq!(
        fs::read_to_string(root.join("fish/config.fish")).unwrap(),
        "changed while reviewing"
    );
    assert!(!temp.0.join("backups").exists());
}
#[test]
fn imported_preset_persists_privately_and_supports_backup_restore() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    let path = root.join("myapp/prefs.ini");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "my original\n").unwrap();
    let plan = sharing::import_file(&path, "My config", &root, &temp.0.join("presets")).unwrap();
    assert_eq!(plan.files[0].0, "myapp/prefs.ini");
    let storage = temp.0.join("presets").join(&plan.id).join("preset.json");
    assert_eq!(
        fs::metadata(storage).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "my original\n");
    fs::write(&path, "later config").unwrap();
    let backup = apply_plan(&plan, &root, &temp.0.join("backups")).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "my original\n");
    restore_snapshot(&backup, &root).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "later config");
    std::os::unix::fs::symlink(&path, root.join("linked")).unwrap();
    assert!(
        sharing::import_file(
            &root.join("linked"),
            "Linked",
            &root,
            &temp.0.join("presets")
        )
        .is_err()
    );
    assert!(
        sharing::import_file(
            &temp.0.join("external"),
            "Outside",
            &root,
            &temp.0.join("presets")
        )
        .is_err()
    );
    let mut malicious = plan.json();
    malicious["files"][0]["path"] = serde_json::json!("../outside");
    assert!(Plan::parse(&malicious).is_err());
}
#[test]
fn gist_publishing_is_explicit_unlisted_by_default_and_cleans_its_staging() {
    let temp = Temp::new();
    let program = temp.0.join("gh");
    fs::write(&program,"#!/bin/bash\nprintf '%s\\n' \"$@\" > \"$(dirname -- \"$0\")/args\"\nprintf 'https://gist.github.com/example/abcdef\\n'\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let plan = Plan::builtin(&PRESETS[1]);
    let staging = temp.0.join("export");
    sharing::publish(&plan, false, &program, &staging).unwrap();
    let args = fs::read_to_string(temp.0.join("args")).unwrap();
    assert!(args.starts_with("gist\ncreate\n"));
    assert!(!args.contains("--public"));
    assert!(args.contains(".png.b64"));
    assert!(args.contains("archeasy-preset.json"));
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    sharing::publish(&plan, true, &program, &staging).unwrap();
    assert!(
        fs::read_to_string(temp.0.join("args"))
            .unwrap()
            .contains("--public")
    );
}
fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn setup_fixture(temp: &Temp) -> (PathBuf, PathBuf, PathBuf) {
    let bin = temp.0.join("bin");
    fs::create_dir(&bin).unwrap();
    for tool in ["mkdir", "flock", "cut", "dirname"] {
        std::os::unix::fs::symlink(format!("/usr/bin/{tool}"), bin.join(tool)).unwrap();
    }
    executable(&bin.join("sudo"), "#!/bin/bash\n\"$@\"\n");
    executable(
        &bin.join("pacman"),
        "#!/bin/bash\nprintf 'install\\n' >> \"$TEST_ROOT/trace\"\nif [[ ${FAIL_INSTALL:-0} == 1 ]]; then exit 1; fi\nfor command in fish starship ghostty fastfetch; do printf '#!/bin/bash\\nexit 0\\n' > \"$TEST_ROOT/bin/$command\"; /usr/bin/chmod 700 \"$TEST_ROOT/bin/$command\"; done\n",
    );
    executable(
        &bin.join("apply"),
        "#!/bin/bash\nprintf 'apply\\n' >> \"$TEST_ROOT/trace\"\n[[ ${FAIL_APPLY:-0} != 1 ]]\n",
    );
    executable(
        &bin.join("chsh"),
        "#!/bin/bash\nprintf 'chsh\\n' >> \"$TEST_ROOT/trace\"\n[[ ${FAIL_CHSH:-0} != 1 ]]\n",
    );
    executable(
        &bin.join("getent"),
        "#!/bin/bash\nprintf 'user:x:1000:1000:User:/tmp:/bin/bash\\n'\n",
    );
    executable(&bin.join("id"), "#!/bin/bash\nprintf 'user\\n'\n");
    let fish = bin.join("fish");
    let shells = temp.0.join("shells");
    fs::write(&shells, format!("{}\n", fish.display())).unwrap();
    let script = temp.0.join("setup.sh");
    fs::write(
        &script,
        include_str!("../../scripts/config-setup.sh")
            .replace("/usr/bin/fish", &fish.to_string_lossy())
            .replace("< /etc/shells", &format!("< '{}'", shells.display())),
    )
    .unwrap();
    (bin, script, temp.0.join("job.json"))
}
#[test]
fn setup_chain_installs_then_applies_then_changes_shell_and_stops_on_failures() {
    for scenario in [
        "success",
        "install-fails",
        "apply-fails",
        "chsh-fails",
        "no-shell-change",
    ] {
        let temp = Temp::new();
        let (bin, script, job) = setup_fixture(&temp);
        let output = Command::new("/usr/bin/bash")
            .arg(&script)
            .arg(bin.join("apply"))
            .arg(&job)
            .arg(if scenario == "no-shell-change" {
                "false"
            } else {
                "true"
            })
            .arg("true")
            .args(["fish", "starship", "ghostty", "fastfetch"])
            .env("PATH", &bin)
            .env("TEST_ROOT", &temp.0)
            .env("XDG_RUNTIME_DIR", temp.0.join("locks"))
            .env(
                "FAIL_INSTALL",
                if scenario == "install-fails" {
                    "1"
                } else {
                    "0"
                },
            )
            .env(
                "FAIL_APPLY",
                if scenario == "apply-fails" { "1" } else { "0" },
            )
            .env(
                "FAIL_CHSH",
                if scenario == "chsh-fails" { "1" } else { "0" },
            )
            .output()
            .unwrap();
        let trace = fs::read_to_string(temp.0.join("trace")).unwrap();
        assert_eq!(
            output.status.success(),
            ["success", "no-shell-change"].contains(&scenario),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(trace.starts_with("install\n"));
        assert_eq!(trace.contains("apply"), scenario != "install-fails");
        assert_eq!(
            trace.contains("chsh"),
            ["success", "chsh-fails"].contains(&scenario)
        );
        assert!(PathBuf::from(format!("{}.done", job.display())).exists());
    }
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
fn wait_until(condition: impl Fn() -> bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        if condition() {
            return;
        }
        assert!(std::time::Instant::now() < end, "UI timeout");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
#[ignore = "requires a desktop and isolated XDG config directories"]
#[allow(deprecated)]
fn configs_workflow_ui() {
    gtk::init().unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.archeasy.ConfigTest")
        .build();
    app.register(gtk::gio::Cancellable::NONE).unwrap();
    crate::build_ui(&app);
    let window = app
        .active_window()
        .unwrap()
        .downcast::<gtk::ApplicationWindow>()
        .unwrap();
    let stack = descendants(window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "archeasy-tabs")
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    stack.set_visible_child_name("configs");
    let start = std::time::Instant::now();
    wait_until(|| start.elapsed() > std::time::Duration::from_secs(2));
    assert!(
        descendants(window.upcast_ref())
            .iter()
            .any(|w| w.widget_name() == "import-config-preset")
    );
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
        .save_to_png("/tmp/archeasy-configs.png")
        .unwrap();
    let apply = descendants(window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "apply-terminal-stack")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    apply.emit_clicked();
    let dialog = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "config-diff-dialog")
        .unwrap()
        .downcast::<gtk::Dialog>()
        .unwrap();
    let notebook = descendants(dialog.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Notebook>().ok())
        .unwrap();
    assert_eq!(notebook.n_pages() as usize, Plan::stack().files.len());
    let options: Vec<_> = descendants(dialog.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::CheckButton>().ok())
        .collect();
    assert!(options[0].is_active());
    assert!(!options[1].is_active());
    let frame = std::time::Instant::now();
    wait_until(|| frame.elapsed() > std::time::Duration::from_millis(250));
    let paintable = gtk::WidgetPaintable::new(Some(&dialog));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(dialog.width()),
        f64::from(dialog.height()),
    );
    let node = snapshot.to_node().unwrap();
    dialog
        .renderer()
        .unwrap()
        .render_texture(&node, None)
        .save_to_png("/tmp/archeasy-config-diff.png")
        .unwrap();
    dialog.response(gtk::ResponseType::Cancel);
    assert!(!glib::user_config_dir().join("fish/config.fish").exists());
    let config = glib::user_config_dir().join("customapp/settings.ini");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "private local config\n").unwrap();
    let imported = sharing::import_file(
        &config,
        "Imported test preset",
        &glib::user_config_dir(),
        &glib::user_data_dir().join("archeasy/presets"),
    )
    .unwrap();
    let status = gtk::Label::new(None);
    let refresh = gtk::Button::new();
    workflow::review_dialog(
        &window,
        imported.clone(),
        workflow::Mode::Direct,
        &status,
        &refresh,
        None,
    );
    let dialog = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "config-diff-dialog" && w.is_visible())
        .unwrap()
        .downcast::<gtk::Dialog>()
        .unwrap();
    dialog.response(gtk::ResponseType::Accept);
    assert!(latest_backup(&imported.id, &backups_dir()).is_some());
    sharing::export_dialog(&window, imported);
    let export = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "gist-export-dialog")
        .unwrap()
        .downcast::<gtk::Dialog>()
        .unwrap();
    let public = descendants(export.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::CheckButton>().ok())
        .unwrap();
    assert!(!public.is_active());
    export.response(gtk::ResponseType::Cancel);
    window.close();
}

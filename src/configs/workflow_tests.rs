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
fn binary_configs_survive_apply_history_zip_export_and_restore() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    let backups = temp.0.join("backups");
    let binary = b"\xff__CONFIG_DIR__\x80".to_vec();
    let nul_binary = b"\0__CONFIG_DIR__".to_vec();
    let plan = Plan::parse(&serde_json::json!({
        "id":"binary-fixture", "name":"Binary fixture", "dependencies":[],
        "files":[
            {"path":"app/font.ttf", "bytes":binary},
            {"path":"app/cache.db", "bytes":nul_binary},
            {"path":"app/settings.conf", "bytes":b"path=__CONFIG_DIR__/app".to_vec()}
        ]
    }))
    .unwrap();
    let backup = apply_plan(&plan, &root, &backups).unwrap();
    assert_eq!(fs::read(root.join("app/font.ttf")).unwrap(), binary);
    assert_eq!(fs::read(root.join("app/cache.db")).unwrap(), nul_binary);
    assert_eq!(
        fs::read(root.join("app/settings.conf")).unwrap(),
        format!("path={}/app", root.display()).as_bytes()
    );
    let current = history::current_plan(&root, &backups).unwrap();
    let version = history::versions(&backups).remove(0).1;
    for saved in [&current, &version] {
        assert_eq!(
            saved
                .files
                .iter()
                .find(|(n, _)| n == "app/font.ttf")
                .unwrap()
                .1,
            binary
        );
        assert_eq!(
            saved
                .files
                .iter()
                .find(|(n, _)| n == "app/cache.db")
                .unwrap()
                .1,
            nul_binary
        );
        assert_eq!(
            saved
                .files
                .iter()
                .find(|(n, _)| n == "app/settings.conf")
                .unwrap()
                .1,
            b"path=__CONFIG_DIR__/app"
        );
    }
    let output = temp.0.join("portable.zip");
    archive::export(&backups, &root, &output).unwrap();
    let imported = archive::read(&output).unwrap();
    let restored = temp.0.join("other-config");
    apply_plan(&imported, &restored, &temp.0.join("other-backups")).unwrap();
    assert_eq!(fs::read(restored.join("app/font.ttf")).unwrap(), binary);
    assert_eq!(fs::read(restored.join("app/cache.db")).unwrap(), nul_binary);
    assert_eq!(
        fs::read(restored.join("app/settings.conf")).unwrap(),
        format!("path={}/app", restored.display()).as_bytes()
    );
    restore_snapshot(&backup, &root).unwrap();
    assert!(!root.join("app/font.ttf").exists());
}
#[test]
fn publishing_secrets_is_blocked_before_staging_or_running_github_cli() {
    let temp = Temp::new();
    let mut plan = Plan::builtin(&PRESETS[0]);
    plan.files = vec![(
        "app/settings".into(),
        b"token=ghp_0123456789abcdefghijklmnop".to_vec(),
    )];
    let stage = temp.0.join("stage");
    for public in [false, true] {
        let error =
            sharing::publish(&plan, public, Path::new("/does-not-exist"), &stage).unwrap_err();
        assert!(error.contains("Publishing blocked"));
        assert!(!error.contains("ghp_"));
        assert!(!stage.exists());
    }
}
#[test]
fn failed_fish_syntax_or_startup_restores_entire_transaction() {
    for bad_fish in ["if true\n", "__archeasy_missing_startup_command__\n"] {
        let temp = Temp::new();
        let root = temp.0.join("config");
        let backups = temp.0.join("backups");
        fs::create_dir_all(root.join("fish")).unwrap();
        let original = root.join("fish/config.fish");
        fs::write(&original, "# original config\n").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o640)).unwrap();
        let mut plan = Plan::builtin(PRESETS.iter().find(|p| p.id == "fish").unwrap());
        plan.files[0].1 = bad_fish.as_bytes().to_vec();
        let error = apply_plan(&plan, &root, &backups).unwrap_err();
        assert!(
            error.contains("Previous configs automatically restored"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(&original).unwrap(),
            "# original config\n"
        );
        assert_eq!(
            fs::metadata(&original).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(!root.join("fish/functions/fish_prompt.fish").exists());
        let backup = fs::read_dir(backups.join("fish"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.join("manifest.json").is_file())
            .unwrap();
        assert!(backup.with_extension("zip").is_file());
        assert!(backup.join("restored").is_file());
        assert!(!backup.join("applied-version.json").exists());
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
    adw::init().unwrap();
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
    descendants(window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "dry-run-fish")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let dry_run = gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == "config-diff-dialog" && w.is_visible())
        .unwrap()
        .downcast::<gtk::Dialog>()
        .unwrap();
    assert!(
        dry_run
            .widget_for_response(gtk::ResponseType::Accept)
            .is_none()
    );
    dry_run.response(gtk::ResponseType::Cancel);
    assert!(!glib::user_config_dir().join("fish").exists());
    assert!(!backups_dir().exists());
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
    wait_until(|| !dialog.is_visible());
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
    export.response(gtk::ResponseType::Cancel);
    for (button_name, window_name) in [
        ("config-timeline", "config-history-window"),
        ("community-presets", "community-presets-window"),
    ] {
        descendants(window.upcast_ref())
            .into_iter()
            .find(|w| w.widget_name() == button_name)
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        let child = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.widget_name() == window_name && w.is_visible())
            .unwrap();
        child.close();
    }
    // Cancelling conflict resolution keeps the original link and manager source untouched.
    let managed = root_for_ui_fixture();
    workflow::review_dialog(
        &window,
        Plan {
            id: "managed-ui".into(),
            name: "Managed fixture".into(),
            files: vec![(
                "managed-fixture/settings.conf".into(),
                b"new config".to_vec(),
            )],
            dependencies: vec![],
            managed_parents: vec![],
        },
        workflow::Mode::Direct,
        &status,
        &refresh,
        None,
    );
    let conflict = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Dialog>().ok())
        .find(|w| w.widget_name() == "managed-config-conflicts" && w.is_visible())
        .unwrap();
    conflict.response(gtk::ResponseType::Cancel);
    assert!(glib::user_config_dir().join("managed-fixture").is_symlink());
    assert_eq!(
        fs::read_to_string(managed.join("settings.conf")).unwrap(),
        "original managed fixture"
    );
    crate::system_tools::update_dialog(&window, &status);
    let update = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Dialog>().ok())
        .find(|w| w.widget_name() == "system-update-snapshot-dialog" && w.is_visible())
        .unwrap();
    update.response(gtk::ResponseType::Cancel);
    window.close();
}

#[test]
fn portable_zip_contains_history_and_restores_current_configs_on_another_pc() {
    let temp = Temp::new();
    let root = temp.0.join("old-pc");
    fs::create_dir_all(root.join("fish")).unwrap();
    fs::write(root.join("fish/config.fish"), "before setup\n").unwrap();
    let plan = Plan::stack();
    let backups = temp.0.join("backups");
    let backup = apply_plan(&plan, &root, &backups).unwrap();
    let before = archive::read(&backup.with_extension("zip")).unwrap();
    assert_eq!(
        before.files,
        vec![("fish/config.fish".into(), b"before setup\n".to_vec())]
    );
    let zip = temp.0.join("transfer.zip");
    archive::export(&backups, &root, &zip).unwrap();
    assert_eq!(
        fs::metadata(&zip).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let portable = archive::read(&zip).unwrap();
    assert_eq!(portable.files.len(), plan.files.len());
    let new_root = temp.0.join("new-pc");
    apply_plan(&portable, &new_root, &temp.0.join("new-backups")).unwrap();
    for (name, _) in portable.files {
        assert_eq!(
            fs::read(new_root.join(&name)).unwrap(),
            fs::read(root.join(name)).unwrap()
        );
    }
    assert!(archive::export(&backups, &root, &zip).is_err());
    let output = Command::new("python3").args(["-c", "import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert any(n.endswith('/manifest.json') for n in z.namelist()); assert any(n.endswith('/files/fish/config.fish') for n in z.namelist())"]).arg(&zip).output().unwrap();
    assert!(output.status.success());
}
#[test]
fn every_new_preset_roundtrips_and_full_setup_includes_the_desktop() {
    let full = Plan::stack();
    for name in [
        "fish/config.fish",
        "starship.toml",
        "ghostty/config",
        "kitty/kitty.conf",
        "fastfetch/config.jsonc",
        "hypr/hyprland.lua",
    ] {
        assert!(full.files.iter().any(|(n, _)| n == name));
    }
    for preset in &PRESETS[7..] {
        let temp = Temp::new();
        let root = temp.0.join("config");
        let plan = Plan::builtin(preset);
        assert_eq!(
            Plan::parse(&plan.json()).unwrap().dependencies,
            plan.dependencies
        );
        let backup = apply_plan(&plan, &root, &temp.0.join("backups")).unwrap();
        assert!(backup.with_extension("zip").is_file());
        restore_snapshot(&backup, &root).unwrap();
        assert!(plan.files.iter().all(|(n, _)| !root.join(n).exists()));
    }
}
#[test]
fn github_import_rejects_other_hosts_options_and_traversal_before_network() {
    for url in [
        "https://evil.example/a",
        "https://gist.github.com/--help/abcd",
        "https://gist.github.com/user/../../bad",
        "https://github.com/u/r/blob/main/../bad.json",
        "https://github.com/u/r/blob/main/-/bad?x=y",
    ] {
        assert!(sharing::fetch_url(url).is_err());
    }
}

#[test]
fn github_import_roundtrips_text_binary_dependencies_and_repo_files_without_network() {
    let temp = Temp::new();
    let program = temp.0.join("gh");
    let manifest = serde_json::json!({"format_version":1,"name":"Imported Fish","dependencies":["fish", "evil-command"],"files":[{"gist_file":"config.fish","config_path":"fish/config.fish","encoding":"utf-8"},{"gist_file":"image.b64","config_path":"fastfetch/arch.png","encoding":"base64"}]});
    fs::write(temp.0.join("archeasy-preset.json"), manifest.to_string()).unwrap();
    fs::write(temp.0.join("config.fish"), "set -g greeting hello\n").unwrap();
    fs::write(temp.0.join("image.b64"), "AAECAw==").unwrap();
    executable(
        &program,
        "#!/bin/bash\nif [[ $1 == gist ]]; then /usr/bin/cat \"${0%/*}/$5\"; else /usr/bin/cat \"${0%/*}/repo.json\"; fi\n",
    );
    let plan = sharing::fetch_url_with("https://gist.github.com/user/abc123", &program).unwrap();
    assert_eq!(plan.dependencies, ["fish"]);
    assert_eq!(plan.files[0].1, b"set -g greeting hello\n");
    assert_eq!(plan.files[1].1, [0, 1, 2, 3]);
    fs::write(temp.0.join("repo.json"), plan.json().to_string()).unwrap();
    let repo = sharing::fetch_url_with(
        "https://github.com/user/repo/blob/main/archeasy/fish.json",
        &program,
    )
    .unwrap();
    assert_eq!(repo.files, plan.files);
    let mut malicious = manifest;
    malicious["files"][0]["config_path"] = serde_json::json!("../../outside");
    fs::write(temp.0.join("archeasy-preset.json"), malicious.to_string()).unwrap();
    assert!(sharing::fetch_url_with("https://gist.github.com/user/abc123", &program).is_err());
}

#[test]
fn managed_folder_resolution_detaches_locally_and_restore_relinks_original_manager() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    let source = temp.0.join("stow/fish");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(source.join("config.fish"), "manager original\n").unwrap();
    fs::write(source.join("unrelated"), "keep this file\n").unwrap();
    std::os::unix::fs::symlink(&source, root.join("fish")).unwrap();
    let mut plan = Plan::builtin(&PRESETS[2]);
    assert!(reviewed(&plan, &root).is_err());
    plan.managed_parents = conflicts::links(&plan, &root).unwrap();
    let before = reviewed(&plan, &root).unwrap();
    assert!(root.join("fish").is_symlink());
    let backup = apply_reviewed(&plan, &before, &root, &temp.0.join("backups")).unwrap();
    assert!(!root.join("fish").is_symlink());
    assert_eq!(
        fs::read_to_string(source.join("config.fish")).unwrap(),
        "manager original\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("fish/unrelated")).unwrap(),
        "keep this file\n"
    );
    assert!(backup.with_extension("zip").is_file());
    fs::write(root.join("fish/local-added-later"), "retain me").unwrap();
    restore_snapshot(&backup, &root).unwrap();
    assert!(root.join("fish").is_symlink());
    assert_eq!(fs::read_link(root.join("fish")).unwrap(), source);
    assert_eq!(
        fs::read_dir(backup.join("managed-retained"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn git_versions_preserve_unrelated_staged_files_and_skip_staged_config_paths() {
    let temp = Temp::new();
    let root = temp.0.join("config");
    fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    fs::write(root.join("unrelated.txt"), "staged by user").unwrap();
    git(&["add", "--", "unrelated.txt"]);
    let mut plan = Plan::builtin(&PRESETS[2]);
    let backups = temp.0.join("backups");
    let first = apply_plan(&plan, &root, &backups).unwrap();
    assert!(
        history::message(&first).starts_with("Git commit:"),
        "{}",
        history::message(&first)
    );
    assert_eq!(git(&["diff", "--cached", "--name-only"]), "unrelated.txt");
    assert!(!git(&["show", "--format=", "--name-only", "HEAD"]).contains("unrelated"));
    fs::write(root.join("fish/config.fish"), "staged config\n").unwrap();
    git(&["add", "--", "fish/config.fish"]);
    plan.files[0].1 = b"# new working tree config\n".to_vec();
    let second = apply_plan(&plan, &root, &backups).unwrap();
    assert!(history::message(&second).contains("staged changes"));
    assert_eq!(git(&["show", ":fish/config.fish"]), "staged config");
    assert_eq!(history::versions(&backups).len(), 2);
    let old = history::versions(&backups)
        .into_iter()
        .find(|(_, p, _)| p.files[0].1 != b"# new working tree config\n")
        .unwrap()
        .1;
    apply_plan(&old, &root, &backups).unwrap();
    assert_ne!(
        fs::read(root.join("fish/config.fish")).unwrap(),
        b"# new working tree config\n"
    );
}
#[test]
fn imported_presets_cannot_approve_link_detachment() {
    let mut value = Plan::builtin(&PRESETS[2]).json();
    value["managed_parents"] = serde_json::json!([["fish", "/outside"]]);
    assert!(Plan::parse(&value).unwrap().managed_parents.is_empty());
}

fn root_for_ui_fixture() -> PathBuf {
    let managed = glib::user_data_dir().join("managed-ui-source");
    fs::create_dir_all(&managed).unwrap();
    fs::write(managed.join("settings.conf"), "original managed fixture").unwrap();
    let link = glib::user_config_dir().join("managed-fixture");
    if !link.is_symlink() {
        std::os::unix::fs::symlink(&managed, &link).unwrap();
    }
    managed
}

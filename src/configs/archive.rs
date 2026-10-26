#![allow(deprecated)] // GTK 4.10-compatible dialogs.
use super::*;
use std::process::Command;
fn run(mode: &str, source: &Path, root: &Path, target: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new("python3");
    command.args(["-c", include_str!("../../scripts/config-archive.py"), mode]);
    command.arg(source).arg(root);
    if let Some(target) = target {
        command.arg(target);
    }
    let out = command
        .output()
        .map_err(|e| format!("Python 3 is required for ZIP backups: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
pub(super) fn snapshot(backup: &Path, root: &Path) -> Result<PathBuf, String> {
    let output = backup.with_extension("zip");
    run("write", backup, root, Some(&output))?;
    Ok(output)
}
pub(super) fn export(backups: &Path, root: &Path, output: &Path) -> Result<(), String> {
    run("write", backups, root, Some(output)).map(|_| ())
}
pub(super) fn read(path: &Path) -> Result<workflow::Plan, String> {
    let text = run("read", path, Path::new("."), None)?;
    let value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    workflow::Plan::parse(&value)
}
// Keep disk and network work off GTK's main loop.
fn task<T: Send + 'static>(
    status: &gtk::Label,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(T) + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    let status = status.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        match rx.try_recv() {
            Ok(Ok(value)) => done(value),
            Ok(Err(e)) => status.set_text(&e),
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(_) => status.set_text("Backup task stopped unexpectedly"),
        }
        glib::ControlFlow::Break
    });
}
pub(super) fn buttons(
    bar: &gtk::FlowBox,
    parent: &gtk::ApplicationWindow,
    status: &gtk::Label,
    refresh: &gtk::Button,
) {
    let export_button = gtk::Button::with_label("Export backups ZIP…");
    let weak = parent.downgrade();
    let s = status.clone();
    export_button.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; }; let s = s.clone();
        gtk::FileDialog::builder().title("Save backup history and current preset configs").initial_name(format!("archeasy-backups-{}.zip", stamp())).build().save(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result && let Some(path) = file.path() {
                s.set_text("Creating backup ZIP…"); let label = s.clone();
                task(&s, move || { export(&backups_dir(), &glib::user_config_dir(), &path)?; Ok(path) }, move |p| label.set_text(&format!("Saved {}. Includes all backup history and current managed configs for a new PC.", p.display())));
            }
        });
    });
    bar.insert(&export_button, -1);
    let restore = gtk::Button::with_label("Restore from ZIP…");
    let weak = parent.downgrade();
    let s = status.clone();
    let r = refresh.clone();
    restore.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else {
            return;
        };
        let weak = parent.downgrade();
        let s = s.clone();
        let r = r.clone();
        gtk::FileDialog::builder()
            .title("Restore portable configs from an ArchEasy ZIP")
            .build()
            .open(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result
                    && let Some(path) = file.path()
                {
                    let weak = weak.clone();
                    let status = s.clone();
                    let refresh = r.clone();
                    task(
                        &s,
                        move || read(&path),
                        move |plan| {
                            if let Some(parent) = weak.upgrade() {
                                workflow::review_dialog(
                                    &parent,
                                    plan,
                                    workflow::Mode::Direct,
                                    &status,
                                    &refresh,
                                    None,
                                );
                            }
                        },
                    );
                }
            });
    });
    bar.insert(&restore, -1);
    let drive = gtk::Button::with_label("Save backups to Drive…");
    let weak = parent.downgrade();
    let s = status.clone();
    drive.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; };
        #[allow(deprecated)] let dialog = gtk::Dialog::builder().title("Save backup ZIP to cloud").transient_for(&parent).modal(true).default_width(520).build();
        let body = dialog.content_area(); body.set_spacing(10); body.set_margin_top(16); body.set_margin_bottom(16); body.set_margin_start(16); body.set_margin_end(16);
        let note = super::super::label("Choose an rclone connection. Set up Google Drive in the Cloud Backup tab first. Backups remain private according to your cloud account permissions.", "subtitle"); note.set_wrap(true); body.append(&note);
        let picker = gtk::ComboBoxText::new(); body.append(&picker);
        let message = super::super::label("Loading cloud connections…", "subtitle"); message.set_wrap(true); body.append(&message);
        let picker_load = picker.clone(); let message_done = message.clone();
        task(&message, || {
            let out = Command::new("rclone").args(["listremotes", "--json"]).output().map_err(|e| e.to_string())?;
            if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).into()); }
            serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout).map_err(|e| e.to_string())
        }, move |values| { for v in values { if let Some(name) = v["name"].as_str() { picker_load.append(Some(name), name); } } picker_load.set_active(Some(0)); message_done.set_text("ZIPs are stored in ArchEasy/Backups. On a new PC, download the ZIP from Drive and use Restore from ZIP."); });
        #[allow(deprecated)] { dialog.add_button("Cancel", gtk::ResponseType::Cancel); dialog.add_button("Save to cloud", gtk::ResponseType::Accept); }
        let s = s.clone();
        #[allow(deprecated)] dialog.connect_response(move |dialog, response| {
            if response != gtk::ResponseType::Accept { dialog.close(); return; }
            let Some(remote) = picker.active_id() else { message.set_text("Configure and select a cloud connection first."); return; };
            if remote.starts_with('-') || remote.contains([':', '/', '\\', '\n', '\r']) { return; }
            let target = format!("{remote}:ArchEasy/Backups/archeasy-{}.zip", stamp());
            s.set_text("Creating and uploading backup ZIP…"); let label = s.clone();
            task(&s, move || {
                let directory = glib::user_data_dir().join("archeasy/exports"); fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
                let path = directory.join(format!("backups-{}.zip", stamp())); export(&backups_dir(), &glib::user_config_dir(), &path)?;
                let out = Command::new("rclone").arg("copyto").arg(&path).arg(&target).args(["--immutable", "--ask-password=false", "--contimeout=15s", "--timeout=60s", "--retries=1", "--max-duration=10m"]).output().map_err(|e| e.to_string())?;
                if !out.status.success() { return Err(format!("Upload failed. ZIP kept at {}. {}", path.display(), String::from_utf8_lossy(&out.stderr))); }
                Ok(format!("Saved to {target}. Local ZIP: {}", path.display()))
            }, move |text| label.set_text(&text));
            dialog.close();
        }); dialog.present();
    });
    bar.insert(&drive, -1);
}

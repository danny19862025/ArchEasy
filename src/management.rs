//! GTK adapters for the shared management helper; no GUI process runs as root.
use gtk::prelude::*;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

const HELPER: &str = include_str!("../scripts/system-management.py");
fn source() -> String {
    format!(
        "import types, sys\n_core = types.ModuleType('management')\nexec({}, _core.__dict__)\nsys.modules['management'] = _core\n{}",
        serde_json::to_string(HELPER).expect("helper string"),
        include_str!("../scripts/workbench.py")
    )
}

pub(crate) fn inspect(request: Value) -> Result<String, String> {
    let mut child = Command::new("python3")
        .args(["-c", &source()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let written = child
        .stdin
        .take()
        .ok_or("Helper input unavailable")?
        .write_all(request.to_string().as_bytes());
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    written.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

pub(crate) fn launch(request: Value) -> Result<(), String> {
    // The helper owns package/update locks for the whole transaction. A launcher
    // flock would conflict with its independently opened lock in the child.
    crate::terminal_with_lock(terminal_args(request), false)
}

fn terminal_args(request: Value) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        source(),
        "--terminal".into(),
        request.to_string(),
    ]
}

pub fn update(snapshot: &str) -> Result<(), String> {
    if !["none", "snapper", "timeshift"].contains(&snapshot) {
        return Err("Invalid snapshot provider".into());
    }
    launch(json!({"action":"official", "snapshot":snapshot}))
}

pub(crate) fn read_button(
    body: &gtk::Box,
    title: &str,
    request: Value,
    text: &gtk::TextView,
    status: &gtk::Label,
) {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};
    let button = gtk::Button::with_label(title);
    let cancel = gtk::Button::with_label("Cancel scan");
    cancel.set_sensitive(false);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&button);
    row.append(&cancel);
    let spinner = gtk::Spinner::new();
    row.append(&spinner);
    body.append(&row);
    let active: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let active_cancel = active.clone();
    cancel.connect_clicked(move |button| {
        if let Some(directory) = active_cancel.borrow().as_ref() {
            let _ = std::fs::write(directory.join("cancel"), b"cancel");
            button.set_sensitive(false);
        }
    });
    let text = text.clone();
    let status = status.clone();
    button.connect_clicked(move |button| {
        use std::os::unix::fs::DirBuilderExt;
        let directory = std::env::temp_dir().join(format!(
            "archeasy-scan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if let Err(error) = std::fs::DirBuilder::new().mode(0o700).create(&directory) {
            status.set_text(&error.to_string());
            return;
        }
        *active.borrow_mut() = Some(directory.clone());
        button.set_sensitive(false);
        cancel.set_sensitive(true);
        spinner.start();
        status.set_text(
            "Inspecting… Cancel stops diagnostic tools only; no atomic transaction is interrupted.",
        );
        let weak = button.downgrade();
        let text = text.clone();
        let done = status.clone();
        let mut request = request.clone();
        request["_cancel_path"] = json!(directory.join("cancel"));
        let spinner = spinner.clone();
        let cancel = cancel.clone();
        let active = active.clone();
        crate::system_tools::task_with_finish(
            &status,
            move || {
                let result = inspect(request);
                let _ = std::fs::remove_dir_all(directory);
                result
            },
            move |report| {
                text.buffer().set_text(&report);
                done.set_text("Inspection completed. Review the report and logs.");
            },
            move || {
                active.borrow_mut().take();
                spinner.stop();
                cancel.set_sensitive(false);
                if let Some(button) = weak.upgrade() {
                    button.set_sensitive(true);
                }
            },
        );
    });
}

pub(crate) fn terminal_button(
    body: &gtk::Box,
    title: &str,
    action: &'static str,
    fields: Vec<(&'static str, gtk::Entry)>,
    status: &gtk::Label,
) {
    let button = gtk::Button::with_label(title);
    body.append(&button);
    let status = status.clone();
    button.connect_clicked(move |_| {
        let mut request = json!({"action":action});
        for (key, entry) in &fields { request[*key] = json!(entry.text().as_str()); }
        match launch(request) {
            Ok(()) => status.set_text("Terminal opened for explicit approval. Review the result there, then refresh this page and View Logs. Closing the GUI does not cancel the transaction."),
            Err(error) => status.set_text(&error),
        }
    });
}

pub(crate) fn entry(body: &gtk::Box, title: &str) -> gtk::Entry {
    let entry = gtk::Entry::builder().placeholder_text(title).build();
    entry.set_tooltip_text(Some(title));
    body.append(&entry);
    entry
}

pub fn page(parent: &gtk::ApplicationWindow, kind: &str) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::system_tools::margins(&body);
    page.append(
        &gtk::ScrolledWindow::builder()
            .child(&body)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    body.append(&crate::label(kind, "name"));
    let status = crate::label(
        "Ready. Read-only scans need no confirmation. Changes require approval in your terminal.",
        "subtitle",
    );
    status.set_wrap(true);
    let text = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    text.buffer()
        .set_text("Choose an inspection below. No system changes have started.");
    match kind {
        "Backup & Restore" => {
            let note = crate::label(
                "Local snapshots do not protect against disk failure. Root restore and deletion use guided workflows; no automatic retention is enabled.",
                "subtitle",
            );
            note.set_wrap(true);
            body.append(&note);
            read_button(
                &body,
                "View Snapshots / Storage Usage",
                json!({"action":"backups"}),
                &text,
                &status,
            );
            terminal_button(&body, "Create Snapshot…", "snapshot", vec![], &status);
            let first = entry(&body, "First Snapper snapshot ID");
            let second = entry(&body, "Second Snapper snapshot ID");
            let compare = gtk::Button::with_label("Compare Snapshots");
            body.append(&compare);
            let report = text.clone();
            let done = status.clone();
            compare.connect_clicked(move |_| {
                let request = json!({"action":"compare", "first":first.text().as_str(), "second":second.text().as_str()});
                let report = report.clone();
                crate::system_tools::task(&done, move || inspect(request), move |result| report.buffer().set_text(&result));
            });
            read_button(
                &body,
                "Restore Snapshot — Recovery Guide",
                json!({"action":"recovery"}),
                &text,
                &status,
            );
            read_button(
                &body,
                "Delete Snapshot / Configure Automatic Snapshots — Guide",
                json!({"action":"policy"}),
                &text,
                &status,
            );
            crate::snapshots::button(&body, parent);
            body.append(&crate::label("External encrypted backups: use an existing Restic repository on a separately mounted disk. Passwords are prompted in the terminal. Sources may contain secrets; choose carefully.", "subtitle"));
            let repo = entry(&body, "Absolute path to existing local Restic repository");
            let source = entry(&body, "Dedicated source folder inside your home");
            terminal_button(
                &body,
                "Export Backup with Restic…",
                "restic-export",
                vec![("repository", repo.clone()), ("source", source)],
                &status,
            );
            terminal_button(
                &body,
                "Check Backup Health (read all data)…",
                "restic-check",
                vec![("repository", repo.clone())],
                &status,
            );
            terminal_button(
                &body,
                "List External Snapshots…",
                "restic-list",
                vec![("repository", repo.clone())],
                &status,
            );
            let snapshot = entry(&body, "Restic snapshot ID");
            let destination = entry(&body, "New restore folder (must not exist)");
            terminal_button(
                &body,
                "Restore External Backup to New Folder…",
                "restic-restore",
                vec![
                    ("repository", repo),
                    ("snapshot", snapshot),
                    ("destination", destination),
                ],
                &status,
            );
        }
        "System Updates" => {
            read_button(
                &body,
                "Check for Updates / Review Updates",
                json!({"action":"updates"}),
                &text,
                &status,
            );
            let update = gtk::Button::with_label("Update System / Configure Pre-update Backup…");
            body.append(&update);
            let weak = parent.downgrade();
            let update_status = status.clone();
            update.connect_clicked(move |_| {
                if let Some(parent) = weak.upgrade() {
                    crate::system_tools::update_dialog(&parent, &update_status);
                }
            });
            terminal_button(
                &body,
                "Update Flatpak Separately…",
                "flatpak",
                vec![],
                &status,
            );
            read_button(
                &body,
                "Retry Failed Operation — Inspect and Recheck First",
                json!({"action":"history"}),
                &text,
                &status,
            );
            body.append(&gtk::LinkButton::with_label(
                "https://archlinux.org/news/",
                "Read Arch News / Manual Intervention Notices ↗",
            ));
        }
        _ => {
            read_button(
                &body,
                "Scan Hardware / Check Drivers / Review Recommended Actions",
                json!({"action":"hardware"}),
                &text,
                &status,
            );
            read_button(
                &body,
                "Check Firmware (no installation)",
                json!({"action":"firmware"}),
                &text,
                &status,
            );
            read_button(
                &body,
                "Check GPU Acceleration / Check Vulkan Support",
                json!({"action":"acceleration"}),
                &text,
                &status,
            );
            body.append(&gtk::LinkButton::with_label(
                "https://wiki.archlinux.org/title/NVIDIA",
                "Review NVIDIA Generation and Kernel Compatibility ↗",
            ));
        }
    }
    read_button(
        &body,
        "View Logs / Recent and Failed Operations",
        json!({"action":"history"}),
        &text,
        &status,
    );
    body.append(&status);
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&text)
            .min_content_height(300)
            .vexpand(true)
            .build(),
    );
    page
}

#[cfg(test)]
mod tests {
    #[test]
    fn management_terminal_leaves_package_lock_to_helper() {
        let args =
            super::terminal_args(serde_json::json!({"action":"official", "snapshot":"none"}));
        let command = crate::terminal_command(args.clone(), false).unwrap();
        assert_eq!(command, args);
        assert_eq!(command[0], "python3");
        let guarded = crate::terminal_command(args, true).unwrap();
        assert_eq!(guarded[0], "flock");
    }

    #[test]
    #[ignore = "requires an isolated graphical session; builds widgets only"]
    fn management_pages_build_without_system_operations() {
        gtk::init().unwrap();
        let app = gtk::Application::builder()
            .application_id("io.github.archeasy.ManagementTest")
            .build();
        use gtk::prelude::*;
        app.register(gtk::gio::Cancellable::NONE).unwrap();
        let parent = gtk::ApplicationWindow::builder().application(&app).build();
        for title in ["Backup & Restore", "System Updates", "Hardware & Drivers"] {
            let page = super::page(&parent, title);
            assert!(page.first_child().is_some());
            parent.set_child(Some(&page));
        }
        parent.close();
    }
}

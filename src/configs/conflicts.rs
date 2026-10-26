#![allow(deprecated)]
use super::*;
use std::{collections::BTreeMap, process::Command};
fn valid(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}
pub(super) fn links(plan: &workflow::Plan, root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut found = BTreeMap::new();
    for (name, _) in &plan.files {
        if !valid(name) {
            return Err("Invalid config path".into());
        }
        let mut path = root.to_path_buf();
        for part in Path::new(name)
            .parent()
            .unwrap_or(Path::new(""))
            .components()
        {
            path.push(part);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    let name = path
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .to_string();
                    let target = fs::read_link(&path)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .to_string();
                    found.insert(name, target);
                }
                Ok(meta) if !meta.is_dir() => {
                    return Err(format!("{} is not a directory", path.display()));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    Ok(found.into_iter().collect())
}
pub(super) fn preflight(
    root: &Path,
    name: &str,
    approved: &[(String, String)],
) -> Result<PathBuf, String> {
    if !valid(name) {
        return Err("Invalid config path".into());
    }
    let mut path = root.to_path_buf();
    for part in Path::new(name)
        .parent()
        .unwrap_or(Path::new(""))
        .components()
    {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy();
                let target = fs::read_link(&path)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string();
                if !approved
                    .iter()
                    .any(|(n, t)| n == rel.as_ref() && t == &target)
                {
                    return Err(format!(
                        "{} is managed by another tool; resolve this link first.",
                        path.display()
                    ));
                }
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(format!("{} is not a directory", path.display()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(root.join(name))
}
fn run(mode: &str, root: &Path, backup: &Path, links: &[(String, String)]) -> Result<(), String> {
    if links.is_empty() {
        return Ok(());
    }
    for (name, _) in links {
        if !valid(name) {
            return Err("Invalid managed parent path".into());
        }
    }
    let out = Command::new("python3")
        .args(["-c", include_str!("../../scripts/managed-links.py"), mode])
        .arg(root)
        .arg(backup)
        .arg(serde_json::to_string(links).map_err(|e| e.to_string())?)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into())
    }
}
pub(super) fn detach(root: &Path, backup: &Path, links: &[(String, String)]) -> Result<(), String> {
    run("detach", root, backup, links)
}
pub(super) fn restore(
    root: &Path,
    backup: &Path,
    links: &[(String, String)],
) -> Result<(), String> {
    run("restore", root, backup, links)
}
pub(super) fn dialog(
    parent: &gtk::ApplicationWindow,
    plan: workflow::Plan,
    mode: workflow::Mode,
    status: &gtk::Label,
    refresh: &gtk::Button,
    preview: Option<&'static Preset>,
) {
    let root = glib::user_config_dir();
    let found = match links(&plan, &root) {
        Ok(v) => v,
        Err(e) => {
            notice(parent, "Cannot inspect managed config", &e);
            return;
        }
    };
    let dialog = gtk::Dialog::builder()
        .title("Resolve configs managed by another tool")
        .transient_for(parent)
        .modal(true)
        .default_width(980)
        .default_height(650)
        .build();
    crate::about::follow_theme(parent, dialog.upcast_ref());
    dialog.set_widget_name("managed-config-conflicts");
    let body = dialog.content_area();
    crate::system_tools::margins(&body);
    let detail = found
        .iter()
        .map(|(name, target)| format!("{} → {target}", root.join(name).display()))
        .collect::<Vec<_>>()
        .join("\n");
    let note = super::super::label(
        &format!(
            "Your config folders are managed by another tool (for example chezmoi or Stow):\n{detail}\n\nOverwrite: replace selected parent links with local copies, preserving their other files. Merge: edit the current text on the right, then use that result. Skip: leave this file alone. The manager’s source files are never overwritten. Your original links are recorded for backup restore. Nothing changes until you accept the next diff."
        ),
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_vexpand(true);
    let mut controls = vec![];
    for (name, bytes) in &plan.files {
        if !found
            .iter()
            .any(|(parent, _)| Path::new(name).starts_with(parent))
        {
            controls.push((name.clone(), bytes.clone(), None));
            continue;
        }
        let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let picker = gtk::ComboBoxText::new();
        for (id, title) in [
            ("skip", "Skip · keep managed config"),
            ("overwrite", "Overwrite · use proposed config locally"),
            ("merge", "Merge · use edited current config"),
        ] {
            picker.append(Some(id), title);
        }
        picker.set_active_id(Some("skip"));
        page.append(&picker);
        let current = fs::metadata(root.join(name))
            .ok()
            .filter(|m| m.len() <= 8 * 1024 * 1024)
            .and_then(|_| fs::read_to_string(root.join(name)).ok())
            .unwrap_or_default();
        let old = gtk::TextView::builder()
            .editable(false)
            .monospace(true)
            .build();
        old.buffer().set_text(&format!(
            "Proposed preset:\n{}",
            String::from_utf8_lossy(bytes)
        ));
        let edited = gtk::TextView::builder()
            .monospace(true)
            .editable(true)
            .build();
        edited.buffer().set_text(&current);
        let pane = gtk::Paned::new(gtk::Orientation::Horizontal);
        pane.set_position(450);
        pane.set_start_child(Some(
            &gtk::ScrolledWindow::builder()
                .child(&old)
                .vexpand(true)
                .build(),
        ));
        pane.set_end_child(Some(
            &gtk::ScrolledWindow::builder()
                .child(&edited)
                .vexpand(true)
                .build(),
        ));
        page.append(&pane);
        notebook.append_page(&page, Some(&gtk::Label::new(Some(name))));
        controls.push((name.clone(), bytes.clone(), Some((picker, edited))));
    }
    body.append(&notebook);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Review resolved changes", gtk::ResponseType::Accept);
    let weak = parent.downgrade();
    let status = status.clone();
    let refresh = refresh.clone();
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept {
            dialog.close();
            return;
        }
        let Some(parent) = weak.upgrade() else {
            return;
        };
        let mut resolved = plan.clone();
        resolved.files.clear();
        for (name, bytes, control) in &controls {
            match control {
                None => resolved.files.push((name.clone(), bytes.clone())),
                Some((picker, edited)) => match picker.active_id().as_deref() {
                    Some("overwrite") => resolved.files.push((name.clone(), bytes.clone())),
                    Some("merge") => {
                        if std::str::from_utf8(bytes).is_err() {
                            notice(
                                &parent,
                                "Binary file",
                                "Choose Overwrite or Skip for binary assets.",
                            );
                            return;
                        }
                        let buffer = edited.buffer();
                        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                        resolved
                            .files
                            .push((name.clone(), text.as_bytes().to_vec()));
                    }
                    _ => {}
                },
            }
        }
        if resolved.files.is_empty() {
            status.set_text("All conflicting files skipped. No configs changed.");
            dialog.close();
            return;
        }
        resolved.managed_parents = found
            .iter()
            .filter(|(name, _)| {
                resolved
                    .files
                    .iter()
                    .any(|(p, _)| Path::new(p).starts_with(name))
            })
            .cloned()
            .collect();
        dialog.close();
        workflow::review_dialog(&parent, resolved, mode, &status, &refresh, preview);
    });
    dialog.present();
}

//! File-by-file backup and review-only extraction. Backend remains the safety authority.
use gtk::prelude::*;
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, rc::Rc};

fn selection(paths: Vec<String>, destination: &str) -> Result<Vec<String>, String> {
    if paths.is_empty() || paths.len() > 100 {
        return Err("Select 1–100 files using the checkboxes.".into());
    }
    for name in &paths {
        if name.starts_with('/') || name.split('/').any(|part| matches!(part, "" | "." | "..")) {
            return Err("Choose configuration files inside your home folder.".into());
        }
    }
    let target = Path::new(destination.trim());
    if destination.trim().is_empty() || !target.is_absolute() {
        return Err("Choose a save location first.".into());
    }
    if target.exists() || target.is_symlink() {
        return Err("Choose a new destination that does not already exist.".into());
    }
    if !target.parent().is_some_and(Path::is_dir) {
        return Err("The destination's parent folder must already exist.".into());
    }
    Ok(paths)
}

fn preview_paths(preview: &str) -> Result<Vec<(String, String)>, String> {
    let value: Value = serde_json::from_str(preview).map_err(|e| e.to_string())?;
    value["changes"]
        .as_array()
        .ok_or("Backup file list is unavailable")?
        .iter()
        .map(|item| {
            Ok((
                item["path"]
                    .as_str()
                    .ok_or("Invalid backup file path")?
                    .to_owned(),
                item["state"]
                    .as_str()
                    .unwrap_or("Extract to a separate folder for review")
                    .to_owned(),
            ))
        })
        .collect()
}

#[derive(Clone)]
struct Checklist {
    rows: Rc<RefCell<Vec<(String, gtk::CheckButton)>>>,
    list: gtk::Box,
    count: gtk::Label,
    action: gtk::Button,
    destination: gtk::Entry,
    feedback: gtk::Label,
}
impl Checklist {
    fn new(body: &gtk::Box, action: &str) -> Self {
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let all = gtk::Button::with_label("Select All");
        let clear = gtk::Button::with_label("Clear Selection");
        let count = note("0 files selected");
        toolbar.append(&all);
        toolbar.append(&clear);
        toolbar.append(&count);
        body.append(&toolbar);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let scroll = gtk::ScrolledWindow::builder()
            .min_content_height(180)
            .max_content_height(300)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        body.append(&scroll);
        let result = Self {
            rows: Rc::default(),
            list,
            count,
            action: gtk::Button::with_label(action),
            destination: gtk::Entry::builder()
                .editable(false)
                .hexpand(true)
                .placeholder_text("No location chosen")
                .build(),
            feedback: note(""),
        };
        let weak = Rc::downgrade(&result.rows);
        all.connect_clicked(move |_| {
            if let Some(rows) = weak.upgrade() {
                let buttons: Vec<_> = rows
                    .borrow()
                    .iter()
                    .map(|(_, check)| check.clone())
                    .collect();
                for check in buttons {
                    check.set_active(true);
                }
            }
        });
        let weak = Rc::downgrade(&result.rows);
        clear.connect_clicked(move |_| {
            if let Some(rows) = weak.upgrade() {
                let buttons: Vec<_> = rows
                    .borrow()
                    .iter()
                    .map(|(_, check)| check.clone())
                    .collect();
                for check in buttons {
                    check.set_active(false);
                }
            }
        });
        let weak = Rc::downgrade(&result.rows);
        let (count, action, feedback) = (
            result.count.clone(),
            result.action.clone(),
            result.feedback.clone(),
        );
        result.destination.connect_changed(move |destination| {
            if let Some(rows) = weak.upgrade() {
                let selected: Vec<_> = rows
                    .borrow()
                    .iter()
                    .filter(|(_, check)| check.is_active())
                    .map(|(path, _)| path.clone())
                    .collect();
                count.set_text(&format!("{} files selected", selected.len()));
                match selection(selected, &destination.text()) {
                    Ok(_) => {
                        action.set_sensitive(true);
                        feedback.set_text(
                            "Ready. Review the selected paths in the terminal before approving.",
                        );
                    }
                    Err(error) => {
                        action.set_sensitive(false);
                        feedback.set_text(&error);
                    }
                }
            }
        });
        result.refresh();
        result
    }
    fn selected(&self) -> Vec<String> {
        self.rows
            .borrow()
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(path, _)| path.clone())
            .collect()
    }
    fn refresh(&self) {
        let selected = self.selected();
        self.count
            .set_text(&format!("{} files selected", selected.len()));
        match selection(selected, &self.destination.text()) {
            Ok(_) => {
                self.action.set_sensitive(true);
                self.feedback
                    .set_text("Ready. Review the selected paths in the terminal before approving.");
            }
            Err(error) => {
                self.action.set_sensitive(false);
                self.feedback.set_text(&error);
            }
        }
    }
    fn add(&self, path: String, description: &str) {
        if self.rows.borrow().iter().any(|(name, _)| name == &path) {
            return;
        }
        self.add_to(&self.list, path, description);
    }
    fn add_to(&self, container: &gtk::Box, path: String, description: &str) {
        if self.rows.borrow().iter().any(|(name, _)| name == &path) {
            return;
        }
        let title = path.rsplit('/').next().unwrap_or(&path);
        let check = gtk::CheckButton::with_label(title);
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.append(&check);
        row.append(&note(&format!("~/{path}\n{description}")));
        container.append(&row);
        self.rows.borrow_mut().push((path, check.clone()));
        let weak = Rc::downgrade(&self.rows);
        let (count, action, destination, feedback) = (
            self.count.clone(),
            self.action.clone(),
            self.destination.clone(),
            self.feedback.clone(),
        );
        check.connect_toggled(move |_| {
            if let Some(rows) = weak.upgrade() {
                let selected: Vec<_> = rows
                    .borrow()
                    .iter()
                    .filter(|(_, check)| check.is_active())
                    .map(|(path, _)| path.clone())
                    .collect();
                count.set_text(&format!("{} files selected", selected.len()));
                match selection(selected, &destination.text()) {
                    Ok(_) => {
                        action.set_sensitive(true);
                        feedback.set_text(
                            "Ready. Review the selected paths in the terminal before approving.",
                        );
                    }
                    Err(error) => {
                        action.set_sensitive(false);
                        feedback.set_text(&error);
                    }
                }
            }
        });
        self.refresh();
    }
    fn add_app(&self, app: crate::migration_discovery::Recommendation) {
        let group = gtk::Box::new(gtk::Orientation::Vertical, 6);
        group.set_margin_bottom(12);
        let title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        title.append(&crate::label(
            &format!("{} · {} files", app.name, app.files.len()),
            "heading",
        ));
        let all = gtk::Button::with_label("Select All for This App");
        let clear = gtk::Button::with_label("Clear");
        all.set_widget_name(&format!("select-app-{}", app.name.to_lowercase()));
        title.append(&all);
        title.append(&clear);
        group.append(&title);
        group.append(&note(if app.installed {
            "App detected · Recommended settings"
        } else {
            "Saved settings · App not detected"
        }));
        group.append(&note(app.description));
        if app.limited {
            group.append(&note("Some settings were omitted to keep this scan small. Use Add Other Files to choose more."));
        }
        let paths = app.files;
        for path in &paths {
            self.add_to(&group, path.clone(), "Configuration file");
        }
        for (button, active) in [(all, true), (clear, false)] {
            let weak = Rc::downgrade(&self.rows);
            let paths = paths.clone();
            button.connect_clicked(move |_| {
                if let Some(rows) = weak.upgrade() {
                    let buttons: Vec<_> = rows
                        .borrow()
                        .iter()
                        .filter(|(path, _)| paths.contains(path))
                        .map(|(_, check)| check.clone())
                        .collect();
                    for button in buttons {
                        button.set_active(active);
                    }
                }
            });
        }
        self.list.append(&group);
    }
    fn clear(&self) {
        self.rows.borrow_mut().clear();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.refresh();
    }
    fn finish(&self, body: &gtk::Box) {
        body.append(&self.feedback);
        body.append(&self.action);
    }
}
fn note(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build()
}
fn section(title: &str, explanation: &str) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.append(&crate::label(title, "heading"));
    body.append(&note(explanation));
    body
}

pub(crate) fn build(body: &gtk::Box, parent: &gtk::ApplicationWindow) {
    body.append(&note(
        "Choose the settings you want to keep or recover. Each checkbox represents one file.",
    ));
    let stack = gtk::Stack::new();
    stack.set_vhomogeneous(false);
    let switcher = gtk::StackSwitcher::builder()
        .stack(&stack)
        .halign(gtk::Align::Center)
        .build();
    body.append(&switcher);
    body.append(&stack);
    let backup = section(
        "1. Choose settings to back up",
        "Recommended settings are grouped by app. Select an app’s files together, or choose individual files. Nothing is selected automatically.",
    );
    let export = Checklist::new(&backup, "3. Open Terminal & Create Backup…");
    let loading = note("Looking for installed apps and saved settings…");
    export.list.append(&loading);
    let home = gtk::glib::home_dir();
    let search_paths = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let files = export.clone();
    crate::system_tools::task(
        &export.feedback,
        move || Ok(crate::migration_discovery::discover(&home, &search_paths)),
        move |apps| {
            files.list.remove(&loading);
            let mut without_files = Vec::new();
            for app in apps {
                if app.files.is_empty() {
                    if app.limited {
                        without_files.push(format!(
                            "{}: scan limit reached; use Add Other Files",
                            app.name
                        ));
                    } else {
                        without_files.push(format!(
                            "{}: app detected, no custom settings found",
                            app.name
                        ));
                    }
                } else {
                    files.add_app(app);
                }
            }
            if files.rows.borrow().is_empty() {
                files.list.append(&note(
                    "No supported settings found. Use Add Other Files to choose yours.",
                ));
            }
            if !without_files.is_empty() {
                let details = gtk::Expander::builder()
                    .label(format!(
                        "Detected apps without selectable settings ({})",
                        without_files.len()
                    ))
                    .build();
                details.set_child(Some(&note(&without_files.join("\n"))));
                files.list.append(&details);
            }
            files.refresh();
        },
    );
    let add = gtk::Button::with_label("Add Other Files…");
    backup.append(&add);
    let (parent_add, files) = (parent.clone(), export.clone());
    add.connect_clicked(move |_| {
        let files = files.clone();
        gtk::FileDialog::builder()
            .title("Add configuration files from your home folder")
            .initial_folder(&gtk::gio::File::for_path(gtk::glib::home_dir()))
            .build()
            .open_multiple(
                Some(&parent_add),
                gtk::gio::Cancellable::NONE,
                move |result| {
                    let Ok(chosen) = result else {
                        return;
                    };
                    let home = gtk::glib::home_dir();
                    for index in 0..chosen.n_items() {
                        let Some(path) = chosen
                            .item(index)
                            .and_downcast::<gtk::gio::File>()
                            .and_then(|file| file.path())
                        else {
                            continue;
                        };
                        let Some(relative) = path.strip_prefix(&home).ok().and_then(Path::to_str)
                        else {
                            files
                                .feedback
                                .set_text("Only files inside your home folder can be added.");
                            continue;
                        };
                        if !crate::migration_discovery::safe_file(&home, relative) {
                            files
                                .feedback
                                .set_text("Choose a supported configuration file you own (up to 1 MiB), without symlinks or credential paths.");
                            continue;
                        }
                        files.add(
                            relative.into(),
                            "Added by you; safety checks run before the backup is written",
                        );
                    }
                },
            );
    });
    let package_options = gtk::Expander::builder()
        .label("Package list options")
        .build();
    let options = gtk::Box::new(gtk::Orientation::Vertical, 6);
    options.append(&note("Backups also record official package and Flatpak names. Restoring files does not install packages."));
    let foreign = gtk::CheckButton::with_label("Also record AUR / foreign package names");
    options.append(&foreign);
    package_options.set_child(Some(&options));
    backup.append(&package_options);
    backup.append(&crate::label(
        "2. Choose where to save the backup",
        "heading",
    ));
    backup.append(&export.destination);
    let save = gtk::Button::with_label("Choose Backup Location…");
    backup.append(&save);
    let (save_parent, destination) = (parent.clone(), export.destination.clone());
    save.connect_clicked(move |_| {
        let destination = destination.clone();
        gtk::FileDialog::builder()
            .title("Save a new ArchEasy backup")
            .initial_name("archeasy-migration.json")
            .build()
            .save(
                Some(&save_parent),
                gtk::gio::Cancellable::NONE,
                move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        destination.set_text(&path.to_string_lossy());
                    }
                },
            );
    });
    let action_label = note("");
    action_label.set_selectable(false);
    action_label.set_xalign(0.5);
    action_label.set_markup(
        "<span size=\"large\" weight=\"bold\">3. Open Terminal &amp; Create Backup…</span>",
    );
    export.action.set_child(Some(&action_label));
    export.action.add_css_class("suggested-action");
    export.action.set_size_request(-1, 52);
    export.finish(&backup);
    let approval_instruction = note("");
    approval_instruction.set_markup(
        "<span size=\"large\">Click Step 3 above. In the terminal, check the selected files, type <b>YES</b>, and press <b>Enter</b> to approve.</span>",
    );
    backup.append(&approval_instruction);
    backup.append(&note(
        "Files are checked for unsafe paths and possible secrets before writing.",
    ));
    let files = export.clone();
    let action = export.action.clone();
    action.connect_clicked(move |_| {
        let paths = match selection(files.selected(), &files.destination.text()) { Ok(paths) => paths, Err(error) => { files.feedback.set_text(&error); return; } };
        if paths.iter().any(|path| !gtk::glib::home_dir().join(path).is_file()) { files.feedback.set_text("A selected file is missing. Update the selection before continuing."); return; }
        let request = json!({"action":"migration-export","paths":paths,"foreign":foreign.is_active(),"destination":files.destination.text().trim()});
        match crate::management::launch(request) { Ok(()) => files.feedback.set_text("Terminal opened. Review the selected files, type YES, and press Enter to create the backup."), Err(error) => files.feedback.set_text(&error) }
    });
    stack.add_titled(&backup, Some("backup"), "Back Up Settings");

    let restore = section(
        "1. Open an ArchEasy backup",
        "Recover selected files into a separate folder for review. Your current settings will stay untouched.",
    );
    let source = gtk::Entry::builder()
        .editable(false)
        .placeholder_text("No backup opened")
        .build();
    restore.append(&source);
    let open = gtk::Button::with_label("Open Backup…");
    restore.append(&open);
    restore.append(&crate::label("2. Choose files from the backup", "heading"));
    let recover = Checklist::new(&restore, "Review & Extract Selected Files…");
    recover
        .list
        .append(&note("Open a backup to see the files it contains."));
    let details = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let technical = gtk::Expander::builder()
        .label("Compatibility and package details")
        .build();
    technical.set_child(Some(&details));
    let (open_parent, files, input, report) = (
        parent.clone(),
        recover.clone(),
        source.clone(),
        details.clone(),
    );
    open.connect_clicked(move |button| {
        let (files, input, report, button) =
            (files.clone(), input.clone(), report.clone(), button.clone());
        gtk::FileDialog::builder()
            .title("Open an ArchEasy backup (.json)")
            .build()
            .open(
                Some(&open_parent),
                gtk::gio::Cancellable::NONE,
                move |result| {
                    let Ok(file) = result else {
                        return;
                    };
                    let Some(path) = file.path() else {
                        files.feedback.set_text("Choose a local backup file.");
                        return;
                    };
                    files.clear();
                    input.set_text("");
                    report.buffer().set_text("");
                    button.set_sensitive(false);
                    files.feedback.set_text("Checking backup contents…");
                    let source_path = path.to_string_lossy().into_owned();
                    let done_button = button.clone();
                    let status = files.feedback.clone();
                    crate::system_tools::task_with_finish(
                        &status,
                        move || {
                            let preview = crate::management::inspect(
                                json!({"action":"migration-preview","source":source_path}),
                            )?;
                            let paths = preview_paths(&preview)?;
                            Ok((source_path, paths, preview))
                        },
                        move |(path, paths, preview)| {
                            input.set_text(&path);
                            report.buffer().set_text(&preview);
                            for (path, state) in paths {
                                files.add(path, &state);
                            }
                            if files.rows.borrow().is_empty() {
                                files
                                    .list
                                    .append(&note("This backup contains no configuration files."));
                            }
                            files.refresh();
                        },
                        move || done_button.set_sensitive(true),
                    );
                },
            );
    });
    restore.append(&technical);
    restore.append(&crate::label(
        "3. Choose a folder for extracted files",
        "heading",
    ));
    restore.append(&note("Choose a parent folder, then give the new review folder a name. It must not already exist."));
    let folder = gtk::Entry::builder()
        .editable(false)
        .placeholder_text("No parent folder chosen")
        .build();
    restore.append(&folder);
    let folder_name = gtk::Entry::builder()
        .text("archeasy-restored-settings")
        .placeholder_text("New folder name")
        .build();
    restore.append(&folder_name);
    let choose_folder = gtk::Button::with_label("Choose Parent Folder…");
    restore.append(&choose_folder);
    let (folder_parent, folder_entry) = (parent.clone(), folder.clone());
    choose_folder.connect_clicked(move |_| {
        let folder_entry = folder_entry.clone();
        gtk::FileDialog::builder()
            .title("Choose where to create the review folder")
            .build()
            .select_folder(
                Some(&folder_parent),
                gtk::gio::Cancellable::NONE,
                move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        folder_entry.set_text(&path.to_string_lossy());
                    }
                },
            );
    });
    for input in [&folder, &folder_name] {
        let (folder, name, destination) = (
            folder.clone(),
            folder_name.clone(),
            recover.destination.clone(),
        );
        input.connect_changed(move |_| {
            let name = name.text();
            if folder.text().is_empty()
                || name.is_empty()
                || matches!(name.as_str(), "." | "..")
                || name.contains('/')
                || name.contains('\\')
            {
                destination.set_text("");
            } else {
                destination.set_text(
                    &Path::new(folder.text().as_str())
                        .join(name.as_str())
                        .to_string_lossy(),
                );
            }
        });
    }
    restore.append(&recover.destination);
    restore.append(&note("Extraction does not apply settings or install packages. The terminal shows the selected paths and asks for approval."));
    recover.finish(&restore);
    let files = recover.clone();
    recover.action.connect_clicked(move |_| {
        let paths = match selection(files.selected(), &files.destination.text()) { Ok(paths) => paths, Err(error) => { files.feedback.set_text(&error); return; } };
        if !Path::new(source.text().as_str()).is_file() { files.feedback.set_text("Open and check an existing backup first."); return; }
        match crate::management::launch(json!({"action":"migration-stage","source":source.text().as_str(),"paths":paths,"destination":files.destination.text().trim()})) {
            Ok(()) => files.feedback.set_text("Terminal opened. Approve extraction into the new review folder; current settings stay untouched."), Err(error) => files.feedback.set_text(&error)
        }
    });
    stack.add_titled(&restore, Some("restore"), "Restore Settings");

    let cloud = gtk::Box::new(gtk::Orientation::Vertical, 12);
    cloud.append(&note("Back up selected settings to Google Drive or Dropbox. Connect an account below, review the selected files, then choose Save selected configs to cloud."));
    let update_selection = gtk::Button::with_label("Use Current Backup Selection");
    cloud.append(&update_selection);
    let cloud_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    cloud_content.set_size_request(-1, 650);
    cloud.append(&cloud_content);
    let (weak_parent, files, content) = (parent.downgrade(), export.clone(), cloud_content.clone());
    update_selection.connect_clicked(move |_| {
        let Some(parent) = weak_parent.upgrade() else {
            return;
        };
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }
        let home = gtk::glib::home_dir();
        let selected: Vec<_> = files
            .selected()
            .iter()
            .map(|path| home.join(path))
            .collect();
        content.append(&crate::cloud::page_with_selection(&parent, &selected));
    });
    stack.add_titled(&cloud, Some("cloud"), "Cloud Backup");
    stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("cloud")
            && cloud_content.first_child().is_none()
        {
            update_selection.emit_clicked();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_requires_new_destination_and_safe_relative_files() {
        let target =
            std::env::temp_dir().join(format!("archeasy-selection-{}", std::process::id()));
        let target = target.to_str().unwrap();
        assert!(selection(vec![], target).is_err());
        assert!(selection(vec![".bashrc".into()], "").is_err());
        assert!(selection(vec!["../.bashrc".into()], target).is_err());
        assert!(selection(vec!["/etc/passwd".into()], target).is_err());
        assert!(selection(vec![".bashrc".into()], "/tmp").is_err());
        assert_eq!(
            selection(vec![".config/a,b.conf".into()], target).unwrap(),
            vec![".config/a,b.conf"]
        );
    }
    #[test]
    fn restore_paths_come_from_checked_preview() {
        assert_eq!(
            preview_paths(
                r#"{"changes":[{"path":".config/fish/config.fish","state":"new file"}]}"#
            )
            .unwrap(),
            vec![(".config/fish/config.fish".into(), "new file".into())]
        );
        assert!(preview_paths(r#"{"files":[{"path":".bashrc"}]}"#).is_err());
    }
    #[test]
    #[ignore = "requires isolated graphical session; constructs widgets only"]
    fn app_selection_only_toggles_its_own_files() {
        adw::init().unwrap();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let checklist = Checklist::new(&body, "Backup");
        checklist.add(".bashrc".into(), "Other app");
        checklist.add_app(crate::migration_discovery::Recommendation {
            name: "Fish",
            installed: true,
            description: "Shell settings",
            files: vec![
                ".config/fish/config.fish".into(),
                ".config/fish/functions/a.fish".into(),
            ],
            limited: false,
        });
        let group = checklist
            .list
            .last_child()
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap();
        let header = group.first_child().unwrap().downcast::<gtk::Box>().unwrap();
        let select = header
            .first_child()
            .unwrap()
            .next_sibling()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        select.emit_clicked();
        assert_eq!(
            checklist.selected(),
            vec![".config/fish/config.fish", ".config/fish/functions/a.fish"]
        );
        header
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        assert!(checklist.selected().is_empty());
    }
    #[test]
    #[ignore = "requires isolated graphical session; constructs widgets only"]
    fn checklist_preserves_choices_and_keeps_restore_independent() {
        adw::init().unwrap();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let backup = Checklist::new(&body, "Backup");
        let restore = Checklist::new(&body, "Extract");
        backup.add(".bashrc".into(), "Bash");
        backup.rows.borrow()[0].1.set_active(true);
        backup.add(".gitconfig".into(), "Git");
        backup.add(".bashrc".into(), "Duplicate");
        assert_eq!(backup.rows.borrow().len(), 2);
        assert_eq!(backup.selected(), vec![".bashrc"]);
        restore.add(".profile".into(), "From backup");
        assert!(restore.selected().is_empty());
        restore.clear();
        assert_eq!(backup.selected(), vec![".bashrc"]);
        assert!(!backup.action.is_sensitive());
    }
}

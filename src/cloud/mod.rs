#![allow(deprecated)] // Match the existing account/theme selectors while retaining GTK 4.10 support.
mod core;
#[cfg(test)]
mod tests;
use core::{Client, Remote};
use gtk::{glib, prelude::*};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

type Choices = Rc<RefCell<Vec<(gtk::CheckButton, PathBuf)>>>;
fn text(value: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label
}
fn job<T: Send + 'static>(
    form: &gtk::Box,
    spinner: &gtk::Spinner,
    status: &gtk::Label,
    message: &str,
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(T) -> String + 'static,
) {
    form.set_sensitive(false);
    spinner.start();
    status.set_text(message);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(task());
    });
    let weak = form.downgrade();
    let spinner = spinner.clone();
    let status = status.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let Some(form) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        match rx.try_recv() {
            Ok(result) => {
                form.set_sensitive(true);
                spinner.stop();
                status.set_text(&match result {
                    Ok(value) => done(value),
                    Err(error) => format!("Could not complete: {error}"),
                });
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                form.set_sensitive(true);
                spinner.stop();
                status.set_text("The cloud task stopped unexpectedly. Try again.");
                glib::ControlFlow::Break
            }
            Err(_) => glib::ControlFlow::Continue,
        }
    });
}
fn add_source(
    choices: &Choices,
    list: &gtk::Box,
    home: &std::path::Path,
    path: PathBuf,
    save: &gtk::Button,
    count: &gtk::Label,
) -> Result<(), String> {
    let relative = core::validate_source(home, &path)?;
    if choices.borrow().iter().any(|(_, p)| p == &path) {
        return Ok(());
    }
    let check = gtk::CheckButton::with_label(&format!("~/{}", relative.display()));
    if let Some(label) = check.child().and_downcast::<gtk::Label>() {
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_xalign(0.0);
    }
    check.set_tooltip_text(Some(&path.to_string_lossy()));
    choices.borrow_mut().push((check.clone(), path));
    let state = Rc::downgrade(choices);
    let save = save.downgrade();
    let count = count.downgrade();
    check.connect_toggled(move |_| {
        if let (Some(choices), Some(save), Some(count)) =
            (state.upgrade(), save.upgrade(), count.upgrade())
        {
            let n = choices
                .borrow()
                .iter()
                .filter(|(c, _)| c.is_active())
                .count();
            count.set_text(&format!("{n} config files or folders selected"));
            save.set_sensitive(n > 0);
        }
    });
    list.append(&check);
    Ok(())
}
fn account(picker: &gtk::ComboBoxText) -> Result<String, String> {
    picker
        .active_id()
        .map(|s| s.to_string())
        .ok_or_else(|| "Connect a cloud account, then click Refresh accounts.".into())
}
fn populate(picker: &gtk::ComboBoxText, remotes: &[Remote]) {
    let old = picker.active_id();
    picker.remove_all();
    for remote in remotes {
        picker.append(Some(&remote.name), &remote.label());
    }
    if old.is_none_or(|id| !picker.set_active_id(Some(&id))) && !remotes.is_empty() {
        picker.set_active(Some(0));
    }
}
pub fn page(window: &gtk::ApplicationWindow) -> gtk::Box {
    page_with_client(
        window,
        Client::system(),
        glib::home_dir(),
        glib::user_config_dir(),
    )
}
fn page_with_client(
    window: &gtk::ApplicationWindow,
    client: Client,
    home: PathBuf,
    config: PathBuf,
) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_widget_name("cloud-backup-page");
    page.set_hexpand(true);
    page.set_vexpand(true);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_end(12);
    let heading = text("Your configs, ready for your next laptop.");
    heading.add_css_class("heading");
    form.append(&heading);
    form.append(&text("Save selected existing configuration files to Google Drive or Dropbox. Each backup has its own dated folder."));
    let accounts = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let account_label = text("Cloud account");
    account_label.set_wrap(false);
    accounts.append(&account_label);
    let picker = gtk::ComboBoxText::new();
    picker.set_widget_name("cloud-account");
    picker.set_hexpand(true);
    accounts.append(&picker);
    let refresh = gtk::Button::with_label("Refresh accounts");
    accounts.append(&refresh);
    form.append(&accounts);
    let connections = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    for (name, provider, kind) in [
        ("Connect Google Drive", "google-drive", "drive"),
        ("Connect Dropbox", "dropbox", "dropbox"),
    ] {
        let button = gtk::Button::with_label(name);
        let weak_window = window.downgrade();
        button.connect_clicked(move |_| {
            let Some(window) = weak_window.upgrade() else { return; };
            let name = format!("archeasy-{provider}-{}", core::unique_name());
            let result = crate::terminal(vec!["rclone".into(), "config".into(), "create".into(), name, kind.into(), "--no-output".into()]);
            let (message, detail) = match result { Ok(()) => ("Finish cloud sign-in", "Complete sign-in in the browser opened by rclone. Return here and click Refresh accounts. Existing connections are kept.".to_owned()), Err(error) => ("Could not start cloud setup", error) };
            gtk::AlertDialog::builder().message(message).detail(detail).buttons(["Close"]).build().show(Some(&window));
        });
        connections.append(&button);
    }
    connections.append(&gtk::LinkButton::with_label(
        "https://rclone.org/docs/",
        "Connection help",
    ));
    form.append(&connections);
    let folder_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let folder_label = text("Cloud folder");
    folder_label.set_wrap(false);
    folder_row.append(&folder_label);
    let folder = gtk::Entry::builder()
        .text("ArchEasy/ConfigBackups")
        .hexpand(true)
        .build();
    folder.set_widget_name("cloud-folder");
    folder_row.append(&folder);
    form.append(&folder_row);
    let source_title = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let label = text("Choose config files and folders");
    label.set_hexpand(true);
    source_title.append(&label);
    let add_files = gtk::Button::with_label("Add files…");
    let add_folder = gtk::Button::with_label("Add folder…");
    source_title.append(&add_files);
    source_title.append(&add_folder);
    form.append(&source_title);
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Filter config paths…")
        .build();
    form.append(&search);
    let sources = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sources.set_widget_name("cloud-sources");
    let source_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(160)
        .max_content_height(220)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&sources)
        .build();
    form.append(&source_scroll);
    let count = text("0 config files or folders selected");
    form.append(&count);
    let note = text(
        "Config files can contain passwords and tokens. Review your selection before uploading. Linked files are skipped. Limit: 512 MiB / 100,000 entries per backup.",
    );
    note.add_css_class("subtitle");
    form.append(&note);
    let save = gtk::Button::with_label("Save selected configs to cloud");
    save.set_widget_name("cloud-save");
    save.add_css_class("suggested-action");
    save.set_halign(gtk::Align::Start);
    save.set_sensitive(false);
    let history = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let history_title = text("Cloud backups");
    history_title.add_css_class("heading");
    history.append(&history_title);
    let backup_picker = gtk::ComboBoxText::new();
    backup_picker.set_hexpand(true);
    backup_picker.set_widget_name("cloud-backups");
    let history_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    history_row.append(&backup_picker);
    let browse = gtk::Button::with_label("List backups");
    let download = gtk::Button::with_label("Download backup…");
    history_row.append(&browse);
    history_row.append(&download);
    history.append(&history_row);
    history.append(&text("Downloads go into a new folder you choose. Review the files/ folder and manifest.json before copying anything into your live config."));
    form.append(&history);
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&form)
        .build();
    page.append(&scroll);
    let progress = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let spinner = gtk::Spinner::new();
    progress.append(&spinner);
    let status = text("Choose an account and config paths. Nothing uploads automatically.");
    status.set_widget_name("cloud-status");
    status.set_selectable(true);
    status.set_hexpand(true);
    progress.append(&status);
    progress.append(&save);
    page.append(&progress);
    let choices: Choices = Rc::new(RefCell::new(vec![]));
    let weak_save = save.downgrade();
    let weak_choices = Rc::downgrade(&choices);
    form.connect_sensitive_notify(move |form| {
        if let (Some(save), Some(choices)) = (weak_save.upgrade(), weak_choices.upgrade()) {
            save.set_sensitive(
                form.is_sensitive() && choices.borrow().iter().any(|(check, _)| check.is_active()),
            );
        }
    });
    for path in core::suggestions(&home, &config) {
        let _ = add_source(&choices, &sources, &home, path, &save, &count);
    }
    let filtered = choices.clone();
    search.connect_search_changed(move |entry| {
        let q = entry.text().to_lowercase();
        for (check, path) in filtered.borrow().iter() {
            check.set_visible(path.to_string_lossy().to_lowercase().contains(&q));
        }
    });
    for (button, is_folder) in [(add_files, false), (add_folder, true)] {
        let weak_window = window.downgrade();
        let choices = choices.clone();
        let sources = sources.clone();
        let home = home.clone();
        let save = save.clone();
        let count = count.clone();
        let status = status.clone();
        button.connect_clicked(move |_| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let choices = choices.clone();
            let sources = sources.clone();
            let home = home.clone();
            let save = save.clone();
            let count = count.clone();
            let status = status.clone();
            let dialog = gtk::FileDialog::builder()
                .title("Choose existing config files")
                .initial_folder(&gtk::gio::File::for_path(&home))
                .build();
            if is_folder {
                dialog.select_folder(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                        && let Err(error) =
                            add_source(&choices, &sources, &home, path, &save, &count)
                    {
                        status.set_text(&error);
                    }
                });
            } else {
                dialog.open_multiple(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(files) = result {
                        for file in files.iter::<gtk::gio::File>().filter_map(Result::ok) {
                            if let Some(path) = file.path()
                                && let Err(error) =
                                    add_source(&choices, &sources, &home, path, &save, &count)
                            {
                                status.set_text(&error);
                            }
                        }
                    }
                });
            }
        });
    }
    let remotes: Rc<RefCell<Vec<Remote>>> = Rc::new(RefCell::new(vec![]));
    let refresh_form = form.clone();
    let refresh_spin = spinner.clone();
    let refresh_status = status.clone();
    let refresh_picker = picker.clone();
    let remote_state = remotes.clone();
    let refresh_client = client.clone();
    refresh.connect_clicked(move |_| {
        let picker = refresh_picker.clone();
        let state = remote_state.clone();
        job(
            &refresh_form,
            &refresh_spin,
            &refresh_status,
            "Reading cloud connections…",
            {
                let client = refresh_client.clone();
                move || client.remotes()
            },
            move |values| {
                populate(&picker, &values);
                let n = values.len();
                *state.borrow_mut() = values;
                format!("{n} cloud connections found. Sign in with Connect if needed.")
            },
        );
    });
    let weak_window = window.downgrade();
    let save_form = form.clone();
    let save_spin = spinner.clone();
    let save_status = status.clone();
    let save_picker = picker.clone();
    let save_folder = folder.clone();
    let save_client = client.clone();
    save.connect_clicked(move |_| {
        let client = save_client.clone();
        let Some(window) = weak_window.upgrade() else { return; };
        let remote = match account(&save_picker) { Ok(remote) => remote, Err(error) => { save_status.set_text(&error); return; } };
        let paths: Vec<_> = choices.borrow().iter().filter(|(c,_)|c.is_active()).map(|(_,p)|p.clone()).collect();
        if paths.is_empty() { return; }
        let cloud_folder = save_folder.text().to_string();
        let details = format!("Destination: {remote}:{cloud_folder}\n\n{}\n\nOnly these selections are uploaded. Config files may include private data. SSH/GPG keys, credential stores, browser profiles, links and special files are excluded.", paths.iter().map(|p|p.display().to_string()).collect::<Vec<_>>().join("\n"));
        let form = save_form.clone(); let spinner = save_spin.clone(); let status = save_status.clone(); let home = home.clone(); let staging = glib::user_cache_dir().join("archeasy/cloud-staging");
        gtk::AlertDialog::builder().message("Save these configs to cloud?").detail(details).buttons(["Cancel", "Save backup"]).cancel_button(0).default_button(0).build().choose(Some(&window), gtk::gio::Cancellable::NONE, move |response| {
            if response != Ok(1) { return; }
            job(&form, &spinner, &status, "Preparing and uploading the selected configs…", move || {
                let snapshot = core::snapshot(&home, &paths, &staging)?;
                let target = client.upload(&snapshot, &remote, &cloud_folder)?;
                Ok((target, snapshot.files, snapshot.bytes, snapshot.skipped))
            }, |(target, files, bytes, skipped)| format!("Saved {files} files ({:.1} KiB) to {target}. Skipped {skipped} sensitive paths and links/special files.", bytes as f64 / 1024.0));
        });
    });
    let browse_form = form.clone();
    let browse_spin = spinner.clone();
    let browse_status = status.clone();
    let browse_picker = picker.clone();
    let browse_folder = folder.clone();
    let browse_backups = backup_picker.clone();
    // The listing belongs to this exact account/folder; stale listings cannot be downloaded elsewhere.
    let listed: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let browse_listed = listed.clone();
    let browse_client = client.clone();
    browse.connect_clicked(move |_| {
        let client = browse_client.clone();
        let remote = match account(&browse_picker) {
            Ok(value) => value,
            Err(error) => {
                browse_status.set_text(&error);
                return;
            }
        };
        let cloud_folder = browse_folder.text().to_string();
        let origin = (remote.clone(), cloud_folder.clone());
        let backups = browse_backups.clone();
        let listed = browse_listed.clone();
        backups.remove_all();
        *listed.borrow_mut() = None;
        job(
            &browse_form,
            &browse_spin,
            &browse_status,
            "Listing cloud backups…",
            move || client.list(&remote, &cloud_folder),
            move |names| {
                for name in &names {
                    backups.append(Some(name), name);
                }
                if !names.is_empty() {
                    backups.set_active(Some(0));
                }
                *listed.borrow_mut() = Some(origin.clone());
                format!(
                    "{} backups found. If the folder does not exist yet, save your first backup.",
                    names.len()
                )
            },
        );
    });
    let weak_window = window.downgrade();
    let download_form = form.clone();
    let download_spin = spinner.clone();
    let download_status = status.clone();
    download.connect_clicked(move |_| {
        let client = client.clone();
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let Some(name) = backup_picker.active_id() else {
            download_status.set_text("Click List backups, then choose a backup.");
            return;
        };
        let Some((remote, cloud_folder)) = listed.borrow().clone() else {
            return;
        };
        if picker.active_id().as_deref() != Some(&remote) || folder.text() != cloud_folder {
            download_status.set_text("The destination changed. Click List backups again.");
            return;
        }
        let form = download_form.clone();
        let spinner = download_spin.clone();
        let status = download_status.clone();
        gtk::FileDialog::builder()
            .title("Choose a folder for the downloaded backup")
            .build()
            .select_folder(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result
                    && let Some(output) = file.path()
                {
                    job(
                        &form,
                        &spinner,
                        &status,
                        "Downloading the cloud backup…",
                        move || client.download(&remote, &cloud_folder, &name, &output),
                        |path| {
                            format!(
                                "Downloaded to {}. Live configs have not been changed.",
                                path.display()
                            )
                        },
                    );
                }
            });
    });
    refresh.emit_clicked();
    page
}

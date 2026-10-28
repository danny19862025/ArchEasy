use super::*;
use serde_json::json;
use std::{cell::RefCell, io::Write, path::PathBuf, rc::Rc};

const HELPER: &str = include_str!("../scripts/portable-snapshot.py");
fn run(mut request: serde_json::Value) -> Result<String, String> {
    request["helper"] = json!(HELPER);
    let mut child = Command::new("python3")
        .args(["-c", HELPER])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let write = child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes());
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().to_owned());
    }
    write.map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

type Choices = Rc<RefCell<Vec<(gtk::CheckButton, PathBuf)>>>;
fn add(choices: &Choices, list: &gtk::Box, path: PathBuf) {
    if choices.borrow().iter().any(|(_, old)| old == &path) {
        return;
    }
    let check = gtk::CheckButton::with_label(&path.to_string_lossy());
    check.set_active(false);
    if let Some(label) = check.child().and_downcast::<gtk::Label>() {
        label.set_wrap(true);
        label.set_xalign(0.0);
    }
    list.append(&check);
    choices.borrow_mut().push((check, path));
}

const INSTRUCTIONS: &str = "Create a snapshot for another computer

1. Close the apps whose files you want to back up.
2. Leave ‘Include installed app lists…’ ticked to include the Arch Linux app installer and managed configs.
3. Tick the files and folders to copy. Use Add files… or Add folder… for other locations inside your home folder. Added paths must also be ticked.
4. For custom/local apps, include Projects, .local/bin, .local/share/applications and their settings/data. Include .config for settings and .var/app for Flatpak data. Only select folders you need.
5. Click Create snapshot / backup ZIP… and save it somewhere outside all selected folders, such as a USB drive or an unselected Downloads folder.
6. Wait for ‘Saved…’ before closing ArchEasy or removing the drive. Copy that ZIP to the other computer.

Make a files-only backup

Untick ‘Include installed app lists…’, tick your files/folders, then create the ZIP. Keep another copy on an external drive. The ZIP is private but is not encrypted; it may contain passwords and tokens.

Restore using ArchEasy

1. Open Snapshots & backups… at the top of the main window.
2. Click Verify & unpack backup… and choose your ZIP.
3. Choose a parent folder. ArchEasy creates a new archeasy-restored-… folder there, verifies the file checksums and unpacks the backup.
4. Open that new folder in your file manager. Review home/ and copy the files you want into your home folder. Back up current files before replacing them.
5. On another Arch Linux computer, review my-system.sh, open a terminal in the unpacked folder and run: bash my-system.sh
6. Follow the password, package and build-review prompts. Custom apps may need rebuilding or their original dependencies. Restart restored apps afterward.

Restore without ArchEasy

Extract restore-snapshot.py from the ZIP and review it. Open a terminal in that folder and run:
python3 restore-snapshot.py restore /path/to/snapshot.zip /path/to/NEW_FOLDER

Replace both paths with your actual paths; NEW_FOLDER must not exist. Then follow steps 4–6 above. The ZIP also contains README.txt with these restore instructions.

What is included

Packaged apps reinstall from current repositories, rather than preserving exact versions. Vendor or unavailable packages need their original installation source. Personal files are included only when selected. Links and special files are skipped and counted. Limit: 20 GiB / 200,000 entries. Unpacking never replaces your live files or runs the installer automatically.";

fn instructions(parent: &gtk::ApplicationWindow) {
    let window = gtk::Window::builder()
        .title("Snapshot & backup instructions")
        .transient_for(parent)
        .modal(true)
        .default_width(680)
        .default_height(660)
        .build();
    about::follow_theme(parent, &window);
    window.set_widget_name("snapshot-instructions");
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    system_tools::margins(&body);
    let text = label(INSTRUCTIONS, "");
    text.set_wrap(true);
    text.set_selectable(true);
    text.set_max_width_chars(75);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    body.append(&scroll);
    let close = gtk::Button::with_label("Close instructions");
    close.set_halign(gtk::Align::End);
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    body.append(&close);
    window.set_child(Some(&body));
    window.present();
}

#[allow(deprecated)]
fn dialog(parent: &gtk::ApplicationWindow) {
    let dialog = gtk::Dialog::builder()
        .title("Snapshots & file backups")
        .transient_for(parent)
        .modal(true)
        .default_width(720)
        .default_height(640)
        .build();
    about::follow_theme(parent, dialog.upcast_ref());
    dialog.set_widget_name("portable-snapshot-dialog");
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&body);
    let help = gtk::Button::with_label("How to create, transfer & restore a backup…");
    help.set_widget_name("snapshot-instructions-button");
    help.set_halign(gtk::Align::Start);
    let weak = parent.downgrade();
    help.connect_clicked(move |_| {
        if let Some(parent) = weak.upgrade() {
            instructions(&parent);
        }
    });
    body.append(&help);
    let steps = label(
        "1. Choose apps and files below.  2. Create a ZIP outside the selected folders.  3. Copy it to the other computer and use Verify & unpack backup…",
        "subtitle",
    );
    steps.set_wrap(true);
    body.append(&steps);
    let note = label(
        "Save one ZIP for another computer or keep a local file backup. Close apps before capture. Select your app data, local programs and personal files below. SSH/GPG keys, credential stores, browser profiles, links and special files are excluded. Limit: 20 GiB / 200,000 entries. ZIP contents may contain passwords; keep the backup private.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let apps = gtk::CheckButton::with_label(
        "Include installed app lists, managed configs and my-system.sh installer (Arch Linux)",
    );
    apps.set_widget_name("snapshot-include-apps");
    apps.set_active(true);
    body.append(&apps);
    let hint = label(
        "Packaged apps are reinstalled from current repositories. Select Projects and local app folders for custom programs. Untick app lists for a files-only backup. No personal files are selected automatically.",
        "subtitle",
    );
    hint.set_wrap(true);
    body.append(&hint);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list.set_widget_name("snapshot-sources");
    let choices: Choices = Rc::new(RefCell::new(vec![]));
    let home = glib::home_dir();
    for name in [
        "Documents",
        "Pictures",
        "Desktop",
        "Projects",
        ".config",
        ".local/bin",
        ".local/share/applications",
        ".local/share",
        ".var/app",
    ] {
        let path = home.join(name);
        if path.exists() && !path.is_symlink() {
            add(&choices, &list, path);
        }
    }
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(180)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    body.append(&scroll);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    for folder in [false, true] {
        let button = gtk::Button::with_label(if folder {
            "Add folder…"
        } else {
            "Add files…"
        });
        let weak = parent.downgrade();
        let choices = choices.clone();
        let list = list.clone();
        button.connect_clicked(move |_| {
            let Some(parent) = weak.upgrade() else {
                return;
            };
            let chooser = gtk::FileDialog::builder()
                .title("Select files inside your home folder")
                .initial_folder(&gtk::gio::File::for_path(glib::home_dir()))
                .build();
            let choices = choices.clone();
            let list = list.clone();
            if folder {
                chooser.select_folder(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        add(&choices, &list, path);
                    }
                });
            } else {
                chooser.open_multiple(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(files) = result {
                        for file in files.iter::<gtk::gio::File>().filter_map(Result::ok) {
                            if let Some(path) = file.path() {
                                add(&choices, &list, path);
                            }
                        }
                    }
                });
            }
        });
        buttons.append(&button);
    }
    body.append(&buttons);
    let status = label(
        "Choose files and create a ZIP, or unpack an existing snapshot for review.",
        "subtitle",
    );
    status.set_widget_name("snapshot-status");
    status.set_wrap(true);
    status.set_selectable(true);
    body.append(&status);
    let create = gtk::Button::with_label("Create snapshot / backup ZIP…");
    create.set_widget_name("snapshot-create");
    create.add_css_class("suggested-action");
    body.append(&create);
    let restore = gtk::Button::with_label("Verify & unpack backup…");
    restore.set_widget_name("snapshot-unpack");
    body.append(&restore);
    dialog.content_area().append(&body);
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.connect_response(|dialog, _| dialog.close());
    let weak = parent.downgrade();
    let status_create = status.clone();
    let form = body.clone();
    create.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; };
        let paths: Vec<_> = choices.borrow().iter().filter(|(c, _)| c.is_active()).map(|(_, p)| p.clone()).collect();
        let include_apps = apps.is_active();
        if paths.is_empty() && !include_apps {
            status_create.set_text("Select files/folders or include installed app lists first.");
            return;
        }
        let status = status_create.clone();
        let form = form.clone();
        let name = format!("archeasy-snapshot-{}.zip", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
        gtk::FileDialog::builder().title("Save portable snapshot outside selected folders").initial_name(name).build().save(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return; };
            let Some(destination) = file.path() else { return; };
            form.set_sensitive(false);
            status.set_text("Creating snapshot… Keep ArchEasy open until it finishes.");
            let done = status.clone();
            system_tools::task_with_finish(&status, move || {
                let script = if include_apps { system_export::inventory("")? } else { String::new() };
                run(json!({"home":glib::home_dir(), "destination":destination, "paths":paths, "script":script}))
            }, move |message| done.set_text(&message), move || form.set_sensitive(true));
        });
    });
    let weak = parent.downgrade();
    let form = body.clone();
    restore.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; };
        let weak = parent.downgrade();
        let status = status.clone();
        let form = form.clone();
        gtk::FileDialog::builder().title("Choose an ArchEasy snapshot ZIP").build().open(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return; };
            let Some(source) = file.path() else { return; };
            let Some(parent) = weak.upgrade() else { return; };
            gtk::FileDialog::builder().title("Choose a parent folder for unpacking").build().select_folder(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                let Ok(folder) = result else { return; };
                let Some(folder) = folder.path() else { return; };
                let destination = folder.join(format!("archeasy-restored-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
                form.set_sensitive(false);
                status.set_text("Verifying and unpacking backup… Keep ArchEasy open.");
                let done = status.clone();
                system_tools::task_with_finish(&status, move || run(json!({"action":"restore", "source":source, "destination":destination})), move |message| done.set_text(&message), move || form.set_sensitive(true));
            });
        });
    });
    dialog.present();
}

pub fn button(page: &gtk::Box, parent: &gtk::ApplicationWindow) {
    let button = gtk::Button::with_label("Snapshots & backups…");
    button.set_widget_name("portable-snapshots");
    button.set_halign(gtk::Align::Start);
    let weak = parent.downgrade();
    button.connect_clicked(move |_| {
        if let Some(parent) = weak.upgrade() {
            dialog(&parent);
        }
    });
    page.append(&button);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a graphical session"]
    fn snapshot_dialog_selection_and_empty_backup_guard() {
        fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
            if root.widget_name() == name {
                return Some(root.clone());
            }
            let mut child = root.first_child();
            while let Some(widget) = child {
                if let Some(found) = find(&widget, name) {
                    return Some(found);
                }
                child = widget.next_sibling();
            }
            None
        }
        gtk::init().unwrap();
        let app = gtk::Application::builder()
            .application_id("io.github.archeasy.SnapshotTest")
            .build();
        app.register(gtk::gio::Cancellable::NONE).unwrap();
        let parent = gtk::ApplicationWindow::builder().application(&app).build();
        let page = system_tools::page(&parent);
        parent.set_child(Some(&page));
        find(page.upcast_ref(), "portable-snapshots")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        let window = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.widget_name() == "portable-snapshot-dialog")
            .unwrap();
        find(window.upcast_ref(), "snapshot-instructions-button")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        let guide = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.widget_name() == "snapshot-instructions")
            .unwrap();
        assert!(guide.is_visible());
        guide.close();
        let sources = find(window.upcast_ref(), "snapshot-sources").unwrap();
        let mut child = sources.first_child();
        while let Some(widget) = child {
            assert!(
                !widget
                    .clone()
                    .downcast::<gtk::CheckButton>()
                    .unwrap()
                    .is_active()
            );
            child = widget.next_sibling();
        }
        find(window.upcast_ref(), "snapshot-include-apps")
            .unwrap()
            .downcast::<gtk::CheckButton>()
            .unwrap()
            .set_active(false);
        find(window.upcast_ref(), "snapshot-create")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        assert!(
            find(window.upcast_ref(), "snapshot-status")
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Select files/folders")
        );
        window.close();
        parent.close();
    }
}

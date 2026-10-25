#![allow(deprecated)] // GTK 4.10-compatible modal review dialogs.
use super::workflow::Plan;
use super::*;
use serde_json::{Value, json};
fn presets_dir() -> PathBuf {
    glib::user_data_dir().join("archeasy/presets")
}
pub(super) fn import_file(
    path: &Path,
    name: &str,
    root: &Path,
    directory: &Path,
) -> Result<Plan, String> {
    if name.trim().is_empty() || name.len() > 100 {
        return Err("Choose a preset name of 1–100 characters.".into());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Choose a file inside your config folder.")?
        .to_str()
        .ok_or("The config path must be UTF-8")?;
    // Read without creating parents and refuse links, including linked parent folders.
    let parent = path.parent().ok_or("Invalid file path")?;
    let mut check = root.to_path_buf();
    for component in parent
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .components()
    {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err("Invalid config path".into());
        }
        check.push(component);
        if fs::symlink_metadata(&check)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Choose the original config file; linked parents are excluded.".into());
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err("Import a regular text config file up to 1 MiB; links are excluded.".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if std::str::from_utf8(&bytes).is_err() || bytes.contains(&0) {
        return Err("Import supports text configs only.".into());
    }
    let plan = Plan {
        id: format!("custom-{}", stamp()),
        name: name.trim().to_owned(),
        files: vec![(relative.to_owned(), bytes)],
        dependencies: vec![],
    };
    let plan = Plan::parse(&plan.json())?;
    let target = directory.join(&plan.id);
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    write_atomic(
        &target.join("preset.json"),
        plan.json().to_string().as_bytes(),
        0o600,
    )?;
    Ok(plan)
}
fn load_presets(directory: &Path) -> Vec<Plan> {
    let mut values: Vec<_> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path().join("preset.json");
            let meta = fs::symlink_metadata(&path).ok()?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 8 * 1024 * 1024 {
                return None;
            }
            let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            Plan::parse(&value).ok()
        })
        .collect();
    values.sort_by(|a, b| a.name.cmp(&b.name));
    values
}
pub(super) fn gh_program() -> PathBuf {
    if super::super::available("gh") {
        PathBuf::from("gh")
    } else {
        glib::home_dir().join(".local/bin/gh")
    }
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn export_files(plan: &Plan, root: &Path) -> Result<(Stage, Vec<PathBuf>), String> {
    let directory = root.join(stamp());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let stage = Stage(directory.clone());
    let mut files = vec![];
    let mut mapping = vec![];
    for (index, (path, bytes)) in plan.files.iter().enumerate() {
        let is_text = std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0);
        let filename = format!(
            "{:02}-{}{}",
            index + 1,
            path.replace('/', "_"),
            if is_text { "" } else { ".b64" }
        );
        let target = directory.join(&filename);
        let contents = if is_text {
            bytes.clone()
        } else {
            glib::base64_encode(bytes).as_bytes().to_vec()
        };
        write_atomic(&target, &contents, 0o600)?;
        mapping.push(json!({"gist_file":filename,"config_path":path,"encoding":if is_text{"utf-8"}else{"base64"}}));
        files.push(target);
    }
    let manifest = directory.join("archeasy-preset.json");
    write_atomic(&manifest,json!({"format_version":1,"name":plan.name,"files":mapping,"destination":"Paths are relative to XDG_CONFIG_HOME. Review before applying."}).to_string().as_bytes(),0o600)?;
    files.push(manifest);
    Ok((stage, files))
}
pub(super) fn publish(
    plan: &Plan,
    public: bool,
    program: &Path,
    staging: &Path,
) -> Result<String, String> {
    let (_stage, files) = export_files(plan, staging)?;
    let mut command = std::process::Command::new(program);
    command
        .args([
            "gist",
            "create",
            "--desc",
            &format!("ArchEasy preset: {}", plan.name),
        ])
        .env("GH_PROMPT_DISABLED", "1");
    if public {
        command.arg("--public");
    }
    command.arg("--").args(files);
    let output = command
        .output()
        .map_err(|e| format!("GitHub CLI unavailable: {e}. Use Install GitHub CLI first."))?;
    if !output.status.success() {
        return Err(format!(
            "Gist export failed. Sign in with the GitHub button and retry. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !url.starts_with("https://gist.github.com/") || url.contains(char::is_whitespace) {
        return Err(
            "GitHub CLI did not return a valid Gist URL. Check your Gists before retrying.".into(),
        );
    }
    Ok(url)
}
pub(super) fn export_dialog(parent: &gtk::ApplicationWindow, plan: Plan) {
    #[allow(deprecated)]
    let dialog = gtk::Dialog::builder()
        .title(format!("Export to GitHub Gist · {}", plan.name))
        .transient_for(parent)
        .modal(true)
        .default_width(800)
        .default_height(600)
        .build();
    dialog.set_widget_name("gist-export-dialog");
    #[allow(deprecated)]
    {
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Publish Gist", gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Cancel);
    }
    let body = dialog.content_area();
    body.set_spacing(12);
    for setter in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        setter(&body, 14);
    }
    let note = super::super::label(
        "Review every file below for passwords, tokens, and private details before publishing. Secret Gists are unlisted, not private: anyone with the link can read them. Binary assets are exported as base64 with a manifest.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let connection = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let sign_in = gtk::Button::with_label("GitHub sign-in");
    sign_in.connect_clicked(move |_| {
        let _ = super::super::terminal(vec![
            gh_program().to_string_lossy().into_owned(),
            "auth".into(),
            "login".into(),
            "--hostname".into(),
            "github.com".into(),
            "--web".into(),
            "--scopes".into(),
            "gist".into(),
            "--git-protocol".into(),
            "https".into(),
        ]);
    });
    connection.append(&sign_in);
    let install = gtk::Button::with_label("Install GitHub CLI");
    install.connect_clicked(move |_| {
        let _ = super::super::terminal(
            ["sudo", "pacman", "-Syu", "--needed", "github-cli"]
                .map(str::to_owned)
                .to_vec(),
        );
    });
    install.set_visible(!(super::super::available("gh") || gh_program().is_file()));
    connection.append(&install);
    body.append(&connection);
    let public = gtk::CheckButton::with_label("Publish publicly (unchecked = secret / unlisted)");
    body.append(&public);
    let view = gtk::TextView::builder()
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let mut contents = String::new();
    for (name, bytes) in &plan.files {
        contents.push_str(&format!(
            "──── {name} ────\n{}\n\n",
            std::str::from_utf8(bytes).unwrap_or("[Binary asset: exported as base64]")
        ));
    }
    view.buffer().set_text(&contents);
    body.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&view)
            .build(),
    );
    let status = super::super::label("", "subtitle");
    status.set_wrap(true);
    status.set_selectable(true);
    body.append(&status);
    #[allow(deprecated)]
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept {
            dialog.close();
            return;
        }
        let public = public.is_active();
        let plan = plan.clone();
        let program = gh_program();
        let staging = glib::user_cache_dir().join("archeasy/gist-export");
        dialog.set_response_sensitive(gtk::ResponseType::Accept, false);
        dialog.set_response_sensitive(gtk::ResponseType::Cancel, false);
        dialog.set_deletable(false);
        status.set_text("Publishing the reviewed preset…");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(publish(&plan, public, &program, &staging));
        });
        let dialog = dialog.downgrade();
        let status = status.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let Some(dialog) = dialog.upgrade() else {
                return glib::ControlFlow::Break;
            };
            match rx.try_recv() {
                Ok(result) => {
                    dialog.set_deletable(true);
                    dialog.set_response_sensitive(gtk::ResponseType::Cancel, true);
                    match result {
                        Ok(url) => {
                            status.set_text(&format!("Published: {url}\nYou can copy this URL."))
                        }
                        Err(e) => {
                            status.set_text(&e);
                            dialog.set_response_sensitive(gtk::ResponseType::Accept, true);
                        }
                    }
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    status.set_text(
                        "Export task stopped unexpectedly. Check your Gists before retrying.",
                    );
                    dialog.set_deletable(true);
                    dialog.set_response_sensitive(gtk::ResponseType::Cancel, true);
                    glib::ControlFlow::Break
                }
            }
        });
    });
    dialog.present();
}
pub(super) fn add_custom_cards(
    list: &gtk::Box,
    parent: &gtk::ApplicationWindow,
    status: &gtk::Label,
    refresh: &gtk::Button,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for plan in load_presets(&presets_dir()) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("card");
        card.append(&super::super::label(
            &format!("{} · Imported preset", plan.name),
            "name",
        ));
        let target = super::super::label(
            &format!(
                "Target: {}",
                plan.files
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "subtitle",
        );
        target.set_wrap(true);
        card.append(&target);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let apply = gtk::Button::with_label("Diff & apply");
        let weak = parent.downgrade();
        let p = plan.clone();
        let s = status.clone();
        let r = refresh.clone();
        apply.connect_clicked(move |_| {
            if let Some(parent) = weak.upgrade() {
                workflow::review_dialog(&parent, p.clone(), workflow::Mode::Direct, &s, &r, None);
            }
        });
        actions.append(&apply);
        let export = gtk::Button::with_label("Export to Gist");
        let weak = parent.downgrade();
        let p = plan.clone();
        export.connect_clicked(move |_| {
            if let Some(parent) = weak.upgrade() {
                export_dialog(&parent, p.clone());
            }
        });
        actions.append(&export);
        let restore = gtk::Button::with_label("Restore backup");
        let weak = parent.downgrade();
        let s = status.clone();
        restore.connect_clicked(move |_| {
            if let Some(parent) = weak.upgrade() {
                if let Some(backup) = latest_backup(&plan.id, &backups_dir()) {
                    match restore_snapshot(&backup, &glib::user_config_dir()) {
                        Ok(()) => s.set_text(
                            "Restored the config saved before this imported preset was applied.",
                        ),
                        Err(e) => notice(&parent, "Restore failed", &e),
                    }
                } else {
                    notice(&parent, "No backup yet", "Apply this preset first.");
                }
            }
        });
        actions.append(&restore);
        card.append(&actions);
        list.append(&card);
    }
}
pub(super) fn add_import_button(
    folders: &gtk::Box,
    list: &gtk::Box,
    parent: &gtk::ApplicationWindow,
    status: &gtk::Label,
    refresh: &gtk::Button,
) {
    let import = gtk::Button::with_label("Import current config…");
    import.set_widget_name("import-config-preset");
    folders.append(&import);
    let weak = parent.downgrade();
    let list = list.clone();
    let status = status.clone();
    let refresh = refresh.clone();
    import.connect_clicked(move |_|{
        let Some(parent)=weak.upgrade()else{return;};let weak=parent.downgrade();let list=list.clone();let status=status.clone();let refresh=refresh.clone();
        gtk::FileDialog::builder().title("Import a current config file as a preset").initial_folder(&gtk::gio::File::for_path(glib::user_config_dir())).build().open(Some(&parent),gtk::gio::Cancellable::NONE,move|result|{
            if let Ok(file)=result && let Some(path)=file.path() && let Some(parent)=weak.upgrade(){
                #[allow(deprecated)]let dialog=gtk::Dialog::builder().title("Name your imported preset").transient_for(&parent).modal(true).default_width(480).build();dialog.set_widget_name("import-preset-name-dialog");
                let entry=gtk::Entry::builder().text(path.file_stem().and_then(|n|n.to_str()).unwrap_or("Custom config")).max_length(100).build();entry.set_widget_name("import-preset-name");
                let body=dialog.content_area();body.set_spacing(12);body.set_margin_top(16);body.set_margin_bottom(16);body.set_margin_start(16);body.set_margin_end(16);body.append(&super::super::label(&path.display().to_string(),"subtitle"));body.append(&entry);
                #[allow(deprecated)]{dialog.add_button("Cancel",gtk::ResponseType::Cancel);dialog.add_button("Save preset",gtk::ResponseType::Accept);}
                let weak=parent.downgrade();#[allow(deprecated)]dialog.connect_response(move|dialog,response|{if response!=gtk::ResponseType::Accept{dialog.close();return;}
if let Some(parent)=weak.upgrade(){match import_file(&path,&entry.text(),&glib::user_config_dir(),&presets_dir()){Ok(plan)=>{status.set_text(&format!("Saved {} as a local preset. Your current file was not changed.",plan.name));add_custom_cards(&list,&parent,&status,&refresh);dialog.close();},Err(e)=>notice(&parent,"Could not import config",&e)}}});dialog.present();
            }
        });
    });
}

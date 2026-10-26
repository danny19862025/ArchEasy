#![allow(deprecated)]
use super::*;
const PERMISSIONS: &[(&str, &str, &str, &str)] = &[
    (
        "Audio · speakers and microphone",
        "sockets",
        "pulseaudio",
        "socket",
    ),
    ("Network", "shared", "network", "share"),
    ("Home folder", "filesystems", "home", "filesystem"),
    ("All host files", "filesystems", "host", "filesystem"),
    ("Documents", "filesystems", "xdg-documents", "filesystem"),
    ("Downloads", "filesystems", "xdg-download", "filesystem"),
    ("Devices · includes cameras", "devices", "all", "device"),
];
fn values(text: &str, group: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix(&format!("{group}=")))
        .flat_map(|line| line.split(';'))
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}
pub fn permission_args(
    id: &str,
    changes: &[(String, String, bool)],
) -> Result<Vec<String>, String> {
    if !valid_id(id) || id.contains('/') {
        return Err("Invalid Flatpak application ID".into());
    }
    let mut args = vec!["override".into(), "--user".into()];
    for (kind, permission, allow) in changes {
        if !PERMISSIONS
            .iter()
            .any(|(_, _, value, flag)| flag == kind && value == permission)
        {
            return Err("Unknown Flatpak permission".into());
        }
        let flag = if *allow {
            kind.clone()
        } else if kind == "share" {
            "unshare".into()
        } else {
            format!("no{kind}")
        };
        args.push(format!("--{flag}={permission}"));
    }
    args.push(id.into());
    Ok(args)
}
pub fn open(id: &str, scope: &str) {
    if let Some(parent) = gtk::gio::Application::default()
        .and_downcast::<gtk::Application>()
        .and_then(|a| a.active_window())
        .and_downcast::<gtk::ApplicationWindow>()
    {
        dialog(&parent, id, scope);
    }
}
fn dialog(parent: &gtk::ApplicationWindow, id: &str, scope: &str) {
    if !valid_id(id) || !["--user", "--system"].contains(&scope) {
        show_error("Invalid Flatpak app or installation scope");
        return;
    }
    let dialog = gtk::Dialog::builder()
        .title(format!("Flatpak permissions · {id}"))
        .transient_for(parent)
        .modal(true)
        .default_width(680)
        .default_height(640)
        .build();
    crate::about::follow_theme(parent, dialog.upcast_ref());
    dialog.set_widget_name("flatpak-permissions-dialog");
    let body = dialog.content_area();
    system_tools::margins(&body);
    let note = label(
        "Choose Override beside the permissions you want to change, then allow or deny access. Other overrides stay unchanged. Audio access covers speakers and microphone together. Restart the app after saving. Folder grants can overlap, and file-picker/camera portals have their own permissions.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 8);
    form.set_sensitive(false);
    body.append(&form);
    let mut controls = vec![];
    for (title, group, value, flag) in PERMISSIONS {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let title = label(title, "");
        title.set_hexpand(true);
        row.append(&title);
        let change = gtk::CheckButton::with_label("Override");
        row.append(&change);
        let toggle = gtk::Switch::new();
        toggle.set_sensitive(false);
        toggle.set_valign(gtk::Align::Center);
        row.append(&toggle);
        let weak = toggle.downgrade();
        change.connect_toggled(move |check| {
            if let Some(toggle) = weak.upgrade() {
                toggle.set_sensitive(check.is_active());
            }
        });
        controls.push((
            change,
            toggle,
            (*group).to_string(),
            (*value).to_string(),
            (*flag).to_string(),
        ));
        form.append(&row);
    }
    let view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&view)
            .min_content_height(130)
            .vexpand(true)
            .build(),
    );
    let status = label("Loading app permissions…", "subtitle");
    status.set_wrap(true);
    body.append(&status);
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.add_button("Apply selected overrides", gtk::ResponseType::Apply);
    dialog.set_response_sensitive(gtk::ResponseType::Apply, false);
    let app = id.to_owned();
    let installation = scope.to_owned();
    let toggles = controls.clone();
    let form_ready = form.clone();
    let ready = dialog.downgrade();
    let status_done = status.clone();
    system_tools::task(
        &status,
        move || {
            let metadata = output(
                "flatpak",
                &["info", &installation, "--show-permissions", &app],
            )?;
            let global_system =
                output("flatpak", &["override", "--system", "--show"]).unwrap_or_default();
            let app_system =
                output("flatpak", &["override", "--system", "--show", &app]).unwrap_or_default();
            let global_user =
                output("flatpak", &["override", "--user", "--show"]).unwrap_or_default();
            let app_user = output("flatpak", &["override", "--user", "--show", &app])?;
            Ok(vec![
                metadata,
                global_system,
                app_system,
                global_user,
                app_user,
            ])
        },
        move |layers| {
            for (_, toggle, group, value, _) in &toggles {
                let mut allow = false;
                for layer in &layers {
                    for token in values(layer, group) {
                        if token == *value || token.starts_with(&format!("{value}:")) {
                            allow = true;
                        } else if token == format!("!{value}") {
                            allow = false;
                        }
                    }
                }
                toggle.set_active(allow);
            }
            let names = [
                "App permissions",
                "System global overrides",
                "System app overrides",
                "User global overrides",
                "User app overrides",
            ];
            view.buffer().set_text(
                &names
                    .into_iter()
                    .zip(layers)
                    .map(|(n, t)| format!("── {n} ──\n{t}\n"))
                    .collect::<String>(),
            );
            form_ready.set_sensitive(true);
            if let Some(dialog) = ready.upgrade() {
                dialog.set_response_sensitive(gtk::ResponseType::Apply, true);
            }
            status_done.set_text("Switches summarize listed grants. Select Override to change a permission; off denies, on allows.");
        },
    );
    let app = id.to_owned();
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Apply { dialog.close(); return; }
        let changes: Vec<_> = controls.iter().filter(|(change, _, _, _, _)| change.is_active()).map(|(_, toggle, _, value, flag)| (flag.clone(), value.clone(), toggle.is_active())).collect();
        if changes.is_empty() { status.set_text("Select Override for at least one permission."); return; }
        let args = match permission_args(&app, &changes) { Ok(args) => args, Err(e) => { status.set_text(&e); return; } };
        form.set_sensitive(false); dialog.set_response_sensitive(gtk::ResponseType::Apply, false);
        let weak = dialog.downgrade(); let form = form.clone(); let status_done = status.clone();
        system_tools::task_with_finish(&status, move || output("flatpak", &args.iter().map(String::as_str).collect::<Vec<_>>()), move |_| { status_done.set_text("Permissions saved for your account. Restart this app to use them. Close and reopen this screen to reload the permission layers."); }, move || { form.set_sensitive(true); if let Some(dialog)=weak.upgrade(){dialog.set_response_sensitive(gtk::ResponseType::Apply,true);} });

    });
    dialog.present();
}
pub fn page(parent: &gtk::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&label("Flatpak permissions", "name"));
    let refresh = gtk::Button::with_label("Refresh installed Flatpaks");
    refresh.set_halign(gtk::Align::Start);
    page.append(&refresh);
    let picker = gtk::ComboBoxText::new();
    picker.set_widget_name("flatpak-permissions-apps");
    page.append(&picker);
    let edit = gtk::Button::with_label("Edit selected app permissions");
    edit.set_halign(gtk::Align::Start);
    page.append(&edit);
    let status = label(
        "Install an app, then refresh to manage its permissions. Both user and system installations are listed; edits are per-user.",
        "subtitle",
    );
    status.set_wrap(true);
    page.append(&status);
    let choices = picker.clone();
    let s = status.clone();
    refresh.connect_clicked(move |_| {
        let choices = choices.clone();
        choices.remove_all();
        let done = s.clone();
        system_tools::task(
            &s,
            || {
                let mut apps = vec![];
                for scope in ["--user", "--system"] {
                    let text = output(
                        "flatpak",
                        &["list", scope, "--app", "--columns=application,name"],
                    )?;
                    for line in text.lines() {
                        let mut fields = line.split('\t');
                        if let Some(id) = fields.next()
                            && valid_id(id)
                        {
                            apps.push((
                                scope.to_owned(),
                                id.to_owned(),
                                fields.next().unwrap_or(id).to_owned(),
                            ));
                        }
                    }
                }
                Ok(apps)
            },
            move |apps| {
                for (scope, id, name) in &apps {
                    choices.append(
                        Some(&format!("{scope}|{id}")),
                        &format!("{name} · {id} ({scope})"),
                    );
                }
                choices.set_active(Some(0));
                done.set_text(&format!(
                    "{} apps found. Select an app to edit its permissions.",
                    apps.len()
                ));
            },
        );
    });
    let weak = parent.downgrade();
    edit.connect_clicked(move |_| {
        if let Some(parent) = weak.upgrade()
            && let Some(choice) = picker.active_id()
            && let Some((scope, id)) = choice.split_once('|')
        {
            dialog(&parent, id, scope);
        }
    });
    page.connect_map(move |_| refresh.emit_clicked());
    page
}
#[cfg(test)]
mod tests {
    #[test]
    fn overrides_are_scoped_and_preserve_unselected_permissions() {
        assert_eq!(
            super::permission_args(
                "org.example.App",
                &[("socket".into(), "pulseaudio".into(), false)]
            )
            .unwrap(),
            [
                "override",
                "--user",
                "--nosocket=pulseaudio",
                "org.example.App"
            ]
        );
        assert!(super::permission_args("--help", &[]).is_err());
    }
    #[test]
    fn real_flatpak_accepts_overrides_in_an_isolated_data_folder() {
        if !crate::available("flatpak") {
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "archeasy-permissions-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let changes = super::PERMISSIONS
            .iter()
            .map(|(_, _, value, flag)| (flag.to_string(), value.to_string(), false))
            .collect::<Vec<_>>();
        let args = super::permission_args("org.archeasy.PermissionTest", &changes).unwrap();
        let out = std::process::Command::new("flatpak")
            .args(args)
            .env("XDG_DATA_HOME", &directory)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = std::fs::read_to_string(
            directory.join("flatpak/overrides/org.archeasy.PermissionTest"),
        )
        .unwrap();
        assert!(text.contains("!pulseaudio"));
        assert!(text.contains("!network"));
        assert!(text.contains("!home"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

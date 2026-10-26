#![allow(deprecated)] // GTK 4.10-compatible modal review dialogs.
use super::*;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub id: String,
    pub name: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub dependencies: Vec<String>,
    pub managed_parents: Vec<(String, String)>,
}
impl Plan {
    pub fn builtin(preset: &Preset) -> Self {
        Self {
            id: preset.id.into(),
            name: preset.name.into(),
            files: preset
                .files
                .iter()
                .map(|(n, b)| (n.to_string(), b.to_vec()))
                .collect(),
            dependencies: dependencies(preset).iter().map(|s| s.to_string()).collect(),
            managed_parents: vec![],
        }
    }
    pub fn stack() -> Self {
        let mut files = vec![];
        for index in [2, 3, 4, 5, 0, 6] {
            files.extend(Self::builtin(&PRESETS[index]).files);
        }
        files.push(("fish/conf.d/archeasy-starship.fish".into(),b"# Starship integration for the terminal stack.\nif status is-interactive; and type -q starship\n    starship init fish | source\nend\n".to_vec()));
        Self {
            managed_parents: vec![],
            id: "terminal-stack".into(),
            name: "Danny’s Full Setup".into(),
            files,
            dependencies: PRESETS[..7]
                .iter()
                .flat_map(|p| dependencies(p).iter().map(|s| s.to_string()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }
    pub fn json(&self) -> Value {
        json!({"id":self.id,"name":self.name,"files":self.files.iter().map(|(p,b)|json!({"path":p,"bytes":b})).collect::<Vec<_>>(),"dependencies":self.dependencies})
    }
    pub fn parse(value: &Value) -> Result<Self, String> {
        let id = value["id"].as_str().ok_or("Missing preset ID")?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        {
            return Err("Invalid preset ID".into());
        }
        let name = value["name"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() < 300)
            .ok_or("Invalid preset name")?;
        let mut files = vec![];
        let mut seen = BTreeSet::new();
        let mut total = 0;
        for entry in value["files"].as_array().ok_or("Missing files")? {
            let path = entry["path"].as_str().ok_or("Invalid target")?;
            if path.is_empty()
                || !Path::new(path)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                || Path::new(path)
                    .components()
                    .any(|c| c.as_os_str() == ".git")
                || !seen.insert(path.to_owned())
            {
                return Err("Invalid or duplicate preset path".into());
            }
            let bytes: Vec<u8> =
                serde_json::from_value(entry["bytes"].clone()).map_err(|e| e.to_string())?;
            total += bytes.len();
            if total > 8 * 1024 * 1024 || files.len() >= 100 {
                return Err("Preset exceeds 8 MiB / 100 files".into());
            }
            files.push((path.to_owned(), bytes));
        }
        if files.is_empty() {
            return Err("Preset contains no files".into());
        }
        let dependencies = value["dependencies"]
            .as_array()
            .map(|d| {
                d.iter()
                    .filter_map(Value::as_str)
                    .filter(|d| {
                        [
                            "fish",
                            "starship",
                            "ghostty",
                            "fastfetch",
                            "kitty",
                            "Hyprland",
                            "nvim",
                            "tmux",
                            "waybar",
                            "rofi",
                            "wofi",
                            "btop",
                            "thunar",
                            "swaync",
                            "xdg-desktop-portal-hyprland",
                            "hyprpolkitagent",
                            "wl-copy",
                            "cliphist",
                            "grim",
                            "slurp",
                            "wpctl",
                            "brightnessctl",
                        ]
                        .contains(d)
                    })
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            id: id.into(),
            name: name.into(),
            files,
            dependencies,
            managed_parents: vec![],
        })
    }
}
pub(super) fn dependencies(preset: &Preset) -> &'static [&'static str] {
    match preset.id {
        "fish" => &["fish", "starship"],
        "neovim" => &["nvim"],
        "tmux" => &["tmux"],
        "waybar" => &["waybar"],
        "rofi" => &["rofi"],
        "wofi" => &["wofi"],
        "btop" => &["btop"],
        "starship" => &["starship"],
        "ghostty" => &["ghostty"],
        "kitty" => &["kitty"],
        "hyprland" => &[
            "Hyprland",
            "kitty",
            "wofi",
            "thunar",
            "waybar",
            "swaync",
            "xdg-desktop-portal-hyprland",
            "hyprpolkitagent",
            "wl-copy",
            "cliphist",
            "grim",
            "slurp",
            "wpctl",
            "brightnessctl",
        ],
        _ => &["fastfetch"],
    }
}
fn preflight(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || !Path::new(name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Invalid config path".into());
    }
    let mut parent = root.to_path_buf();
    for part in Path::new(name)
        .parent()
        .unwrap_or(Path::new(""))
        .components()
    {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(format!(
                    "{} is a linked or non-directory parent",
                    parent.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(root.join(name))
}
fn state(root: &Path, name: &str, approved: &[(String, String)]) -> Result<Value, String> {
    let path = if approved.is_empty() {
        preflight(root, name)?
    } else {
        conflicts::preflight(root, name, approved)?
    };
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({"kind":"missing"})),
        Err(e) => Err(e.to_string()),
        Ok(m) if m.file_type().is_symlink() => {
            Ok(json!({"kind":"symlink","link":fs::read_link(path).map_err(|e|e.to_string())?}))
        }
        Ok(m) if m.is_file() && m.len() <= 8 * 1024 * 1024 => Ok(
            json!({"kind":"file","bytes":fs::read(path).map_err(|e|e.to_string())?,"mode":m.permissions().mode() & 0o777}),
        ),
        _ => Err(format!(
            "{} is not a regular config file or exceeds 8 MiB",
            path.display()
        )),
    }
}
pub(super) fn reviewed(plan: &Plan, root: &Path) -> Result<Vec<Value>, String> {
    plan.files
        .iter()
        .map(|(n, _)| state(root, n, &plan.managed_parents))
        .collect()
}
pub(super) fn apply_reviewed(
    plan: &Plan,
    before: &[Value],
    root: &Path,
    backups: &Path,
) -> Result<PathBuf, String> {
    if reviewed(plan, root)? != before {
        return Err(
            "Configs changed after the preview. Open the diff again before applying.".into(),
        );
    }
    apply_plan(plan, root, backups)
}
fn rendered(bytes: &[u8], name: &str, root: &Path) -> Vec<u8> {
    if name.ends_with(".png") {
        bytes.to_vec()
    } else {
        String::from_utf8_lossy(bytes)
            .replace("__CONFIG_DIR__", &root.to_string_lossy())
            .into_bytes()
    }
}
fn content(state: &Value) -> String {
    match state["kind"].as_str() {
        Some("missing") => "[File does not exist yet]\n".into(),
        Some("symlink") => format!(
            "[Symbolic link to {} — the link will be backed up and replaced locally]\n",
            state["link"].as_str().unwrap_or("?")
        ),
        _ => {
            let bytes: Vec<u8> = serde_json::from_value(state["bytes"].clone()).unwrap_or_default();
            readable(&bytes)
        }
    }
}
fn readable(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) if !s.contains('\0') => s.to_owned(),
        _ => format!("[Binary content: {} bytes]", bytes.len()),
    }
}
// Exact line matching with a bounded LCS matrix; large files retain the complete before/after view.
fn changed_lines(old: &str, new: &str) -> (Vec<usize>, Vec<usize>) {
    let a: Vec<_> = old.lines().collect();
    let b: Vec<_> = new.lines().collect();
    if a.len().saturating_mul(b.len()) > 1_000_000 {
        return (vec![], vec![]);
    }
    let w = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * w];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * w + j] = if a[i] == b[j] {
                table[(i + 1) * w + j + 1] + 1
            } else {
                table[(i + 1) * w + j].max(table[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut left, mut right) = (vec![], vec![]);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || table[i * w + j + 1] >= table[(i + 1) * w + j]) {
            right.push(j);
            j += 1;
        } else {
            left.push(i);
            i += 1;
        }
    }
    (left, right)
}
fn diff_view(text: &str, lines: &[usize], color: &str) -> gtk::ScrolledWindow {
    let view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .cursor_visible(false)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(12)
        .right_margin(12)
        .build();
    let buffer = view.buffer();
    buffer.set_text(text);
    let tag = buffer
        .create_tag(None, &[("background", &color), ("foreground", &"#ffffff")])
        .unwrap();
    for line in lines {
        if let Some(start) = buffer.iter_at_line(*line as i32) {
            let mut end = start;
            end.forward_line();
            buffer.apply_tag(&tag, &start, &end);
        }
    }
    gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_width(240)
        .child(&view)
        .build()
}
#[derive(Clone, Copy)]
pub(super) enum Mode {
    Direct,
    Setup,
    DryRun,
}
pub(super) fn dry_run_button(
    parent: &gtk::ApplicationWindow,
    plan: Plan,
    status: &gtk::Label,
    refresh: &gtk::Button,
) -> gtk::Button {
    let button = gtk::Button::with_label("Show what will change");
    button.set_widget_name(&format!("dry-run-{}", plan.id));
    button.set_tooltip_text(Some(
        "Dry run: preview the diff without writing files or installing packages",
    ));
    let weak = parent.downgrade();
    let status = status.clone();
    let refresh = refresh.clone();
    button.connect_clicked(move |_| {
        if let Some(parent) = weak.upgrade() {
            review_dialog(&parent, plan.clone(), Mode::DryRun, &status, &refresh, None);
        }
    });
    button
}
pub(super) fn review_dialog(
    parent: &gtk::ApplicationWindow,
    plan: Plan,
    mode: Mode,
    status: &gtk::Label,
    refresh: &gtk::Button,
    preview: Option<&'static Preset>,
) {
    let root = glib::user_config_dir();
    match conflicts::links(&plan, &root) {
        Ok(links)
            if links
                .iter()
                .any(|link| !plan.managed_parents.contains(link)) =>
        {
            conflicts::dialog(parent, plan, mode, status, refresh, preview);
            return;
        }
        Err(e) => {
            notice(parent, "Could not inspect config paths", &e);
            return;
        }
        _ => {}
    }
    let before = match reviewed(&plan, &root) {
        Ok(v) => v,
        Err(e) => {
            notice(parent, "Could not preview config", &e);
            return;
        }
    };
    #[allow(deprecated)]
    let dialog = gtk::Dialog::builder()
        .title(format!("Review changes · {}", plan.name))
        .transient_for(parent)
        .modal(true)
        .default_width(1000)
        .default_height(650)
        .build();
    crate::about::follow_theme(parent, dialog.upcast_ref());
    dialog.set_widget_name("config-diff-dialog");
    #[allow(deprecated)]
    {
        dialog.add_button("Close", gtk::ResponseType::Cancel);
        if !matches!(mode, Mode::DryRun) {
            dialog.add_button("Apply reviewed changes", gtk::ResponseType::Accept);
        }
        dialog.set_default_response(gtk::ResponseType::Cancel);
    }
    let body = dialog.content_area();
    body.set_spacing(10);
    body.set_margin_top(14);
    body.set_margin_bottom(14);
    body.set_margin_start(14);
    body.set_margin_end(14);
    let note = super::super::label(
        &format!(
            "Will write {} files. Backup will be saved to {}/{}/<snapshot-id> before changes. Current files on the left; proposed files on the right. Highlighted lines change.",
            plan.files.len(),
            backups_dir().display(),
            plan.id
        ),
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_vexpand(true);
    for ((name, bytes), previous) in plan.files.iter().zip(&before) {
        let old = content(previous);
        let new = readable(&rendered(bytes, name, &root));
        let (removed, added) = changed_lines(&old, &new);
        let pane = gtk::Paned::new(gtk::Orientation::Horizontal);
        pane.set_wide_handle(true);
        pane.set_position(470);
        pane.set_start_child(Some(&diff_view(&old, &removed, "#813b46")));
        pane.set_end_child(Some(&diff_view(&new, &added, "#255a48")));
        notebook.append_page(&pane, Some(&gtk::Label::new(Some(name))));
    }
    let diff = gtk::Expander::builder()
        .label("View Diff")
        .expanded(true)
        .child(&notebook)
        .vexpand(true)
        .build();
    body.append(&diff);
    if matches!(mode, Mode::DryRun) {
        dialog.set_title(Some(&format!("Dry run · {}", plan.name)));
        note.set_text("Dry run — no files, backups, packages or login-shell settings will be changed. Current files are on the left; proposed files are on the right.");
        dialog.connect_response(|dialog, _| dialog.close());
        dialog.present();
        return;
    }
    let install = gtk::CheckButton::with_label(
        "Install missing dependencies first (Pacman may update the system)",
    );
    install.set_active(true);
    install.set_visible(matches!(mode, Mode::Setup));
    body.append(&install);
    let default = gtk::CheckButton::with_label(
        "Make Fish my default login shell after applying (asks for your password in the terminal)",
    );
    default
        .set_visible(matches!(mode, Mode::Setup) && plan.dependencies.iter().any(|d| d == "fish"));
    body.append(&default);
    let weak = parent.downgrade();
    let status = status.clone();
    let refresh = refresh.clone();
    #[allow(deprecated)]
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept {
            dialog.close();
            return;
        }
        if let Some(parent) = weak.upgrade() {
            if matches!(mode, Mode::Setup) {
                match start_setup(
                    &plan,
                    &before,
                    install.is_active(),
                    default.is_active(),
                    &status,
                    &refresh,
                ) {
                    Ok(()) => dialog.close(),
                    Err(e) => notice(&parent, "Could not start setup", &e),
                }
            } else {
                dialog.set_sensitive(false);
                status.set_text("Applying reviewed configs and checking Fish in the background…");
                let plan_work = plan.clone();
                let before_work = before.clone();
                let plan = plan.clone();
                let status = status.clone();
                let refresh = refresh.clone();
                let dialog = dialog.clone();
                let parent = parent.clone();
                let status_work = status.clone();
                crate::system_tools::task(
                    &status_work,
                    move || {
                        Ok(apply_reviewed(
                            &plan_work,
                            &before_work,
                            &glib::user_config_dir(),
                            &backups_dir(),
                        ))
                    },
                    move |result| match result {
                        Ok(backup) => {
                            status.set_text(&format!(
                                "Applied {}. Backup: {}. {}",
                                plan.name,
                                backup.display(),
                                history::message(&backup)
                            ));
                            applied_toast(&plan, backup.clone(), &status, &refresh);
                            dialog.close();
                            refresh.emit_clicked();
                            if let Some(preset) = preview
                                && let Err(e) = show_result(preset, &parent)
                            {
                                notice(&parent, "Config applied — preview unavailable", &e);
                            }
                        }
                        Err(e) => {
                            dialog.set_sensitive(true);
                            status.set_text(&e);
                            crate::ux::toast(
                                "Config apply failed; review the validation and restore result",
                            );
                            notice(&parent, "Could not apply reviewed configs", &e);
                        }
                    },
                );
            }
        }
    });
    dialog.present();
}
fn jobs_dir() -> PathBuf {
    glib::user_data_dir().join("archeasy/config-jobs")
}
pub(super) fn start_setup(
    plan: &Plan,
    before: &[Value],
    install: bool,
    default: bool,
    status: &gtk::Label,
    refresh: &gtk::Button,
) -> Result<(), String> {
    let directory = jobs_dir().join(stamp());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let path = directory.join("job.json");
    if reviewed(plan, &glib::user_config_dir())? != before {
        return Err("Configs changed after the preview. Open the diff again.".into());
    }
    let backup = save_backup(plan, &glib::user_config_dir(), &backups_dir())?;
    let job = json!({"plan":plan.json(),"before":before,"root":glib::user_config_dir(),"backup":backup,"managed_parents":plan.managed_parents});
    write_atomic(&path, job.to_string().as_bytes(), 0o600)?;
    let mut args = vec![
        "/usr/bin/bash".into(),
        "-c".into(),
        include_str!("../../scripts/config-setup.sh").into(),
        "archeasy-config-setup".into(),
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
        path.to_string_lossy().into_owned(),
        default.to_string(),
        install.to_string(),
    ];
    args.extend(plan.dependencies.clone());
    super::super::terminal(args)?;
    status.set_text(&format!(
        "Backup ZIP saved: {}. Setup terminal opened; review its package/password prompts.",
        backup.with_extension("zip").display()
    ));
    let completed_plan = plan.clone();
    let weak_status = status.downgrade();
    let weak_refresh = refresh.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let Some(status) = weak_status.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if let Ok(done) = fs::read_to_string(directory.join("job.json.done")) {
            if done.trim() != "0"
                && let Ok(error) = fs::read_to_string(directory.join("apply-error.txt"))
            {
                status.set_text(&error);
                crate::ux::toast("Fish validation failed; review the automatic restore result");
                if let Some(refresh) = weak_refresh.upgrade() {
                    refresh.emit_clicked();
                }
                return glib::ControlFlow::Break;
            }
            let summary = fs::read_to_string(directory.join("applied.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .and_then(|v| v["backup"].as_str().map(str::to_owned));
            status.set_text(&format!(
                "Setup {}. {} See the summary terminal.",
                if done.trim() == "0" {
                    "completed"
                } else {
                    "stopped or partially completed"
                },
                summary
                    .as_ref()
                    .map(|p| format!("Config backup: {p}."))
                    .unwrap_or_default()
            ));
            if let Some(refresh) = weak_refresh.upgrade() {
                if let Some(backup) = summary {
                    applied_toast(&completed_plan, PathBuf::from(backup), &status, &refresh);
                }
                refresh.emit_clicked();
            }
            let _ = fs::remove_file(&path);
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    Ok(())
}
pub(super) fn run_job(path: &Path) -> Result<PathBuf, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() > 64 * 1024 * 1024 {
        return Err("Invalid setup job size".into());
    }
    let job: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if job["root"].as_str() != glib::user_config_dir().to_str() {
        return Err("Setup job belongs to a different config directory".into());
    }
    let mut plan = Plan::parse(&job["plan"])?;
    plan.managed_parents =
        serde_json::from_value(job["managed_parents"].clone()).unwrap_or_default();
    let before = job["before"]
        .as_array()
        .ok_or("Missing reviewed file state")?;
    if reviewed(&plan, &glib::user_config_dir())? != *before {
        return Err(
            "Configs changed after the preview. Open the diff again before applying.".into(),
        );
    }
    let backup = PathBuf::from(job["backup"].as_str().ok_or("Missing pre-setup backup")?);
    let expected = backups_dir().join(&plan.id);
    if backup.parent() != Some(expected.as_path())
        || !backup.with_extension("zip").is_file()
        || !backup.join("manifest.json").is_file()
    {
        return Err("Pre-setup backup is missing or invalid. Open the diff again.".into());
    }
    apply_saved(&plan, &glib::user_config_dir(), &backup)?;
    write_atomic(
        &path
            .parent()
            .ok_or("Invalid job path")?
            .join("applied.json"),
        json!({"backup":backup,"name":plan.name})
            .to_string()
            .as_bytes(),
        0o600,
    )?;
    Ok(backup)
}
pub(super) type Refreshers = Rc<RefCell<Vec<Box<dyn Fn()>>>>;
pub(super) fn badge(card: &gtk::Box, dependencies: Vec<String>, refreshers: &Refreshers) {
    let badge = super::super::label("", "badge");
    badge.set_wrap(true);
    card.append(&badge);
    let weak = badge.downgrade();
    refreshers.borrow_mut().push(Box::new(move || {
        if let Some(badge) = weak.upgrade() {
            let missing: Vec<_> = dependencies
                .iter()
                .filter(|d| !dependency_met(d))
                .cloned()
                .collect();
            badge.remove_css_class("deps-met");
            badge.remove_css_class("deps-missing");
            if missing.is_empty() {
                badge.set_text("✓ Dependencies met");
                badge.add_css_class("deps-met");
            } else {
                badge.set_text(&format!(
                    "● Missing: {} · use Install & Apply",
                    missing.join(", ")
                ));
                badge.add_css_class("deps-missing");
            }
        }
    }));
}
pub(super) fn dependency_met(command: &str) -> bool {
    if super::super::available(command) {
        return true;
    }
    matches!(command, "xdg-desktop-portal-hyprland" | "hyprpolkitagent")
        && std::process::Command::new("pacman")
            .args(["-Q", command])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

fn applied_toast(plan: &Plan, backup: PathBuf, status: &gtk::Label, refresh: &gtk::Button) {
    let name = plan.name.clone();
    let plan = plan.clone();
    let applied = reviewed(&plan, &glib::user_config_dir());
    let status = status.downgrade();
    let refresh = refresh.downgrade();
    crate::ux::undo_toast(
        &format!("{name} config applied • backup saved"),
        move || {
            if applied.is_err() || reviewed(&plan, &glib::user_config_dir()) != applied {
                crate::ux::toast(
                    "Configs changed after apply. Review the backup in Config history before restoring.",
                );
                return;
            }
            match restore_snapshot(&backup, &glib::user_config_dir()) {
                Ok(()) => {
                    crate::ux::toast("Previous config restored");
                    if let Some(refresh) = refresh.upgrade() {
                        refresh.emit_clicked();
                    }
                }
                Err(e) => {
                    if let Some(status) = status.upgrade() {
                        status.set_text(&e);
                    }
                    crate::ux::toast(&format!("Could not undo: {e}"));
                }
            }
        },
    );
}

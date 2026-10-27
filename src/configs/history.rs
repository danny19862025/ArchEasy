use super::*;
use serde_json::{Value, json};
use std::process::Command;
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
pub(super) fn auto_commit(root: &Path, plan: &workflow::Plan, backup: &Path) -> String {
    let Ok(top) = git(root, &["rev-parse", "--show-toplevel"]) else {
        return "Git: config folder is not a repository".into();
    };
    if fs::canonicalize(top).ok() != fs::canonicalize(root).ok() {
        return "Git: skipped because the repository root is outside the config folder".into();
    }
    let paths: Vec<_> = plan.files.iter().map(|(p, _)| p.as_str()).collect();
    let mut staged = vec!["diff", "--cached", "--quiet", "--"];
    staged.extend(&paths);
    if git(root, &staged).is_err() {
        return "Git: these config paths have staged changes; kept your index unchanged. Commit them manually.".into();
    }
    let index = backup.join("git-index");
    let result: Result<String, String> = (|| {
        let head = git(root, &["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
        let run = |args: &[&str]| -> Result<String, String> {
            let out = Command::new("git")
                .arg("-C")
                .arg(root)
                .env("GIT_INDEX_FILE", &index)
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().into());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().into())
        };
        if head.is_empty() {
            run(&["read-tree", "--empty"])?;
        } else {
            run(&["read-tree", &head])?;
        }
        let mut add = vec!["add", "--"];
        add.extend(&paths);
        run(&add)?;
        if !head.is_empty() && run(&["diff", "--cached", "--quiet"]).is_ok() {
            return Ok("Git: configuration unchanged; no new commit".into());
        }
        let tree = run(&["write-tree"])?;
        let author = git(root, &["config", "user.name"]).unwrap_or_else(|_| "ArchEasy".into());
        let email =
            git(root, &["config", "user.email"]).unwrap_or_else(|_| "archeasy@localhost".into());
        let message = format!("ArchEasy: apply {}", plan.name);
        let mut args = vec![
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "-c",
        ];
        let name_config = format!("user.name={author}");
        let email_config = format!("user.email={email}");
        args.extend([&name_config, "-c", &email_config, "commit-tree", &tree]);
        if !head.is_empty() {
            args.extend(["-p", &head]);
        }
        args.extend(["-m", &message]);
        let commit = run(&args)?;
        // Compare-and-swap refuses to replace a concurrently changed HEAD.
        git(root, &["update-ref", "HEAD", &commit, &head])?;
        let mut reset = vec!["reset", "-q", "HEAD", "--"];
        reset.extend(&paths);
        git(root, &reset)?;
        Ok(format!("Git commit: {commit}"))
    })();
    let _ = fs::remove_file(&index);
    result.unwrap_or_else(|e| {
        format!("Git commit unavailable; configs and version snapshot were saved. {e}")
    })
}
pub(super) fn record(plan: &workflow::Plan, root: &Path, backup: &Path) -> Result<(), String> {
    let mut applied = plan.clone();
    applied.managed_parents.clear();
    for (name, bytes) in &mut applied.files {
        *bytes = fs::read(root.join(&*name)).map_err(|e| e.to_string())?;
        *bytes = substitute_text(bytes, &root.to_string_lossy(), "__CONFIG_DIR__");
    }
    let git = auto_commit(root, plan, backup);
    write_atomic(&backup.join("applied-version.json"), json!({"plan":applied.json(),"created":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),"git":git}).to_string().as_bytes(), 0o600)
}
pub(super) fn message(backup: &Path) -> String {
    fs::read(backup.join("applied-version.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["git"].as_str().map(str::to_owned))
        .unwrap_or_else(|| {
            "Version history could not be recorded; the before-state backup is available.".into()
        })
}
fn legacy_version(path: &Path, id: &str) -> Option<(u64, workflow::Plan)> {
    let data = fs::read(path.join("manifest.json")).ok()?;
    if data.len() > 1024 * 1024 {
        return None;
    }
    let manifest: Value = serde_json::from_slice(&data).ok()?;
    let mut files = vec![];
    for entry in manifest["entries"].as_array()? {
        if entry["existed"] != true || !entry["symlink"].is_null() {
            continue;
        }
        let name = entry["path"].as_str()?;
        let source = conflicts::preflight(&path.join("files"), name, &[]).ok()?;
        let meta = fs::symlink_metadata(&source).ok()?;
        if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
            return None;
        }
        files.push(serde_json::json!({"path":name,"bytes":fs::read(source).ok()?}));
    }
    let preset_name = PRESETS
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name)
        .unwrap_or(id);
    let plan=workflow::Plan::parse(&serde_json::json!({"id":id,"name":format!("{preset_name} · before Apply"),"files":files,"dependencies":[]})).ok()?;
    let created = path
        .file_name()?
        .to_str()?
        .parse::<u128>()
        .ok()
        .map(|n| (n / 1_000_000_000) as u64)
        .unwrap_or_default();
    Some((created, plan))
}
pub(super) fn versions(backups: &Path) -> Vec<(u64, workflow::Plan, String)> {
    let mut versions = vec![];
    for app in fs::read_dir(backups)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        if !app.path().is_dir() {
            continue;
        }
        for entry in fs::read_dir(app.path())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            let json_path = path.join("applied-version.json");
            if let Ok(meta) = fs::metadata(&json_path)
                && meta.len() <= 40 * 1024 * 1024
                && let Ok(data) = fs::read(&json_path)
                && let Ok(value) = serde_json::from_slice::<Value>(&data)
                && let Ok(plan) = workflow::Plan::parse(&value["plan"])
            {
                versions.push((
                    value["created"].as_u64().unwrap_or_default(),
                    plan,
                    value["git"].as_str().unwrap_or_default().into(),
                ));
            } else if let Some((created, plan)) =
                legacy_version(&path, &app.file_name().to_string_lossy())
            {
                versions.push((created, plan, "Legacy before-state: restores saved regular files; files absent in this snapshot are left unchanged. Use Restore backup for full rollback semantics.".into()));
            }
        }
    }
    versions.sort_by_key(|a| std::cmp::Reverse(a.0));
    versions
}
pub(super) fn current_plan(root: &Path, backups: &Path) -> Result<workflow::Plan, String> {
    let mut names = std::collections::BTreeSet::new();
    for (_, plan, _) in versions(backups) {
        for (name, _) in plan.files {
            names.insert(name);
        }
    }
    // Include presets applied before version history was introduced.
    for preset in PRESETS {
        if latest_backup(preset.id, backups).is_some()
            || latest_backup("terminal-stack", backups).is_some()
                && [
                    "fish",
                    "starship",
                    "ghostty",
                    "kitty",
                    "fastfetch",
                    "hyprland",
                ]
                .contains(&preset.id)
        {
            for (name, _) in preset.files {
                names.insert((*name).to_owned());
            }
        }
    }
    let mut files = vec![];
    for name in names {
        // These are already-applied paths. Read linked configs without changing their manager.
        let path = root.join(&name);
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
            return Err(format!(
                "Cannot export config {}: expected a regular file under 8 MiB",
                path.display()
            ));
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let bytes = substitute_text(&bytes, &root.to_string_lossy(), "__CONFIG_DIR__");
        files.push((name, bytes));
    }
    let plan = workflow::Plan {
        id: "my-system".into(),
        name: "My applied configs".into(),
        files,
        dependencies: vec![],
        managed_parents: vec![],
    };
    if !plan.files.is_empty() {
        workflow::Plan::parse(&plan.json())?;
    }
    Ok(plan)
}
fn age(created: u64) -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(created);
    if seconds < 60 {
        "just now".into()
    } else if seconds < 3600 {
        format!("{} minutes ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{} hours ago", seconds / 3600)
    } else {
        format!("{} days ago", seconds / 86400)
    }
}
pub(super) fn button(
    bar: &gtk::FlowBox,
    parent: &gtk::ApplicationWindow,
    status: &gtk::Label,
    refresh: &gtk::Button,
) {
    let button = gtk::Button::with_label("Config timeline…");
    button.set_widget_name("config-timeline");
    bar.insert(&button, -1);
    let weak = parent.downgrade();
    let status = status.clone();
    let refresh = refresh.clone();
    button.connect_clicked(move |_| { let Some(parent) = weak.upgrade() else { return; };
        let window = gtk::Window::builder().title("Config version timeline").transient_for(&parent).default_width(780).default_height(580).build(); crate::about::follow_theme(&parent,&window); window.set_widget_name("config-history-window");
        let list = gtk::Box::new(gtk::Orientation::Vertical,10); list.set_margin_top(16); list.set_margin_bottom(16); list.set_margin_start(16); list.set_margin_end(16);
        let note = super::super::label("Every successful Apply saves a version here. Restore opens a diff and makes a new backup; Git commits include only the changed config paths.","subtitle"); note.set_wrap(true); list.append(&note);
        let entries = versions(&backups_dir()); if entries.is_empty() { list.append(&super::super::label("No version history yet. Apply a preset to record its first version. Existing before-state backups remain available through Restore backup.","subtitle")); }
        for (created,plan,git) in entries.into_iter().take(200) {
            let card = gtk::Box::new(gtk::Orientation::Vertical,8); card.add_css_class("card");
            let date = glib::DateTime::from_unix_local(created as i64).ok().and_then(|d|d.format("%Y-%m-%d %H:%M").ok()).map(|s|s.to_string()).unwrap_or_default();
            card.append(&super::super::label(&format!("{} · {} · {date}",plan.name,age(created)),"name")); let detail = super::super::label(&git,"subtitle"); detail.set_wrap(true); card.append(&detail);
            let restore = gtk::Button::with_label(&format!("Restore {} from {}",plan.name,age(created))); restore.set_halign(gtk::Align::Start);
            let weak = parent.downgrade(); let status = status.clone(); let refresh = refresh.clone(); restore.connect_clicked(move |_| { if let Some(parent)=weak.upgrade() { workflow::review_dialog(&parent,plan.clone(),workflow::Mode::Direct,&status,&refresh,None); } }); card.append(&restore); list.append(&card);
        }
        window.set_child(Some(&gtk::ScrolledWindow::builder().child(&list).vexpand(true).build())); window.present();
    });
}

use super::*;
use serde_json::{Value, json};
fn pinned_raw_url(entry: &Value) -> Result<String, String> {
    let url = entry["url"].as_str().ok_or("Missing community URL")?;
    let path = url
        .strip_prefix("https://github.com/")
        .ok_or("Expected a GitHub file URL")?;
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() < 5
        || parts[2] != "blob"
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("Invalid GitHub file URL".into());
    }
    let commit = entry["commit"].as_str().unwrap_or(parts[3]);
    if commit.len() != 40 || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Community file entries need a full 40-character commit SHA. Pin the catalog entry before downloading.".into());
    }
    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        parts[0],
        parts[1],
        commit,
        parts[4..].join("/")
    ))
}
fn verify_plan_hash(entry: &Value, plan: workflow::Plan) -> Result<workflow::Plan, String> {
    let expected = entry["sha256"]
        .as_str()
        .ok_or("Community Gist/JSON entries need a sha256 of the canonical imported plan JSON")?;
    let actual = glib::compute_checksum_for_data(
        glib::ChecksumType::Sha256,
        plan.json().to_string().as_bytes(),
    )
    .ok_or("Checksum unavailable")?;
    if !actual.as_str().eq_ignore_ascii_case(expected) {
        return Err("Community preset checksum mismatch; nothing was saved or applied.".into());
    }
    Ok(plan)
}
pub(super) fn catalog() -> Vec<Value> {
    let path = glib::user_config_dir().join("archeasy/community.json");
    let text = fs::metadata(&path)
        .ok()
        .filter(|m| m.len() < 1024 * 1024)
        .and_then(|_| fs::read_to_string(path).ok())
        .unwrap_or_else(|| include_str!("../../presets/community.json").into());
    serde_json::from_str::<Vec<Value>>(&text)
        .unwrap_or_default()
        .into_iter()
        .take(100)
        .enumerate()
        .map(|(index, value)| {
            if let Some(url) = value.as_str() {
                json!({"id":format!("community-{index}"),"name":"Community Gist preset","url":url})
            } else {
                value
            }
        })
        .collect()
}
pub(super) fn download(entry: &Value) -> Result<workflow::Plan, String> {
    if entry["builtin"] == "hyprland" {
        return Ok(workflow::Plan::builtin(
            PRESETS.iter().find(|p| p.id == "hyprland").unwrap(),
        ));
    }
    let url = entry["url"].as_str().ok_or("Missing community URL")?;
    if url.starts_with("https://gist.github.com/") {
        if entry["sha256"].as_str().is_none() {
            return Err("Pin this community Gist with a sha256 before downloading.".into());
        }
        return verify_plan_hash(entry, sharing::fetch_url(url)?);
    }
    if let Some(target) = entry["target"].as_str() {
        let raw = pinned_raw_url(entry)?;
        let text = super::super::output(
            "curl",
            &[
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--max-time",
                "30",
                "--max-filesize",
                "8388608",
                &raw,
            ],
        )?;
        let mut files = vec![json!({"path":target,"bytes":text.as_bytes()})];
        if target == "kitty/archeasy-community.conf" {
            let path = glib::user_config_dir().join("kitty/kitty.conf");
            let mut config = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.to_string()),
            };
            if !config
                .lines()
                .any(|l| l.trim() == "include archeasy-community.conf")
            {
                config.push_str(
                    "\n# Theme downloaded by ArchEasy\ninclude archeasy-community.conf\n",
                );
            }
            files.push(json!({"path":"kitty/kitty.conf","bytes":config.as_bytes()}));
        }
        return workflow::Plan::parse(
            &json!({"id":entry["id"],"name":entry["name"],"files":files,"dependencies":entry["dependencies"]}),
        );
    }
    if entry["sha256"].as_str().is_none() {
        return Err("Pin this community JSON preset with a sha256 before downloading.".into());
    }
    verify_plan_hash(entry, sharing::fetch_url(url)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_downloads_are_immutable_and_hash_mismatches_fail() {
        let sha = "43098316202b84d6a71f71aaf8360f102f4d3f1a";
        let mut entry = json!({"url":"https://github.com/owner/repo/blob/main/theme.conf"});
        assert!(pinned_raw_url(&entry).is_err());
        entry["commit"] = json!(sha);
        assert_eq!(
            pinned_raw_url(&entry).unwrap(),
            format!("https://raw.githubusercontent.com/owner/repo/{sha}/theme.conf")
        );
        let plan = workflow::Plan::builtin(&PRESETS[0]);
        assert!(verify_plan_hash(&entry, plan.clone()).is_err());
        entry["sha256"] = json!("0".repeat(64));
        assert!(verify_plan_hash(&entry, plan.clone()).is_err());
        entry["sha256"] = json!(
            glib::compute_checksum_for_data(
                glib::ChecksumType::Sha256,
                plan.json().to_string().as_bytes()
            )
            .unwrap()
            .as_str()
        );
        assert!(verify_plan_hash(&entry, plan).is_ok());
        let entries: Vec<Value> =
            serde_json::from_str(include_str!("../../presets/community.json")).unwrap();
        for entry in entries.iter().filter(|e| !e["target"].is_null()) {
            assert!(pinned_raw_url(entry).is_ok());
        }
    }
}
pub(super) fn button(
    bar: &gtk::FlowBox,
    parent: &gtk::ApplicationWindow,
    status: &gtk::Label,
    refresh: &gtk::Button,
) {
    let button = gtk::Button::with_label("Community presets…");
    button.set_widget_name("community-presets");
    bar.insert(&button, -1);
    let weak = parent.downgrade();
    let status = status.clone();
    let refresh = refresh.clone();
    button.connect_clicked(move |_| { let Some(parent)=weak.upgrade() else{return;};
        let window=gtk::Window::builder().title("Community preset store").transient_for(&parent).default_width(760).default_height(580).build(); crate::about::follow_theme(&parent,&window); window.set_widget_name("community-presets-window");
        let list=gtk::Box::new(gtk::Orientation::Vertical,12); crate::system_tools::margins(&list);
        let note=super::super::label("Curated GitHub files and ArchEasy Gist URLs. Download opens a diff; nothing runs or applies automatically. Edit ~/.config/archeasy/community.json to curate your own list.","subtitle");note.set_wrap(true);list.append(&note);
        let message=super::super::label("","subtitle");message.set_wrap(true);list.append(&message);
        for entry in catalog() {
            let card=gtk::Box::new(gtk::Orientation::Vertical,8);card.add_css_class("card");card.append(&super::super::label(entry["name"].as_str().unwrap_or("Community preset"),"name"));let desc=super::super::label(entry["description"].as_str().unwrap_or_default(),"subtitle");desc.set_wrap(true);card.append(&desc);
            if let Some(url)=entry["url"].as_str() && (url.starts_with("https://github.com/") || url.starts_with("https://gist.github.com/")) { card.append(&gtk::LinkButton::with_label(url,"View source ↗")); }
            let download=gtk::Button::with_label("Download & review");download.set_halign(gtk::Align::Start);let weak=parent.downgrade();let status=status.clone();let refresh=refresh.clone();let message=message.clone();download.connect_clicked(move |_| { let entry=entry.clone(); let weak=weak.clone();let status=status.clone();let refresh=refresh.clone();message.set_text("Downloading community preset…"); crate::system_tools::task(&message,move||{ let plan=self::download(&entry)?; let dir=glib::user_data_dir().join("archeasy/presets").join(&plan.id); fs::create_dir_all(&dir).map_err(|e|e.to_string())?; fs::set_permissions(&dir,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?; write_atomic(&dir.join("preset.json"),plan.json().to_string().as_bytes(),0o600)?; Ok(plan) },move|plan|{if let Some(parent)=weak.upgrade(){workflow::review_dialog(&parent,plan,workflow::Mode::Setup,&status,&refresh,None);}}); });card.append(&download);list.append(&card);
        }
        window.set_child(Some(&gtk::ScrolledWindow::builder().child(&list).build()));window.present();
    });
}

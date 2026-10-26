use gtk::{glib, prelude::*};
use std::path::Path;

pub fn load(path: &Path) -> Vec<String> {
    std::fs::read(path)
        .ok()
        .filter(|b| b.len() <= 8192)
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| (2..=200).contains(&s.chars().count()) && !s.contains(char::is_control))
        .take(5)
        .collect()
}

pub fn remember(path: &Path, query: &str) -> Result<(), String> {
    let query = query.trim();
    if !(2..=200).contains(&query.chars().count()) || query.contains(char::is_control) {
        return Ok(());
    }
    let mut history = load(path);
    history.retain(|s| !s.eq_ignore_ascii_case(query));
    history.insert(0, query.to_owned());
    history.truncate(5);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::configs::write_atomic(
        path,
        &serde_json::to_vec(&history).map_err(|e| e.to_string())?,
        0o600,
    )
}

fn path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy/search-history.json")
}

pub fn refresh(chips: &gtk::FlowBox, entry: &gtk::Entry) {
    while let Some(child) = chips.first_child() {
        chips.remove(&child);
    }
    let history = load(&path());
    chips.set_visible(!history.is_empty());
    for query in history {
        let chip = gtk::Button::with_label(&query);
        chip.add_css_class("pill");
        chip.set_halign(gtk::Align::Start);
        if let Some(label) = chip.child().and_downcast::<gtk::Label>() {
            label.set_max_width_chars(20);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
        chip.set_tooltip_text(Some(&format!("Search again: {query}")));
        let entry = entry.downgrade();
        chip.connect_clicked(move |_| {
            if let Some(entry) = entry.upgrade() {
                entry.set_text(&query);
                entry.grab_focus();
            }
        });
        chips.insert(&chip, -1);
    }
}

pub fn record(query: &str, chips: &gtk::FlowBox, entry: &gtk::Entry) {
    if let Err(error) = remember(&path(), query) {
        eprintln!("Search history: {error}");
    }
    refresh(chips, entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_private_deduplicated_and_limited_to_five() {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("archeasy-history-{}.json", std::process::id()));
        for query in [
            "fish", "kitty", "firefox", "ghostty", "neovim", "btop", "FISH",
        ] {
            remember(&path, query).unwrap();
        }
        assert_eq!(
            load(&path),
            ["FISH", "btop", "neovim", "ghostty", "firefox"]
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::write(&path, "not JSON").unwrap();
        assert!(load(&path).is_empty());
        std::fs::remove_file(path).unwrap();
    }
}

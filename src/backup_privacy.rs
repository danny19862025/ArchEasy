use std::path::Path;

pub fn excluded(relative: &Path) -> bool {
    static PATHS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let paths = PATHS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/backup-exclusions.json"))
            .expect("valid bundled backup exclusions")
    });
    paths.iter().any(|path| relative.starts_with(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sensitive_folders_and_descendants_are_excluded_without_matching_siblings() {
        for path in [
            ".ssh/id_ed25519",
            ".gnupg",
            ".config/google-chrome/Default/Cookies",
            ".mozilla/firefox/profile",
            ".config/rclone/rclone.conf",
            ".var/app/org.mozilla.firefox/data",
        ] {
            assert!(excluded(Path::new(path)), "{path}");
        }
        for path in [
            ".ssh-notes",
            ".config/kitty",
            ".config/chromium-theme",
            "Projects/app",
        ] {
            assert!(!excluded(Path::new(path)), "{path}");
        }
    }
}

//! Curated, metadata-only configuration discovery. Never executes applications.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_DIRECTORY_ENTRIES: usize = 128;
const MAX_FILES_PER_APP: usize = 40;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const EXCLUDED: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".mozilla",
    ".netrc",
    ".env",
    ".npmrc",
    ".git-credentials",
    "keyrings",
    "kwalletd",
    "Cookies",
    "Login Data",
    "rclone",
    "chromium",
    "google-chrome",
    "BraveSoftware",
    "firefox",
];
const HARDWARE: &[&str] = &[
    "hyprland.conf",
    "hyprland.lua",
    "monitors.conf",
    "xorg.conf",
    "fstab",
    "mkinitcpio.conf",
];

struct App {
    name: &'static str,
    executable: &'static str,
    description: &'static str,
    files: &'static [&'static str],
    directories: &'static [(&'static str, &'static [&'static str])],
}
const APPS: &[App] = &[
    App {
        name: "Alacritty",
        executable: "alacritty",
        description: "Terminal preferences and themes",
        files: &[
            ".config/alacritty/alacritty.toml",
            ".config/alacritty/alacritty.yml",
            ".config/alacritty/alacritty.yaml",
        ],
        directories: &[(".config/alacritty/themes", &["toml", "yml", "yaml"])],
    },
    App {
        name: "Bash",
        executable: "bash",
        description: "Shell preferences and login environment",
        files: &[".bashrc", ".profile"],
        directories: &[],
    },
    App {
        name: "Fish",
        executable: "fish",
        description: "Shell preferences, functions, startup snippets and completions",
        files: &[".config/fish/config.fish"],
        directories: &[
            (".config/fish/conf.d", &["fish"]),
            (".config/fish/functions", &["fish"]),
            (".config/fish/completions", &["fish"]),
        ],
    },
    App {
        name: "Foot",
        executable: "foot",
        description: "Terminal preferences and themes",
        files: &[".config/foot/foot.ini"],
        directories: &[(".config/foot/themes", &["ini"])],
    },
    App {
        name: "Ghostty",
        executable: "ghostty",
        description: "Terminal preferences",
        files: &[".config/ghostty/config"],
        directories: &[],
    },
    App {
        name: "Git",
        executable: "git",
        description: "User preferences, ignore rules and attributes",
        files: &[
            ".gitconfig",
            ".config/git/config",
            ".config/git/ignore",
            ".config/git/attributes",
        ],
        directories: &[],
    },
    App {
        name: "Kitty",
        executable: "kitty",
        description: "Terminal preferences and themes",
        files: &[
            ".config/kitty/kitty.conf",
            ".config/kitty/current-theme.conf",
        ],
        directories: &[(".config/kitty/themes", &["conf"])],
    },
    App {
        name: "Starship",
        executable: "starship",
        description: "Shell prompt appearance",
        files: &[".config/starship.toml"],
        directories: &[],
    },
    App {
        name: "Tmux",
        executable: "tmux",
        description: "Terminal multiplexer preferences",
        files: &[".tmux.conf", ".config/tmux/tmux.conf"],
        directories: &[],
    },
    App {
        name: "Zsh",
        executable: "zsh",
        description: "Shell preferences",
        files: &[".zshrc"],
        directories: &[],
    },
];

#[derive(Debug)]
pub(crate) struct Recommendation {
    pub name: &'static str,
    pub installed: bool,
    pub description: &'static str,
    pub files: Vec<String>,
    pub limited: bool,
}

/// Mirrors backend path restrictions for UI discovery; backend checks remain authoritative.
pub(crate) fn supported_relative(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    !name.is_empty()
        && name.len() <= 4096
        && !name.contains(['\\', '\0'])
        && parts.iter().all(|part| !matches!(*part, "" | "." | ".."))
        && (parts[0] == ".config"
            || matches!(
                parts[0],
                ".bashrc" | ".zshrc" | ".profile" | ".gitconfig" | ".tmux.conf"
            ))
        && !parts.iter().any(|part| {
            EXCLUDED.contains(part)
                || [".kdbx", ".pem", ".key"]
                    .iter()
                    .any(|suffix| part.to_lowercase().ends_with(suffix))
        })
        && !HARDWARE.contains(parts.last().unwrap_or(&""))
}

fn no_symlinks(home: &Path, relative: &str) -> bool {
    if !supported_relative(relative) {
        return false;
    }
    let mut path = home.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    true
}

pub(crate) fn safe_file(home: &Path, relative: &str) -> bool {
    if !no_symlinks(home, relative) {
        return false;
    }
    let (Ok(metadata), Ok(owner)) = (fs::metadata(home.join(relative)), fs::metadata(home)) else {
        return false;
    };
    metadata.is_file() && metadata.len() <= MAX_FILE_BYTES && metadata.uid() == owner.uid()
}

fn installed(executable: &str, search_paths: &[PathBuf]) -> bool {
    search_paths.iter().any(|directory| {
        fs::metadata(directory.join(executable))
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

pub(crate) fn discover(home: &Path, search_paths: &[PathBuf]) -> Vec<Recommendation> {
    APPS.iter()
        .filter_map(|app| {
            let installed = installed(app.executable, search_paths);
            let mut files: Vec<String> = app
                .files
                .iter()
                .filter(|name| safe_file(home, name))
                .map(|name| (*name).into())
                .collect();
            let mut limited = false;
            for (directory, extensions) in app.directories {
                if !no_symlinks(home, directory) {
                    continue;
                }
                let Ok(entries) = fs::read_dir(home.join(directory)) else {
                    continue;
                };
                // Refuse overfull directories rather than picking an arbitrary subset from read_dir order.
                let entries: Vec<_> = entries.take(MAX_DIRECTORY_ENTRIES + 1).collect();
                if entries.len() > MAX_DIRECTORY_ENTRIES {
                    limited = true;
                    continue;
                }
                for entry in entries.into_iter().flatten() {
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else {
                        continue;
                    };
                    if !Path::new(name)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extensions.contains(&extension))
                    {
                        continue;
                    }
                    let relative = format!("{directory}/{name}");
                    if safe_file(home, &relative) {
                        files.push(relative);
                    }
                }
            }
            files.sort();
            files.dedup();
            if files.len() > MAX_FILES_PER_APP {
                limited = true;
                files.truncate(MAX_FILES_PER_APP);
            }
            (installed || !files.is_empty() || limited).then_some(Recommendation {
                name: app.name,
                installed,
                description: app.description,
                files,
                limited,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "archeasy-discovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str) {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "# preferences\n").unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn distinguishes_installed_saved_settings_and_missing_apps() {
        let fixture = Fixture::new();
        fixture.file("bin/fish");
        fs::set_permissions(
            fixture.0.join("bin/fish"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fixture.file(".gitconfig");
        let found = discover(&fixture.0, &[fixture.0.join("bin")]);
        let fish = found.iter().find(|app| app.name == "Fish").unwrap();
        assert!(fish.installed);
        assert!(fish.files.is_empty());
        let git = found.iter().find(|app| app.name == "Git").unwrap();
        assert!(!git.installed);
        assert_eq!(git.files, vec![".gitconfig"]);
        assert!(!found.iter().any(|app| app.name == "Kitty"));
        fixture.file(".config/fish/config.fish");
        let found = discover(&fixture.0, &[fixture.0.join("bin")]);
        let fish = found.iter().find(|app| app.name == "Fish").unwrap();
        assert!(fish.installed);
        assert_eq!(fish.files, vec![".config/fish/config.fish"]);
    }
    #[test]
    fn discovers_curated_multi_file_settings_in_deterministic_order() {
        let fixture = Fixture::new();
        for path in [
            ".config/fish/functions/z.fish",
            ".config/fish/conf.d/a.fish",
            ".config/fish/completions/c.fish",
            ".config/fish/config.fish",
            ".config/fish/functions/readme.txt",
            ".config/fish/functions/sub/nested.fish",
            ".config/kitty/themes/dark.conf",
        ] {
            fixture.file(path);
        }
        let found = discover(&fixture.0, &[]);
        assert_eq!(
            found.iter().find(|app| app.name == "Fish").unwrap().files,
            vec![
                ".config/fish/completions/c.fish",
                ".config/fish/conf.d/a.fish",
                ".config/fish/config.fish",
                ".config/fish/functions/z.fish"
            ]
        );
        assert_eq!(
            found.iter().find(|app| app.name == "Kitty").unwrap().files,
            vec![".config/kitty/themes/dark.conf"]
        );
    }
    #[test]
    fn refuses_symlink_files_directories_and_unsafe_paths() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        fixture.file("outside/fake.fish");
        fs::create_dir_all(fixture.0.join(".config/fish")).unwrap();
        symlink(
            fixture.0.join("outside/fake.fish"),
            fixture.0.join(".config/fish/config.fish"),
        )
        .unwrap();
        symlink(
            fixture.0.join("outside"),
            fixture.0.join(".config/fish/functions"),
        )
        .unwrap();
        assert!(discover(&fixture.0, &[]).is_empty());
        for path in [
            ".config/.ssh/config",
            ".config/app/key.pem",
            ".config/kitty/monitors.conf",
            ".config/app/.env",
            "../.bashrc",
            ".config/app\\config",
        ] {
            assert!(!supported_relative(path), "{path}");
        }
    }
    #[test]
    fn discovery_bounds_overfull_directories_and_per_app_files() {
        let fixture = Fixture::new();
        for index in 0..=MAX_DIRECTORY_ENTRIES {
            fixture.file(&format!(".config/fish/functions/{index:03}.fish"));
        }
        let found = discover(&fixture.0, &[]);
        let fish = found.iter().find(|app| app.name == "Fish").unwrap();
        assert!(fish.files.is_empty());
        assert!(fish.limited);
        fs::remove_dir_all(fixture.0.join(".config/fish/functions")).unwrap();
        for index in 0..=MAX_FILES_PER_APP {
            fixture.file(&format!(".config/fish/functions/{index:03}.fish"));
        }
        let found = discover(&fixture.0, &[]);
        let fish = found.iter().find(|app| app.name == "Fish").unwrap();
        assert_eq!(fish.files.len(), MAX_FILES_PER_APP);
        assert!(fish.limited);
    }
    #[test]
    fn path_presence_without_executable_permission_is_not_installation() {
        let fixture = Fixture::new();
        fixture.file("bin/fish");
        assert!(!installed("fish", &[fixture.0.join("bin")]));
    }
}

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Clone, Debug)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub executable: String,
    pub desktop: PathBuf,
    pub flatpak: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: &'static str,
    pub bytes: u64,
    pub incomplete: bool,
}
#[derive(Clone)]
pub struct Roots {
    pub home: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
}
impl Roots {
    pub fn current() -> Self {
        let home = PathBuf::from(env::var_os("HOME").expect("HOME must be set"));
        let xdg = |key: &str, fallback| {
            let key = if sandboxed() {
                format!("HOST_{key}")
            } else {
                key.into()
            };
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Self {
            config: xdg("XDG_CONFIG_HOME", ".config"),
            cache: xdg("XDG_CACHE_HOME", ".cache"),
            data: xdg("XDG_DATA_HOME", ".local/share"),
            home,
        }
    }
    pub fn allowed(&self, path: &Path) -> bool {
        // Only immediate application folders, inside real user-owned data roots.
        let Ok(home) = self.home.canonicalize() else {
            return false;
        };
        [
            &self.config,
            &self.cache,
            &self.data,
            &self.home.join(".var/app"),
        ]
        .iter()
        .any(|root| {
            path.parent() == Some(root.as_path())
                && root.canonicalize().is_ok_and(|r| r.starts_with(&home))
                && path
                    .file_name()
                    .is_some_and(|n| safe_name(&n.to_string_lossy()))
                && fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_symlink())
        })
    }
}
fn safe_name(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.contains('\\')
}
pub fn parse_desktop(text: &str, path: PathBuf) -> Option<App> {
    let mut fields = BTreeMap::new();
    let mut active = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
            continue;
        }
        if active
            && !line.starts_with('#')
            && let Some((k, v)) = line.split_once('=')
        {
            fields.insert(k, v);
        }
    }
    if fields.get("Type") != Some(&"Application") || fields.get("Hidden") == Some(&"true") {
        return None;
    }
    let id = path.file_stem()?.to_str()?.to_owned();
    Some(App {
        id,
        name: fields.get("Name")?.to_string(),
        icon: fields
            .get("Icon")
            .unwrap_or(&"application-x-executable")
            .to_string(),
        executable: fields
            .get("Exec")
            .unwrap_or(&"")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .to_string(),
        desktop: path,
        flatpak: fields.get("X-Flatpak").map(|s| s.to_string()),
    })
}
pub fn discover(roots: &Roots) -> Vec<App> {
    let mut dirs = vec![
        roots.data.join("applications"),
        roots
            .home
            .join(".local/share/flatpak/exports/share/applications"),
    ];
    let system = if sandboxed() {
        host_output("printenv", &["XDG_DATA_DIRS"])
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        env::var("XDG_DATA_DIRS").ok()
    }
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(
        system
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| Path::new(s).join("applications")),
    );
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
    if sandboxed() {
        dirs = dirs
            .into_iter()
            .map(|p| {
                if p.starts_with("/usr") {
                    Path::new("/run/host").join(p.strip_prefix("/").unwrap())
                } else {
                    p
                }
            })
            .collect();
    }
    let mut seen = BTreeSet::new();
    let mut apps = vec![];
    for dir in dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "desktop") {
                    continue;
                }
                if !seen.insert(entry.file_name()) {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path)
                    && let Some(app) = parse_desktop(&text, path)
                {
                    apps.push(app);
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}
fn measure(path: &Path) -> (u64, bool) {
    let mut stack = vec![path.to_path_buf()];
    let mut bytes: u64 = 0;
    let mut incomplete = false;
    let mut count = 0;
    while let Some(p) = stack.pop() {
        count += 1;
        if count > 200_000 {
            incomplete = true;
            break;
        }
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => match fs::read_dir(p) {
                Ok(entries) => {
                    for e in entries {
                        match e {
                            Ok(e) => stack.push(e.path()),
                            Err(_) => incomplete = true,
                        }
                    }
                }
                Err(_) => incomplete = true,
            },
            Ok(m) => bytes = bytes.saturating_add(m.len()),
            Err(_) => incomplete = true,
        }
    }
    (bytes, incomplete)
}
pub fn candidates(app: &App, roots: &Roots) -> Vec<Candidate> {
    let mut names = BTreeSet::new();
    for s in [&app.id, &app.name] {
        if safe_name(s) {
            names.insert(s.clone());
            names.insert(s.to_lowercase());
        }
    }
    if let Some(s) = &app.flatpak
        && safe_name(s)
    {
        names.insert(s.clone());
    }
    // Exec is supporting evidence only: generic wrappers must never become cleanup targets.
    let mut result = vec![];
    for (root, kind) in [
        (&roots.config, "Settings"),
        (&roots.cache, "Cache"),
        (&roots.data, "Application data"),
    ] {
        for name in &names {
            let path = root.join(name);
            if roots.allowed(&path) {
                let (bytes, incomplete) = measure(&path);
                result.push(Candidate {
                    path,
                    kind,
                    bytes,
                    incomplete,
                });
            }
        }
    }
    if let Some(id) = &app.flatpak {
        let path = roots.home.join(".var/app").join(id);
        if roots.allowed(&path) {
            let (bytes, incomplete) = measure(&path);
            result.push(Candidate {
                path,
                kind: "Flatpak sandbox",
                bytes,
                incomplete,
            });
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result.dedup_by(|a, b| a.path == b.path);
    result
}
pub fn size(bytes: u64) -> String {
    let mut n = bytes as f64;
    let mut unit = "B";
    for u in ["KiB", "MiB", "GiB", "TiB"] {
        if n < 1024.0 {
            break;
        }
        n /= 1024.0;
        unit = u;
    }
    format!("{n:.1} {unit}")
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
pub fn trash_path(path: &Path) -> Result<(), String> {
    if sandboxed() {
        // Directory trashing is not supported reliably by desktop portals.
        // Use the host's GIO utility so restoration works in its file manager.
        let path = path.to_str().ok_or("The folder path is not valid UTF-8")?;
        let output = host_output("gio", &["trash", "--", path])
            .map_err(|e| format!("Could not run the host Trash tool: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Host Trash failed: {}. Ensure gio is installed (libglib2.0-bin on Debian/Ubuntu, glib2 on Arch/Fedora).",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    } else {
        use gtk::gio::prelude::*;
        gtk::gio::File::for_path(path)
            .trash(gtk::gio::Cancellable::NONE)
            .map_err(|e| e.to_string())
    }
}
fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}
fn host_output(tool: &str, args: &[&str]) -> std::io::Result<Output> {
    if sandboxed() {
        Command::new("flatpak-spawn")
            .args(["--host", tool])
            .args(args)
            .output()
    } else {
        Command::new(tool).args(args).output()
    }
}
fn available(tool: &str) -> bool {
    host_output("sh", &["-c", "command -v \"$1\"", "appcleaner", tool])
        .is_ok_and(|out| out.status.success())
}
fn rpm_remove_command(
    has_ostree: bool,
    has_zypper: bool,
    has_dnf: bool,
    has_yum: bool,
) -> Option<&'static str> {
    // The RPM database on an immutable OS includes base packages which cannot be removed normally.
    if has_ostree {
        None
    } else if has_zypper {
        Some("sudo zypper remove")
    } else if has_dnf {
        Some("sudo dnf remove")
    } else if has_yum {
        Some("sudo yum remove")
    } else {
        None
    }
}
fn package_from_query(tool: &str, output: &str) -> Option<String> {
    let line = output.lines().next()?.trim();
    let package = if tool == "dpkg-query" {
        line.split_once(": ")?.0
    } else {
        line
    };
    // Do not guess when a query returns diversions, multiple owners, or an invalid package identifier.
    if package.is_empty()
        || !package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-._:".contains(c))
    {
        None
    } else {
        Some(package.to_owned())
    }
}
pub fn uninstall_hint(app: &App) -> String {
    if let Some(id) = &app.flatpak {
        return format!("flatpak uninstall {}", quote(id));
    }
    // Prefer ownership of the launcher over Exec, which may name a generic wrapper.
    let desktop = app
        .desktop
        .strip_prefix("/run/host")
        .unwrap_or(&app.desktop)
        .to_path_buf();
    let mut targets = vec![desktop];
    if app.executable.starts_with('/') {
        targets.push(PathBuf::from(&app.executable));
    } else if !app.executable.is_empty()
        && let Ok(out) = host_output(
            "sh",
            &["-c", "command -v \"$1\"", "appcleaner", &app.executable],
        )
        && out.status.success()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if path.starts_with('/') {
            targets.push(PathBuf::from(path));
        }
    }
    let rpm_prefix = rpm_remove_command(
        available("rpm-ostree"),
        available("zypper"),
        available("dnf"),
        available("yum"),
    );
    for target in targets {
        let Some(target) = target.to_str() else {
            continue;
        };
        for (tool, args, prefix) in [
            ("pacman", vec!["-Qqo", target], Some("sudo pacman -Rns")),
            ("dpkg-query", vec!["-S", target], Some("sudo apt remove")),
            ("rpm", vec!["-qf", "--qf", "%{NAME}", target], rpm_prefix),
        ] {
            let Some(prefix) = prefix else { continue };
            if let Ok(out) = host_output(tool, &args)
                && out.status.success()
                && let Some(package) =
                    package_from_query(tool, &String::from_utf8_lossy(&out.stdout))
            {
                return format!("{prefix} {}", quote(&package));
            }
        }
    }
    "No supported package owner found. Use your software center or the original installer (including Snap, AppImage, Nix, and immutable system apps).".into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rpm_commands_respect_distro_and_immutable_hosts() {
        assert_eq!(
            rpm_remove_command(false, true, true, false),
            Some("sudo zypper remove")
        );
        assert_eq!(
            rpm_remove_command(false, false, true, false),
            Some("sudo dnf remove")
        );
        assert_eq!(
            rpm_remove_command(false, false, false, true),
            Some("sudo yum remove")
        );
        assert_eq!(rpm_remove_command(true, false, true, false), None);
        assert_eq!(rpm_remove_command(false, false, false, false), None);
    }
    #[test]
    fn package_owner_parsing_preserves_debian_architecture() {
        assert_eq!(
            package_from_query(
                "dpkg-query",
                "example:amd64: /usr/share/applications/example.desktop\n"
            ),
            Some("example:amd64".into())
        );
        assert_eq!(
            package_from_query("pacman", "example\n"),
            Some("example".into())
        );
        assert_eq!(package_from_query("rpm", "example; touch /tmp/oops"), None);
        assert_eq!(
            package_from_query("dpkg-query", "diversion by other from: /path"),
            None
        );
    }
    #[test]
    fn parses_only_main_section() {
        let app = parse_desktop("[Desktop Entry]\nType=Application\nName=Example\nExec=example %U\n[Desktop Action Other]\nName=Wrong",PathBuf::from("example.desktop")).unwrap();
        assert_eq!(app.name, "Example");
        assert_eq!(app.executable, "example");
    }
    #[test]
    fn hidden_is_not_listed() {
        assert!(
            parse_desktop(
                "[Desktop Entry]\nType=Application\nName=Hidden\nHidden=true",
                PathBuf::from("hidden.desktop")
            )
            .is_none()
        );
    }
    #[test]
    fn rejects_traversal() {
        for s in ["", ".", "..", "../oops", "/tmp"] {
            assert!(!safe_name(s));
        }
    }
    #[test]
    fn scan_does_not_follow_links_or_match_prefixes() {
        let home = env::temp_dir().join(format!("appcleaner-test-{}", std::process::id()));
        fs::create_dir_all(home.join("config/Example")).unwrap();
        fs::create_dir_all(home.join("config/Example-other")).unwrap();
        fs::write(home.join("config/Example/file"), b"hello").unwrap();
        let roots = Roots {
            home: home.clone(),
            config: home.join("config"),
            cache: home.join("cache"),
            data: home.join("data"),
        };
        let app = parse_desktop(
            "[Desktop Entry]\nType=Application\nName=Example",
            PathBuf::from("example.desktop"),
        )
        .unwrap();
        let c = candidates(&app, &roots);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].bytes, 5);
        std::os::unix::fs::symlink("/tmp", home.join("config/example")).unwrap();
        assert_eq!(candidates(&app, &roots).len(), 1);
        assert!(!roots.allowed(&home.join("config")));
        fs::remove_dir_all(home).unwrap();
    }
}

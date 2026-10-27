use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub kind: String,
}
impl Remote {
    pub fn label(&self) -> String {
        let provider = match self.kind.as_str() {
            "drive" => "Google Drive",
            "dropbox" => "Dropbox",
            "crypt" => "Encrypted remote",
            other => other,
        };
        format!("{} · {provider}", self.name)
    }
}
#[derive(Clone)]
pub struct Client {
    pub config: Option<PathBuf>,
}
impl Client {
    pub fn system() -> Self {
        Self { config: None }
    }
    fn run(&self, args: &[String]) -> Result<String, String> {
        let mut command = Command::new("rclone");
        command.args(args).args([
            "--ask-password=false",
            "--contimeout=15s",
            "--timeout=60s",
            "--retries=1",
            "--low-level-retries=2",
            "--max-duration=10m",
        ]);
        if let Some(config) = &self.config {
            command.arg("--config").arg(config);
        }
        let out = command.output().map_err(|e| {
            format!("Could not run rclone: {e}. Install it with your package manager.")
        })?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    pub fn remotes(&self) -> Result<Vec<Remote>, String> {
        parse_remotes(&self.run(&["listremotes".into(), "--json".into()])?)
    }
    pub fn list(&self, remote: &str, folder: &str) -> Result<Vec<String>, String> {
        let dest = destination(remote, folder)?;
        let result = self.run(&[
            "lsjson".into(),
            dest,
            "--dirs-only".into(),
            "--no-modtime".into(),
            "--no-mimetype".into(),
        ])?;
        let entries: Vec<Value> = serde_json::from_str(&result).map_err(|e| e.to_string())?;
        let mut names: Vec<_> = entries
            .iter()
            .filter_map(|v| v["Name"].as_str())
            .filter(|s| valid_backup_name(s))
            .map(str::to_owned)
            .collect();
        names.sort();
        names.reverse();
        Ok(names)
    }
    pub fn upload(
        &self,
        snapshot: &Snapshot,
        remote: &str,
        folder: &str,
    ) -> Result<String, String> {
        let target = format!("{}/{}", destination(remote, folder)?, snapshot.name);
        self.run(&[
            "copy".into(),
            snapshot.path.to_string_lossy().into_owned(),
            target.clone(),
            "--immutable".into(),
            "--create-empty-src-dirs".into(),
        ])
        .map_err(|e| {
            format!("Upload incomplete at {target}. A partial cloud folder may remain. {e}")
        })?;
        Ok(target)
    }
    pub fn download(
        &self,
        remote: &str,
        folder: &str,
        name: &str,
        output: &Path,
    ) -> Result<PathBuf, String> {
        if !valid_backup_name(name) {
            return Err("Invalid backup name.".into());
        }
        let target = output.join(name);
        private_dir(&target).map_err(|e| {
            format!(
                "Choose a new folder; download destination already exists or cannot be created: {e}"
            )
        })?;
        let source = format!("{}/{}", destination(remote, folder)?, name);
        self.run(&[
            "copy".into(),
            source,
            target.to_string_lossy().into_owned(),
            "--immutable".into(),
            "--create-empty-src-dirs".into(),
        ])
        .map_err(|e| format!("Download incomplete at {}. {e}", target.display()))?;
        Ok(target)
    }
}
pub fn parse_remotes(text: &str) -> Result<Vec<Remote>, String> {
    let values: Vec<Value> =
        serde_json::from_str(text).map_err(|e| format!("Could not read connections: {e}"))?;
    Ok(values
        .iter()
        .filter_map(|v| {
            Some(Remote {
                name: v["name"].as_str()?.into(),
                kind: v["type"].as_str()?.into(),
            })
        })
        .collect())
}
fn destination(remote: &str, folder: &str) -> Result<String, String> {
    if remote.is_empty() || remote.starts_with('-') || remote.contains([':', '/', '\\', '\n', '\r'])
    {
        return Err("Choose a configured cloud account.".into());
    }
    let path = Path::new(folder);
    if folder.is_empty()
        || folder.contains([':', '\\', '\n', '\r'])
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(
            "Cloud folder must be a relative folder, such as ArchEasy/ConfigBackups.".into(),
        );
    }
    Ok(format!("{remote}:{folder}"))
}
fn valid_backup_name(name: &str) -> bool {
    name.starts_with("config-")
        && name.len() < 120
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn unique_name() -> String {
    let date = gtk::glib::DateTime::now_local()
        .ok()
        .and_then(|d| d.format("%Y%m%d-%H%M%S").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "backup".into());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("config-{date}-{nonce}")
}
fn private_dir(path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)
}
#[derive(Debug)]
pub struct Snapshot {
    pub name: String,
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub skipped: usize,
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
pub fn validate_source(home: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(home)
        .map_err(|_| "Choose configuration files inside your home folder.".to_owned())?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("Choose a file or config folder, rather than your entire home folder.".into());
    }
    let mut current = home.to_path_buf();
    for part in relative.components() {
        current.push(part);
        if fs::symlink_metadata(&current)
            .map_err(|e| format!("{}: {e}", current.display()))?
            .file_type()
            .is_symlink()
        {
            return Err(format!(
                "Linked paths are excluded: {}. Choose the original file instead.",
                path.display()
            ));
        }
    }
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() && !meta.is_dir() {
        return Err("Only regular config files and folders can be saved.".into());
    }
    Ok(relative.to_path_buf())
}
// Resolve existing ancestors too, so a symlinked cache parent cannot hide an
// overlap when the staging directory has not been created yet.
fn resolved_staging(path: &Path) -> Result<PathBuf, String> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|e| e.to_string())?
                    .join(path)
            };
            let parent = absolute.parent().ok_or("Invalid staging directory")?;
            let name = absolute.file_name().ok_or("Invalid staging directory")?;
            Ok(resolved_staging(parent)?.join(name))
        }
        Err(error) => Err(error.to_string()),
    }
}
pub fn snapshot(home: &Path, sources: &[PathBuf], staging: &Path) -> Result<Snapshot, String> {
    if sources.is_empty() {
        return Err("Choose at least one config file or folder.".into());
    }
    let resolved_stage = resolved_staging(staging)?;
    let mut selections = vec![];
    for source in sources {
        selections.push(
            validate_source(home, source)?
                .to_string_lossy()
                .into_owned(),
        );
        let resolved_source = fs::canonicalize(source).map_err(|e| e.to_string())?;
        if resolved_stage.starts_with(&resolved_source)
            || resolved_source.starts_with(&resolved_stage)
        {
            return Err("Selected files and folders must not overlap the cloud staging folder. Choose a smaller config folder or a different staging location.".into());
        }
    }
    fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    fs::set_permissions(staging, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let name = unique_name();
    let path = staging.join(&name);
    private_dir(&path).map_err(|e| e.to_string())?;
    let mut snapshot = Snapshot {
        name,
        path,
        files: 0,
        bytes: 0,
        skipped: 0,
    };
    let mut pending = vec![];
    for source in sources {
        pending.push(source.clone());
    }
    let mut seen = BTreeSet::new();
    let mut entries = 0;
    while let Some(source) = pending.pop() {
        if !seen.insert(source.clone()) {
            continue;
        }
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err("Selection exceeds 100,000 entries. Choose smaller config folders.".into());
        }
        let meta =
            fs::symlink_metadata(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
            snapshot.skipped += 1;
            continue;
        }
        let relative = validate_source(home, &source)?;
        let target = snapshot.path.join("files").join(relative);
        if meta.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(&source).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        } else {
            snapshot.bytes = snapshot.bytes.saturating_add(meta.len());
            if snapshot.bytes > MAX_BYTES {
                return Err("Selection exceeds 512 MiB. Choose smaller config folders.".into());
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut input = fs::File::open(&source)
                .map_err(|e| e.to_string())?
                .take(meta.len().saturating_add(1));
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(meta.permissions().mode() & 0o700),
            )
            .map_err(|e| e.to_string())?;
            let copied = std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
            let after = fs::metadata(&source).map_err(|e| e.to_string())?;
            if copied != meta.len() || after.modified().ok() != meta.modified().ok() {
                return Err(format!(
                    "{} changed during the backup. Close the app and try again.",
                    source.display()
                ));
            }
            snapshot.files += 1;
        }
    }
    let manifest = json!({"format_version":1,"created_unix":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),"sources_relative_to_home":selections,"files":snapshot.files,"bytes":snapshot.bytes,"skipped_links_or_special_files":snapshot.skipped,"restore":"Copy desired files from files/ into your home directory after reviewing them. Downloading never changes live configs."});
    fs::write(
        snapshot.path.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(snapshot)
}
pub fn suggestions(home: &Path, config: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(config)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().is_none_or(|n| n != "rclone") && validate_source(home, p).is_ok())
        .collect();
    for name in [
        ".bashrc",
        ".bash_profile",
        ".zshrc",
        ".profile",
        ".gitconfig",
        ".tmux.conf",
    ] {
        let p = home.join(name);
        if validate_source(home, &p).is_ok() {
            paths.push(p);
        }
    }
    paths.sort();
    paths
}

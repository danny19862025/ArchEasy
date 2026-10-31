//! Process and protocol boundary. This module does not depend on GTK.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

const SCRIPT: &str = include_str!("../../scripts/gaming.py");

pub fn lock_directory() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
                })
                .join("archeasy")
        })
}

pub fn catalog() -> Result<Vec<Value>, String> {
    let out = Command::new("/usr/bin/python3")
        .args(["-I", "-u", "-c", SCRIPT, "catalog"])
        .output()
        .map_err(|e| format!("Gaming requires Python 3: {e}"))?;
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let value: Value =
        serde_json::from_str(&first).map_err(|e| format!("Cannot read gaming catalog: {e}"))?;
    value["catalog"]
        .as_array()
        .cloned()
        .ok_or_else(|| "Invalid gaming catalog".into())
}

pub struct Session {
    input: ChildStdin,
}
impl Session {
    pub fn send(&mut self, message: Value) -> Result<(), String> {
        writeln!(self.input, "{message}").map_err(|e| format!("Setup session ended: {e}"))
    }
}

pub fn start(mode: &str, selected: &[String]) -> Result<(Session, Receiver<Value>), String> {
    start_script(SCRIPT, mode, selected)
}

pub fn start_script(
    script: &str,
    mode: &str,
    selected: &[String],
) -> Result<(Session, Receiver<Value>), String> {
    let mut child = Command::new("/usr/bin/python3")
        .args(["-I", "-u", "-c", script, mode])
        .args(selected)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot start setup backend: {e}"))?;
    let input = child.stdin.take().ok_or("No backend input")?;
    let stdout = child.stdout.take().ok_or("No backend output")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let event = line
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string()));
            match event {
                Ok(value) => {
                    let _ = tx.send(value);
                }
                Err(error) => {
                    let _ = tx.send(serde_json::json!({"error": error}));
                }
            }
        }
        let result = child.wait();
        let _ = tx.send(
            serde_json::json!({"exited": true, "success": result.is_ok_and(|s| s.success())}),
        );
    });
    Ok((Session { input }, rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_catalog_and_stream_protocol() {
        let entries = catalog().unwrap();
        assert_eq!(entries.len(), 12);
        assert!(
            entries
                .iter()
                .any(|v| v["id"] == "steam" && v["recommended"] == true)
        );
        assert!(
            entries
                .iter()
                .any(|v| v["id"] == "faugus" && v["recommended"] == false)
        );
        let (_session, events) = start("catalog", &[]).unwrap();
        assert_eq!(
            events.recv().unwrap()["catalog"].as_array().unwrap().len(),
            12
        );
        assert_eq!(events.recv().unwrap()["done"], true);
        assert_eq!(events.recv().unwrap()["exited"], true);
    }
}

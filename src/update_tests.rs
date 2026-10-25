use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("archeasy-update-test-{id}"));
        fs::create_dir(&path).unwrap();
        for (name, script) in [
            (
                "sudo",
                "#!/usr/bin/bash\nprintf 'sudo %s\\n' \"$*\" >> \"$TRACE\"\nexit \"${PACMAN_RESULT:-0}\"\n",
            ),
            (
                "paru",
                "#!/usr/bin/bash\nprintf 'paru %s\\n' \"$*\" >> \"$TRACE\"\nexit \"${AUR_RESULT:-0}\"\n",
            ),
            (
                "flatpak",
                "#!/usr/bin/bash\nprintf 'flatpak %s\\n' \"$*\" >> \"$TRACE\"\nexit 0\n",
            ),
        ] {
            let file = path.join(name);
            fs::write(&file, script).unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }
    fn run(
        &self,
        helper: &str,
        pacman_result: &str,
        aur_result: &str,
    ) -> (std::process::Output, String) {
        let out = Command::new("/usr/bin/bash")
            .args(["-c", SYSTEM_UPDATE_SCRIPT, "test-update", helper])
            .env("PATH", format!("{}:/usr/bin", self.0.display()))
            .env("XDG_RUNTIME_DIR", &self.0)
            .env("TRACE", self.0.join("trace"))
            .env("PACMAN_RESULT", pacman_result)
            .env("AUR_RESULT", aur_result)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let trace = fs::read_to_string(self.0.join("trace")).unwrap_or_default();
        (out, trace)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn all_updates_run_in_order_with_user_and_system_flatpaks() {
    let fixture = Fixture::new();
    let (out, trace) = fixture.run("paru", "0", "0");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        trace,
        "sudo pacman -Syu\nparu -Sua\nflatpak update --user\nflatpak update --system\n"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("All update steps completed successfully")
    );
}
#[test]
fn failed_pacman_skips_aur_and_still_updates_flatpaks() {
    let fixture = Fixture::new();
    let (out, trace) = fixture.run("paru", "1", "0");
    assert!(!out.status.success());
    assert!(!trace.contains("paru"));
    assert!(trace.contains("flatpak update --user\nflatpak update --system"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("resolve the Pacman update error first"));
}
#[test]
fn missing_helper_is_reported_as_incomplete() {
    let fixture = Fixture::new();
    let (out, trace) = fixture.run("", "0", "0");
    assert!(!out.status.success());
    assert!(!trace.contains("paru"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("install paru or yay"));
}
#[test]
fn failed_aur_does_not_prevent_flatpak_updates() {
    let fixture = Fixture::new();
    let (out, trace) = fixture.run("paru", "0", "2");
    assert!(!out.status.success());
    assert!(trace.contains("flatpak update --system"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("failed or cancelled (exit 2)"));
}
#[test]
fn active_update_lock_prevents_duplicate_transactions() {
    let fixture = Fixture::new();
    let ready = fixture.0.join("ready");
    let mut holder = Command::new("/usr/bin/flock")
        .arg(fixture.0.join("archeasy-update.lock"))
        .args([
            "/usr/bin/bash",
            "-c",
            "touch \"$1\"; read -r _",
            "lock-holder",
        ])
        .arg(&ready)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if ready.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ready.exists());
    let (out, trace) = fixture.run("paru", "0", "0");
    drop(holder.stdin.take());
    let _ = holder.wait();
    assert!(!out.status.success());
    assert!(trace.is_empty());
    assert!(String::from_utf8_lossy(&out.stdout).contains("already running"));
}

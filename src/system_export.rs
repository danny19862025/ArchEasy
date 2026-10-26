use super::*;
use serde_json::{Value, json};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn packages(text: &str) -> Result<Vec<&str>, String> {
    text.lines()
        .map(|p| {
            if valid_id(p) && !p.contains('/') {
                Ok(p)
            } else {
                Err("Invalid package name in system inventory".into())
            }
        })
        .collect()
}
pub fn render(
    native: &str,
    foreign: &str,
    flatpaks: &Value,
    plan: &Value,
    gist: &str,
) -> Result<String, String> {
    if !gist.is_empty()
        && (!gist.starts_with("https://gist.github.com/")
            || gist.contains(char::is_whitespace)
            || gist.contains(['\'', '"', '`', '$', '#']))
    {
        return Err("Use a valid HTTPS GitHub Gist URL, or leave it empty.".into());
    }
    let native = packages(native)?
        .into_iter()
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ");
    let foreign = packages(foreign)?
        .into_iter()
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut script = format!(
        "#!/usr/bin/env bash\n# ArchEasy system export. Review before running on Arch Linux.\n# Package versions follow the current repositories, not a version lock.\n# Foreign packages may need their original source if unavailable in the AUR.\n# Config Gist: {}\nset -euo pipefail\n[[ $EUID != 0 ]] || {{ printf 'Run as your desktop user, not root.\\n'; exit 1; }}\nread -r -p 'Install this package selection and restore its bundled configs? [y/N] ' answer\n[[ $answer == y || $answer == Y ]] || exit 0\nnative=({native})\nforeign=({foreign})\nsudo pacman -Syu --needed -- python git base-devel \"${{native[@]}}\"\n",
        if gist.is_empty() {
            "not published; configs are bundled below"
        } else {
            gist
        }
    );
    script.push_str("if (( ${#foreign[@]} )); then\n  if command -v paru >/dev/null; then helper=paru; elif command -v yay >/dev/null; then helper=yay; else\n    build=$(mktemp -d); git clone -- https://aur.archlinux.org/paru.git \"$build/paru\"\n    ( cd -- \"$build/paru\"; cat PKGBUILD; read -r -p 'Build reviewed paru PKGBUILD? [y/N] ' answer; [[ $answer == y || $answer == Y ]] && makepkg -si )\n    helper=paru\n  fi\n  \"$helper\" -S --needed -- \"${foreign[@]}\"\nfi\n");
    let apps = flatpaks["apps"]
        .as_array()
        .ok_or("Invalid Flatpak inventory")?;
    let remotes = flatpaks["remotes"]
        .as_array()
        .ok_or("Invalid Flatpak remotes")?;
    if !apps.is_empty() {
        script.push_str("sudo pacman -S --needed -- flatpak\n");
    }
    for remote in remotes {
        let scope = remote["scope"].as_str().ok_or("Missing remote scope")?;
        let name = remote["name"].as_str().ok_or("Missing remote name")?;
        let url = remote["url"].as_str().ok_or("Missing remote URL")?;
        if !["--user", "--system"].contains(&scope)
            || !valid_id(name)
            || name.contains('/')
            || !url.starts_with("https://")
        {
            return Err(format!(
                "Remote {name} is not portable. Export requires HTTPS remotes."
            ));
        }
        script.push_str(&format!(
            "{}flatpak remote-add {scope} --if-not-exists -- {} {}\n",
            if scope == "--system" { "sudo " } else { "" },
            quote(name),
            quote(url)
        ));
    }
    for app in apps {
        let scope = app["scope"].as_str().ok_or("Missing app scope")?;
        let remote = app["remote"].as_str().ok_or("Missing app remote")?;
        let id = app["id"].as_str().ok_or("Missing app ID")?;
        if !["--user", "--system"].contains(&scope) || !valid_id(remote) || !valid_id(id) {
            return Err("Invalid Flatpak inventory entry".into());
        }
        script.push_str(&format!(
            "{}flatpak install {scope} -- {} {}\n",
            if scope == "--system" { "sudo " } else { "" },
            quote(remote),
            quote(id)
        ));
    }
    let payload = glib::base64_encode(plan.to_string().as_bytes());
    script.push_str(&format!("python3 - <<'ARCHEASY_CONFIG_RESTORE'\nPLAN_PAYLOAD_B64 = {}\n{}\nARCHEASY_CONFIG_RESTORE\nprintf '\\nArchEasy restore completed. Restart apps or choose your desktop session at login.\\n'\n", quote(&payload), include_str!("../scripts/restore-exported-configs.py")));
    Ok(script)
}
pub(crate) fn inventory(gist: &str) -> Result<String, String> {
    let native = output("pacman", &["-Qqen"])?;
    let foreign = match output("pacman", &["-Qqem"]) {
        Err(e) if e.is_empty() => String::new(),
        other => other?,
    };
    let mut apps = vec![];
    let mut remotes = vec![];
    if available("flatpak") {
        for scope in ["--user", "--system"] {
            for line in
                output("flatpak", &["list", scope, "--app", "--columns=ref,origin"])?.lines()
            {
                if let Some((id, remote)) = line.split_once('\t') {
                    apps.push(json!({"scope":scope,"id":id,"remote":remote}));
                }
            }
            for line in output("flatpak", &["remotes", scope, "--columns=name,url"])?.lines() {
                if let Some((name, url)) = line.split_once('\t') {
                    remotes.push(json!({"scope":scope,"name":name,"url":url}));
                }
            }
        }
    }
    render(
        &native,
        &foreign,
        &json!({"apps":apps,"remotes":remotes}),
        &configs::system_plan_json()?,
        gist,
    )
}
pub fn save(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(contents.as_bytes())
        .map_err(|e| e.to_string())
}
#[allow(deprecated)]
pub fn button(page: &gtk::Box, parent: &gtk::ApplicationWindow) {
    let export = gtk::Button::with_label("Export my system · my-system.sh…");
    export.set_widget_name("export-my-system");
    export.set_halign(gtk::Align::Start);
    page.append(&export);
    let weak = parent.downgrade();
    export.connect_clicked(move |_| { let Some(parent) = weak.upgrade() else { return; };
        let dialog = gtk::Dialog::builder().title("Export my system").transient_for(&parent).modal(true).default_width(620).build(); crate::about::follow_theme(&parent, dialog.upcast_ref());
        let body = dialog.content_area(); system_tools::margins(&body);
        let note = label("Creates a reviewable script with explicit official and foreign packages, Flatpaks/remotes, and your currently applied configs. Config contents may contain private details. It recreates your selection using current packages; it does not clone disks, credentials, user files or exact package versions.", "subtitle"); note.set_wrap(true); body.append(&note);
        let publish = gtk::Button::with_label("Publish applied configs to GitHub…"); let weak = parent.downgrade(); publish.connect_clicked(move |_| { if let Some(parent) = weak.upgrade() { configs::publish_system_configs(&parent); } }); body.append(&publish);
        let gist = gtk::Entry::builder().placeholder_text("Optional Gist URL from your published configs").build(); body.append(&gist);
        let status = label("Bundled configs make the script portable even without a published Gist.", "subtitle"); status.set_wrap(true); body.append(&status);
        dialog.add_button("Close", gtk::ResponseType::Close); dialog.add_button("Generate & save script…", gtk::ResponseType::Accept);
        let weak = parent.downgrade(); dialog.connect_response(move |dialog, response| { if response != gtk::ResponseType::Accept { dialog.close(); return; } let Some(parent) = weak.upgrade() else { return; }; let link = gist.text().trim().to_owned(); let weak = parent.downgrade(); let status_done = status.clone();
            system_tools::task(&status, move || inventory(&link), move |script| { let Some(parent) = weak.upgrade() else { return; }; let status = status_done.clone(); gtk::FileDialog::builder().title("Save system restore script").initial_name("my-system.sh").build().save(Some(&parent), gtk::gio::Cancellable::NONE, move |result| { if let Ok(file) = result && let Some(path) = file.path() { match save(&path, &script) { Ok(()) => status.set_text(&format!("Saved {}. Review it before running on the new machine.", path.display())), Err(e) => status.set_text(&e) } } }); });
        }); dialog.present();
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_escapes_data_and_is_valid_bash() {
        let plan = json!({"files":[{"path":"fish/config.fish","bytes":[36,40,41]}]});
        let text = render(
            "fish\nkitty\n",
            "example-bin\n",
            &json!({"apps":[],"remotes":[]}),
            &plan,
            "",
        )
        .unwrap();
        assert!(text.contains("native=('fish' 'kitty')"));
        let mut child = Command::new("bash")
            .arg("-n")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert!(render("--root\n", "", &json!({"apps":[],"remotes":[]}), &plan, "").is_err());
    }
    #[test]
    fn bundled_config_payload_bypasses_environment_argument_limits() {
        let dir = std::env::temp_dir().join(format!(
            "archeasy-export-large-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let data = vec![b'a'; 200_000];
        let plan = json!({"files":[{"path":"large/config","bytes":data}]});
        let script = render("fish", "", &json!({"apps":[],"remotes":[]}), &plan, "").unwrap();
        let python = script
            .split("python3 - <<'ARCHEASY_CONFIG_RESTORE'\n")
            .nth(1)
            .unwrap()
            .split("\nARCHEASY_CONFIG_RESTORE")
            .next()
            .unwrap();
        let mut child = Command::new("python3")
            .arg("-")
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("XDG_DATA_HOME", dir.join("data"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(python.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            std::fs::read(dir.join("config/large/config"))
                .unwrap()
                .len(),
            200_000
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

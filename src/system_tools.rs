#![allow(deprecated)]
use super::*;
pub fn task<T: Send + 'static>(
    status: &gtk::Label,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(T) + 'static,
) {
    task_with_finish(status, work, done, || {});
}
pub fn task_with_finish<T: Send + 'static>(
    status: &gtk::Label,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl Fn(T) + 'static,
    finish: impl Fn() + 'static,
) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(work());
    });
    let status = status.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match rx.try_recv() {
            Ok(Ok(value)) => done(value),
            Ok(Err(e)) => status.set_text(&e),
            Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(_) => status.set_text("Task stopped unexpectedly"),
        }
        finish();
        glib::ControlFlow::Break
    });
}
pub fn update_dialog(parent: &gtk::ApplicationWindow, status: &gtk::Label) {
    let dialog = gtk::Dialog::builder()
        .title("Review system update")
        .transient_for(parent)
        .modal(true)
        .default_width(550)
        .build();
    crate::about::follow_theme(parent, dialog.upcast_ref());
    dialog.set_widget_name("system-update-snapshot-dialog");
    let body = dialog.content_area();
    margins(&body);
    let summary = adw::PreferencesGroup::new();
    summary.set_title("Update summary");
    let totals = adw::ActionRow::builder()
        .title("Checking available updates…")
        .subtitle("No system changes have started")
        .build();
    totals.add_prefix(&crate::ux::icon("software-update-available-symbolic"));
    summary.add(&totals);
    let helper = ["paru", "yay"].into_iter().find(|name| available(name));
    let commands = format!(
        "Will run sudo pacman -Syu{}{}",
        helper
            .map(|h| format!(" then {h} -Sua"))
            .unwrap_or_else(|| "; AUR skipped (no paru/yay)".into()),
        if available("flatpak") {
            ", then flatpak update --user / --system"
        } else {
            ""
        }
    );
    let command_row = adw::ActionRow::builder()
        .title("Review commands")
        .subtitle(&commands)
        .build();
    command_row.set_subtitle_lines(3);
    summary.add(&command_row);
    body.append(&summary);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(update_summary(helper));
    });
    let start = dialog.add_button("Start update", gtk::ResponseType::Accept);
    start.set_sensitive(false);
    let totals_weak = totals.downgrade();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let Some(totals) = totals_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        match rx.try_recv() {
            Ok(text) => {
                start.set_sensitive(true);
                totals.set_title(&text);
                totals.set_subtitle("Pacman uses cached repository metadata; the transaction refreshes it. Counts may change.");
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => {
                start.set_sensitive(true);
                totals.set_title("Update check unavailable; review the terminal transaction");
                glib::ControlFlow::Break
            }
        }
    });
    let note = label(
        "Create a snapshot before updating? Timeshift must already be configured; Snapper uses its root configuration. A snapshot failure stops the update. Snapshots cover the configured filesystem and are not a backup of every disk or home folder.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let picker = gtk::ComboBoxText::new();
    picker.append(Some("none"), "Update without a snapshot");
    for (id, name) in [
        ("timeshift", "Create Timeshift snapshot first"),
        ("snapper", "Create Snapper root snapshot first"),
    ] {
        if available(id) {
            picker.append(Some(id), name);
        }
    }
    if available("timeshift") {
        picker.set_active_id(Some("timeshift"));
    } else if available("snapper") {
        picker.set_active_id(Some("snapper"));
    } else {
        picker.set_active_id(Some("none"));
        body.append(&label("No snapshot tool found. Configure Timeshift or Snapper first to enable snapshot creation.", "subtitle"));
    }
    body.append(&picker);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let status = status.clone();
    dialog.connect_response(move |dialog, response| { if response == gtk::ResponseType::Accept { let snapshot = picker.active_id().unwrap_or_else(|| "none".into()); match system_update(&snapshot) { Ok(()) => { status.set_text("Update terminal opened. Snapshot creation runs first if selected; review its result and the package prompts."); status.set_visible(true); }, Err(e) => status.set_text(&e) } } dialog.close(); });
    dialog.present();
}
fn update_count(program: &str, args: &[&str], empty_exit: bool) -> String {
    let result = Command::new("timeout")
        .args(["30", program])
        .args(args)
        .env("LC_ALL", "C")
        .output();
    match result {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            .to_string(),
        Ok(out)
            if empty_exit
                && out.status.code() == Some(1)
                && out.stdout.is_empty()
                && out.stderr.is_empty() =>
        {
            "0".into()
        }
        _ => "unknown".into(),
    }
}
fn update_summary(helper: Option<&str>) -> String {
    let official = update_count("pacman", &["-Quq"], true);
    let aur = helper
        .map(|h| update_count(h, &["-Qua"], true))
        .unwrap_or_else(|| "unavailable".into());
    let flatpak = if available("flatpak") {
        format!(
            " • {} user + {} system Flatpak",
            update_count(
                "flatpak",
                &["remote-ls", "--updates", "--user", "--columns=application"],
                false
            ),
            update_count(
                "flatpak",
                &[
                    "remote-ls",
                    "--updates",
                    "--system",
                    "--columns=application"
                ],
                false
            )
        )
    } else {
        String::new()
    };
    format!("Found {official} Pacman + {aur} AUR updates{flatpak}")
}
pub fn margins(body: &gtk::Box) {
    body.set_spacing(12);
    body.set_margin_top(18);
    body.set_margin_bottom(18);
    body.set_margin_start(18);
    body.set_margin_end(18);
}
pub fn driver_report(kernel: &str, packages: &str, pci: &str, module: bool) -> String {
    let has = |name: &str| packages.lines().any(|p| p == name);
    let kernels: Vec<_> = ["linux", "linux-zen", "linux-lts", "linux-hardened"]
        .into_iter()
        .filter(|k| has(k))
        .collect();
    let mut report = format!(
        "Running kernel: {}\nInstalled kernel packages: {}\n\nGPU hardware:\n{}\n",
        kernel.trim(),
        if kernels.is_empty() {
            "No standard kernel package detected".into()
        } else {
            kernels.join(", ")
        },
        pci.trim()
    );
    if pci.contains("PCI listing unavailable") {
        report.push_str("\nGPU vendor detection unavailable; install pciutils and retry.\n");
    } else if pci.to_lowercase().contains("nvidia") {
        let dkms = [
            "nvidia-dkms",
            "nvidia-open-dkms",
            "nvidia-470xx-dkms",
            "nvidia-390xx-dkms",
        ]
        .iter()
        .any(|p| has(p));
        if dkms {
            report.push_str("\nNVIDIA DKMS package detected.\n");
            for k in &kernels {
                let headers = format!("{k}-headers");
                if !has(&headers) {
                    report.push_str(&format!(
                        "Missing {headers}: DKMS needs headers for this kernel.\n"
                    ));
                }
            }
        } else if kernels
            .iter()
            .any(|k| ["linux-zen", "linux-hardened"].contains(k))
        {
            report.push_str("\nCheck NVIDIA support: a custom kernel is installed but no known NVIDIA DKMS package was detected. If you use the NVIDIA driver, choose the package matching your GPU and install matching kernel headers. Nouveau users may not need NVIDIA packages.\n");
        } else if !module
            && !["nvidia", "nvidia-lts", "nvidia-open", "nvidia-open-lts"]
                .iter()
                .any(|p| has(p))
        {
            report.push_str("\nNVIDIA hardware detected; no NVIDIA driver package or loaded nvidia module detected. Nouveau or a disabled GPU may be intentional. Check the driver guide before changing packages.\n");
        } else {
            report.push_str("\nKernel-specific NVIDIA package or loaded module detected. DKMS is not required for every setup.\n");
        }
    } else {
        report.push_str("\nNo NVIDIA hardware detected in the PCI listing.\n");
    }
    report.push_str("\nThis helper does not install drivers or change the boot kernel.");
    report
}
pub fn page(parent: &gtk::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.append(&label("Kernel & driver helper", "name"));
    let refresh = gtk::Button::with_label("Detect kernels & graphics");
    refresh.set_widget_name("detect-kernel-drivers");
    refresh.set_halign(gtk::Align::Start);
    page.append(&refresh);
    let text = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .vexpand(true)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .vexpand(true)
        .build();
    page.append(&scroll);
    let status = label(
        "Click Detect to inspect installed kernel packages and GPU hardware.",
        "subtitle",
    );
    status.set_wrap(true);
    page.append(&status);
    refresh.connect_clicked(move |button| {
        button.set_sensitive(false);
        let text = text.clone();
        let status_done = status.clone();
        let weak = button.downgrade();
        task_with_finish(
            &status,
            || {
                let kernel = output("uname", &["-r"])?;
                let packages = output("pacman", &["-Qq"])?;
                let pci = output("lspci", &["-nn"]).unwrap_or_else(|_| {
                    "PCI listing unavailable. Install pciutils to inspect GPU hardware.".into()
                });
                let graphics = if pci.starts_with("PCI listing unavailable") {
                    pci
                } else {
                    pci.lines()
                        .filter(|l| {
                            l.contains("VGA")
                                || l.contains("3D controller")
                                || l.contains("Display controller")
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(driver_report(
                    &kernel,
                    &packages,
                    &graphics,
                    std::path::Path::new("/sys/module/nvidia").exists(),
                ))
            },
            move |report| {
                text.buffer().set_text(&report);
                status_done.set_text(
                    "Detection completed. Review the guide before changing driver packages.",
                );
            },
            move || {
                if let Some(button) = weak.upgrade() {
                    button.set_sensitive(true);
                }
            },
        );
    });
    page.append(&gtk::LinkButton::with_label(
        "https://wiki.archlinux.org/title/NVIDIA",
        "Arch NVIDIA driver guide ↗",
    ));
    system_export::button(&page, parent);
    snapshots::button(&page, parent);
    page
}
#[cfg(test)]
mod tests {
    #[test]
    fn driver_variants_avoid_blanket_dkms_warning() {
        let report = super::driver_report("6.18", "linux\nnvidia-open\n", "NVIDIA VGA", true);
        assert!(report.contains("DKMS is not required"));
        let report = super::driver_report("zen", "linux-zen\n", "NVIDIA VGA", false);
        assert!(report.contains("no known NVIDIA DKMS"));
    }
}

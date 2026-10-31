//! Integrated GTK workbench pages. All system operations go through management.
use crate::management::{entry, inspect, read_button, terminal_button};
use gtk::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

type Samples = Rc<RefCell<VecDeque<Option<f64>>>>;
fn graph(body: &gtk::Box, title: &str) -> (Samples, gtk::DrawingArea, gtk::Label) {
    let label = crate::label(title, "heading");
    body.append(&label);
    let samples: Samples = Rc::new(RefCell::new(VecDeque::new()));
    let area = gtk::DrawingArea::builder()
        .content_height(65)
        .hexpand(true)
        .build();
    let values = samples.clone();
    area.set_draw_func(move |_, context, width, height| {
        context.set_source_rgba(0.5, 0.5, 0.5, 0.2);
        context.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = context.stroke();
        context.set_source_rgb(0.2, 0.55, 0.95);
        context.set_line_width(2.0);
        let values = values.borrow();
        let mut connected = false;
        for (index, value) in values.iter().enumerate() {
            if let Some(value) = value {
                let x = index as f64 * width as f64 / 119.0;
                let y = height as f64 * (1.0 - value.clamp(0.0, 100.0) / 100.0);
                if connected {
                    context.line_to(x, y);
                } else {
                    context.move_to(x, y);
                }
                connected = true;
            } else {
                connected = false;
            }
        }
        let _ = context.stroke();
    });
    body.append(&area);
    (samples, area, label)
}
fn percent(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1}%"))
        .unwrap_or_else(|| "Unavailable".into())
}
fn sample_delta(before: &Value, after: &Value) -> Option<f64> {
    let total = after["cpu_ticks"]["total"].as_f64()? - before["cpu_ticks"]["total"].as_f64()?;
    let idle = after["cpu_ticks"]["idle"].as_f64()? - before["cpu_ticks"]["idle"].as_f64()?;
    if total <= 0.0 || idle < 0.0 || idle > total {
        None
    } else {
        Some((total - idle) / total * 100.0)
    }
}
fn update_graph(graph: &(Samples, gtk::DrawingArea, gtk::Label), title: &str, value: Option<f64>) {
    let mut values = graph.0.borrow_mut();
    values.push_back(value);
    if values.len() > 120 {
        values.pop_front();
    }
    graph.2.set_text(&format!("{title}: {}", percent(value)));
    graph.1.queue_draw();
}

fn health(
    body: &gtk::Box,
    status: &gtk::Label,
    report: &gtk::TextView,
    parent: &gtk::ApplicationWindow,
) {
    let interval = gtk::SpinButton::with_range(2.0, 60.0, 1.0);
    interval.set_value(5.0);
    body.append(&crate::label("Monitoring interval (seconds); only polls while this page is visible. Graphs retain the last 120 samples in this session.", "subtitle"));
    body.append(&interval);
    let refresh = gtk::Button::with_label("Refresh Metrics");
    body.append(&refresh);
    let cpu = graph(body, "CPU");
    let gpu = graph(body, "GPU (first readable device)");
    let ram = graph(body, "Memory");
    let storage = gtk::ProgressBar::builder().show_text(true).build();
    body.append(&storage);
    let overview = crate::label("Collecting the first sample…", "subtitle");
    overview.set_wrap(true);
    overview.set_selectable(true);
    body.append(&overview);
    let busy = Rc::new(Cell::new(false));
    let previous: Rc<RefCell<Value>> = Rc::new(RefCell::new(Value::Null));
    let last = Rc::new(Cell::new(Instant::now()));
    let status = status.clone();
    let interval_timer = interval.clone();
    let last_timer = last.clone();
    let weak = refresh.downgrade();
    gtk::glib::timeout_add_local(Duration::from_secs(1), move || {
        let Some(button) = weak.upgrade() else {
            return gtk::glib::ControlFlow::Break;
        };
        if button.is_mapped() && last_timer.get().elapsed().as_secs_f64() >= interval_timer.value()
        {
            button.emit_clicked();
        }
        gtk::glib::ControlFlow::Continue
    });
    refresh.connect_clicked(move |_| {
        if busy.replace(true) { return; }
        last.set(Instant::now());
        let (previous, overview, storage, busy) = (previous.clone(), overview.clone(), storage.clone(), busy.clone());
        let (cpu, gpu, ram) = (cpu.clone(), gpu.clone(), ram.clone());
        crate::system_tools::task_with_finish(&status, || {
            let output = inspect(json!({"action":"sample"}))?;
            serde_json::from_str::<Value>(&output).map_err(|e| e.to_string())
        }, move |value| {
            update_graph(&cpu, "CPU", sample_delta(&previous.borrow(), &value));
            let gpu_value = value["gpu"].as_array().and_then(|items| items.iter().find_map(|item| item["utilisation"].as_f64()));
            update_graph(&gpu, "GPU", gpu_value);
            update_graph(&ram, "Memory", value["memory"]["used_percent"].as_f64());
            if let Some(used) = value["storage"]["used_percent"].as_f64() { storage.set_fraction((used / 100.0).clamp(0.0, 1.0)); storage.set_text(Some(&format!("Root storage {used:.1}% used"))); }
            let elapsed = value["time"].as_f64().unwrap_or(0.0) - previous.borrow()["time"].as_f64().unwrap_or(0.0);
            let mut network = Vec::new();
            if elapsed > 0.0 && !previous.borrow().is_null() && let Some(interfaces) = value["network"].as_object() {
                for (name, counters) in interfaces {
                    let before = previous.borrow();
                    if let (Some(rx), Some(tx), Some(old_rx), Some(old_tx)) = (counters["rx"].as_u64(), counters["tx"].as_u64(), before["network"][name]["rx"].as_u64(), before["network"][name]["tx"].as_u64()) && rx >= old_rx && tx >= old_tx {
                        network.push(format!("{name}: ↓ {:.0} B/s ↑ {:.0} B/s", (rx-old_rx) as f64 / elapsed, (tx-old_tx) as f64 / elapsed));
                    }
                }
            }
            let frequency = value["frequency_khz"].as_f64().map(|number| format!("{number:.0} kHz")).unwrap_or_else(|| "Unavailable".into());
            let swap = value["memory"]["swap_used"].as_f64().map(|number| format!("{:.0} MiB", number / 1048576.0)).unwrap_or_else(|| "Unavailable".into());
            let temperatures: Vec<_> = value["sensors"].as_array().into_iter().flatten().filter_map(|sensor| Some(format!("{}: {:.1} °C", sensor["sensor"].as_str()?, sensor["celsius"].as_f64()?))).collect();
            let graphics: Vec<_> = value["gpu"].as_array().into_iter().flatten().map(|device| format!("{} · {} · VRAM used/total: {} / {}", device["device"].as_str().unwrap_or("GPU"), device["model"].as_str().unwrap_or("model unavailable"), device["vram_used"].as_f64().map(|v| format!("{:.0} MiB", v / 1048576.0)).unwrap_or_else(|| "Unavailable".into()), device["vram_total"].as_f64().map(|v| format!("{:.0} MiB", v / 1048576.0)).unwrap_or_else(|| "Unavailable".into()))).collect();
            overview.set_text(&format!("{} · kernel {} · uptime {} s\nCPU0 frequency: {frequency} · Swap used: {swap}\n{}\nSensors: {}\nGPU: {}\nNo universal temperature safety limit is assumed. Use Diagnostic Report for PCI model/driver details.", value["cpu_model"].as_str().unwrap_or("Unavailable"), value["kernel"].as_str().unwrap_or("Unavailable"), value["uptime_seconds"].as_str().unwrap_or("Unavailable"), if network.is_empty() { "Network rate: unavailable until two valid samples".into() } else { network.join("\n") }, if temperatures.is_empty() { "Unavailable".into() } else { temperatures.join(" · ") }, if graphics.is_empty() { "Unavailable".into() } else { graphics.join("\n") }));
            *previous.borrow_mut() = value;
        }, move || busy.set(false));
    });
    for (title, action) in [
        ("System Status / Diagnostic Report", "health"),
        ("Process Viewer", "processes"),
        ("Failed Services Viewer", "services"),
    ] {
        read_button(
            body,
            title,
            json!({"action":action}),
            report,
            &status_for(body),
        );
    }
    let export = gtk::Button::with_label("Export Diagnostic Report…");
    body.append(&export);
    let weak = parent.downgrade();
    let report = report.clone();
    export.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else {
            return;
        };
        let buffer = report.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        gtk::FileDialog::builder()
            .title("Save displayed diagnostics — may include personal paths and network addresses")
            .initial_name("archeasy-diagnostics.txt")
            .build()
            .save(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result
                    && let Some(path) = file.path()
                {
                    use std::io::Write;
                    use std::os::unix::fs::OpenOptionsExt;
                    let result = std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .mode(0o600)
                        .open(path)
                        .and_then(|mut file| file.write_all(text.as_bytes()));
                    if let Err(error) = result {
                        crate::show_error(&format!("Report not saved: {error}"));
                    }
                }
            });
    });
}
fn status_for(body: &gtk::Box) -> gtk::Label {
    let status = crate::label("Ready", "subtitle");
    status.set_wrap(true);
    body.append(&status);
    status
}

fn creator(body: &gtk::Box, status: &gtk::Label, report: &gtk::TextView) {
    let groups: [(&str, &[&str]); 6] = [
        (
            "Creator Essentials",
            &["obs-studio", "kdenlive", "blender", "gimp", "audacity"],
        ),
        ("Video Studio", &["obs-studio", "kdenlive", "shotcut"]),
        ("Graphic Design Suite", &["gimp", "krita", "inkscape"]),
        ("Audio Studio", &["audacity", "ardour"]),
        ("3D Studio", &["blender"]),
        ("Photography", &["darktable", "rawtherapee"]),
    ];
    let packages = [
        "obs-studio",
        "kdenlive",
        "shotcut",
        "blender",
        "gimp",
        "krita",
        "inkscape",
        "audacity",
        "ardour",
        "darktable",
        "rawtherapee",
    ];
    let choices: Vec<_> = packages
        .iter()
        .map(|name| ((*name).to_string(), gtk::CheckButton::with_label(name)))
        .collect();
    for (name, selected) in groups {
        let button = gtk::Button::with_label(&format!("Select {name}"));
        let choices = choices.clone();
        button.connect_clicked(move |_| {
            for (package, check) in &choices {
                check.set_active(selected.contains(&package.as_str()));
            }
        });
        body.append(&button);
    }
    body.append(&crate::label("Custom Selection — nothing selected by default. Official packages only; review dependency/download/installed sizes before approval. Flatpak alternatives are available in Software Centre.", "subtitle"));
    for (_, check) in &choices {
        body.append(check);
    }
    for (title, action) in [
        (
            "Review Availability, Versions, Dependencies and Sizes",
            "creator-plan",
        ),
        ("Install Selected Applications…", "creator-install"),
    ] {
        let button = gtk::Button::with_label(title);
        body.append(&button);
        let choices = choices.clone();
        let status = status.clone();
        let report = report.clone();
        button.connect_clicked(move |_| {
            let selected: Vec<_> = choices.iter().filter(|(_, check)| check.is_active()).map(|(name, _)| name.clone()).collect();
            let request = json!({"action":action, "packages":selected});
            if action == "creator-install" { match crate::management::launch(request) { Ok(()) => status.set_text("Review the package plan and approve in the terminal. No AUR or upstream scripts are used."), Err(error) => status.set_text(&error) } }
            else { let report = report.clone(); crate::system_tools::task(&status, move || inspect(request), move |result| report.buffer().set_text(&result)); }
        });
    }
}

pub fn page(parent: &gtk::ApplicationWindow, kind: &str) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    crate::system_tools::margins(&body);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(
        &gtk::ScrolledWindow::builder()
            .child(&body)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    body.append(&crate::label(kind, "name"));
    let status = status_for(&body);
    let report = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    report.buffer().set_text("Ready. Read-only checks do not change settings. Unavailable tools and unsupported features are reported explicitly.");
    match kind {
        "System Health" => health(&body, &status, &report, parent),
        "Creator Studio" => creator(&body, &status, &report),
        "Desktop Customisation" => {
            read_button(
                &body,
                "Open Native Desktop Settings",
                json!({"action":"native-settings"}),
                &report,
                &status,
            );
            read_button(
                &body,
                "Detect Desktop / Themes and Compatibility",
                json!({"action":"desktop"}),
                &report,
                &status,
            );
            let preset = entry(
                &body,
                "GNOME preset: Minimal, Modern Dark, Classic Desktop, Developer Workstation or Gaming Desktop",
            );
            terminal_button(
                &body,
                "Apply GNOME Colour Preference…",
                "appearance",
                vec![("preset", preset)],
                &status,
            );
            terminal_button(
                &body,
                "Restore Previous GNOME Appearance…",
                "appearance-restore",
                vec![],
                &status,
            );
            body.append(&crate::label("Other desktops: use the native appearance panel for themes, icons, cursors, fonts, GTK/Qt and wallpaper. Configs provides managed presets with previews, conflict checks and rollback. No desktop/display manager is replaced.", "subtitle"));
        }
        "System Rescue" => {
            read_button(
                &body,
                "Diagnose Boot, EFI, Kernels, fstab and Services",
                json!({"action":"rescue"}),
                &report,
                &status,
            );
            read_button(
                &body,
                "Recovery Environment / Initramfs / Package Guidance",
                json!({"action":"rescue-guide"}),
                &report,
                &status,
            );
        }
        "Disk Manager" => {
            let scan = gtk::Button::with_label("Visual Drive / Partition Usage");
            let layout = gtk::Box::new(gtk::Orientation::Vertical, 6);
            body.append(&scan);
            body.append(&layout);
            let scan_status = status.clone();
            scan.connect_clicked(move |_| {
                let layout = layout.clone();
                crate::system_tools::task(
                    &scan_status,
                    || {
                        let output = inspect(json!({"action":"disk-layout"}))?;
                        serde_json::from_str::<Value>(&output).map_err(|error| error.to_string())
                    },
                    move |data| {
                        while let Some(child) = layout.first_child() {
                            layout.remove(&child);
                        }
                        for row in data.as_array().into_iter().flatten() {
                            let title = format!(
                                "{}{} · {} · {} bytes · {}",
                                "  ".repeat(row["depth"].as_u64().unwrap_or(0).min(16) as usize),
                                row["name"].as_str().unwrap_or("unknown"),
                                row["type"].as_str().unwrap_or("unknown"),
                                row["size"],
                                row["fstype"].as_str().unwrap_or("no filesystem")
                            );
                            let label = crate::label(&title, "subtitle");
                            label.set_wrap(true);
                            layout.append(&label);
                            let usage = row["fsuse%"]
                                .as_str()
                                .and_then(|text| text.trim_end_matches('%').parse::<f64>().ok());
                            if let Some(value) = usage {
                                let bar = gtk::ProgressBar::builder().show_text(true).build();
                                bar.set_fraction((value / 100.0).clamp(0.0, 1.0));
                                bar.set_text(Some(&format!("{value:.1}% filesystem used")));
                                layout.append(&bar);
                            } else {
                                layout.append(&crate::label(
                                    "Filesystem usage unavailable (not zero)",
                                    "subtitle",
                                ));
                            }
                        }
                    },
                );
            });

            read_button(
                &body,
                "Scan Drives, Partitions, Encryption and Usage",
                json!({"action":"disks"}),
                &report,
                &status,
            );
            read_button(
                &body,
                "Review Cache, Orphans, Logs and Temporary File Policy",
                json!({"action":"disk-maintenance"}),
                &report,
                &status,
            );
            let device = entry(
                &body,
                "Physical drive, e.g. /dev/nvme0n1 — read-only SMART; may require privileges",
            );
            let button = gtk::Button::with_label("Inspect SMART Health");
            body.append(&button);
            let status = status.clone();
            let report = report.clone();
            button.connect_clicked(move |_| {
                let request = json!({"action":"smart", "device":device.text().as_str()});
                let report = report.clone();
                crate::system_tools::task(
                    &status,
                    move || inspect(request),
                    move |result| report.buffer().set_text(&result),
                );
            });
            body.append(&crate::label("Selected cache/orphan cleanup remains in System Utilities. This page performs no deletion or repair.", "subtitle"));
        }
        "Network & Security" => {
            for (title, action) in [
                ("Network / DNS / Service Diagnostics", "network"),
                ("Test DNS and HTTPS Connectivity", "connectivity"),
                ("Inspect Firewall / SSH / WireGuard", "security"),
            ] {
                read_button(&body, title, json!({"action":action}), &report, &status);
            }
        }
        "Laptop Optimisation" => {
            read_button(
                &body,
                "Battery / Power / Lid / Suspend Diagnostics",
                json!({"action":"laptop"}),
                &report,
                &status,
            );
            let profile = entry(&body, "Power profile: power-saver, balanced or performance");
            terminal_button(
                &body,
                "Select Existing Power Profile…",
                "power-profile",
                vec![("profile", profile)],
                &status,
            );
            let brightness = entry(&body, "Backlight brightness: 10–100 percent");
            terminal_button(
                &body,
                "Set Supported Backlight…",
                "brightness",
                vec![("percent", brightness)],
                &status,
            );
        }
        "Migration & Dotfiles" => {
            body.append(&crate::label("Private versioned manifests; explicitly select individual user configs. Common secrets and hardware-specific paths are refused. Imports stage files for review and never overwrite live dotfiles or execute scripts.", "subtitle"));
            let paths = entry(
                &body,
                "Relative files separated by commas, e.g. .bashrc,.config/kitty/kitty.conf",
            );
            let foreign = gtk::CheckButton::with_label(
                "Include optional foreign/AUR package names (never build scripts)",
            );
            body.append(&foreign);
            let destination = entry(&body, "Export manifest: new absolute JSON file path");
            let export =
                gtk::Button::with_label("Review and Export Selected Dotfiles / Package Manifest…");
            body.append(&export);
            let status_export = status.clone();
            let paths_export = paths.clone();
            export.connect_clicked(move |_| {
                let selected: Vec<_> = paths_export.text().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect();
                match crate::management::launch(json!({"action":"migration-export", "paths":selected, "foreign":foreign.is_active(), "destination":destination.text().as_str()})) { Ok(()) => status_export.set_text("Review paths and approve in the terminal. Secret scanning can refuse export."), Err(error) => status_export.set_text(&error) }
            });
            let source = entry(
                &body,
                "Import: absolute path to an ArchEasy migration JSON manifest",
            );
            let preview = gtk::Button::with_label(
                "Import Configuration / Preview Changes / Compatibility Report",
            );
            body.append(&preview);
            let (done, text, input) = (status.clone(), report.clone(), source.clone());
            preview.connect_clicked(move |_| {
                let request = json!({"action":"migration-preview", "source":input.text().as_str()});
                let text = text.clone();
                crate::system_tools::task(
                    &done,
                    move || inspect(request),
                    move |result| text.buffer().set_text(&result),
                );
            });
            let target = entry(
                &body,
                "Restore staging: new absolute folder (must not exist)",
            );
            let stage = gtk::Button::with_label("Stage Selected Dotfiles in New Folder…");
            body.append(&stage);
            let status = status.clone();
            stage.connect_clicked(move |_| {
                let selected: Vec<_> = paths.text().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect();
                match crate::management::launch(json!({"action":"migration-stage", "source":source.text().as_str(), "destination":target.text().as_str(), "paths":selected})) { Ok(()) => status.set_text("Review selected paths in terminal; current dotfiles will stay untouched."), Err(error) => status.set_text(&error) }
            });
            crate::snapshots::button(&body, parent);
            crate::system_export::button(&body, parent);
        }
        _ => {}
    }
    read_button(
        &body,
        "View Operation Logs",
        json!({"action":"history"}),
        &report,
        &status,
    );
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&report)
            .min_content_height(260)
            .vexpand(true)
            .build(),
    );
    wrap_labels(body.upcast_ref());
    page
}
fn wrap_labels(widget: &gtk::Widget) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        label.set_wrap(true);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        wrap_labels(&widget);
        child = widget.next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_sample_protocol_works_without_persistent_poll_logs() {
        let result = crate::management::inspect(json!({"action":"sample"})).unwrap();
        let value: Value = serde_json::from_str(&result).unwrap();
        assert!(value["memory"].is_object());
        assert!(value["sensors"].is_array());
    }
    #[test]
    fn cpu_delta_handles_first_sample_and_resets() {
        assert!(sample_delta(&Value::Null, &Value::Null).is_none());
        assert_eq!(
            sample_delta(
                &json!({"cpu_ticks":{"total":100,"idle":50}}),
                &json!({"cpu_ticks":{"total":200,"idle":100}})
            ),
            Some(50.0)
        );
        assert!(
            sample_delta(
                &json!({"cpu_ticks":{"total":200,"idle":100}}),
                &json!({"cpu_ticks":{"total":100,"idle":50}})
            )
            .is_none()
        );
    }
    #[test]
    #[ignore = "requires isolated graphical session; constructs widgets only"]
    fn pages_build_without_changes() {
        adw::init().unwrap();
        let window = gtk::ApplicationWindow::builder().build();
        for title in [
            "System Health",
            "Creator Studio",
            "Desktop Customisation",
            "System Rescue",
            "Disk Manager",
            "Network & Security",
            "Laptop Optimisation",
            "Migration & Dotfiles",
        ] {
            let page = page(&window, title);
            assert!(page.first_child().is_some());
        }
    }
}

/// Keep every page discoverable without opening a menu. The sidebar scrolls
/// independently of page content and GTK supplies keyboard selection handling.
pub fn navigation(stack: &gtk::Stack) -> gtk::ScrolledWindow {
    let sidebar = gtk::StackSidebar::new();
    sidebar.set_stack(stack);
    sidebar.set_widget_name("archeasy-navigation-sidebar");
    gtk::ScrolledWindow::builder()
        .child(&sidebar)
        .min_content_width(210)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build()
}

pub fn software_categories(body: &gtk::Box) {
    fn search(widget: &gtk::Widget) -> Option<gtk::SearchEntry> {
        if widget.widget_name() == "software-search" {
            return widget.clone().downcast().ok();
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(found) = search(&widget) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }
    let Some(input) = search(body.upcast_ref()) else {
        return;
    };
    let menu = gtk::MenuButton::builder()
        .label("Browse Categories")
        .build();
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 6);
    for (title, query) in [
        ("Video Creation", "video editor"),
        ("Graphic Design", "photo editor"),
        ("Development", "code editor"),
        ("Music", "music player"),
        ("Office", "office"),
        ("Web Browsers", "web browser"),
        ("Gaming", "windows games"),
    ] {
        let button = gtk::Button::with_label(title);
        let input = input.clone();
        let weak = popover.downgrade();
        button.connect_clicked(move |_| {
            input.set_text(query);
            if let Some(popover) = weak.upgrade() {
                popover.popdown();
            }
        });
        choices.append(&button);
    }
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    body.prepend(&menu);
}

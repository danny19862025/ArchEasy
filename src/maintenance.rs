#![allow(deprecated)]
use super::*;
use std::{cell::RefCell, rc::Rc};

fn parse_orphans(out: std::process::Output) -> Result<Vec<(String, String)>, String> {
    if out.status.code() == Some(1) && out.stdout.is_empty() && out.stderr.is_empty() {
        return Ok(vec![]);
    }
    if !out.status.success() {
        return Err(format!(
            "Orphan scan failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| {
            let (name, version) = line.split_once(' ').ok_or("Unexpected pacman output")?;
            if !valid_id(name) || name.contains('/') {
                return Err("Invalid orphan package name".into());
            }
            Ok((name.to_owned(), version.trim().to_owned()))
        })
        .collect()
}

fn orphan_window(parent: &gtk::ApplicationWindow) {
    let window = gtk::Window::builder()
        .title("Orphan packages")
        .transient_for(parent)
        .default_width(600)
        .default_height(500)
        .build();
    window.set_widget_name("orphan-cleaner-window");
    crate::about::follow_theme(parent, &window);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&body);
    let note = label(
        "Packages installed as dependencies that are no longer required or optionally required by other packages. Select only those you want to remove; Pacman reviews the final transaction in your terminal.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let scan = gtk::Button::with_label("Find orphans");
    scan.set_widget_name("scan-orphans");
    body.append(&scan);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build(),
    );
    let status = label("Ready to inspect pacman -Qdt.", "subtitle");
    status.set_wrap(true);
    body.append(&status);
    let remove = gtk::Button::with_label("Remove selected…");
    remove.set_widget_name("remove-selected-orphans");
    remove.set_sensitive(false);
    body.append(&remove);
    let selections: Rc<RefCell<Vec<(String, gtk::CheckButton)>>> = Rc::new(RefCell::new(vec![]));
    let selections_scan = selections.clone();
    let status_scan = status.clone();
    let remove_scan = remove.clone();
    scan.connect_clicked(move |button| {
        button.set_sensitive(false);
        remove_scan.set_sensitive(false);
        selections_scan.borrow_mut().clear();
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        status_scan.set_text("Scanning pacman -Qdt…");
        let weak = button.downgrade();
        let list = list.clone();
        let status_done = status_scan.clone();
        let selections = selections_scan.clone();
        let remove = remove_scan.clone();
        system_tools::task_with_finish(
            &status_scan,
            || {
                parse_orphans(
                    Command::new("pacman")
                        .arg("-Qdt")
                        .env("LC_ALL", "C")
                        .output()
                        .map_err(|e| e.to_string())?,
                )
            },
            move |packages| {
                status_done.set_text(&format!(
                    "{} orphan packages found. Nothing has been removed.",
                    packages.len()
                ));
                for (name, version) in packages {
                    let select = gtk::CheckButton::with_label(&format!("{name}  {version}"));
                    let selections_changed = selections.clone();
                    let remove = remove.clone();
                    select.connect_toggled(move |_| {
                        remove.set_sensitive(
                            selections_changed
                                .borrow()
                                .iter()
                                .any(|(_, select)| select.is_active()),
                        );
                    });
                    list.append(&select);
                    selections.borrow_mut().push((name, select));
                }
            },
            move || {
                if let Some(button) = weak.upgrade() {
                    button.set_sensitive(true);
                }
            },
        );
    });
    let weak = parent.downgrade();
    remove.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; };
        let packages: Vec<_> = selections.borrow().iter().filter(|(_, check)| check.is_active()).map(|(name, _)| name.clone()).collect();
        if packages.is_empty() { return; }
        let alert = gtk::AlertDialog::builder().message("Remove selected orphan packages?")
            .detail(format!("{}\n\nOrphan status is checked again before removal. Pacman shows the full transaction, including unused dependencies, and asks for confirmation in your terminal.", packages.join("\n")))
            .buttons(["Cancel", "Review in terminal"]).cancel_button(0).default_button(0).build();
        let status = status.clone();
        alert.choose(Some(&parent), gtk::gio::Cancellable::NONE, move |choice| {
            if choice != Ok(1) { return; }
            let mut args: Vec<String> = vec!["/usr/bin/bash".into(), "-c".into(), include_str!("../scripts/remove-orphans.sh").into(), "archeasy-remove-orphans".into()];
            args.extend(packages);
            match terminal(args) {
                Ok(()) => status.set_text("Review the terminal transaction. Find orphans again after it finishes."),
                Err(e) => status.set_text(&e),
            }
        });
    });
    window.set_child(Some(&body));
    window.present();
    scan.emit_clicked();
}

pub fn add_controls(page: &gtk::Box, parent: &gtk::ApplicationWindow) {
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let orphans = gtk::Button::with_label("Find orphans");
    orphans.set_widget_name("find-orphans");
    actions.append(&orphans);
    let weak = parent.downgrade();
    orphans.connect_clicked(move |_| {
        if let Some(parent) = weak.upgrade() {
            orphan_window(&parent);
        }
    });
    let clean = gtk::Button::with_label("Clean Pacman cache…");
    clean.set_widget_name("clean-pacman-cache");
    actions.append(&clean);
    let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh.set_tooltip_text(Some("Refresh Pacman cache size"));
    actions.append(&refresh);
    page.append(&actions);
    let status = label("Pacman cache: checking…", "subtitle");
    status.set_wrap(true);
    status.set_widget_name("pacman-cache-size");
    page.append(&status);
    let status_scan = status.clone();
    refresh.connect_clicked(move |button| {
        button.set_sensitive(false);
        let done = status_scan.clone();
        let weak = button.downgrade();
        system_tools::task_with_finish(
            &status_scan,
            || output("du", &["-sh", "/var/cache/pacman/pkg"]),
            move |size| {
                done.set_text(&format!(
                    "Pacman cache: {} · cleaning keeps the newest 3 versions per package.",
                    size.split_whitespace().next().unwrap_or("unknown")
                ));
            },
            move || {
                if let Some(button) = weak.upgrade() {
                    button.set_sensitive(true);
                }
            },
        );
    });
    let weak = parent.downgrade();
    clean.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else { return; };
        if !available("paccache") {
            status.set_text("Install pacman-contrib to enable cache cleaning (sudo pacman -S --needed pacman-contrib)."); return;
        }
        let alert = gtk::AlertDialog::builder().message("Clean older cached packages?")
            .detail("Keeps the newest three cached versions of every package. Installed packages stay installed. The terminal shows the cache size and asks before running sudo paccache --remove --keep 3.")
            .buttons(["Cancel", "Review in terminal"]).cancel_button(0).default_button(0).build();
        let status = status.clone();
        alert.choose(Some(&parent), gtk::gio::Cancellable::NONE, move |choice| {
            if choice != Ok(1) { return; }
            match terminal(vec!["/usr/bin/bash".into(), "-c".into(), include_str!("../scripts/clean-cache.sh").into(), "archeasy-clean-cache".into()]) {
                Ok(()) => status.set_text("Cache cleanup opened in the terminal. Refresh the size after it finishes."),
                Err(e) => status.set_text(&e),
            }
        });
    });
    refresh.emit_clicked();
}

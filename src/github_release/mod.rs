//! Published release installation is separate from the existing PKGBUILD workflow.
use crate::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const SCRIPT: &str = include_str!("../../scripts/github-release.py");

#[derive(Clone)]
struct View {
    url: gtk::Entry,
    inspect: gtk::Button,
    install: gtk::Button,
    refresh: gtk::Button,
    remove: gtk::Button,
    assets: gtk::DropDown,
    installed: gtk::DropDown,
    review: gtk::Label,
    status: gtk::Label,
    progress: gtk::ProgressBar,
    log: gtk::TextView,
    candidates: Rc<RefCell<Vec<Value>>>,
    records: Rc<RefCell<Vec<Value>>>,
    active: Rc<RefCell<Option<gaming::backend::Session>>>,
}
impl View {
    fn sensitivity(&self) {
        let idle = self.active.borrow().is_none();
        for button in [&self.inspect, &self.refresh] {
            button.set_sensitive(idle);
        }
        self.url.set_sensitive(idle);
        self.assets.set_sensitive(idle);
        self.installed.set_sensitive(idle);
        self.install
            .set_sensitive(idle && !self.candidates.borrow().is_empty());
        self.remove
            .set_sensitive(idle && !self.records.borrow().is_empty());
    }
    fn review(&self) {
        let items = self.candidates.borrow();
        let Some(asset) = items.get(self.assets.selected() as usize) else {
            self.install.set_sensitive(false);
            return;
        };
        let size = asset["size"].as_u64().map_or_else(
            || "Not published".into(),
            |n| format!("{:.2} MiB ({n} bytes)", n as f64 / 1_048_576.0),
        );
        let checksums = asset["digest"].is_string()
            || asset["checksums"].as_array().is_some_and(|a| !a.is_empty());
        self.review.set_text(&format!(
            "App: {}\nRepository: {}\nRelease: {}\nArchitecture: {}\nMethod: {}\nDownload: {}\nSize: {size}\nChecksum: {}\n\nSource: {}\n\nFiles: ~/.local/share/archeasy/apps\nLauncher: ~/.local/bin/archeasy-{}\nA desktop menu entry is created for AppImages and archives containing desktop metadata.\n\nInstall only releases from publishers you trust. No downloaded code is executed during installation. AppImages may require FUSE; binaries may require system libraries. Existing installations must be removed before changing versions.",
            asset["title"].as_str().unwrap_or_default(), asset["repo"].as_str().unwrap_or_default(),
            asset["version"].as_str().unwrap_or_default(), asset["arch"].as_str().unwrap_or_default(),
            asset["method"].as_str().unwrap_or_default(), asset["name"].as_str().unwrap_or_default(),
            if checksums { "Published checksum will be verified" } else { "Not published; HTTPS download only" },
            asset["url"].as_str().unwrap_or_default(), asset["repo"].as_str().unwrap_or_default().replace('/', "-").to_lowercase(),
        ));
        self.sensitivity();
    }
    fn start(&self, mode: &'static str, request: Value) {
        if self.active.borrow().is_some() {
            return;
        }
        let (session, events) =
            match gaming::backend::start_script(SCRIPT, mode, &[request.to_string()]) {
                Ok(value) => value,
                Err(error) => {
                    self.status.set_text(&error);
                    return;
                }
            };
        *self.active.borrow_mut() = Some(session);
        self.sensitivity();
        self.status.set_text(match mode {
            "inspect" => "Inspecting repository and latest stable release…",
            "install" => "Downloading and verifying release…",
            "remove" => "Removing tracked application…",
            _ => "Loading tracked GitHub apps…",
        });
        self.progress.set_fraction(0.0);
        self.log.buffer().set_text("");
        let view = self.clone();
        let mut failed = false;
        let mut done = false;
        glib::timeout_add_local(Duration::from_millis(80), move || {
            while let Ok(event) = events.try_recv() {
                for key in ["log", "error"] {
                    if let Some(text) = event[key].as_str() {
                        let buffer = view.log.buffer();
                        buffer.insert(&mut buffer.end_iter(), text);
                        if key == "error" {
                            buffer.insert(&mut buffer.end_iter(), "\n");
                            view.status.set_text(text);
                            failed = true;
                        }
                        if buffer.char_count() > 500_000 {
                            buffer.delete(
                                &mut buffer.start_iter(),
                                &mut buffer.iter_at_offset(50_000),
                            );
                        }
                        view.log
                            .scroll_to_iter(&mut buffer.end_iter(), 0.0, false, 0.0, 1.0);
                    }
                }
                if let Some(fraction) = event["progress"].as_f64() {
                    view.progress.set_fraction(fraction.clamp(0.0, 1.0));
                }
                if let Some(inspection) = event.get("inspection") {
                    *view.candidates.borrow_mut() = inspection["candidates"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let names: Vec<String> = view
                        .candidates
                        .borrow()
                        .iter()
                        .map(|a| a["name"].as_str().unwrap_or_default().to_owned())
                        .collect();
                    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                    view.assets.set_model(Some(&gtk::StringList::new(&refs)));
                    view.assets.set_selected(0);
                    if names.is_empty() {
                        view.review.set_text(
                            inspection["explanation"]
                                .as_str()
                                .unwrap_or("No supported downloads."),
                        );
                    } else {
                        view.review();
                    }
                }
                if let Some(records) = event["installed_apps"].as_array() {
                    *view.records.borrow_mut() = records.clone();
                    let names: Vec<String> = records
                        .iter()
                        .map(|r| {
                            format!(
                                "{} — {} ({})",
                                r["repo"].as_str().unwrap_or_default(),
                                r["version"].as_str().unwrap_or_default(),
                                r["method"].as_str().unwrap_or_default()
                            )
                        })
                        .collect();
                    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                    view.installed.set_model(Some(&gtk::StringList::new(&refs)));
                    view.installed.set_selected(0);
                }
                if event["done"] == true {
                    done = true;
                }
                if event["exited"] == true {
                    view.active.borrow_mut().take();
                    view.sensitivity();
                    if !failed {
                        if done && event["success"] == true {
                            view.progress.set_fraction(1.0);
                            view.status.set_text(match mode {
                                "inspect" => "Inspection complete. Select a download and review its details before installing.",
                                "install" => "Installed. Use the launcher or desktop menu entry. Removal is available below.",
                                "remove" => "Application removed. User settings were preserved.",
                                _ => "Tracked apps refreshed.",
                            });
                        } else {
                            view.status.set_text(
                                "GitHub installer stopped unexpectedly. Review the log and retry.",
                            );
                        }
                    }
                    return glib::ControlFlow::Break;
                }
            }
            glib::ControlFlow::Continue
        });
    }
}

pub fn page(parent: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&body);
    body.append(&label("Install from GitHub", "heading"));
    let intro = label(
        "Install published Linux releases as your user. Paste a public repository URL to inspect its latest stable release. README commands, source builds and downloaded installers are not run.",
        "subtitle",
    );
    intro.set_wrap(true);
    body.append(&intro);
    let url = gtk::Entry::builder()
        .placeholder_text("https://github.com/owner/repository")
        .build();
    url.set_widget_name("github-release-url");
    let inspect = gtk::Button::with_label("Inspect Repository");
    inspect.set_widget_name("github-release-inspect");
    body.append(&url);
    body.append(&inspect);
    let assets = gtk::DropDown::from_strings(&[]);
    assets.set_widget_name("github-release-assets");
    body.append(&assets);
    let review = label(
        "Release details will appear here before installation.",
        "card",
    );
    review.set_wrap(true);
    review.set_selectable(true);
    review.set_xalign(0.0);
    review.set_widget_name("github-release-review");
    body.append(&review);
    let install = gtk::Button::with_label("Install Reviewed Release");
    install.set_widget_name("github-release-install");
    install.add_css_class("suggested-action");
    body.append(&install);
    let status = label("Ready to inspect a repository.", "subtitle");
    status.set_wrap(true);
    status.set_widget_name("github-release-status");
    body.append(&status);
    let progress = gtk::ProgressBar::new();
    body.append(&progress);
    let log = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    log.set_widget_name("github-release-log");
    body.append(
        &gtk::Expander::builder()
            .label("Installation Log")
            .expanded(true)
            .child(
                &gtk::ScrolledWindow::builder()
                    .child(&log)
                    .min_content_height(180)
                    .build(),
            )
            .build(),
    );
    body.append(&label("Installed GitHub Apps", "heading"));
    let installed = gtk::DropDown::from_strings(&[]);
    installed.set_widget_name("github-release-installed");
    let refresh = gtk::Button::with_label("Refresh Installed GitHub Apps");
    let remove = gtk::Button::with_label("Remove Selected GitHub App…");
    remove.add_css_class("destructive-action");
    remove.set_widget_name("github-release-remove");
    body.append(&installed);
    body.append(&refresh);
    body.append(&remove);
    let view = View {
        url,
        inspect,
        install,
        refresh,
        remove,
        assets,
        installed,
        review,
        status,
        progress,
        log,
        candidates: Rc::new(RefCell::new(vec![])),
        records: Rc::new(RefCell::new(vec![])),
        active: Rc::new(RefCell::new(None)),
    };
    view.sensitivity();
    let v = view.clone();
    view.inspect.connect_clicked(move |_| {
        v.candidates.borrow_mut().clear();
        v.assets.set_model(Some(&gtk::StringList::new(&[])));
        v.review.set_text("Inspecting release details…");
        v.start("inspect", json!({"url": v.url.text().as_str()}));
    });
    let v = view.clone();
    view.url.connect_changed(move |_| {
        v.candidates.borrow_mut().clear();
        v.install.set_sensitive(false);
        v.review
            .set_text("Inspect this URL to review its available releases.");
    });
    let v = view.clone();
    view.assets.connect_selected_notify(move |_| v.review());
    let v = view.clone();
    view.install.connect_clicked(move |_| {
        let asset = v
            .candidates
            .borrow()
            .get(v.assets.selected() as usize)
            .cloned();
        if let Some(asset) = asset {
            v.start("install", json!({"asset": asset}));
        }
    });
    let v = view.clone();
    view.refresh
        .connect_clicked(move |_| v.start("list", json!({})));
    let v = view.clone();
    let weak = parent.downgrade();
    view.remove.connect_clicked(move |_| {
        let record = v.records.borrow().get(v.installed.selected() as usize).cloned();
        if let (Some(record), Some(parent)) = (record, weak.upgrade()) {
            let heading = format!("Remove {}?", record["repo"].as_str().unwrap_or("this app"));
            let dialog = adw::MessageDialog::new(Some(&parent), Some(&heading), Some("Remove its tracked app files, launcher and desktop entry. Personal app settings are preserved."));
            dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
            dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            let v = v.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "remove" { v.start("remove", json!({"id": record["id"]})); }
            });
            dialog.present();
        }
    });
    let active = view.active.clone();
    parent.connect_close_request(move |_| {
        if active.borrow().is_some() {
            show_error(
                "A GitHub app operation is active. Wait for it to finish before closing ArchEasy.",
            );
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.start("list", json!({}));
    gtk::ScrolledWindow::builder()
        .child(&body)
        .vexpand(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_release_backend_lists_tracked_apps() {
        let (_session, events) =
            gaming::backend::start_script(SCRIPT, "list", &[json!({}).to_string()]).unwrap();
        assert!(events.recv().unwrap()["installed_apps"].is_array());
        assert_eq!(events.recv().unwrap()["done"], true);
        assert_eq!(events.recv().unwrap()["exited"], true);
    }
    #[test]
    #[ignore = "requires an isolated graphical session; uses no network or installation"]
    fn release_page_controls_and_invalid_url() {
        adw::init().unwrap();
        let window = gtk::ApplicationWindow::builder().build();
        let page = page(&window);
        fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
            let mut result = vec![widget.as_ref().clone()];
            let mut child = widget.as_ref().first_child();
            while let Some(next) = child {
                result.extend(descendants(&next));
                child = next.next_sibling();
            }
            result
        }
        let widgets = descendants(&page);
        let find = |name: &str| {
            widgets
                .iter()
                .find(|w| w.widget_name() == name)
                .unwrap()
                .clone()
        };
        let install = find("github-release-install")
            .downcast::<gtk::Button>()
            .unwrap();
        assert!(!install.is_sensitive());
        let status = find("github-release-status")
            .downcast::<gtk::Label>()
            .unwrap();
        let wait = |expected: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !status.text().contains(expected) && std::time::Instant::now() < deadline {
                while glib::MainContext::default().pending() {
                    glib::MainContext::default().iteration(false);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(status.text().contains(expected), "{}", status.text());
        };
        wait("Tracked apps refreshed");
        let url = find("github-release-url").downcast::<gtk::Entry>().unwrap();
        url.set_text("https://evil.example/owner/repo");
        find("github-release-inspect")
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        wait("Use https://github.com/owner/repository");
        assert!(!install.is_sensitive());
        assert!(find("github-release-remove").is::<gtk::Button>());
    }
}

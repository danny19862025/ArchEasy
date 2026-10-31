//! Gaming view. Package discovery, planning, privileges and installation live in backend.
pub(crate) mod backend;
use crate::*;
use std::{cell::RefCell, rc::Rc};

pub fn page(parent: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&body);
    body.append(&label("One-Click Gaming Setup", "heading"));
    let note = label(
        "Choose your gaming apps. Installation uses official Arch packages and approved Flathub fallbacks. You review system upgrades and package-manager prompts here; administrator authentication opens graphically. Existing graphics drivers and user configurations are preserved.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let choices = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(3)
        .min_children_per_line(1)
        .build();
    let mut checks = Vec::new();
    match backend::catalog() {
        Ok(catalog) => {
            for item in catalog {
                let check =
                    gtk::CheckButton::with_label(item["title"].as_str().unwrap_or_default());
                let id = item["id"].as_str().unwrap_or_default().to_owned();
                check.set_widget_name(&format!("gaming-{id}"));
                check.set_active(item["recommended"].as_bool().unwrap_or(false));
                choices.insert(&check, -1);
                checks.push((id, check));
            }
        }
        Err(error) => {
            body.append(&label(&error, "error"));
        }
    }
    body.append(&choices);
    let install = gtk::Button::with_label("🎮 Install Gaming Essentials");
    install.set_widget_name("gaming-install");
    install.add_css_class("suggested-action");
    install.set_height_request(64);
    body.append(&install);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let check = gtk::Button::with_label("Gaming System Check");
    check.set_widget_name("gaming-check");
    let missing = gtk::Button::with_label("Install Missing Packages");
    missing.set_widget_name("gaming-missing");
    let retry = gtk::Button::with_label("Retry Failed Installation");
    retry.set_widget_name("gaming-retry");
    retry.set_sensitive(false);
    for button in [&check, &missing, &retry] {
        actions.append(button);
    }
    body.append(&actions);
    let status = label(
        "Ready. Recommended apps are selected; advanced tools are optional.",
        "subtitle",
    );
    status.set_widget_name("gaming-status");
    status.set_wrap(true);
    body.append(&status);
    let completed = label("", "card");
    completed.set_widget_name("gaming-completed");
    completed.set_wrap(true);
    completed.set_selectable(true);
    completed.set_visible(false);
    body.append(&completed);
    let progress = gtk::ProgressBar::new();
    body.append(&progress);
    let log = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    log.set_widget_name("gaming-log");
    let buffer = log.buffer();
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&log)
            .min_content_height(240)
            .vexpand(true)
            .build(),
    );
    let responses = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let response = gtk::Entry::builder()
        .placeholder_text("Response to the current package-manager prompt (yes, no, or a number)")
        .hexpand(true)
        .build();
    response.set_widget_name("gaming-response");
    let send = gtk::Button::with_label("Send Response");
    let cancel = gtk::Button::with_label("Cancel After Current Stage");
    cancel.set_widget_name("gaming-cancel");
    for child in [
        response.clone().upcast::<gtk::Widget>(),
        send.clone().upcast(),
        cancel.clone().upcast(),
    ] {
        responses.append(&child);
    }
    responses.set_sensitive(false);
    body.append(&responses);
    let session: Rc<RefCell<Option<backend::Session>>> = Rc::new(RefCell::new(None));
    let s = session.clone();
    let r = response.clone();
    let send_response = move || {
        if let Some(session) = s.borrow_mut().as_mut() {
            let _ = session.send(serde_json::json!({"answer": r.text().as_str()}));
            r.set_text("");
        }
    };
    let activate = send_response.clone();
    response.connect_activate(move |_| activate());
    send.connect_clicked(move |_| send_response());
    let s = session.clone();
    let st = status.clone();
    cancel.connect_clicked(move |button| {
        if let Some(session) = s.borrow_mut().as_mut() {
            let _ = session.send(serde_json::json!({"cancel": true}));
            st.set_text("Cancellation requested. Let the active transaction finish, or decline its pending prompt below. No further stages will start.");
            button.set_sensitive(false);
        }
    });
    let s = session.clone();
    parent.connect_close_request(move |_| {
        if s.borrow().is_some() {
            show_error("A gaming session is still active. Cancel after the current stage and finish or decline any pending package-manager prompt before closing ArchEasy.");
            glib::Propagation::Stop
        } else { glib::Propagation::Proceed }
    });
    let buttons = vec![
        install.clone(),
        check.clone(),
        missing.clone(),
        retry.clone(),
    ];
    let checks = Rc::new(checks);
    // Retry keeps the failed selection; installing missing packages always re-detects state.
    let previous: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(vec![]));
    for (button, mode, retrying) in [
        (install, "install", false),
        (check, "check", false),
        (missing, "install", false),
        (retry.clone(), "install", true),
    ] {
        let (session, checks, previous) = (session.clone(), checks.clone(), previous.clone());
        let (buttons, status, progress, buffer, responses, cancel, retry, choices) = (
            buttons.clone(),
            status.clone(),
            progress.clone(),
            buffer.clone(),
            responses.clone(),
            cancel.clone(),
            retry.clone(),
            choices.clone(),
        );
        let parent = parent.downgrade();
        let completed = completed.clone();
        let log = log.clone();
        button.connect_clicked(move |_| {
            if session.borrow().is_some() { return; }
            let selected: Vec<String> = if retrying { previous.borrow().clone() } else { checks.iter().filter(|(_, c)| c.is_active()).map(|(id, _)| id.clone()).collect() };
            if mode == "install" { *previous.borrow_mut() = selected.clone(); }
            let (handle, events) = match backend::start(mode, &selected) {
                Ok(started) => started,
                Err(e) => { status.set_text(&e); return; }
            };
            *session.borrow_mut() = Some(handle);
            for b in &buttons { b.set_sensitive(false); }
            choices.set_sensitive(false);
            responses.set_sensitive(mode == "install");
            cancel.set_sensitive(true);
            completed.set_visible(false);
            buffer.set_text("");
            progress.set_fraction(0.0);
            status.set_text("Checking your gaming system…");
            let (session, buttons, choices, responses, retry, buffer, progress, status, parent) = (session.clone(), buttons.clone(), choices.clone(), responses.clone(), retry.clone(), buffer.clone(), progress.clone(), status.clone(), parent.clone());
            let completed = completed.clone();
            let log = log.clone();
            let mut failed = false;
            let mut done = false;
            let mut warning = None;
            glib::timeout_add_local(Duration::from_millis(80), move || {
                progress.pulse();
                while let Ok(event) = events.try_recv() {
                    for key in ["log", "prompt", "error"] {
                        if let Some(text) = event[key].as_str() {
                            buffer.insert(&mut buffer.end_iter(), text);
                            if key != "log" { buffer.insert(&mut buffer.end_iter(), "\n"); status.set_text(text); }
                            if key == "error" { failed = true; }
                            // Keep the latest output visible and cap retained log memory.
                            if buffer.char_count() > 250_000 {
                                buffer.delete(&mut buffer.start_iter(), &mut buffer.iter_at_offset(50_000));
                            }
                            log.scroll_to_iter(&mut buffer.end_iter(), 0.0, false, 0.0, 1.0);
                            if key == "prompt" {
                                responses.set_sensitive(false);
                                let responses_dialog = responses.clone();
                                let s = session.clone();
                                let parent = parent.upgrade();
                                gtk::AlertDialog::builder().message("Gaming setup permission").detail(text).buttons(["Stop", "Approve"]).cancel_button(0).default_button(0).build()
                                    .choose(parent.as_ref(), gtk::gio::Cancellable::NONE, move |answer| {
                                        responses_dialog.set_sensitive(true);
                                        if let Some(session) = s.borrow_mut().as_mut() { let _ = session.send(serde_json::json!({"answer": if answer == Ok(1) { "yes" } else { "no" }})); }
                                    });
                            }
                        }
                    }
                    if let Some(message) = event["warning"].as_str() { warning = Some(message.to_owned()); }
                    if let Some(apps) = event["completed"].as_array() {
                        let names: Vec<_> = apps.iter().filter_map(|a| a.as_str()).collect();
                        completed.set_text(&format!("Gaming essentials installed\n\n{}\n\nSteam → Settings → Compatibility: enable Steam Play if offered. For a game, open Properties → Compatibility and select Proton.\n\nReview the final Vulkan check in the log below.", names.join(" • ")));
                        completed.set_visible(true);
                    }
                    if event["done"] == true { done = true; }
                    if event["exited"] == true {
                        let success = done && !failed && event["success"] == true;
                        session.borrow_mut().take();
                        for b in &buttons { b.set_sensitive(true); }
                        retry.set_sensitive(!success && mode == "install");
                        choices.set_sensitive(true);
                        responses.set_sensitive(false);
                        progress.set_fraction(if success { 1.0 } else { 0.0 });
                        if success { status.set_text(if let Some(message) = warning.as_deref() { message } else if mode == "check" { "System check complete. Review the results below." } else { "Gaming setup complete. Installed applications and Steam Play instructions are listed below." }); }
                        else if !failed { status.set_text("Gaming backend exited unexpectedly. Retry after checking Python and system prerequisites."); }
                        return glib::ControlFlow::Break;
                    }
                }
                glib::ControlFlow::Continue
            });
        });
    }
    gtk::ScrolledWindow::builder()
        .child(&body)
        .vexpand(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
        let mut all = vec![widget.as_ref().clone()];
        let mut child = widget.as_ref().first_child();
        while let Some(next) = child {
            all.extend(descendants(&next));
            child = next.next_sibling();
        }
        all
    }
    #[test]
    #[ignore = "requires a graphical session; only builds widgets, never installs packages"]
    fn gaming_defaults_and_controls() {
        adw::init().unwrap();
        let window = gtk::ApplicationWindow::builder().build();
        let page = page(&window);
        let widgets = descendants(&page);
        for name in [
            "gaming-install",
            "gaming-check",
            "gaming-missing",
            "gaming-retry",
            "gaming-cancel",
            "gaming-log",
            "gaming-completed",
        ] {
            assert!(widgets.iter().any(|w| w.widget_name() == name), "{name}");
        }
        let checks: Vec<_> = widgets
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::CheckButton>().ok())
            .collect();
        assert_eq!(checks.len(), 12);
        assert_eq!(checks.iter().filter(|c| c.is_active()).count(), 6);
        assert!(
            !checks
                .iter()
                .find(|c| c.widget_name() == "gaming-gamescope")
                .unwrap()
                .is_active()
        );
    }
}

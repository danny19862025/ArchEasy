//! Manifest-driven developer view; all package policy lives in the backend.
mod backend;
use crate::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
struct Choice {
    id: String,
    check: gtk::CheckButton,
    source: gtk::DropDown,
    sources: Vec<String>,
    state: gtk::Label,
}
type Active = Rc<RefCell<Option<gaming::backend::Session>>>;

fn request(choices: &[Choice], aur: bool, selected: Option<Vec<String>>) -> serde_json::Value {
    let selected = selected.unwrap_or_else(|| {
        choices
            .iter()
            .filter(|c| c.check.is_active())
            .map(|c| c.id.clone())
            .collect()
    });
    let sources: serde_json::Map<String, serde_json::Value> = choices
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                serde_json::json!(c.sources[c.source.selected() as usize]),
            )
        })
        .collect();
    serde_json::json!({"selected": selected, "sources": sources, "allow_aur": aur})
}

fn permission(
    parent: Option<&gtk::ApplicationWindow>,
    text: &str,
    active: Active,
    responses: gtk::Box,
) {
    // Long transaction plans and AUR file reviews must be readable before approval.
    let dialog = gtk::Window::builder()
        .title("Review developer setup operation")
        .modal(true)
        .default_width(780)
        .default_height(580)
        .build();
    if let Some(parent) = parent {
        dialog.set_transient_for(Some(parent));
        about::follow_theme(parent, &dialog);
    }
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    system_tools::margins(&body);
    body.append(&label("Review before approving", "heading"));
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .vexpand(true)
        .build();
    view.buffer().set_text(text);
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&view)
            .vexpand(true)
            .build(),
    );
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let stop = gtk::Button::with_label("Stop");
    let approve = gtk::Button::with_label("Approve This Operation");
    approve.add_css_class("suggested-action");
    actions.append(&stop);
    actions.append(&approve);
    body.append(&actions);
    dialog.set_default_widget(Some(&stop));
    let responded = Rc::new(std::cell::Cell::new(false));
    for (button, answer) in [(stop, "no"), (approve, "yes")] {
        let (active, responses, responded, weak) = (
            active.clone(),
            responses.clone(),
            responded.clone(),
            dialog.downgrade(),
        );
        button.connect_clicked(move |_| {
            if !responded.replace(true) {
                if let Some(session) = active.borrow_mut().as_mut() {
                    let _ = session.send(serde_json::json!({"answer": answer}));
                }
                responses.set_sensitive(true);
            }
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
        });
    }
    dialog.connect_close_request(move |_| {
        if !responded.replace(true) {
            if let Some(session) = active.borrow_mut().as_mut() {
                let _ = session.send(serde_json::json!({"answer": "no"}));
            }
            responses.set_sensitive(true);
        }
        glib::Propagation::Proceed
    });
    dialog.set_child(Some(&body));
    dialog.present();
}

pub fn page(parent: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let manifest = backend::catalog();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    system_tools::margins(&body);
    body.append(&label("One-Click Developer Setup", "heading"));
    let note = label(
        "Build your development environment. Choose tools and their installation sources, then review the plan. Services, shell settings, databases and privileged groups stay under your control.",
        "subtitle",
    );
    note.set_wrap(true);
    body.append(&note);
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let presets = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let names = [
        "Recommended",
        "Advanced",
        "Basic Developer",
        "Full Stack Developer",
        "Linux App Developer",
        "AI Developer",
    ];
    let preset = gtk::DropDown::from_strings(&names);
    preset.set_widget_name("developer-preset");
    preset.set_hexpand(true);
    let apply = gtk::Button::with_label("Apply Preset");
    apply.set_widget_name("developer-apply-preset");
    presets.append(&preset);
    presets.append(&apply);
    controls.append(&presets);
    let selections = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let all = gtk::Button::with_label("Select All");
    let none = gtk::Button::with_label("Deselect All");
    all.set_widget_name("developer-select-all");
    none.set_widget_name("developer-deselect-all");
    selections.append(&all);
    selections.append(&none);
    controls.append(&selections);
    let aur = gtk::CheckButton::with_label(
        "Allow reviewed AUR builds (advanced; runs approved build code as your user)",
    );
    aur.set_widget_name("developer-allow-aur");
    controls.append(&aur);
    let groups = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let mut choices = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    for item in manifest["tools"].as_array().expect("developer tools") {
        let name = item["category"].as_str().expect("category").to_owned();
        if !categories.contains(&name) {
            categories.push(name);
        }
    }
    for category in categories {
        let group = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let select = gtk::Button::with_label("Select Category");
        let clear = gtk::Button::with_label("Clear Category");
        actions.append(&select);
        actions.append(&clear);
        group.append(&actions);
        let mut category_checks = Vec::new();
        for item in manifest["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["category"] == category)
        {
            let id = item["id"].as_str().unwrap().to_owned();
            let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
            row.add_css_class("card");
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let check = gtk::CheckButton::with_label(item["title"].as_str().unwrap());
            check.set_hexpand(true);
            check.set_active(item["recommended"] == true);
            check.set_widget_name(&format!("developer-{id}"));
            let state = label("Not checked", "dim-label");
            state.set_widget_name(&format!("developer-status-{id}"));
            let mut source_ids: Vec<String> = Vec::new();
            let mut source_labels = Vec::new();
            if !item["packages"].as_array().unwrap().is_empty() {
                source_ids.push("official".into());
                source_labels.push("Official Arch");
            }
            for (key, title) in [
                ("flatpak", "Flathub (user)"),
                ("aur", "AUR (review required)"),
                ("upstream", "Upstream npm (user)"),
            ] {
                if item[key].is_string() {
                    source_ids.push(key.into());
                    source_labels.push(title);
                }
            }
            let source = gtk::DropDown::from_strings(&source_labels);
            source.set_widget_name(&format!("developer-source-{id}"));
            source.set_selected(
                source_ids
                    .iter()
                    .position(|s| s == item["default_source"].as_str().unwrap_or("official"))
                    .unwrap_or(0) as u32,
            );
            source.set_sensitive(source_ids.len() > 1);
            let st = state.clone();
            source.connect_selected_notify(move |_| {
                st.set_text("Not checked");
                st.remove_css_class("success");
                st.remove_css_class("error");
                st.add_css_class("dim-label");
            });
            line.append(&check);
            line.append(&state);
            row.append(&line);
            row.append(&source);
            if let Some(text) = item["note"].as_str() {
                let note = label(text, "subtitle");
                note.set_wrap(true);
                row.append(&note);
            }
            group.append(&row);
            category_checks.push(check.clone());
            choices.push(Choice {
                id,
                check,
                source,
                sources: source_ids,
                state,
            });
        }
        for (button, selected) in [(select, true), (clear, false)] {
            let checks = category_checks.clone();
            button.connect_clicked(move |_| {
                for check in &checks {
                    check.set_active(selected);
                }
            });
        }
        let expander = gtk::Expander::builder()
            .label(&category)
            .expanded(category == "Essentials")
            .child(&group)
            .build();
        groups.append(&expander);
    }
    controls.append(&groups);
    body.append(&controls);
    let choices = Rc::new(choices);
    for (button, selected) in [(all, true), (none, false)] {
        let choices = choices.clone();
        button.connect_clicked(move |_| {
            for c in choices.iter() {
                c.check.set_active(selected);
            }
        });
    }
    let values = manifest["presets"].clone();
    let p = preset.clone();
    let c = choices.clone();
    apply.connect_clicked(move |_| {
        let selected = values[names[p.selected() as usize]].as_array().unwrap();
        for choice in c.iter() {
            choice
                .check
                .set_active(selected.iter().any(|v| v == &choice.id));
        }
    });
    let install = gtk::Button::with_label("Install Developer Essentials");
    install.set_widget_name("developer-install");
    install.add_css_class("suggested-action");
    install.set_height_request(64);
    body.append(&install);
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let check = gtk::Button::with_label("Check Development Environment");
    check.set_widget_name("developer-check");
    let missing = gtk::Button::with_label("Install Missing Tools");
    missing.set_widget_name("developer-missing");
    let retry = gtk::Button::with_label("Retry Failed Packages");
    retry.set_widget_name("developer-retry");
    retry.set_sensitive(false);
    for b in [&check, &missing, &retry] {
        actions.append(b);
    }
    body.append(&actions);
    let status = label(
        "Essentials are selected. Presets and category selections can be customized before installing.",
        "subtitle",
    );
    status.set_widget_name("developer-status");
    status.set_wrap(true);
    body.append(&status);
    let progress = gtk::ProgressBar::new();
    progress.set_widget_name("developer-progress");
    progress.set_show_text(true);
    body.append(&progress);
    let completed = label("", "card");
    completed.set_wrap(true);
    completed.set_selectable(true);
    completed.set_visible(false);
    body.append(&completed);
    let report = gtk::Label::builder()
        .selectable(true)
        .wrap(true)
        .xalign(0.0)
        .build();
    report.set_widget_name("developer-readiness-report");
    let report_panel = gtk::Expander::builder()
        .label("Development Environment Readiness")
        .child(&report)
        .visible(false)
        .build();
    body.append(&report_panel);
    let log = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    log.set_widget_name("developer-log");
    let logs = gtk::Expander::builder()
        .label("Live Installation Logs")
        .child(
            &gtk::ScrolledWindow::builder()
                .child(&log)
                .min_content_height(260)
                .build(),
        )
        .build();
    logs.set_widget_name("developer-logs-expander");
    body.append(&logs);
    let responses = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let entry = gtk::Entry::builder()
        .placeholder_text(
            "Answer the current package-manager question (y, n, or a provider number)",
        )
        .build();
    entry.set_widget_name("developer-response");
    let send = gtk::Button::with_label("Send Response");
    let cancel = gtk::Button::with_label("Cancel After Current Stage");
    cancel.set_widget_name("developer-cancel");
    responses.append(&entry);
    responses.append(&send);
    responses.append(&cancel);
    responses.set_sensitive(false);
    body.append(&responses);
    let active: Active = Rc::new(RefCell::new(None));
    let a = active.clone();
    let e = entry.clone();
    let st = status.clone();
    let answer = move || {
        if let Some(session) = a.borrow_mut().as_mut() {
            if let Err(error) = session.send(serde_json::json!({"answer": e.text().as_str()})) {
                st.set_text(&error);
            }
            e.set_text("");
        }
    };
    let answer_enter = answer.clone();
    entry.connect_activate(move |_| answer_enter());
    send.connect_clicked(move |_| answer());
    let a = active.clone();
    let st = status.clone();
    cancel.connect_clicked(move |button| {
        if let Some(session) = a.borrow_mut().as_mut() { let _ = session.send(serde_json::json!({"cancel": true})); }
        st.set_text("Cancellation requested. Finish the current transaction or decline its pending prompt. Completed packages are retained."); button.set_sensitive(false);
    });
    let a = active.clone();
    parent.connect_close_request(move |_| {
        if a.borrow().is_some() { show_error("A developer setup session is active. Cancel after the current stage and finish or decline the current package-manager prompt before closing."); glib::Propagation::Stop } else { glib::Propagation::Proceed }
    });
    let buttons = vec![
        install.clone(),
        check.clone(),
        missing.clone(),
        retry.clone(),
    ];
    let failed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let last: Rc<RefCell<Option<serde_json::Value>>> = Rc::new(RefCell::new(None));
    for (button, mode, retrying) in [
        (install, "install", false),
        (check, "check", false),
        (missing, "install", false),
        (retry.clone(), "install", true),
    ] {
        let (active, failed, last, choices) = (
            active.clone(),
            failed.clone(),
            last.clone(),
            choices.clone(),
        );
        let (
            buttons,
            controls,
            aur,
            status,
            progress,
            completed,
            log,
            logs,
            responses,
            cancel,
            retry,
        ) = (
            buttons.clone(),
            controls.clone(),
            aur.clone(),
            status.clone(),
            progress.clone(),
            completed.clone(),
            log.clone(),
            logs.clone(),
            responses.clone(),
            cancel.clone(),
            retry.clone(),
        );
        let (report, report_panel) = (report.clone(), report_panel.clone());
        let parent = parent.downgrade();
        button.connect_clicked(move |_| {
            if active.borrow().is_some() { return; }
            let mut req = request(&choices, aur.is_active(), None);
            if retrying {
                if let Some(previous) = last.borrow().as_ref() { req = previous.clone(); }
                req["selected"] = serde_json::json!(*failed.borrow());
            }
            if mode == "install" { *last.borrow_mut() = Some(req.clone()); failed.borrow_mut().clear(); }
            let (session, events) = match backend::start(mode, req) { Ok(value) => value, Err(e) => { status.set_text(&e); return; } };
            *active.borrow_mut() = Some(session);
            for b in &buttons { b.set_sensitive(false); } controls.set_sensitive(false); responses.set_sensitive(mode == "install"); cancel.set_sensitive(true);
            report_panel.set_visible(false);
            completed.set_visible(false); log.buffer().set_text(""); logs.set_expanded(true); progress.set_fraction(0.0); progress.set_text(Some("Checking and planning…")); status.set_text("Checking the current environment and validating package availability…");
            let (active, failed, choices, buttons, controls, status, progress, completed, log, responses, retry, parent) = (active.clone(), failed.clone(), choices.clone(), buttons.clone(), controls.clone(), status.clone(), progress.clone(), completed.clone(), log.clone(), responses.clone(), retry.clone(), parent.clone());
            let (report, report_panel) = (report.clone(), report_panel.clone());
            let mut error = false; let mut done = false;
            glib::timeout_add_local(Duration::from_millis(80), move || {
                while let Ok(event) = events.try_recv() {
                    for key in ["log", "error"] {
                        if let Some(text) = event[key].as_str() {
                            let buffer = log.buffer(); buffer.insert(&mut buffer.end_iter(), text); buffer.insert(&mut buffer.end_iter(), if key == "error" { "\n" } else { "" });
                            if buffer.char_count() > 600_000 { buffer.delete(&mut buffer.start_iter(), &mut buffer.iter_at_offset(50_000)); }
                            log.scroll_to_iter(&mut buffer.end_iter(), 0.0, false, 0.0, 1.0);
                            if key == "error" { error = true; status.set_text(text); }
                        }
                    }
                    if let Some(text) = event["prompt"].as_str() {
                        status.set_text("Waiting for your review and approval…"); responses.set_sensitive(false);
                        permission(parent.upgrade().as_ref(), text, active.clone(), responses.clone());
                    }
                    if let Some(text) = event["readiness_report"].as_str() {
                        report.set_text(text);
                        report_panel.set_visible(true);
                        report_panel.set_expanded(true);
                    }
                    if let Some(value) = event["progress"].as_f64() { progress.set_fraction(value.clamp(0.0, 1.0)); progress.set_text(Some(&format!("{:.0}% of installation stages", value * 100.0))); }
                    if let Some(state) = event["tool_status"].as_object()
                        && let Some(choice) = choices.iter().find(|c| state.get("id").and_then(|s| s.as_str()) == Some(c.id.as_str())) {
                            let text = state.get("state").and_then(|s| s.as_str()).unwrap_or("Not Installed"); choice.state.set_text(text);
                            choice.state.remove_css_class("error"); choice.state.remove_css_class("success"); choice.state.remove_css_class("dim-label");
                            choice.state.add_css_class(if text == "Failed" { "error" } else if text == "Installed" { "success" } else { "dim-label" });
                    }
                    if let Some(items) = event["failed"].as_array() { *failed.borrow_mut() = items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(); }
                    if let Some(items) = event["completed"].as_array() {
                        completed.set_text(&format!("Developer setup complete\n\n{}\n\nReview the readiness report when you are ready. Services, groups and shell configuration were preserved.", items.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" • "))); completed.set_visible(true);
                    }
                    if event["done"] == true { done = true; }
                    if event["exited"] == true {
                        active.borrow_mut().take(); for b in &buttons { b.set_sensitive(true); } controls.set_sensitive(true); responses.set_sensitive(false);
                        retry.set_sensitive(!failed.borrow().is_empty());
                        if done && !error && event["success"] == true {
                            progress.set_fraction(1.0); progress.set_text(Some("Complete")); status.set_text(if mode == "check" { "Readiness report complete. Review versions, tool status and missing dependencies below." } else { "Selected tools are installed. No services, groups or shell profiles were configured automatically." });
                        } else if !error { status.set_text("Developer backend exited unexpectedly. Check the log and retry remaining tools."); }
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
        // Collapsed expanders can keep their child outside the widget tree.
        if let Some(expander) = widget.as_ref().downcast_ref::<gtk::Expander>()
            && let Some(content) = expander.child()
            && !all.contains(&content)
        {
            all.extend(descendants(&content));
        }
        all
    }
    #[test]
    #[ignore = "requires an isolated graphical session; never installs packages"]
    fn developer_presets_sources_and_selection() {
        adw::init().unwrap();
        let window = gtk::ApplicationWindow::builder().build();
        let page = page(&window);
        let widgets = descendants(&page);
        let button = |name: &str| {
            widgets
                .iter()
                .find(|w| w.widget_name() == name)
                .unwrap()
                .clone()
                .downcast::<gtk::Button>()
                .unwrap()
        };
        let checks: Vec<_> = widgets
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::CheckButton>().ok())
            .filter(|c| c.widget_name() != "developer-allow-aur")
            .collect();
        assert!(
            widgets
                .iter()
                .any(|w| w.widget_name() == "developer-readiness-report")
        );
        assert_eq!(checks.iter().filter(|c| c.is_active()).count(), 8);
        button("developer-select-all").emit_clicked();
        assert!(checks.iter().all(|c| c.is_active()));
        button("developer-deselect-all").emit_clicked();
        assert!(checks.iter().all(|c| !c.is_active()));
        let preset = widgets
            .iter()
            .find(|w| w.widget_name() == "developer-preset")
            .unwrap()
            .clone()
            .downcast::<gtk::DropDown>()
            .unwrap();
        preset.set_selected(5);
        button("developer-apply-preset").emit_clicked();
        assert!(
            checks
                .iter()
                .find(|c| c.widget_name() == "developer-codex")
                .unwrap()
                .is_active()
        );
        assert!(
            !checks
                .iter()
                .find(|c| c.widget_name() == "developer-ollama")
                .unwrap()
                .is_active()
        );
        assert!(
            widgets
                .iter()
                .any(|w| w.widget_name() == "developer-source-vscode")
        );
    }
}

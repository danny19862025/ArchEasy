mod core;
#[cfg(test)]
mod tests;
use core::{App, Candidate, Roots};
use gtk::{glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    if !class.is_empty() {
        l.add_css_class(match class {
            "muted" => "subtitle",
            "eyebrow" => "badge",
            other => other,
        });
    }
    l
}
#[allow(deprecated)] // Cancel-default cleanup confirmation on GTK 4.10.
pub fn page(window: &gtk::ApplicationWindow) -> gtk::Box {
    page_with_roots(window, Roots::current())
}
#[allow(deprecated)]
fn page_with_roots(window: &gtk::ApplicationWindow, roots: Roots) -> gtk::Box {
    let apps = Rc::new(core::discover(&roots));
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    layout.add_css_class("appcleaner-page");
    layout.set_widget_name("cleaner-page");
    layout.set_vexpand(true);
    layout.set_hexpand(true);
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 16);
    sidebar.add_css_class("sidebar");
    sidebar.add_css_class("card");
    sidebar.set_hexpand(false);
    sidebar.set_width_request(250);
    sidebar.append(&label("◈  AppCleaner", "heading"));
    sidebar.append(&label(
        &format!("{} applications discovered", apps.len()),
        "muted",
    ));
    let search = gtk::SearchEntry::new();
    search.set_widget_name("cleaner-search");
    search.set_placeholder_text(Some("Search applications…"));
    sidebar.append(&search);
    let list = gtk::ListBox::new();
    list.add_css_class("app-list");
    list.set_widget_name("cleaner-apps");
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    sidebar.append(&scroll);
    let about_button = gtk::Button::with_label("About AppCleaner");
    sidebar.append(&about_button);
    about_button.set_widget_name("cleaner-about");
    let about = crate::about::window(
        window,
        "AppCleaner",
        "edit-clear-all",
        "Review and safely clean Linux application data. Built into ArchEasy.\n\nInspect application settings, cache and data before moving selected files to Trash. Restore files through your file manager.",
    );
    about_button.connect_clicked(move |_| about.present());
    for a in apps.iter() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = gtk::Image::from_icon_name(&a.icon);
        icon.set_pixel_size(24);
        row.append(&icon);
        let name = label(&a.name, "app-name");
        name.set_hexpand(true);
        name.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        name.set_lines(2);
        name.set_tooltip_text(Some(&a.name));
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(28);
        row.append(&name);
        list.append(&row);
    }
    // Filtering preserves original row indices for stable selection.
    let search_list = list.clone();
    let search_apps = apps.clone();
    search.connect_search_changed(move |s| {
        let q = s.text().to_lowercase();
        for (i, a) in search_apps.iter().enumerate() {
            if let Some(row) = search_list.row_at_index(i as i32) {
                row.set_visible(
                    a.name.to_lowercase().contains(&q) || a.id.to_lowercase().contains(&q),
                );
            }
        }
    });
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.add_css_class("content");
    content.set_hexpand(true);
    content.append(&label("A LITTLE MORE SPACE. A CLEANER LINUX.", "eyebrow"));
    let title = label("Make room for what’s next.", "title");
    content.append(&title);
    let subtitle = label(
        "Choose an application to review its settings, cache, and local data.",
        "muted",
    );
    content.append(&subtitle);
    let summary = label("Your files stay in your control.", "heading");
    content.append(&summary);
    let file_list = gtk::Box::new(gtk::Orientation::Vertical, 10);
    file_list.set_widget_name("cleaner-files");
    let file_scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .min_content_height(120)
        .child(&file_list)
        .build();
    content.append(&file_scroll);
    let note = label(
        "Matches are suggestions. Review every path and close the application before cleanup. Settings and application data may include personal files. Restore removed folders through your file manager’s Trash.",
        "muted",
    );
    content.append(&note);
    let command = gtk::Entry::new();
    command.set_widget_name("cleaner-command");
    command.set_editable(false);
    command.set_placeholder_text(Some("Package uninstall command appears here"));
    content.append(&command);
    let copy = gtk::Button::with_label("Copy uninstall command");
    copy.set_sensitive(false);
    let run = gtk::Button::with_label("Run in terminal");
    run.set_widget_name("cleaner-run-command");
    run.set_tooltip_text(Some(
        "Run the displayed uninstall command. Review package and password prompts in the terminal.",
    ));
    run.set_sensitive(false);
    let status = label(
        "No files selected. Nothing is removed automatically.",
        "muted",
    );
    let clean = gtk::Button::with_label("Move selected to Trash");
    clean.add_css_class("suggested-action");
    clean.set_widget_name("cleaner-trash");
    clean.set_sensitive(false);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions.append(&copy);
    actions.append(&run);
    actions.append(&clean);
    content.append(&actions);
    content.append(&status);
    layout.append(&sidebar);
    let content_scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .build();
    layout.append(&content_scroll);
    let selected: Rc<RefCell<Vec<(gtk::CheckButton, Candidate)>>> = Rc::new(RefCell::new(vec![]));
    let generation = Rc::new(Cell::new(0u64));
    let (tx, rx) = std::sync::mpsc::channel::<(u64, Vec<Candidate>, String)>();
    let current = generation.clone();
    let app_data = apps.clone();
    let roots_scan = roots.clone();
    let sel = selected.clone();
    let files = file_list.clone();
    let clean_select = clean.clone();
    let command_select = command.clone();
    let copy_select = copy.clone();
    let run_select = run.clone();
    let summary_select = summary.clone();
    let status_select = status.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else { return };
        let a: &App = &app_data[row.index() as usize];
        current.set(current.get() + 1);
        let generation = current.get();
        title.set_text(&a.name);
        subtitle.set_text(&format!(
            "{} · {}",
            a.id,
            if a.flatpak.is_some() {
                "Flatpak"
            } else {
                "Desktop application"
            }
        ));
        summary_select.set_text("Finding related folders…");
        status_select.set_text("No files selected. Nothing is removed automatically.");
        while let Some(child) = files.first_child() {
            files.remove(&child);
        }
        sel.borrow_mut().clear();
        clean_select.set_sensitive(false);
        copy_select.set_sensitive(false);
        run_select.set_sensitive(false);
        command_select.set_text("");
        let a = a.clone();
        let roots = roots_scan.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let c = core::candidates(&a, &roots);
            let hint = core::uninstall_hint(&a);
            let _ = tx.send((generation, c, hint));
        });
    });
    let sel_poll = selected.clone();
    let clean_poll = clean.clone();
    let status_poll = status.clone();
    let command_poll = command.clone();
    let run_poll = run.clone();
    let weak_layout = layout.downgrade();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if weak_layout.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        while let Ok((id, candidates, hint)) = rx.try_recv() {
            if id != generation.get() {
                continue;
            }
            let total: u64 = candidates.iter().map(|c| c.bytes).sum();
            summary.set_text(&format!(
                "{} related folders · {}{}",
                candidates.len(),
                core::size(total),
                if candidates.iter().any(|c| c.incomplete) {
                    " or more (partial scan)"
                } else {
                    ""
                }
            ));
            if candidates.is_empty() {
                file_list.append(&label(
                    "No exact folder matches found. Shared and uncertain folders are left alone.",
                    "card",
                ));
            }
            command_poll.set_text(&hint);
            let executable = core::uninstall_terminal_args(&hint).is_ok();
            copy.set_sensitive(executable);
            run_poll.set_sensitive(executable);
            for candidate in candidates {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
                card.add_css_class("card");
                let check = gtk::CheckButton::with_label(&format!(
                    "{} · {}{}",
                    candidate.kind,
                    core::size(candidate.bytes),
                    if candidate.incomplete {
                        " (partial)"
                    } else {
                        ""
                    }
                ));
                let path = label(&candidate.path.to_string_lossy(), "muted");
                path.set_selectable(true);
                card.append(&check);
                card.append(&path);
                file_list.append(&card);
                sel_poll.borrow_mut().push((check.clone(), candidate));
                let state = Rc::downgrade(&sel_poll);
                let button = clean_poll.downgrade();
                let status = status_poll.downgrade();
                check.connect_toggled(move |_| {
                    let (Some(state), Some(button), Some(status)) =
                        (state.upgrade(), button.upgrade(), status.upgrade())
                    else {
                        return;
                    };
                    let data = state.borrow();
                    let n = data.iter().filter(|(c, _)| c.is_active()).count();
                    let bytes = data
                        .iter()
                        .filter(|(c, _)| c.is_active())
                        .map(|(_, c)| c.bytes)
                        .sum();
                    button.set_sensitive(n > 0);
                    status.set_text(&format!("{n} folders selected · {}", core::size(bytes)));
                });
            }
        }
        glib::ControlFlow::Continue
    });
    // Copy is connected independently of discovery completion.
    let copy_button = actions
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    let cmd = command.clone();
    copy_button.connect_clicked(move |b| b.clipboard().set_text(&cmd.text()));
    let cmd = command.clone();
    let run_status = status.clone();
    run.connect_clicked(move |_| {
        let hint = cmd.text();
        match core::uninstall_terminal_args(&hint).and_then(crate::terminal) {
            Ok(()) => run_status.set_text("Uninstall terminal opened. The command remains shown above. Review the package manager's prompts there; file cleanup is separate."),
            Err(error) => run_status.set_text(&format!("Could not run uninstall command: {error}")),
        }
    });
    let win = window.downgrade();
    clean.connect_clicked(move |_| {
        let Some(win) = win.upgrade() else { return; };
        let paths: Vec<_> = selected.borrow().iter().filter(|(c,_)|c.is_active()).map(|(_,c)|c.path.clone()).collect(); if paths.is_empty(){return;}
        let dialog = gtk::MessageDialog::builder().transient_for(&win).modal(true).message_type(gtk::MessageType::Warning).text(format!("Move {} folders to Trash?",paths.len())).secondary_text(format!("Close the application first. This can reset settings and remove local documents or saved sessions. The application itself stays installed.\n\n{}",paths.iter().map(|p|p.display().to_string()).collect::<Vec<_>>().join("\n"))).build();
        dialog.add_button("Cancel",gtk::ResponseType::Cancel); dialog.add_button("Move to Trash",gtk::ResponseType::Accept); dialog.set_default_response(gtk::ResponseType::Cancel);
        let roots = roots.clone(); let status = status.clone(); let selected = selected.clone();
        dialog.connect_response(move |d,response| { if response == gtk::ResponseType::Accept {
            let mut moved = 0; let mut errors = vec![];
            for path in &paths { if !roots.allowed(path) { errors.push(format!("{}: path changed or is outside allowed roots",path.display())); continue; }
                match core::trash_path(path) { Ok(()) => moved+=1, Err(e) => errors.push(format!("{}: {e}",path.display())) }
            }
            for (check,c) in selected.borrow().iter() { if !c.path.exists() { check.set_active(false); check.set_sensitive(false); } }
            status.set_text(&format!("Moved {moved} folders to Trash. {}",errors.join("\n")));
        } d.close(); }); dialog.present();
    });
    layout
}

pub(super) fn css() -> &'static str {
    include_str!("layout.css")
}

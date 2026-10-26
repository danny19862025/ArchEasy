use super::*;
use std::{cell::RefCell, rc::Rc};
pub struct Selection {
    items: RefCell<Vec<Package>>,
    checks: RefCell<Vec<glib::WeakRef<gtk::CheckButton>>>,
}
pub type Queue = Rc<Selection>;
pub fn queue_bar(parent: &gtk::Box, status: &gtk::Label) -> (Queue, gtk::Button) {
    let queue: Queue = Rc::new(Selection {
        items: RefCell::new(vec![]),
        checks: RefCell::new(vec![]),
    });
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let run = gtk::Button::with_label("Install queue (0)");
    run.set_widget_name("install-queue");
    let clear = gtk::Button::with_label("Clear queue");
    let q = queue.clone();
    let r = run.clone();
    clear.connect_clicked(move |_| {
        q.items.borrow_mut().clear();
        for check in q.checks.borrow().iter().filter_map(|w| w.upgrade()) {
            check.set_active(false);
        }
        r.set_label("Install queue (0)");
    });
    let q = queue.clone();
    let s = status.clone();
    run.connect_clicked(move |_| {
        let packages = q.items.borrow().clone();
        if packages.is_empty() { s.set_text("Tick packages in your search results to queue them."); return; }
        let mut args = vec!["/usr/bin/bash".into(), "-c".into(), include_str!("../scripts/install-queue.sh").into(), "archeasy-queue".into()];
        for p in packages { args.extend([p.source, p.remote, p.name]); }
        match terminal(args) { Ok(()) => s.set_text("Queue opened in one terminal. Each package is installed in order; a failed step stops the queue. Clear the queue after checking its summary."), Err(e) => s.set_text(&e) }
    });
    bar.append(&run);
    bar.append(&clear);
    parent.append(&bar);
    (queue, run)
}
pub fn checkbox(p: &Package, queue: &Queue, run: &gtk::Button) -> gtk::CheckButton {
    let check = gtk::CheckButton::with_label("Queue");
    let q = queue.clone();
    let r = run.clone();
    let p = p.clone();
    check.set_sensitive(!p.installed);
    check.set_active(q.items.borrow().iter().any(|x| key(x) == key(&p)));
    queue.checks.borrow_mut().retain(|w| w.upgrade().is_some());
    queue.checks.borrow_mut().push(check.downgrade());
    check.connect_toggled(move |check| {
        let mut items = q.items.borrow_mut();
        items.retain(|x| key(x) != key(&p));
        if check.is_active() {
            items.push(p.clone());
        }
        r.set_label(&format!("Install queue ({})", items.len()));
    });
    check
}
fn key(p: &Package) -> (&str, &str, &str) {
    (&p.source, &p.remote, &p.name)
}
pub fn installed_page() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let refresh = gtk::Button::with_label("Refresh installed packages");
    refresh.set_halign(gtk::Align::Start);
    let status = label(
        "Explicitly installed Pacman packages, including AUR packages. Removal asks for confirmation in your terminal.",
        "subtitle",
    );
    status.set_wrap(true);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Filter installed packages"));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    page.append(&refresh);
    page.append(&status);
    page.append(&search);
    page.append(&scroll);
    let l = list.clone();
    search.connect_search_changed(move |s| {
        let query = s.text().to_lowercase();
        l.set_filter_func(move |row| row.widget_name().to_lowercase().contains(&query));
    });
    refresh.connect_clicked(move |button| {
        button.set_sensitive(false);
        let button = button.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(output("pacman", &["-Qe"]));
        });
        let list = list.clone();
        let status = status.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let result = match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(_) => Err("Package query stopped unexpectedly".into()),
            };
            button.set_sensitive(true);
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            match result {
                Err(e) => status.set_text(&e),
                Ok(text) => {
                    status.set_text(&format!(
                        "{} explicitly installed packages. Refresh after removing a package.",
                        text.lines().count()
                    ));
                    for line in text.lines() {
                        let Some((name, version)) = line.split_once(' ') else {
                            continue;
                        };
                        if !valid_id(name) {
                            continue;
                        }
                        let row = gtk::ListBoxRow::new();
                        row.set_widget_name(name);
                        let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                        body.add_css_class("card");
                        let title = label(&format!("{name}  {version}"), "name");
                        title.set_hexpand(true);
                        body.append(&title);
                        let remove = gtk::Button::with_label("Remove");
                        let name = name.to_owned();
                        let status = status.clone();
                        remove.connect_clicked(move |_| {
                            match terminal(vec![
                                "sudo".into(),
                                "pacman".into(),
                                "-R".into(),
                                "--".into(),
                                name.clone(),
                            ]) {
                                Ok(()) => status.set_text(
                                    "Review the removal in your terminal, then refresh this list.",
                                ),
                                Err(e) => status.set_text(&e),
                            }
                        });
                        body.append(&remove);
                        row.set_child(Some(&body));
                        list.append(&row);
                    }
                }
            }
            glib::ControlFlow::Break
        });
    });
    // Load when the user first opens this page.
    page.connect_map(move |_| {
        refresh.emit_clicked();
    });
    page
}

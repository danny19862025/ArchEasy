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
        let requires_upgrade = packages.iter().any(|p| p.source == "Pacman" || p.source == "AUR");
        let s = s.clone();
        confirm_upgrade(requires_upgrade, move || {
        let mut args = vec!["/usr/bin/bash".into(), "-c".into(), include_str!("../scripts/install-queue.sh").into(), "archeasy-queue".into()];
        for p in packages { args.extend([p.source, p.remote, p.name]); }
        match terminal(args) { Ok(()) => s.set_text("Queue opened in one terminal. Each package is installed in order; a failed step stops the queue. Clear the queue after checking its summary."), Err(e) => s.set_text(&e) }
        });
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
mod installed;
pub use installed::page as installed_page;

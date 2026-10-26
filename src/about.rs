use gtk::{glib, prelude::*};

pub fn window(
    parent: &gtk::ApplicationWindow,
    name: &str,
    icon: &str,
    comments: &str,
) -> gtk::Window {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .hide_on_close(true)
        .title(format!("About {name}"))
        .default_width(540)
        .default_height(380)
        .build();
    window.set_widget_name("archeasy-about-window");
    follow_theme(parent, &window);
    // An explicit X works regardless of the desktop's title-button layout.
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    header.set_title_widget(Some(&gtk::Label::new(Some(&format!("About {name}")))));
    let close_x = gtk::Button::with_label("×");
    close_x.set_widget_name("about-close-x");
    close_x.set_tooltip_text(Some("Close About"));
    close_x.add_css_class("about-close");
    let weak = window.downgrade();
    close_x.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    header.pack_end(&close_x);
    window.set_titlebar(Some(&header));
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_margin_top(24);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(56);
    body.append(&image);
    let title = gtk::Label::new(Some(name));
    title.add_css_class("title");
    body.append(&title);
    let version = gtk::Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    version.add_css_class("subtitle");
    body.append(&version);
    let description = gtk::Label::new(Some(comments));
    description.set_wrap(true);
    description.set_max_width_chars(60);
    description.set_xalign(0.0);
    description.set_selectable(true);
    body.append(&description);
    let close = gtk::Button::with_label("Close");
    close.set_widget_name("about-close");
    close.set_halign(gtk::Align::End);
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    body.append(&close);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .build();
    window.set_child(Some(&scroll));
    let keys = gtk::EventControllerKey::new();
    let weak = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
    window
}
fn sync_theme(parent: &gtk::ApplicationWindow, window: &gtk::Window) {
    for class in ["light", "dark", "custom"] {
        if parent.has_css_class(class) {
            window.add_css_class(class);
        } else {
            window.remove_css_class(class);
        }
    }
}

pub fn follow_theme(parent: &gtk::ApplicationWindow, window: &gtk::Window) {
    window.add_css_class("archeasy");
    sync_theme(parent, window);
    let weak = window.downgrade();
    parent.connect_notify_local(Some("css-classes"), move |parent, _| {
        if let Some(window) = weak.upgrade() {
            sync_theme(parent, &window);
        }
    });
}

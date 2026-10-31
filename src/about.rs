use gtk::{glib, prelude::*};

fn svg_icon(bytes: &[u8], size: i32, fallback: &str) -> gtk::Image {
    if let Ok(loader) = gtk::gdk_pixbuf::PixbufLoader::with_type("svg") {
        loader.set_size(size * 2, size * 2);
        if loader.write(bytes).is_ok()
            && loader.close().is_ok()
            && let Some(pixbuf) = loader.pixbuf()
        {
            let image = gtk::Image::from_pixbuf(Some(&pixbuf));
            image.set_pixel_size(size);
            return image;
        }
    }
    gtk::Image::from_icon_name(fallback)
}

pub fn support_button(show_text: bool) -> gtk::LinkButton {
    let button = gtk::LinkButton::new("https://buymeacoffee.com/danny1986");
    button.set_widget_name("buy-me-a-coffee");
    button.set_tooltip_text(Some("Support Danny on Buy Me a Coffee"));
    button.set_valign(gtk::Align::Center);
    let icon = svg_icon(
        include_bytes!("../assets/coffee.svg"),
        24,
        "emblem-favorite-symbolic",
    );
    if show_text {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.append(&icon);
        content.append(&gtk::Label::new(Some("Buy Me a Coffee")));
        button.set_child(Some(&content));
    } else {
        button.set_child(Some(&icon));
    }
    button.update_property(&[gtk::accessible::Property::Label(
        "Buy Me a Coffee — support Danny",
    )]);
    button
}

fn animate_open(body: &gtk::Box) {
    let generation = std::rc::Rc::new(std::cell::Cell::new(0_u64));
    let current = generation.clone();
    body.connect_unmap(move |body| {
        current.set(current.get().wrapping_add(1));
        body.set_opacity(1.0);
        body.set_margin_top(20);
        body.set_margin_bottom(20);
    });
    body.connect_map(move |body| {
        let id = generation.get().wrapping_add(1);
        generation.set(id);
        if gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations()) {
            body.set_opacity(1.0);
            return;
        }
        body.set_opacity(0.0);
        let generation = generation.clone();
        let start = std::cell::Cell::new(None);
        body.add_tick_callback(move |body, clock| {
            if generation.get() != id || !body.is_mapped() {
                return glib::ControlFlow::Break;
            }
            let started = start.get().unwrap_or_else(|| {
                start.set(Some(clock.frame_time()));
                clock.frame_time()
            });
            let progress = ((clock.frame_time() - started) as f64 / 320_000.0).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - progress).powi(3);
            let offset = ((1.0 - eased) * 12.0).round() as i32;
            body.set_opacity(eased);
            body.set_margin_top(20 + offset);
            body.set_margin_bottom(20 - offset);
            if progress >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    });
}

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
        .default_width(840)
        .default_height(620)
        .build();
    window.set_widget_name("archeasy-about-window");
    follow_theme(parent, &window);
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
    let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
    body.set_widget_name("about-content");
    body.set_margin_top(20);
    body.set_margin_bottom(20);
    body.set_margin_start(28);
    body.set_margin_end(28);
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(72);
    hero.append(&image);
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.add_css_class("title");
    identity.append(&title);
    let version = gtk::Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    version.set_xalign(0.0);
    version.add_css_class("subtitle");
    identity.append(&version);
    if name == "ArchEasy" {
        let author = gtk::Label::new(Some("Created by Danny Rudd"));
        author.set_xalign(0.0);
        author.set_widget_name("about-author");
        author.add_css_class("heading");
        identity.append(&author);
    }
    hero.append(&identity);
    body.append(&hero);
    let overview = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    overview.set_vexpand(true);
    let description = gtk::Label::new(Some(comments));
    description.set_wrap(true);
    description.set_max_width_chars(72);
    description.set_xalign(0.0);
    description.set_valign(gtk::Align::Start);
    description.set_hexpand(true);
    description.set_selectable(true);
    overview.append(&description);
    if name == "ArchEasy" {
        let links = gtk::Box::new(gtk::Orientation::Vertical, 12);
        links.set_valign(gtk::Align::Start);
        let profile = gtk::LinkButton::new("https://github.com/danny19862025");
        profile.set_widget_name("about-github");
        profile.set_tooltip_text(Some("Open Danny Rudd’s GitHub profile"));
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.append(&svg_icon(
            include_bytes!("../assets/github.svg"),
            24,
            "web-browser-symbolic",
        ));
        content.append(&gtk::Label::new(Some("danny19862025 · GitHub")));
        profile.set_child(Some(&content));
        links.append(&profile);
        links.append(&support_button(true));
        overview.append(&links);
    }
    body.append(&overview);
    if name == "ArchEasy" {
        crate::updates::add_controls(&body);
    }
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
    animate_open(&body);
    // Natural content sizing keeps every control visible; no nested scrolling.
    window.set_child(Some(&body));
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

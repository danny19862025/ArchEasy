use super::*;
pub(super) fn add(card: &gtk::Box, preset: &'static Preset, parent: &gtk::ApplicationWindow) {
    let path = glib::user_data_dir()
        .join("archeasy/previews")
        .join(format!("{}.png", preset.id));
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_size_request(320, 180);
    picture.set_halign(gtk::Align::Start);
    picture.set_visible(false);
    let caption = super::super::label("Your saved screenshot", "subtitle");
    caption.set_visible(false);
    if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 640, 340, true) {
        picture.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
        picture.set_visible(true);
        caption.set_visible(true);
    } else if preset.id == "hyprland" {
        let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(
            include_bytes!("../../assets/hyprland-preview.svg"),
        ));
        if let Ok(pixbuf) =
            gtk::gdk_pixbuf::Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE)
        {
            picture.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
            picture.set_visible(true);
            caption.set_text("Preset illustration · gaps, rounded corners and blue/pink borders. Add your screenshot below.");
            caption.set_wrap(true);
            caption.set_visible(true);
        }
    }
    card.append(&picture);
    card.append(&caption);
    let add = gtk::Button::with_label("Add screenshot…");
    add.set_halign(gtk::Align::Start);
    let weak = parent.downgrade();
    add.connect_clicked(move |_| {
        let Some(parent) = weak.upgrade() else {
            return;
        };
        let weak = parent.downgrade();
        let picture = picture.clone();
        let caption = caption.clone();
        let path = path.clone();
        let filter = gtk::FileFilter::new();
        filter.add_mime_type("image/png");
        filter.add_mime_type("image/jpeg");
        filter.set_name(Some("Screenshots (PNG, JPEG)"));
        gtk::FileDialog::builder()
            .title("Choose a screenshot of your configured app or desktop")
            .default_filter(&filter)
            .build()
            .open(Some(&parent), gtk::gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else {
                    return;
                };
                let Some(source) = file.path() else {
                    return;
                };
                let result = (|| {
                    let meta = fs::metadata(&source).map_err(|e| e.to_string())?;
                    if meta.len() > 16 * 1024 * 1024 {
                        return Err("Choose an image under 16 MiB".into());
                    }
                    let (_, w, h) =
                        gtk::gdk_pixbuf::Pixbuf::file_info(&source).ok_or("Unsupported image")?;
                    if w > 16000 || h > 16000 {
                        return Err("Image dimensions are too large".into());
                    }
                    let pixbuf =
                        gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&source, 960, 500, true)
                            .map_err(|e| e.to_string())?;
                    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700))
                        .map_err(|e| e.to_string())?;
                    let bytes = pixbuf
                        .save_to_bufferv("png", &[])
                        .map_err(|e| e.to_string())?;
                    write_atomic(&path, &bytes, 0o600)?;
                    picture.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
                    picture.set_visible(true);
                    caption.set_text("Your saved screenshot");
                    caption.set_visible(true);
                    Ok::<_, String>(())
                })();
                if let Err(e) = result
                    && let Some(parent) = weak.upgrade()
                {
                    notice(&parent, "Could not save screenshot", &e);
                }
            });
    });
    card.append(&add);
}

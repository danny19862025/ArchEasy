use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

const LOGO: &[u8] = include_bytes!("../assets/archlinux-logo.svg");
const CLICK: &[u8] = include_bytes!("../assets/click.wav");

/// Run once using GTK's frame clock, then remove the intro and stop ticking.
pub fn opening_animation(overlay: &gtk::Overlay) {
    if gtk::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations()) {
        return;
    }
    let Ok(loader) = gtk::gdk_pixbuf::PixbufLoader::with_type("svg") else {
        return;
    };
    loader.set_size(512, 512);
    if loader.write(LOGO).is_err() || loader.close().is_err() {
        return;
    }
    let Some(logo) = loader.pixbuf() else {
        return;
    };
    let intro = gtk::Overlay::builder().hexpand(true).vexpand(true).build();
    let canvas = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let elapsed = Rc::new(Cell::new(0.0_f64));
    let progress = elapsed.clone();
    canvas.set_draw_func(move |_, cr, width, height| {
        use gtk::gdk::prelude::GdkCairoContextExt;
        let t = progress.get();
        let x = f64::from(width) / 2.0;
        let y = f64::from(height) / 2.0 - 55.0;
        let reveal = (t / 0.8).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - reveal).powi(3);
        cr.set_source_rgb(0.035, 0.055, 0.09);
        let _ = cr.paint();
        let glow = gtk::cairo::RadialGradient::new(x, y, 0.0, x, y, 290.0);
        glow.add_color_stop_rgba(0.0, 0.09, 0.58, 0.82, 0.32 * ease);
        glow.add_color_stop_rgba(1.0, 0.09, 0.58, 0.82, 0.0);
        if cr.set_source(&glow).is_ok() {
            let _ = cr.paint();
        }
        for offset in [0.0, 0.32] {
            let pulse = ((t - offset) / 1.5).clamp(0.0, 1.0);
            cr.set_source_rgba(0.15, 0.67, 0.94, ease * (1.0 - pulse) * 0.28);
            cr.set_line_width(1.5);
            cr.arc(x, y, 105.0 + pulse * 145.0, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
        }
        let size = 220.0 * (0.72 + 0.28 * ease);
        if cr.save().is_ok() {
            cr.translate(x - size / 2.0, y - size / 2.0 + (1.0 - ease) * 22.0);
            cr.scale(
                size / f64::from(logo.width()),
                size / f64::from(logo.height()),
            );
            cr.set_source_pixbuf(&logo, 0.0, 0.0);
            let _ = cr.paint_with_alpha(ease);
            let _ = cr.restore();
        }
    });
    intro.set_child(Some(&canvas));
    let wordmark = gtk::Box::new(gtk::Orientation::Vertical, 8);
    wordmark.set_halign(gtk::Align::Center);
    wordmark.set_valign(gtk::Align::Center);
    wordmark.set_margin_top(235);
    let title = gtk::Label::new(Some("ArchEasy"));
    title.add_css_class("opening-title");
    wordmark.append(&title);
    let subtitle = gtk::Label::new(Some("Software made simple"));
    subtitle.add_css_class("opening-subtitle");
    wordmark.append(&subtitle);
    wordmark.set_opacity(0.0);
    intro.add_overlay(&wordmark);
    overlay.add_overlay(&intro);

    let weak_overlay = overlay.downgrade();
    let weak_intro = intro.downgrade();
    let start = Cell::new(None);
    canvas.add_tick_callback(move |canvas, clock| {
        let Some(intro) = weak_intro.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let now = clock.frame_time();
        let first = start.get().unwrap_or(now);
        start.set(Some(first));
        let t = (now - first) as f64 / 1_000_000.0;
        elapsed.set(t);
        wordmark.set_opacity(((t - 0.25) / 0.5).clamp(0.0, 1.0));
        intro.set_opacity(1.0 - ((t - 1.35) / 0.45).clamp(0.0, 1.0));
        if t >= 1.8 {
            if let Some(overlay) = weak_overlay.upgrade() {
                overlay.remove_overlay(&intro);
            }
            return glib::ControlFlow::Break;
        }
        canvas.queue_draw();
        glib::ControlFlow::Continue
    });
    let click = gtk::GestureClick::new();
    let weak_overlay = overlay.downgrade();
    let weak_intro = intro.downgrade();
    click.connect_pressed(move |_, _, _, _| {
        if let (Some(overlay), Some(intro)) = (weak_overlay.upgrade(), weak_intro.upgrade()) {
            overlay.remove_overlay(&intro);
        }
    });
    intro.add_controller(click);
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak_overlay = overlay.downgrade();
    let weak_intro = intro.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape
            && let (Some(overlay), Some(intro)) = (weak_overlay.upgrade(), weak_intro.upgrade())
        {
            overlay.remove_overlay(&intro);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    overlay.add_controller(keys);
}

pub fn background(content: &gtk::Box) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    let glow = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .can_target(false)
        .build();
    glow.set_draw_func(|_, cr, width, height| {
        let x = width as f64 - 235.0;
        let y = height as f64 * 0.52;
        let glow = gtk::cairo::RadialGradient::new(x, y, 0.0, x, y, 330.0);
        glow.add_color_stop_rgba(0.0, 0.09, 0.58, 0.82, 0.24);
        glow.add_color_stop_rgba(0.45, 0.09, 0.58, 0.82, 0.11);
        glow.add_color_stop_rgba(1.0, 0.09, 0.58, 0.82, 0.0);
        if cr.set_source(&glow).is_ok() {
            let _ = cr.paint();
        }
    });
    overlay.set_child(Some(&glow));
    // Keep the system's actual Arch vector logo crisp at every display scale.
    if let Ok(loader) = gtk::gdk_pixbuf::PixbufLoader::with_type("svg")
        && loader.write(LOGO).is_ok()
        && loader.close().is_ok()
        && let Some(pixbuf) = loader.pixbuf()
    {
        let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
        let logo = gtk::Picture::for_paintable(&texture);
        logo.set_content_fit(gtk::ContentFit::Contain);
        logo.set_size_request(350, 350);
        logo.set_halign(gtk::Align::End);
        logo.set_valign(gtk::Align::Center);
        logo.set_margin_end(60);
        logo.set_opacity(0.12);
        logo.set_can_target(false);
        overlay.add_overlay(&logo);
    }
    content.set_hexpand(true);
    content.set_vexpand(true);
    overlay.add_overlay(content);
    overlay.set_measure_overlay(content, true);
    overlay
}

fn sound_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy/sound")
}
struct ClickSound {
    enabled: Cell<bool>,
    last_played: Cell<Option<Instant>>,
    player: Option<&'static str>,
    file: Option<std::path::PathBuf>,
}
impl ClickSound {
    fn new() -> Self {
        let enabled = std::fs::read_to_string(sound_path())
            .map(|s| s.trim() != "off")
            .unwrap_or(true);
        let player = ["pw-play", "paplay", "aplay"]
            .into_iter()
            .find(|p| super::available(p));
        let file = glib::user_cache_dir().join("archeasy/click-v1.wav");
        let written = std::fs::create_dir_all(file.parent().unwrap())
            .and_then(|()| std::fs::write(&file, CLICK))
            .is_ok();
        Self {
            enabled: Cell::new(enabled),
            last_played: Cell::new(None),
            player,
            file: written.then_some(file),
        }
    }
    fn play(&self) {
        if !self.enabled.get()
            || self
                .last_played
                .get()
                .is_some_and(|t| t.elapsed() < Duration::from_millis(80))
        {
            return;
        }
        let (Some(player), Some(file)) = (self.player, self.file.as_ref()) else {
            return;
        };
        self.last_played.set(Some(Instant::now()));
        let file = file.clone();
        std::thread::spawn(move || {
            let mut command = std::process::Command::new(player);
            match player {
                "pw-play" => {
                    command.args(["--volume", "0.35", "--media-role", "Notification"]);
                }
                "paplay" => {
                    command.arg("--volume=22000");
                }
                _ => {}
            }
            let _ = command
                .arg(file)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }
}
fn is_control(mut widget: Option<gtk::Widget>) -> bool {
    while let Some(current) = widget {
        if (current.is::<gtk::Button>() || current.is::<gtk::DropDown>()) && current.is_sensitive()
        {
            return true;
        }
        widget = current.parent();
    }
    false
}
// Capture pointer presses without claiming the sequence: normal GTK activation
// and scrolling keep working, including buttons added after a search.
pub fn install_sound(window: &gtk::ApplicationWindow) -> gtk::ToggleButton {
    let sound = Rc::new(ClickSound::new());
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let clicks = sound.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        if let Some(widget) = gesture.widget()
            && is_control(widget.pick(x, y, gtk::PickFlags::DEFAULT))
        {
            clicks.play();
        }
    });
    window.add_controller(gesture);
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let clicks = sound.clone();
    let weak = window.downgrade();
    keys.connect_key_released(move |_, key, _, _| {
        if matches!(
            key,
            gtk::gdk::Key::space | gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter
        ) && let Some(window) = weak.upgrade()
            && is_control(gtk::prelude::GtkWindowExt::focus(&window))
        {
            clicks.play();
        }
    });
    window.add_controller(keys);
    let toggle = gtk::ToggleButton::with_label(if sound.enabled.get() {
        "Sound on"
    } else {
        "Muted"
    });
    toggle.set_active(sound.enabled.get());
    toggle.set_valign(gtk::Align::Center);
    toggle.set_tooltip_text(Some(
        "Toggle subtle button click sounds. Your choice is saved.",
    ));
    toggle.connect_toggled(move |button| {
        sound.enabled.set(button.is_active());
        button.set_label(if button.is_active() {
            "Sound on"
        } else {
            "Muted"
        });
        if button.is_active() {
            sound.play();
        }
        let path = sound_path();
        let result = std::fs::create_dir_all(path.parent().unwrap())
            .and_then(|()| std::fs::write(path, if button.is_active() { "on" } else { "off" }));
        if let Err(error) = result {
            eprintln!("Could not save sound preference: {error}");
        }
    });
    toggle
}

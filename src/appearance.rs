use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

const LOGO: &[u8] = include_bytes!("../assets/archlinux-logo.svg");
const CLICK: &[u8] = include_bytes!("../assets/click.wav");

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

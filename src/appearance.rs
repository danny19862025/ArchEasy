use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

const LOGO: &[u8] = include_bytes!("../assets/archlinux-logo.svg");
const CLICK: &[u8] = include_bytes!("../assets/click.wav");
const BRAND_FONT: &[u8] = include_bytes!("../assets/fonts/Ribeye-Regular.ttf");

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigAppFontAddFile(config: *mut std::ffi::c_void, file: *const u8) -> i32;
}

/// Register the embedded font before GTK creates its font map. This also works
/// from a standalone release binary, without installing fonts on the system.
pub fn load_brand_font() -> std::io::Result<()> {
    let directory = glib::user_cache_dir().join("archeasy/fonts");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("Ribeye-Regular.ttf");
    if std::fs::read(&path).ok().as_deref() != Some(BRAND_FONT) {
        std::fs::write(&path, BRAND_FONT)?;
    }
    let filename = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    // SAFETY: null selects Fontconfig's current configuration; filename is a
    // live, NUL-terminated path. Called on the main thread before GTK starts.
    if unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), filename.as_ptr().cast()) } == 0 {
        return Err(std::io::Error::other(
            "Could not register the bundled Ribeye font",
        ));
    }
    Ok(())
}

/// Type the wordmark once, then leave the completed text without a caret.
fn animate_title(title: &gtk::Label) {
    if gtk::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations()) {
        title.set_text("ArchEasy");
        return;
    }
    title.set_markup("▏");
    let weak = title.downgrade();
    let mut letters = 0;
    glib::timeout_add_local(Duration::from_millis(143), move || {
        let Some(title) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        letters += 1;
        if letters == "ArchEasy".len() {
            title.set_text("ArchEasy");
            return glib::ControlFlow::Break;
        }
        title.set_text(&format!("{}▏", &"ArchEasy"[..letters]));
        glib::ControlFlow::Continue
    });
}

/// Opening splash: only the Arch logo, animated on its own borderless,
/// transparent window (no app window behind it). `done` runs once when the
/// animation ends or is skipped (click / Esc), and is where the main window
/// gets presented.
pub fn splash(app: &gtk::Application, title: &gtk::Label, done: impl FnOnce() + 'static) {
    let title = title.clone();
    let done: Rc<std::cell::RefCell<Option<Box<dyn FnOnce()>>>> =
        Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
            done();
            animate_title(&title);
        }))));
    let finish = {
        let done = done.clone();
        move || {
            if let Some(done) = done.borrow_mut().take() {
                done();
            }
        }
    };
    if gtk::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations()) {
        finish();
        return;
    }
    let Some(logo) = gtk::gdk_pixbuf::PixbufLoader::with_type("svg")
        .ok()
        .and_then(|loader| {
            loader.set_size(512, 512);
            (loader.write(LOGO).is_ok() && loader.close().is_ok()).then(|| loader.pixbuf())?
        })
    else {
        finish();
        return;
    };
    // Maximized + transparent: the compositor centres nothing for us on
    // Wayland, so cover the work area and draw the logo in its middle.
    let window = gtk::Window::builder()
        .application(app)
        .decorated(false)
        .maximized(true)
        .title("ArchEasy")
        .build();
    window.add_css_class("archeasy-splash");
    let canvas = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
    let elapsed = Rc::new(Cell::new(0.0_f64));
    let progress = elapsed.clone();
    canvas.set_draw_func(move |_, cr, width, height| {
        use gtk::gdk::prelude::GdkCairoContextExt;
        let t = progress.get();
        let x = f64::from(width) / 2.0;
        let y = f64::from(height) / 2.0;
        let reveal = (t / 0.8).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - reveal).powi(3);
        let leave = ((t - 1.95) / 0.45).clamp(0.0, 1.0);
        let alpha = ease * (1.0 - leave);
        let size = 220.0 * (0.72 + 0.28 * ease) * (1.0 + 0.12 * leave);
        // A few moons on tilted elliptical orbits: (radius, flatten, tilt,
        // speed rad/s, start angle, dot radius, rgb). The far half of each
        // orbit is drawn behind the logo, the near half in front of it.
        let orbits = [
            (150.0, 0.34, -0.35, 2.6, 0.0, 6.0, (1.0, 0.478, 0.4)),
            (185.0, 0.28, 0.42, -1.9, 2.1, 4.5, (0.62, 0.84, 1.0)),
            (215.0, 0.22, -0.08, 1.4, 4.2, 3.5, (0.5, 0.88, 0.71)),
        ];
        let scale = 0.72 + 0.28 * ease;
        let moons = |cr: &gtk::cairo::Context, front: bool| {
            for (radius, flatten, tilt, speed, phase, dot, (r, g, b)) in orbits {
                let radius = radius * scale;
                if cr.save().is_err() {
                    continue;
                }
                cr.translate(x, y);
                cr.rotate(tilt);
                if !front {
                    // Orbit path, faint, drawn once (behind everything).
                    if cr.save().is_ok() {
                        cr.scale(1.0, flatten);
                        cr.arc(0.0, 0.0, radius, 0.0, std::f64::consts::TAU);
                        let _ = cr.restore();
                    }
                    cr.set_line_width(1.0);
                    cr.set_source_rgba(0.62, 0.84, 1.0, 0.22 * alpha);
                    let _ = cr.stroke();
                }
                let angle = phase + t * speed;
                let in_front = angle.sin() > 0.0;
                if in_front == front {
                    let (mx, my) = (radius * angle.cos(), radius * flatten * angle.sin());
                    let depth = 0.8 + 0.2 * angle.sin();
                    let halo = gtk::cairo::RadialGradient::new(mx, my, 0.0, mx, my, dot * 3.2);
                    halo.add_color_stop_rgba(0.0, r, g, b, 0.45 * alpha);
                    halo.add_color_stop_rgba(1.0, r, g, b, 0.0);
                    if cr.set_source(&halo).is_ok() {
                        cr.arc(mx, my, dot * 3.2, 0.0, std::f64::consts::TAU);
                        let _ = cr.fill();
                    }
                    cr.set_source_rgba(r, g, b, alpha);
                    cr.arc(mx, my, dot * depth, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
                let _ = cr.restore();
            }
        };
        moons(cr, false);
        if cr.save().is_ok() {
            cr.translate(x - size / 2.0, y - size / 2.0 + (1.0 - ease) * 22.0);
            cr.scale(
                size / f64::from(logo.width()),
                size / f64::from(logo.height()),
            );
            cr.set_source_pixbuf(&logo, 0.0, 0.0);
            let _ = cr.paint_with_alpha(alpha);
            let _ = cr.restore();
        }
        moons(cr, true);
    });
    window.set_child(Some(&canvas));

    let close = {
        let weak = window.downgrade();
        let finish = finish.clone();
        move || {
            finish();
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        }
    };
    let start = Cell::new(None);
    let tick_close = close.clone();
    canvas.add_tick_callback(move |canvas, clock| {
        let now = clock.frame_time();
        let first = start.get().unwrap_or(now);
        start.set(Some(first));
        let t = (now - first) as f64 / 1_000_000.0;
        elapsed.set(t);
        if t >= 2.4 {
            tick_close();
            return glib::ControlFlow::Break;
        }
        canvas.queue_draw();
        glib::ControlFlow::Continue
    });
    let click = gtk::GestureClick::new();
    let click_close = close.clone();
    click.connect_pressed(move |_, _, _, _| click_close());
    window.add_controller(click);
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            close();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.present();
}

/// Deep-space backdrop: void fill, coral and ice nebula washes, and a
/// deterministic starfield that twinkles slowly with the frame clock.
fn draw_starfield(
    cr: &gtk::cairo::Context,
    width: f64,
    height: f64,
    clock: Option<gtk::gdk::FrameClock>,
) {
    let t = clock.map_or(0.0, |c| c.frame_time() as f64 / 1_000_000.0);
    cr.set_source_rgb(0.024, 0.039, 0.078); // #060A14
    let _ = cr.paint();
    for (cx, cy, radius, (r, g, b), alpha) in [
        (width * 0.82, height * 0.18, width.max(height) * 0.55, (0.62, 0.84, 1.0), 0.09),
        (width * 0.12, height * 0.95, width.max(height) * 0.5, (1.0, 0.48, 0.40), 0.07),
        (width * 0.55, height * 0.6, width.max(height) * 0.35, (0.45, 0.35, 0.85), 0.05),
    ] {
        let wash = gtk::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, radius);
        wash.add_color_stop_rgba(0.0, r, g, b, alpha);
        wash.add_color_stop_rgba(1.0, r, g, b, 0.0);
        if cr.set_source(&wash).is_ok() {
            let _ = cr.paint();
        }
    }
    // Simple LCG so the sky is identical every frame and every launch.
    let mut seed: u32 = 0x5eed_1a5;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(seed >> 8) / f64::from(1u32 << 24)
    };
    let count = ((width * height) / 5200.0).clamp(80.0, 520.0) as usize;
    for _ in 0..count {
        let (x, y) = (next() * width, next() * height);
        let size = 0.35 + next().powi(3) * 1.4;
        let phase = next() * std::f64::consts::TAU;
        let speed = 0.4 + next() * 1.2;
        let ice = next() > 0.82;
        let twinkle = 0.55 + 0.45 * (t * speed + phase).sin();
        let alpha = (0.25 + size * 0.4).min(0.95) * twinkle;
        if ice {
            cr.set_source_rgba(0.62, 0.84, 1.0, alpha);
        } else {
            cr.set_source_rgba(0.91, 0.93, 0.97, alpha);
        }
        cr.arc(x, y, size, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

/// The original soft blue halo behind the logo (non-Stellar themes).
fn draw_glow(cr: &gtk::cairo::Context, width: f64, height: f64) {
    let x = width - 235.0;
    let y = height * 0.52;
    let glow = gtk::cairo::RadialGradient::new(x, y, 0.0, x, y, 330.0);
    let dark = adw::StyleManager::default().is_dark();
    let (red, green, blue, strength) = if dark {
        (0.39, 0.60, 0.84, 0.12)
    } else {
        (0.09, 0.58, 0.82, 0.24)
    };
    glow.add_color_stop_rgba(0.0, red, green, blue, strength);
    glow.add_color_stop_rgba(0.45, red, green, blue, strength * 0.45);
    glow.add_color_stop_rgba(1.0, 0.09, 0.58, 0.82, 0.0);
    if cr.set_source(&glow).is_ok() {
        let _ = cr.paint();
    }
}

/// Live system orbits around the home-page Arch logo. Each moon is a real
/// reading: CPU (coral, orbit speed = load), memory (ice, moon size = used
/// share) and network (mint, speed = throughput). The logo brightens with CPU.
struct LiveOrbits {
    sampler: crate::live_stats::Sampler,
    sampled_at: Instant,
    last_frame: Instant,
    started: Instant,
    /// Smoothed display values so moons ease between one-second samples.
    cpu: f64,
    mem: f64,
    net: f64,
    angles: [f64; 3],
    labels: [String; 3],
}

impl LiveOrbits {
    // (radius, flatten, tilt) per orbit: CPU, memory, network.
    const ORBITS: [(f64, f64, f64); 3] = [(150.0, 0.34, -0.35), (195.0, 0.28, 0.40), (240.0, 0.22, -0.10)];
    const COLOURS: [(f64, f64, f64); 3] = [(1.0, 0.478, 0.4), (0.62, 0.84, 1.0), (0.5, 0.88, 0.71)];

    fn new() -> Self {
        let now = Instant::now();
        let mut orbits = Self {
            sampler: crate::live_stats::Sampler::new(),
            sampled_at: now,
            last_frame: now,
            started: now,
            cpu: 0.0,
            mem: 0.0,
            net: 0.0,
            angles: [0.0, 2.1, 4.2],
            labels: Default::default(),
        };
        orbits.relabel();
        orbits
    }

    fn relabel(&mut self) {
        let r = self.sampler.latest;
        self.labels = [
            format!("CPU {:.0}%", r.cpu * 100.0),
            format!(
                "RAM {:.1} / {:.0} GB",
                r.mem_used_kib as f64 / 1_048_576.0,
                r.mem_total_kib as f64 / 1_048_576.0
            ),
            format!("NET {}", crate::live_stats::format_rate(r.net_bytes_per_sec)),
        ];
    }

    /// Advance one frame; returns the logo opacity for this frame.
    fn advance(&mut self, animate: bool) -> f64 {
        let now = Instant::now();
        if now.duration_since(self.sampled_at) >= Duration::from_secs(1) {
            self.sampler.sample();
            self.sampled_at = now;
            self.relabel();
        }
        let dt = now.duration_since(self.last_frame).as_secs_f64().min(0.1);
        self.last_frame = now;
        let r = self.sampler.latest;
        // Network: 0 at idle, ~1 around 10 MB/s (log scale).
        let net_target = ((r.net_bytes_per_sec + 1.0).log10() / 7.0).clamp(0.0, 1.0);
        let ease = 1.0 - (-dt * 3.0).exp();
        self.cpu += (r.cpu - self.cpu) * ease;
        self.mem += (r.mem_fraction() - self.mem) * ease;
        self.net += (net_target - self.net) * ease;
        if animate {
            let speeds = [0.25 + 3.2 * self.cpu, -0.45, 0.2 + 2.4 * self.net];
            for (angle, speed) in self.angles.iter_mut().zip(speeds) {
                *angle = (*angle + speed * dt) % std::f64::consts::TAU;
            }
        }
        let breathe = if animate {
            (now.duration_since(self.started).as_secs_f64() * (1.2 + 3.0 * self.cpu)).sin()
        } else {
            0.0
        };
        0.10 + 0.10 * self.cpu + 0.02 * breathe
    }

    /// Draw the far (`front == false`) or near half of every orbit.
    /// Size of the orbit canvas; its right edge sits on the window edge and
    /// the logo centre is 235 px in from there, vertically centred.
    const BOX: (i32, i32) = (520, 380);

    fn draw(&self, cr: &gtk::cairo::Context, width: f64, height: f64, front: bool) {
        let (x, y) = (width - 235.0, height * 0.5);
        let sizes = [5.5, 3.0 + 7.0 * self.mem, 4.0 + 2.0 * self.net];
        for (i, &(radius, flatten, tilt)) in Self::ORBITS.iter().enumerate() {
            let (r, g, b) = Self::COLOURS[i];
            let angle = self.angles[i];
            if cr.save().is_err() {
                continue;
            }
            cr.translate(x, y);
            cr.rotate(tilt);
            if !front {
                if cr.save().is_ok() {
                    cr.scale(1.0, flatten);
                    cr.arc(0.0, 0.0, radius, 0.0, std::f64::consts::TAU);
                    let _ = cr.restore();
                }
                cr.set_line_width(1.0);
                cr.set_source_rgba(r, g, b, 0.16);
                let _ = cr.stroke();
            }
            if (angle.sin() > 0.0) == front {
                let (mx, my) = (radius * angle.cos(), radius * flatten * angle.sin());
                let dot = sizes[i] * (0.8 + 0.2 * angle.sin());
                let halo = gtk::cairo::RadialGradient::new(mx, my, 0.0, mx, my, dot * 3.4);
                halo.add_color_stop_rgba(0.0, r, g, b, 0.45);
                halo.add_color_stop_rgba(1.0, r, g, b, 0.0);
                if cr.set_source(&halo).is_ok() {
                    cr.arc(mx, my, dot * 3.4, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
                cr.set_source_rgba(r, g, b, 0.95);
                cr.arc(mx, my, dot, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
            let _ = cr.restore();
        }
        if front {
            // Fixed legend under the logo: one row per moon, never colliding.
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
            cr.set_font_size(11.5);
            for (i, label) in self.labels.iter().enumerate() {
                let (r, g, b) = Self::COLOURS[i];
                let row = y + 128.0 + 19.0 * i as f64;
                cr.set_source_rgba(r, g, b, 0.95);
                cr.arc(x - 58.0, row - 4.0, 4.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
                cr.set_source_rgba(r, g, b, 0.85);
                cr.move_to(x - 46.0, row);
                let _ = cr.show_text(label);
            }
        }
    }
}

pub fn background(content: &gtk::Box) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    let glow = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .can_target(false)
        .build();
    overlay.set_child(Some(&glow));
    let live = Rc::new(std::cell::RefCell::new(LiveOrbits::new()));
    let behind = live.clone();
    let sky: Rc<std::cell::RefCell<Option<((i32, i32), gtk::cairo::ImageSurface)>>> =
        Rc::default();
    glow.set_draw_func(move |area, cr, width, height| {
        let (w, h) = (f64::from(width), f64::from(height));
        if area.root().is_some_and(|root| root.has_css_class("stellar")) {
            // Rendered once per window size, then just blitted.
            let mut cache = sky.borrow_mut();
            if cache.as_ref().is_none_or(|(size, _)| *size != (width, height))
                && let Ok(surface) =
                    gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, width, height)
                && let Ok(sky_cr) = gtk::cairo::Context::new(&surface)
            {
                draw_starfield(&sky_cr, w, h, None);
                drop(sky_cr);
                *cache = Some(((width, height), surface));
            }
            if let Some((_, surface)) = cache.as_ref()
                && cr.set_source_surface(surface, 0.0, 0.0).is_ok()
            {
                let _ = cr.paint();
            } else {
                draw_starfield(cr, w, h, None);
            }
        } else {
            draw_glow(cr, w, h);
        }
    });
    let orbit_area = |live: Rc<std::cell::RefCell<LiveOrbits>>, front: bool| {
        let area = gtk::DrawingArea::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .content_width(LiveOrbits::BOX.0)
            .content_height(LiveOrbits::BOX.1)
            .can_target(false)
            .build();
        area.set_draw_func(move |_, cr, width, height| {
            live.borrow().draw(cr, f64::from(width), f64::from(height), front);
        });
        area
    };
    let behind = orbit_area(behind, false);
    overlay.add_overlay(&behind);
    // Keep the system's actual Arch vector logo crisp at every display scale.
    let mut logo_picture = None;
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
        logo_picture = Some(logo);
    }
    // Near halves of the orbits pass in front of the logo (still behind content).
    let front = orbit_area(live.clone(), true);
    overlay.add_overlay(&front);
    // ~20 fps animation; /proc is sampled once a second inside `advance`.
    let weak_glow = glow.downgrade();
    let weak_behind = behind.downgrade();
    let weak_front = front.downgrade();
    let mut last_look = (false, false);
    let mut idle_ticks = 0_u32;
    let weak_logo = logo_picture.map(|logo| logo.downgrade());
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let (Some(glow), Some(behind), Some(front)) =
            (weak_glow.upgrade(), weak_behind.upgrade(), weak_front.upgrade())
        else {
            return glib::ControlFlow::Break;
        };
        if !glow.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        // Smooth only while the window is focused; otherwise refresh the
        // readings about once a second so background use stays near zero.
        let active = glow
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
            .is_some_and(|window| window.is_active());
        idle_ticks = if active { 0 } else { idle_ticks + 1 };
        if !active && idle_ticks % 20 != 1 {
            return glib::ControlFlow::Continue;
        }
        let animate = active
            && gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
        let pulse = live.borrow_mut().advance(animate);
        if let Some(logo) = weak_logo.as_ref().and_then(|logo| logo.upgrade()) {
            logo.set_opacity(pulse);
        }
        behind.queue_draw();
        front.queue_draw();
        // The full-window background only redraws when the look changes.
        let stellar = glow.root().is_some_and(|r| r.has_css_class("stellar"));
        let look = (stellar, adw::StyleManager::default().is_dark());
        if look != last_look {
            glow.queue_draw();
        }
        last_look = look;
        glib::ControlFlow::Continue
    });
    content.set_hexpand(true);
    content.set_vexpand(true);
    overlay.add_overlay(content);
    overlay.set_measure_overlay(content, true);
    overlay
}

fn sound_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy/sound")
}
fn volume_path() -> std::path::PathBuf {
    glib::user_config_dir().join("archeasy/sound-volume")
}
fn click_at_volume(volume: u8) -> Vec<u8> {
    let mut wav = CLICK.to_vec();
    let mut chunk = 12;
    while chunk + 8 <= wav.len() {
        let length = u32::from_le_bytes(wav[chunk + 4..chunk + 8].try_into().unwrap()) as usize;
        let start = chunk + 8;
        let end = (start + length).min(wav.len());
        if &wav[chunk..chunk + 4] == b"data" {
            for sample in wav[start..end].as_chunks_mut::<2>().0 {
                let value = i16::from_le_bytes([sample[0], sample[1]]);
                sample.copy_from_slice(
                    &((f64::from(value) * f64::from(volume.min(100)) / 100.0).round() as i16)
                        .to_le_bytes(),
                );
            }
            break;
        }
        chunk = end + length % 2;
    }
    wav
}
struct ClickSound {
    enabled: Cell<bool>,
    volume: Cell<u8>,
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
            volume: Cell::new(
                std::fs::read_to_string(volume_path())
                    .ok()
                    .and_then(|s| s.trim().parse::<u8>().ok())
                    .filter(|n| *n <= 100)
                    .unwrap_or(35),
            ),
            last_played: Cell::new(None),
            player,
            file: written.then_some(file),
        }
    }
    fn play(&self) {
        if !self.enabled.get()
            || self.volume.get() == 0
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
        let volume = self.volume.get();
        let file = file.with_file_name(format!("click-v2-{volume}.wav"));
        std::thread::spawn(move || {
            if !file.is_file() && std::fs::write(&file, click_at_volume(volume)).is_err() {
                return;
            }
            let mut command = std::process::Command::new(player);
            if player == "pw-play" {
                command.args(["--media-role", "Notification"]);
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
pub fn install_sound(window: &gtk::ApplicationWindow) -> gtk::Box {
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
    let sound_toggle = sound.clone();
    toggle.connect_toggled(move |button| {
        let sound = &sound_toggle;
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
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.append(&toggle);
    let settings = gtk::MenuButton::builder()
        .label("⚙")
        .tooltip_text("Sound settings")
        .valign(gtk::Align::Center)
        .build();
    settings.set_widget_name("sound-settings");
    let popover = gtk::Popover::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&gtk::Label::new(Some("Click volume")));
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.set_widget_name("sound-volume");
    volume.set_size_request(170, -1);
    volume.set_digits(0);
    volume.set_draw_value(true);
    volume.set_value(f64::from(sound.volume.get()));
    volume.connect_value_changed(move |scale| {
        let value = scale.value().round().clamp(0.0, 100.0) as u8;
        sound.volume.set(value);
        let path = volume_path();
        let result = std::fs::create_dir_all(path.parent().unwrap())
            .map_err(|e| e.to_string())
            .and_then(|()| {
                crate::configs::write_atomic(&path, value.to_string().as_bytes(), 0o600)
            });
        if let Err(error) = result {
            eprintln!("Could not save click volume: {error}");
        }
        sound.play();
    });
    body.append(&volume);
    popover.set_child(Some(&body));
    settings.set_popover(Some(&popover));
    controls.append(&settings);
    controls
}

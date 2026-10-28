use super::*;
use std::cell::Cell;

#[derive(Clone, Debug, PartialEq)]
struct Installed {
    id: String,
    title: String,
    version: String,
    source: usize,
    scope: Option<&'static str>,
}
impl Installed {
    fn removal_args(&self, clean: bool) -> Vec<String> {
        if let Some(scope) = self.scope {
            vec!["flatpak", "uninstall", scope, "--", &self.id]
        } else {
            vec![
                "sudo",
                "pacman",
                if clean { "-Rns" } else { "-R" },
                "--",
                &self.id,
            ]
        }
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
}
const SOURCES: [&str; 3] = ["Pacman", "AUR", "Flatpak"];
const COLORS: [(f64, f64, f64); 3] = [(0.24, 0.68, 0.96), (0.89, 0.62, 0.98), (0.33, 0.80, 0.71)];
fn parse_packages(text: &str, source: usize) -> Vec<Installed> {
    text.lines()
        .filter_map(|line| {
            let (id, version) = line.split_once(' ')?;
            valid_id(id).then(|| Installed {
                id: id.into(),
                title: id.into(),
                version: version.into(),
                source,
                scope: None,
            })
        })
        .collect()
}
fn parse_flatpaks(text: &str, scope: &'static str) -> Vec<Installed> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?;
            valid_id(id).then(|| Installed {
                id: id.into(),
                title: fields.next().filter(|s| !s.is_empty()).unwrap_or(id).into(),
                version: fields.next().unwrap_or("").into(),
                source: 2,
                scope: Some(scope),
            })
        })
        .collect()
}
fn query_packages(flag: &str, source: usize) -> Result<Vec<Installed>, String> {
    // Pacman exits 1 with empty stderr when no packages match a query.
    match output("pacman", &[flag]) {
        Ok(text) => Ok(parse_packages(&text, source)),
        Err(error) if error.is_empty() => Ok(vec![]),
        Err(error) => Err(error),
    }
}
struct InstalledSnapshot {
    sources: [Result<Vec<Installed>, String>; 3],
    combined_pacman: Option<String>,
}
fn query_installed() -> InstalledSnapshot {
    let (native, foreign, combined_pacman) = query_pacman(query_packages);
    let flatpaks = if available("flatpak") {
        (|| {
            let mut apps = Vec::new();
            for scope in ["--user", "--system"] {
                let text = output(
                    "flatpak",
                    &["list", "--app", scope, "--columns=application,name,version"],
                )?;
                apps.extend(parse_flatpaks(&text, scope));
            }
            Ok(apps)
        })()
    } else {
        Ok(vec![])
    };
    InstalledSnapshot {
        sources: [native, foreign, flatpaks],
        combined_pacman,
    }
}
type PackageResult = Result<Vec<Installed>, String>;
fn query_pacman(
    query: impl Fn(&str, usize) -> PackageResult,
) -> (PackageResult, PackageResult, Option<String>) {
    let native = query("-Qen", 0);
    let foreign = query("-Qem", 1);
    if let Some(error) = native.as_ref().err().or(foreign.as_ref().err()) {
        // Native/foreign classification requires valid sync databases. The local
        // installed database remains useful even when a configured repo is broken.
        if let Ok(packages) = query("-Qe", 0) {
            let reason = error
                .lines()
                .next()
                .unwrap_or("Repository metadata unavailable");
            return (
                Ok(packages),
                Ok(vec![]),
                Some(format!(
                    "Pacman includes AUR/local packages because their source could not be separated. {reason}. Repair repository metadata, then refresh to restore the split."
                )),
            );
        }
    }
    (native, foreign, None)
}
// Largest-remainder rounding keeps the three displayed percentages at 100%.
fn percentages(counts: [usize; 3]) -> [u32; 3] {
    let total: usize = counts.iter().sum();
    if total == 0 {
        return [0; 3];
    }
    let mut result = counts.map(|n| (n * 100 / total) as u32);
    let mut order = [0, 1, 2];
    order.sort_by_key(|&i| std::cmp::Reverse(counts[i] * 100 % total));
    for &i in order
        .iter()
        .take((100 - result.iter().sum::<u32>()) as usize)
    {
        result[i] += 1;
    }
    result
}

#[derive(Clone)]
struct Ring {
    canvas: gtk::DrawingArea,
    count: gtk::Label,
    value: Rc<Cell<Option<(f64, u32)>>>,
    progress: Rc<Cell<f64>>,
    generation: Rc<Cell<u64>>,
    index: usize,
}
impl Ring {
    fn new(index: usize, cards: &gtk::Box) -> Self {
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        card.add_css_class("card");
        card.set_hexpand(true);
        let canvas = gtk::DrawingArea::builder()
            .content_width(112)
            .content_height(112)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        canvas.set_widget_name(&format!("installed-ring-{}", SOURCES[index].to_lowercase()));
        let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
        text.set_valign(gtk::Align::Center);
        text.append(&label(SOURCES[index], "heading"));
        let count = label("Loading…", "subtitle");
        text.append(&count);
        if index == 1 {
            let hint = label("Includes local packages", "subtitle");
            hint.set_wrap(true);
            hint.set_max_width_chars(16);
            text.append(&hint);
        }
        card.append(&canvas);
        card.append(&text);
        cards.append(&card);
        let ring = Self {
            canvas,
            count,
            value: Rc::new(Cell::new(None)),
            progress: Rc::new(Cell::new(1.0)),
            generation: Rc::new(Cell::new(0)),
            index,
        };
        let value = ring.value.clone();
        let progress = ring.progress.clone();
        ring.canvas.set_draw_func(move |area, cr, width, height| {
            let t = progress.get();
            let ease = 1.0 - (1.0 - t).powi(3);
            // Ease-out-back gives each circle a small pop, settling at full size.
            let pop = 1.0 + 2.70158 * (t - 1.0).powi(3) + 1.70158 * (t - 1.0).powi(2);
            let scale = 0.65 + 0.35 * pop;
            cr.translate(f64::from(width) / 2.0, f64::from(height) / 2.0);
            let fit = f64::from(width.min(height)) / 112.0;
            cr.scale(scale * fit, scale * fit);
            let foreground = area.color();
            let rgb = (
                f64::from(foreground.red()),
                f64::from(foreground.green()),
                f64::from(foreground.blue()),
            );
            cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 0.12);
            cr.set_line_width(8.0);
            cr.new_sub_path();
            cr.arc(0.0, 0.0, 43.0, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            let (r, g, b) = COLORS[index];
            if let Some((fraction, _)) = value.get()
                && fraction > 0.0
                && ease > 0.0
            {
                cr.set_source_rgba(r, g, b, ease);
                cr.set_line_cap(gtk::cairo::LineCap::Round);
                cr.new_sub_path();
                cr.arc(
                    0.0,
                    0.0,
                    43.0,
                    -std::f64::consts::FRAC_PI_2,
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * fraction * ease,
                );
                let _ = cr.stroke();
            }
            let text = value.get().map_or("N/A".into(), |(_, percent)| {
                format!("{}%", (f64::from(percent) * ease).round() as u32)
            });
            cr.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Bold,
            );
            cr.set_font_size(23.0);
            cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 0.3 + 0.7 * ease);
            if let Ok(extents) = cr.text_extents(&text) {
                cr.move_to(
                    -extents.width() / 2.0 - extents.x_bearing(),
                    -extents.height() / 2.0 - extents.y_bearing(),
                );
                let _ = cr.show_text(&text);
            }
        });
        let value = ring.value.clone();
        let progress = ring.progress.clone();
        let generation = ring.generation.clone();
        ring.canvas.connect_map(move |canvas| {
            Self::animate_canvas(canvas, &value, &progress, &generation, index);
        });
        ring
    }
    fn animate(&self) {
        Self::animate_canvas(
            &self.canvas,
            &self.value,
            &self.progress,
            &self.generation,
            self.index,
        );
    }
    fn animate_canvas(
        canvas: &gtk::DrawingArea,
        value: &Rc<Cell<Option<(f64, u32)>>>,
        progress: &Rc<Cell<f64>>,
        current_generation: &Rc<Cell<u64>>,
        index: usize,
    ) {
        let generation = current_generation.get() + 1;
        current_generation.set(generation);
        if value.get().is_none()
            || !canvas.is_mapped()
            || gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations())
        {
            progress.set(1.0);
            canvas.queue_draw();
            return;
        }
        progress.set(0.0);
        let progress = progress.clone();
        let current_generation = current_generation.clone();
        let start = Cell::new(None);
        let delay = index as f64 * 0.09;
        canvas.add_tick_callback(move |area, clock| {
            if current_generation.get() != generation || !area.is_mapped() {
                return glib::ControlFlow::Break;
            }
            let first = start.get().unwrap_or(clock.frame_time());
            start.set(Some(first));
            let elapsed = (clock.frame_time() - first) as f64 / 1_000_000.0;
            let t = ((elapsed - delay) / 0.7).clamp(0.0, 1.0);
            progress.set(t);
            area.queue_draw();
            if t >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
    fn update(&self, count: Option<usize>, total: Option<usize>, percent: u32) {
        self.count
            .set_text(&count.map_or("Unavailable".into(), |n| format!("{n} installed")));
        self.value.set(total.map(|total| {
            (
                if total == 0 {
                    0.0
                } else {
                    count.unwrap_or(0) as f64 / total as f64
                },
                percent,
            )
        }));
        let description = match (count, total) {
            (Some(n), Some(total)) => format!(
                "{}: {n} of {total} installed entries ({percent}%)",
                SOURCES[self.index]
            ),
            _ => "Percentages unavailable until every source can be read. Refresh to retry.".into(),
        };
        self.canvas.set_tooltip_text(Some(&description));
        self.canvas
            .update_property(&[gtk::accessible::Property::Label(&description)]);
        self.animate();
    }
}

pub fn page() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let refresh = gtk::Button::with_label("Refresh installed packages");
    refresh.set_halign(gtk::Align::Start);
    let cards = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    cards.set_homogeneous(true);
    let rings = (0..3).map(|i| Ring::new(i, &cards)).collect::<Vec<_>>();
    let caption = label("Your installed software · loading sources…", "subtitle");
    caption.set_wrap(true);
    let status = label(
        "Explicit Pacman packages and Flatpak apps. AUR includes foreign/local packages. User and system Flatpak installations count separately.",
        "subtitle",
    );
    status.set_wrap(true);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Filter by package, app or source"));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    page.append(&cards);
    page.append(&caption);
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
        caption.set_text("Refreshing installed software…");
        let button = button.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || { let _ = tx.send(query_installed()); });
        let list = list.clone();
        let status = status.clone();
        let caption = caption.clone();
        let rings = rings.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let snapshot = match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(_) => InstalledSnapshot { sources: std::array::from_fn(|_| Err("Package query stopped unexpectedly".into())), combined_pacman: None },
            };
            let InstalledSnapshot { sources: results, combined_pacman } = snapshot;
            button.set_sensitive(true);
            while let Some(child) = list.first_child() { list.remove(&child); }
            let complete = results.iter().all(Result::is_ok);
            let counts = std::array::from_fn(|i| results[i].as_ref().map_or(0, Vec::len));
            let total = counts.iter().sum::<usize>();
            let percents = percentages(counts);
            for (i, ring) in rings.iter().enumerate() {
                if i == 1 && combined_pacman.is_some() {
                    ring.update(None, None, 0);
                    ring.count.set_text("Included in Pacman");
                    let detail = "AUR/local packages are included in the Pacman total. Repository metadata could not separate their sources.";
                    ring.canvas.set_tooltip_text(Some(detail));
                    ring.canvas.update_property(&[gtk::accessible::Property::Label(detail)]);
                    continue;
                }
                ring.update(results[i].as_ref().ok().map(Vec::len), complete.then_some(total), percents[i]);
            }
            caption.set_text(&if complete {
                if total == 0 { "No installed entries found.".into() }
                else if combined_pacman.is_some() { format!("{total} installed entries · Pacman includes AUR/local") }
                else { format!("{total} installed entries · share by source") }
            } else { "Some sources could not be read · percentages unavailable".into() });
            let mut errors = results.iter().enumerate().filter_map(|(i, result)| result.as_ref().err().map(|e| format!("{}: {e}", SOURCES[i]))).collect::<Vec<_>>();
            if let Some(warning) = &combined_pacman { errors.insert(0, warning.clone()); }
            status.set_text(&if errors.is_empty() {
                "Explicit Pacman packages and Flatpak apps. AUR includes foreign/local packages. Refresh after removing software.".into()
            } else { errors.join(" · ") });
            for package in results.into_iter().filter_map(Result::ok).flatten() {
                let row = gtk::ListBoxRow::new();
                row.set_widget_name(&format!("{} {} {}", package.id, package.title, SOURCES[package.source]));
                let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                body.add_css_class("card");
                let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                info.set_hexpand(true);
                let title = label(&format!("{}  {}", package.title, package.version), "name");
                title.set_wrap(true);
                info.append(&title);
                let source = if let Some(scope) = package.scope {
                    format!("Flatpak · {} · {}", scope.trim_start_matches('-'), package.id)
                } else if package.source == 1 { "AUR / local".into() } else if combined_pacman.is_some() { "Pacman · includes AUR / local".into() } else { "Pacman · official repositories".into() };
                info.append(&label(&source, "subtitle"));
                body.append(&info);
                let remove = gtk::Button::with_label("Remove");
                remove.set_valign(gtk::Align::Center);
                let status = status.clone();
                let clean = gtk::CheckButton::with_label("Also remove unused dependencies and package configs (-Rns)");
                clean.set_tooltip_text(Some("Also removes unneeded dependencies and disables .pacsave backups for modified package config files. Review Pacman's transaction before confirming."));
                clean.set_visible(package.scope.is_none());
                info.append(&clean);
                remove.connect_clicked(move |_| match terminal(package.removal_args(clean.is_active())) {
                    Ok(()) => status.set_text("Review the removal in your terminal, then refresh this list."),
                    Err(e) => status.set_text(&e),
                });
                body.append(&remove);
                row.set_child(Some(&body));
                list.append(&row);
            }
            glib::ControlFlow::Break
        });
    });
    page.connect_map(move |_| {
        if refresh.is_sensitive() {
            refresh.emit_clicked();
        }
    });
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broken_repository_keeps_local_installed_packages_without_guessing_sources() {
        let calls = std::cell::RefCell::new(Vec::new());
        let (native, foreign, warning) = query_pacman(|flag, source| {
            calls.borrow_mut().push(flag.to_owned());
            if flag == "-Qe" {
                Ok(parse_packages("firefox 1\nlocal-tool 2\n", source))
            } else {
                Err("error: example: missing required signature\nrepository is invalid".into())
            }
        });
        let packages = native.unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[1].id, "local-tool");
        assert!(foreign.unwrap().is_empty());
        assert!(warning.as_ref().unwrap().contains("includes AUR/local"));
        assert!(warning.unwrap().contains("missing required signature"));
        assert_eq!(*calls.borrow(), ["-Qen", "-Qem", "-Qe"]);
    }
    #[test]
    fn healthy_repository_preserves_split_and_local_database_failure_stays_visible() {
        let (native, foreign, warning) = query_pacman(|flag, source| {
            assert_ne!(flag, "-Qe");
            Ok(parse_packages(
                if flag == "-Qen" {
                    "firefox 1"
                } else {
                    "local-tool 2"
                },
                source,
            ))
        });
        assert_eq!(native.unwrap()[0].source, 0);
        assert_eq!(foreign.unwrap()[0].source, 1);
        assert!(warning.is_none());
        let (native, foreign, warning) = query_pacman(|_, _| Err("database unreadable".into()));
        assert!(native.is_err() && foreign.is_err() && warning.is_none());
    }
    #[test]
    #[ignore = "requires a graphical session"]
    fn installed_rings_animate_settle_and_respect_reduced_motion() {
        gtk::init().unwrap();
        let settings = gtk::Settings::default().unwrap();
        let original = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(true);
        let cards = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let rings: Vec<_> = (0..3).map(|i| Ring::new(i, &cards)).collect();
        let window = gtk::Window::builder().child(&cards).build();
        window.present();
        let pump = |milliseconds| {
            let end = std::time::Instant::now() + Duration::from_millis(milliseconds);
            while std::time::Instant::now() < end {
                while glib::MainContext::default().iteration(false) {}
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        pump(100);
        for (i, ring) in rings.iter().enumerate() {
            ring.update(Some([7, 2, 1][i]), Some(10), [70, 20, 10][i]);
            assert_eq!(ring.progress.get(), 0.0);
        }
        pump(150);
        assert!(rings[0].progress.get() > 0.0 && rings[0].progress.get() < 1.0);
        assert!(rings[0].progress.get() > rings[2].progress.get());
        let previous_generation = rings[0].generation.get();
        rings[0].update(Some(7), Some(10), 70);
        assert!(rings[0].generation.get() > previous_generation);
        pump(1100);
        assert!(rings.iter().all(|r| r.progress.get() == 1.0));
        settings.set_gtk_enable_animations(false);
        rings[0].update(Some(7), Some(10), 70);
        assert_eq!(rings[0].progress.get(), 1.0);
        rings[1].update(None, None, 0);
        assert_eq!(rings[1].value.get(), None);
        assert_eq!(rings[1].count.text(), "Unavailable");
        window.close();
        pump(50);
        settings.set_gtk_enable_animations(original);
    }
    #[test]
    fn shares_round_to_one_hundred_and_handle_empty_sources() {
        assert_eq!(percentages([0, 0, 0]), [0, 0, 0]);
        assert_eq!(percentages([1, 1, 1]), [34, 33, 33]);
        assert_eq!(percentages([7, 2, 1]), [70, 20, 10]);
        assert_eq!(percentages([0, 4, 0]), [0, 100, 0]);
    }
    #[test]
    fn package_sources_and_flatpak_scopes_keep_safe_removal_commands() {
        let native = parse_packages("firefox 1.0\n--invalid 2.0\n", 0);
        let foreign = parse_packages("local-tool 2.0\n", 1);
        let user = parse_flatpaks("org.mozilla.firefox\tFirefox\t140\n", "--user");
        let system = parse_flatpaks("org.mozilla.firefox\tFirefox\t140\n", "--system");
        assert_eq!(native.len(), 1);
        assert_eq!(foreign[0].source, 1);
        assert_eq!(
            native[0].removal_args(false),
            ["sudo", "pacman", "-R", "--", "firefox"]
        );
        assert_eq!(
            user[0].removal_args(false),
            [
                "flatpak",
                "uninstall",
                "--user",
                "--",
                "org.mozilla.firefox"
            ]
        );
        assert_eq!(
            native[0].removal_args(true),
            ["sudo", "pacman", "-Rns", "--", "firefox"]
        );
        assert_eq!(system[0].removal_args(true)[2], "--system");
        assert!(parse_flatpaks("--bad\tBad\t1", "--user").is_empty());
    }
}

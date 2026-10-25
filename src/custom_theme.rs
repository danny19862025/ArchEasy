use gtk::prelude::*;
use std::{fs, io, path::Path, rc::Rc};

pub const FIELDS: [&str; 6] = [
    "Background",
    "Surfaces",
    "Text",
    "Secondary text",
    "Accent",
    "Borders",
];
#[derive(Clone, Debug, PartialEq)]
pub struct Palette(pub [String; 6]);
impl Default for Palette {
    fn default() -> Self {
        Self(
            [
                "#101722", "#1b2738", "#e7eef8", "#aebdd0", "#55cbb6", "#537586",
            ]
            .map(str::to_owned),
        )
    }
}
pub fn valid_hex(text: &str) -> bool {
    text.len() == 7
        && text.starts_with('#')
        && text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
impl Palette {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<[String; 6]>(&s).ok())
            .filter(|colors| colors.iter().all(|s| valid_hex(s)))
            .map(Self)
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if !self.0.iter().all(|s| valid_hex(s)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Use #RRGGBB colours.",
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_string_pretty(&self.0)?)?;
        fs::rename(temporary, path)
    }
    fn luminance(color: &str) -> f64 {
        let channels: Vec<_> = [1, 3, 5]
            .map(|i| {
                let c = f64::from(u8::from_str_radix(&color[i..i + 2], 16).unwrap()) / 255.0;
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            })
            .into_iter()
            .collect();
        channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
    }
    pub fn is_light(&self) -> bool {
        Self::luminance(&self.0[0]) > 0.179
    }
    pub fn tokens(&self) -> [String; 12] {
        let [bg, surface, text, muted, accent, border] = &self.0;
        let ink = if Self::luminance(accent) > 0.179 {
            "#000000"
        } else {
            "#ffffff"
        };
        [
            bg.clone(),
            surface.clone(),
            text.clone(),
            muted.clone(),
            accent.clone(),
            surface.clone(),
            border.clone(),
            accent.clone(),
            ink.into(),
            accent.clone(),
            ink.into(),
            border.clone(),
        ]
    }
    pub fn arch_css(&self) -> String {
        let [bg, surface, text, muted, accent, _, _, _, ink, _, _, border] = self.tokens();
        format!(
            r#"
window.archeasy.custom {{ background: {bg}; color: {text}; }}
.archeasy.custom label {{ color: {text}; }}
.archeasy.custom .subtitle {{ color: {muted}; opacity: 1; }}
.archeasy.custom .card {{ background: {surface}; border-color: {border}; }}
.archeasy.custom .badge, .archeasy.custom link {{ color: {accent}; }}
.archeasy.custom button.link label {{ color: {accent}; }}
.archeasy.custom button, .archeasy.custom dropdown, .archeasy.custom entry {{ background: {surface}; color: {text}; border-color: {border}; }}
.archeasy.custom button label, .archeasy.custom dropdown label {{ color: inherit; }}
.archeasy.custom entry text {{ color: {text}; }}
.archeasy.custom button:checked, .archeasy.custom button:hover {{ border-color: {accent}; }}
.archeasy.custom entry:focus-within {{ outline-color: {accent}; }}
.archeasy.custom .suggested-action {{ background: {accent}; color: {ink}; }}
.archeasy.custom .suggested-action label {{ color: {ink}; }}
.archeasy.custom .opening-title {{ color: #f0f7ff; }}
.archeasy.custom .opening-subtitle {{ color: #9cc8e3; }}
"#
        )
    }
}

/// Changes are staged in the editor; only Apply saves and selects Custom.
#[allow(deprecated)]
pub fn editor(
    window: &gtk::ApplicationWindow,
    title: &str,
    path: std::path::PathBuf,
    apply: impl Fn(&Palette) + 'static,
) {
    let dialog = gtk::Dialog::builder()
        .title(title)
        .transient_for(window)
        .modal(true)
        .default_width(440)
        .build();
    dialog.set_widget_name("custom-theme-editor");
    dialog.add_button("Reset colours", gtk::ResponseType::Other(1));
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Apply theme", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let content = dialog.content_area();
    content.set_spacing(14);
    content.set_margin_top(20);
    content.set_margin_bottom(20);
    content.set_margin_start(20);
    content.set_margin_end(20);
    let hint = gtk::Label::new(Some(
        "Choose a colour or enter its #RRGGBB value.\nApply saves your colours as the Custom theme.",
    ));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    content.append(&hint);
    let grid = gtk::Grid::builder()
        .row_spacing(12)
        .column_spacing(16)
        .build();
    let palette = Palette::load(&path);
    let mut entries = Vec::new();
    for (i, name) in FIELDS.iter().enumerate() {
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        let entry = gtk::Entry::builder().max_length(7).width_chars(9).build();
        entry.set_widget_name(&format!("custom-colour-{i}"));
        entry.set_text(&palette.0[i]);
        let picker = gtk::ColorDialogButton::new(Some(
            gtk::ColorDialog::builder()
                .title(*name)
                .with_alpha(false)
                .build(),
        ));
        picker.set_rgba(&gtk::gdk::RGBA::parse(&palette.0[i]).unwrap());
        picker.set_tooltip_text(Some(&format!("Choose {name}")));
        let weak_entry = entry.downgrade();
        picker.connect_rgba_notify(move |picker| {
            if let Some(entry) = weak_entry.upgrade() {
                let c = picker.rgba();
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    (c.red() * 255.0).round() as u8,
                    (c.green() * 255.0).round() as u8,
                    (c.blue() * 255.0).round() as u8
                );
                if entry.text().to_lowercase() != hex {
                    entry.set_text(&hex);
                }
            }
        });
        let weak_picker = picker.downgrade();
        entry.connect_changed(move |entry| {
            let value = entry.text();
            if valid_hex(&value) {
                entry.remove_css_class("error");
                if let Some(picker) = weak_picker.upgrade() {
                    let rgba = gtk::gdk::RGBA::parse(&value).unwrap();
                    if picker.rgba() != rgba {
                        picker.set_rgba(&rgba);
                    }
                }
            } else {
                entry.add_css_class("error");
            }
        });
        grid.attach(&label, 0, i as i32, 1, 1);
        grid.attach(&picker, 1, i as i32, 1, 1);
        grid.attach(&entry, 2, i as i32, 1, 1);
        entries.push(entry);
    }
    content.append(&grid);
    let error = gtk::Label::new(None);
    error.set_wrap(true);
    error.set_visible(false);
    error.add_css_class("error");
    content.append(&error);
    let entries = Rc::new(entries);
    dialog.connect_response(move |dialog, response| match response {
        gtk::ResponseType::Other(1) => {
            for (entry, color) in entries.iter().zip(Palette::default().0) {
                entry.set_text(&color);
            }
            error.set_visible(false);
        }
        gtk::ResponseType::Accept => {
            let palette = Palette(std::array::from_fn(|i| {
                entries[i].text().to_ascii_lowercase()
            }));
            if let Err(e) = palette.save(&path) {
                error.set_text(&e.to_string());
                error.set_visible(true);
                return;
            }
            apply(&palette);
            dialog.close();
        }
        _ => dialog.close(),
    });
    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_palette_validates_persists_and_uses_readable_accent_ink() {
        let dir = std::env::temp_dir().join(format!("archeasy-palette-{}", std::process::id()));
        let path = dir.join("custom.json");
        let mut palette = Palette::default();
        palette.0[4] = "#ffffff".into();
        palette.save(&path).unwrap();
        assert_eq!(Palette::load(&path), palette);
        assert_eq!(palette.tokens()[8], "#000000");
        palette.0[4] = "#000000".into();
        assert_eq!(palette.tokens()[8], "#ffffff");
        for value in ["red", "#123", "#123456;", "#zzzzzz", "#é1234"] {
            assert!(!valid_hex(value));
        }
        palette.0[0] = "bad css".into();
        assert!(palette.save(&path).is_err());
        fs::write(&path, "invalid").unwrap();
        assert_eq!(Palette::load(&path), Palette::default());
        fs::remove_dir_all(dir).unwrap();
    }
}

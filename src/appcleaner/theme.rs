use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Theme {
    Cyan,
    Dark,
    Light,
    Custom,
}
impl Theme {
    pub fn from_id(id: &str) -> Self {
        match id.trim() {
            "dark" => Self::Dark,
            "light" => Self::Light,
            "custom" => Self::Custom,
            _ => Self::Cyan,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Cyan => "cyan",
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Custom => "custom",
        }
    }
    pub fn load(path: &Path) -> Self {
        Self::from_id(&fs::read_to_string(path).unwrap_or_default())
    }
    pub fn save(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, self.id())?;
        fs::rename(temporary, path)
    }
    fn colors(self) -> [&'static str; 12] {
        // Background, sidebar, text, muted, accent, card, hover, selection,
        // selected text, button, button text, control border.
        match self {
            Self::Custom => Self::Cyan.colors(),
            Self::Cyan => [
                "#101b22", "#152630", "#f1fbff", "#a9c6d1", "#58d9f5", "#1b303c", "#223d4c",
                "#12566c", "#ffffff", "#37ccec", "#082632", "#537586",
            ],
            Self::Dark => [
                "#17191f", "#20232b", "#f4f7fb", "#b4bfce", "#a8caff", "#252b35", "#303744",
                "#304a72", "#ffffff", "#a8caff", "#101d34", "#647287",
            ],
            Self::Light => [
                "#f3f7fa", "#e3edf3", "#162b38", "#405c6c", "#005e78", "#ffffff", "#cbdfe9",
                "#005e78", "#ffffff", "#006b86", "#ffffff", "#8aa4b3",
            ],
        }
    }
    pub fn custom_path() -> std::path::PathBuf {
        gtk::glib::user_config_dir().join("archeasy/appcleaner-custom-theme.json")
    }
    pub fn css(self) -> String {
        let keys = [
            "BG",
            "SIDE",
            "TEXT",
            "MUTED",
            "ACCENT",
            "CARD",
            "HOVER",
            "SELECT",
            "SELECTTEXT",
            "BUTTON",
            "BUTTONTEXT",
            "BORDER",
        ];
        let mut css = include_str!("theme.css").to_string();
        let colors = if self == Self::Custom {
            crate::custom_theme::Palette::load(&Self::custom_path()).tokens()
        } else {
            self.colors().map(str::to_owned)
        };
        for (key, color) in keys.into_iter().zip(colors) {
            css = css.replace(&format!("@{key}@"), &color);
        }
        css
    }
    pub fn apply(self, provider: &gtk::CssProvider) {
        provider.load_from_data(&self.css());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(color: &str) -> f64 {
        let channels: Vec<f64> = [1, 3, 5]
            .into_iter()
            .map(|i| u8::from_str_radix(&color[i..i + 2], 16).unwrap() as f64 / 255.0)
            .map(|c| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            })
            .collect();
        channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
    }
    fn contrast(a: &str, b: &str) -> f64 {
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn themes_keep_sidebar_and_controls_readable() {
        for theme in [Theme::Cyan, Theme::Dark, Theme::Light] {
            let c = theme.colors();
            assert!(contrast(c[2], c[1]) >= 7.0, "{theme:?}: sidebar text");
            assert!(contrast(c[3], c[1]) >= 4.5, "{theme:?}: secondary text");
            assert!(contrast(c[8], c[7]) >= 4.5, "{theme:?}: selected text");
            assert!(contrast(c[10], c[9]) >= 4.5, "{theme:?}: button text");
            assert!(!theme.css().contains('@'));
        }
    }
    #[test]
    fn choice_persists_and_invalid_values_fall_back() {
        let dir = std::env::temp_dir().join(format!("appcleaner-theme-{}", std::process::id()));
        let path = dir.join("theme");
        assert_eq!(Theme::load(&path), Theme::Cyan);
        Theme::Light.save(&path).unwrap();
        assert_eq!(Theme::load(&path), Theme::Light);
        fs::write(&path, "invalid").unwrap();
        assert_eq!(Theme::load(&path), Theme::Cyan);
        fs::remove_dir_all(dir).unwrap();
    }
}

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::refresh;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub bg: Color32,
    pub rail: Color32,
    pub panel: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub series_a: Color32,
    pub series_b: Color32,
    pub storage: Color32,
    pub cpu: Color32,
    pub nav: Color32,
    pub success: Color32,
    pub warning: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub static THEMES: [Theme; 5] = [
    Theme {
        id: "perfm-original",
        name: "Perfm Original",
        description: "The familiar charcoal dashboard",
        bg: rgb(0x08090B),
        rail: rgb(0x0C0D10),
        panel: rgb(0x111215),
        border: rgb(0x313339),
        text: rgb(0xF2F3F5),
        muted: rgb(0x9699A0),
        series_a: rgb(0x69C6E0),
        series_b: rgb(0xAA8FF7),
        storage: rgb(0x42D09E),
        cpu: rgb(0xEEBC76),
        nav: rgb(0xAA8FF7),
        success: rgb(0x42D09E),
        warning: rgb(0xEEBC76),
    },
    Theme {
        id: "pastel-soft",
        name: "Pastel & Soft",
        description: "Lavender, sky, and mint on midnight blue",
        bg: rgb(0x111827),
        rail: rgb(0x111827),
        panel: rgb(0x1D2C46),
        border: rgb(0x3A5278),
        text: rgb(0xF2F3F5),
        muted: rgb(0xB8C7DF),
        series_a: rgb(0x71D5FF),
        series_b: rgb(0xD88CF2),
        storage: rgb(0xD88CF2),
        cpu: rgb(0xD88CF2),
        nav: rgb(0xD88CF2),
        success: rgb(0x54E6C1),
        warning: rgb(0x71D5FF),
    },
    Theme {
        id: "neutral-earthy",
        name: "Neutral & Earthy",
        description: "Sage, teal, and ochre on deep green",
        bg: rgb(0x151A16),
        rail: rgb(0x151A16),
        panel: rgb(0x242C25),
        border: rgb(0x465448),
        text: rgb(0xF2F3F0),
        muted: rgb(0xC0C9B8),
        series_a: rgb(0x6DD1BE),
        series_b: rgb(0xF2BB65),
        storage: rgb(0xB5D28A),
        cpu: rgb(0xF2BB65),
        nav: rgb(0xB5D28A),
        success: rgb(0x6DD1BE),
        warning: rgb(0xF2BB65),
    },
    Theme {
        id: "bold-high-contrast",
        name: "Bold & High Contrast",
        description: "Fuchsia, lime, and cyan on deep violet",
        bg: rgb(0x140F1B),
        rail: rgb(0x140F1B),
        panel: rgb(0x241A2C),
        border: rgb(0x493452),
        text: rgb(0xF7F3F9),
        muted: rgb(0xC6B8CE),
        series_a: rgb(0x17DAE5),
        series_b: rgb(0xF02BCC),
        storage: rgb(0xF02BCC),
        cpu: rgb(0xF02BCC),
        nav: rgb(0xF02BCC),
        success: rgb(0x85F04B),
        warning: rgb(0x85F04B),
    },
    Theme {
        id: "minimalist-modern",
        name: "Minimalist & Modern",
        description: "Blue, cyan, and green on charcoal",
        bg: rgb(0x111417),
        rail: rgb(0x111417),
        panel: rgb(0x1B2026),
        border: rgb(0x3A4653),
        text: rgb(0xF2F4F6),
        muted: rgb(0xAFBAC5),
        series_a: rgb(0x10CDE3),
        series_b: rgb(0x168BFF),
        storage: rgb(0x168BFF),
        cpu: rgb(0x168BFF),
        nav: rgb(0x168BFF),
        success: rgb(0x5DE47A),
        warning: rgb(0x10CDE3),
    },
];

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn active() -> &'static Theme {
    &THEMES[ACTIVE.load(Ordering::Relaxed).min(THEMES.len() - 1)]
}

pub fn select(index: usize) {
    ACTIVE.store(index.min(THEMES.len() - 1), Ordering::Relaxed);
}

#[derive(Deserialize, Serialize)]
struct Settings {
    theme: String,
    #[serde(default = "default_refresh_seconds")]
    refresh_seconds: u64,
}

fn default_refresh_seconds() -> u64 {
    1
}

fn read_from(path: &Path) -> Settings {
    fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice::<Settings>(&content).ok())
        .unwrap_or_else(|| Settings {
            theme: THEMES[0].id.to_owned(),
            refresh_seconds: 1,
        })
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Perfm").join("settings.json"))
}

fn load_from(path: &Path) -> usize {
    THEMES
        .iter()
        .position(|theme| theme.id == read_from(path).theme)
        .unwrap_or(0)
}

fn load_refresh_from(path: &Path) -> u64 {
    refresh::normalize(read_from(path).refresh_seconds)
}

fn write_to(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(settings)?;
    fs::write(path, content)
}

fn save_to(path: &Path, index: usize) -> io::Result<()> {
    let mut settings = read_from(path);
    settings.theme = THEMES[index.min(THEMES.len() - 1)].id.to_owned();
    write_to(path, &settings)
}

fn save_refresh_to(path: &Path, seconds: u64) -> io::Result<()> {
    let mut settings = read_from(path);
    settings.refresh_seconds = refresh::normalize(seconds);
    write_to(path, &settings)
}

pub fn load() -> usize {
    settings_path().map_or(0, |path| load_from(&path))
}

pub fn save(index: usize) -> io::Result<()> {
    let path = settings_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))?;
    save_to(&path, index)
}

pub fn load_refresh() -> u64 {
    settings_path().map_or(1, |path| load_refresh_from(&path))
}

pub fn save_refresh(seconds: u64) -> io::Result<()> {
    let path = settings_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))?;
    save_refresh_to(&path, seconds)
}

#[cfg(test)]
mod tests {
    use super::{THEMES, load_from, load_refresh_from, save_refresh_to, save_to};

    #[test]
    fn palette_selection_survives_restart_and_invalid_settings_fall_back() {
        let path =
            std::env::temp_dir().join(format!("perfm-theme-test-{}.json", std::process::id()));
        assert_eq!(load_from(&path), 0);
        save_to(&path, 4).unwrap();
        assert_eq!(load_from(&path), 4);
        std::fs::write(&path, br#"{"theme":"purple-teal"}"#).unwrap();
        assert_eq!(load_from(&path), 0);
        std::fs::write(&path, br#"{"theme":"not-a-theme"}"#).unwrap();
        assert_eq!(load_from(&path), 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn all_supplied_palettes_have_unique_stable_ids() {
        assert_eq!(THEMES.len(), 5);
        assert_eq!(
            THEMES.iter().map(|theme| theme.id).collect::<Vec<_>>(),
            [
                "perfm-original",
                "pastel-soft",
                "neutral-earthy",
                "bold-high-contrast",
                "minimalist-modern",
            ]
        );
        let mut ids = std::collections::HashSet::new();
        for theme in THEMES {
            assert!(ids.insert(theme.id));
        }
    }

    #[test]
    fn supplied_theme_colors_match_requested_hex_values() {
        use super::rgb;

        let expected = [
            (0x111827, 0x1D2C46, 0x3A5278, 0xD88CF2, 0x71D5FF, 0x54E6C1),
            (0x151A16, 0x242C25, 0x465448, 0xB5D28A, 0x6DD1BE, 0xF2BB65),
            (0x140F1B, 0x241A2C, 0x493452, 0xF02BCC, 0x85F04B, 0x17DAE5),
            (0x111417, 0x1B2026, 0x3A4653, 0x168BFF, 0x10CDE3, 0x5DE47A),
        ];
        for (theme, (bg, surface, border, main, support_a, support_b)) in
            THEMES[1..].iter().zip(expected)
        {
            assert_eq!(theme.bg, rgb(bg), "{} background", theme.id);
            assert_eq!(theme.panel, rgb(surface), "{} surface", theme.id);
            assert_eq!(theme.border, rgb(border), "{} border", theme.id);
            assert_eq!(theme.nav, rgb(main), "{} main accent", theme.id);
            let accent_roles = [
                theme.series_a,
                theme.series_b,
                theme.storage,
                theme.cpu,
                theme.nav,
                theme.success,
                theme.warning,
            ];
            assert!(
                accent_roles.contains(&rgb(support_a)),
                "{} supporting accent A",
                theme.id
            );
            assert!(
                accent_roles.contains(&rgb(support_b)),
                "{} supporting accent B",
                theme.id
            );
            assert!(
                accent_roles
                    .iter()
                    .all(|color| [rgb(main), rgb(support_a), rgb(support_b)].contains(color)),
                "{} contains an accent outside the supplied palette",
                theme.id
            );
        }
    }

    #[test]
    fn changing_theme_keeps_existing_refresh_interval() {
        let path = std::env::temp_dir().join(format!(
            "perfm-refresh-preservation-test-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, br#"{"theme":"perfm-original","refresh_seconds":5}"#).unwrap();
        save_to(&path, 2).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["refresh_seconds"], 5);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn refresh_interval_survives_restart_and_keeps_theme() {
        let path = std::env::temp_dir().join(format!(
            "perfm-refresh-roundtrip-test-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, br#"{"theme":"pastel-soft"}"#).unwrap();
        assert_eq!(load_refresh_from(&path), 1);
        save_refresh_to(&path, 300).unwrap();
        assert_eq!(load_refresh_from(&path), 300);
        assert_eq!(THEMES[load_from(&path)].id, "pastel-soft");
        save_refresh_to(&path, 999).unwrap();
        assert_eq!(load_refresh_from(&path), 1);
        let _ = std::fs::remove_file(path);
    }
}

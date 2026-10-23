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

#[allow(clippy::too_many_arguments)]
const fn palette(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    neutral_900: u32,
    neutral_800: u32,
    neutral_700: u32,
    neutral_300: u32,
    neutral_50: u32,
    primary: u32,
    secondary: u32,
    storage: u32,
) -> Theme {
    Theme {
        id,
        name,
        description,
        bg: rgb(neutral_900),
        rail: rgb(neutral_900),
        panel: rgb(neutral_800),
        border: rgb(neutral_700),
        text: rgb(neutral_50),
        muted: rgb(neutral_300),
        series_a: rgb(primary),
        series_b: rgb(secondary),
        storage: rgb(storage),
        cpu: rgb(0xF7C948),
        nav: rgb(primary),
        success: rgb(0x65D6AD),
        warning: rgb(0xF7C948),
    }
}

pub static THEMES: [Theme; 13] = [
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
    palette(
        "cyan-blue-grey",
        "Cyan + Blue Grey",
        "Cool cyan on deep slate",
        0x102A43,
        0x243B53,
        0x334E68,
        0x9FB3C8,
        0xF0F4F8,
        0x54D1DB,
        0x87EAF2,
        0x38BEC9,
    ),
    palette(
        "blue-yellow",
        "Blue + Yellow",
        "Clear blue with vivid gold",
        0x102A43,
        0x243B53,
        0x334E68,
        0x9FB3C8,
        0xF0F4F8,
        0x62B0E8,
        0xF7C948,
        0x4098D7,
    ),
    palette(
        "purple-teal",
        "Purple + Teal",
        "Violet and fresh teal",
        0x102A43,
        0x243B53,
        0x334E68,
        0x9FB3C8,
        0xF0F4F8,
        0x8888FC,
        0x65D6AD,
        0x3EBD93,
    ),
    palette(
        "teal-blue-grey",
        "Teal + Blue Grey",
        "Quiet teal on deep slate",
        0x102A43,
        0x243B53,
        0x334E68,
        0x9FB3C8,
        0xF0F4F8,
        0x65D6AD,
        0x8EEDC7,
        0x3EBD93,
    ),
    palette(
        "blue-grey",
        "Blue Grey",
        "Low-key monochrome blue",
        0x102A43,
        0x243B53,
        0x334E68,
        0x9FB3C8,
        0xF0F4F8,
        0xBCCCDC,
        0x9FB3C8,
        0x829AB1,
    ),
    palette(
        "red-yellow-warm",
        "Red + Yellow",
        "Warm graphite and vivid red",
        0x27241D,
        0x423D33,
        0x504A40,
        0xB8B2A7,
        0xFAF9F7,
        0xE66A6A,
        0xF7C948,
        0xD64545,
    ),
    palette(
        "cyan-warm-grey",
        "Cyan + Warm Grey",
        "Cool accents on warm graphite",
        0x27241D,
        0x423D33,
        0x504A40,
        0xB8B2A7,
        0xFAF9F7,
        0x54D1DB,
        0x87EAF2,
        0x38BEC9,
    ),
    palette(
        "blue-vivid",
        "Blue Vivid",
        "Electric blue with red details",
        0x1F2933,
        0x323F4B,
        0x3E4C59,
        0x9AA5B1,
        0xF5F7FA,
        0x47A3F3,
        0xF86A6A,
        0x2186EB,
    ),
    palette(
        "light-blue-vivid",
        "Light Blue Vivid",
        "Bright sky blue on cool grey",
        0x1F2933,
        0x323F4B,
        0x3E4C59,
        0x9AA5B1,
        0xF5F7FA,
        0x5ED0FA,
        0x81DEFD,
        0x40C3F7,
    ),
    palette(
        "indigo-cool-grey",
        "Indigo + Cool Grey",
        "Soft indigo on cool grey",
        0x1F2933,
        0x323F4B,
        0x3E4C59,
        0x9AA5B1,
        0xF5F7FA,
        0x7B93DB,
        0x98AEEB,
        0x647ACB,
    ),
    palette(
        "pink-vivid",
        "Pink Vivid",
        "Bright pink on cool grey",
        0x1F2933,
        0x323F4B,
        0x3E4C59,
        0x9AA5B1,
        0xF5F7FA,
        0xF364A2,
        0xFF8CBA,
        0xE8368F,
    ),
    palette(
        "green-pure-grey",
        "Green + Pure Grey",
        "Leaf green on neutral grey",
        0x222222,
        0x3B3B3B,
        0x515151,
        0xB1B1B1,
        0xF7F7F7,
        0x7BC47F,
        0xA3D9A5,
        0x57AE5B,
    ),
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
        save_to(&path, 11).unwrap();
        assert_eq!(load_from(&path), 11);
        std::fs::write(&path, br#"{"theme":"not-a-theme"}"#).unwrap();
        assert_eq!(load_from(&path), 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn all_supplied_palettes_have_unique_stable_ids() {
        assert_eq!(THEMES.len(), 13);
        let mut ids = std::collections::HashSet::new();
        for theme in THEMES {
            assert!(ids.insert(theme.id));
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
        std::fs::write(&path, br#"{"theme":"purple-teal"}"#).unwrap();
        assert_eq!(load_refresh_from(&path), 1);
        save_refresh_to(&path, 300).unwrap();
        assert_eq!(load_refresh_from(&path), 300);
        assert_eq!(THEMES[load_from(&path)].id, "purple-teal");
        save_refresh_to(&path, 999).unwrap();
        assert_eq!(load_refresh_from(&path), 1);
        let _ = std::fs::remove_file(path);
    }
}

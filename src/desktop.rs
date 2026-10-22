//! WinDoesNot desktop integration: accent colour, light/dark mode and UI
//! language follow the shared desktop preferences
//! (`~/.config/quickshell/default/settings.txt`) when they exist.
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::OnceLock;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

pub const DEFAULT_ACCENT: &str = "#fe6f2d";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    De,
    En,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopPrefs {
    /// Validated `#rrggbb` accent colour.
    pub accent: Option<String>,
    pub dark: Option<bool>,
    pub language: Option<Language>,
}

pub fn settings_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
    config.join("quickshell/default/settings.txt")
}

pub fn read_prefs() -> DesktopPrefs {
    std::fs::read_to_string(settings_path())
        .map(|text| parse_prefs(&text))
        .unwrap_or_default()
}

fn parse_prefs(text: &str) -> DesktopPrefs {
    let mut prefs = DesktopPrefs::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "accentColor" if is_hex_colour(value) => prefs.accent = Some(value.to_ascii_lowercase()),
            "darkMode" => prefs.dark = Some(value != "false"),
            "uiLanguage" => prefs.language = language_from(value),
            _ => {}
        }
    }
    prefs
}

fn is_hex_colour(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn language_from(value: &str) -> Option<Language> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value == "c" || value == "posix" {
        None
    } else if value.starts_with("de") {
        Some(Language::De)
    } else {
        Some(Language::En)
    }
}

fn system_language() -> Language {
    // LANGUAGE may list several languages ("de:en"); the first one wins.
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if let Some(language) = language_from(value.split(':').next().unwrap_or("")) {
                return language;
            }
        }
    }
    Language::En
}

/// Interface language for this run: the WinDoesNot setting, else the system locale.
/// Labels are built once, so a language change applies after restarting the store.
pub fn language() -> Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    *LANGUAGE.get_or_init(|| read_prefs().language.unwrap_or_else(system_language))
}

pub fn accent() -> String {
    read_prefs().accent.unwrap_or_else(|| DEFAULT_ACCENT.to_string())
}

pub fn accent_rgb(accent: &str) -> (u8, u8, u8) {
    let channel = |i: usize| u8::from_str_radix(&accent[i..i + 2], 16).unwrap_or(0);
    (channel(1), channel(3), channel(5))
}

/// Readable text colour on top of the accent (WCAG relative luminance).
pub fn accent_foreground(accent: &str) -> &'static str {
    let (r, g, b) = accent_rgb(accent);
    let linear = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    if luminance > 0.45 { "#17120e" } else { "#ffffff" }
}

thread_local! {
    static MONITOR: RefCell<Option<gio::FileMonitor>> = RefCell::new(None);
    static PENDING: RefCell<Option<glib::SourceId>> = RefCell::new(None);
}

/// Calls `on_change` (debounced) whenever the desktop preferences file changes.
/// The directory is watched because the desktop may replace the file on save.
pub fn watch<F: Fn() + 'static>(on_change: F) {
    let path = settings_path();
    let Some(directory) = path.parent() else {
        return;
    };
    let Ok(monitor) = gio::File::for_path(directory)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
    else {
        return;
    };
    let on_change = std::rc::Rc::new(on_change);
    monitor.connect_changed(move |_, file, other, _| {
        let touches_settings = [Some(file), other]
            .into_iter()
            .flatten()
            .any(|candidate| candidate.basename().as_deref() == Some(std::path::Path::new("settings.txt")));
        if !touches_settings {
            return;
        }
        let on_change = on_change.clone();
        PENDING.with(|pending| {
            if let Some(id) = pending.borrow_mut().take() {
                id.remove();
            }
            let id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                PENDING.with(|pending| pending.borrow_mut().take());
                on_change();
            });
            *pending.borrow_mut() = Some(id);
        });
    });
    MONITOR.with(|slot| *slot.borrow_mut() = Some(monitor));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_preferences() {
        let prefs = parse_prefs("accentColor=#3A86FF\ndarkMode=false\nuiLanguage=de\nother=x\n");
        assert_eq!(prefs.accent.as_deref(), Some("#3a86ff"));
        assert_eq!(prefs.dark, Some(false));
        assert_eq!(prefs.language, Some(Language::De));
        assert_eq!(parse_prefs("accentColor=red;}\n").accent, None);
        assert_eq!(parse_prefs("uiLanguage=en\n").language, Some(Language::En));
    }

    #[test]
    fn foreground_stays_readable() {
        assert_eq!(accent_foreground("#fe6f2d"), "#ffffff");
        assert_eq!(accent_foreground("#ffe14d"), "#17120e");
        assert_eq!(accent_rgb("#fe6f2d"), (254, 111, 45));
    }
}

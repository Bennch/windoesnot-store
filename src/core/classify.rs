//! Sorts pending updates for the desktop settings into what people recognise:
//! WinDoesNot itself, programs (named and drawn like in the start menu, from the
//! package's desktop entry) and system components in a few plain groups. Also
//! says whether an update needs a restart or a new login.
use std::fs;
use std::path::Path;

use serde::Serialize;

const LOCAL_DB: &str = "/var/lib/pacman/local";
const APPLICATIONS: &str = "usr/share/applications/";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Windoesnot,
    App,
    #[default]
    System,
}

/// Plain-language groups for system components; the settings page names them.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Kernel,
    Drivers,
    Desktop,
    Sound,
    Network,
    #[default]
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Restart {
    Reboot,
    Relogin,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct App {
    pub name: String,
    pub icon: Option<String>,
}

pub fn category(package: &str, app: Option<&App>) -> Category {
    if package.starts_with("windoesnot") {
        Category::Windoesnot
    } else if app.is_some() {
        Category::App
    } else {
        Category::System
    }
}

pub fn group(package: &str) -> Group {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| package.starts_with(p));
    let is_kernel = package == "linux"
        || (package.starts_with("linux-") && !starts(&["linux-firmware", "linux-api-headers"]));
    if is_kernel {
        Group::Kernel
    } else if starts(&["linux-firmware", "mesa", "lib32-mesa", "vulkan-", "lib32-vulkan-", "nvidia", "lib32-nvidia",
        "xf86-video-", "intel-media-driver", "libva", "intel-ucode", "amd-ucode", "opencl-", "egl-"])
    {
        Group::Drivers
    } else if starts(&["hypr", "aquamarine", "quickshell", "qt5-", "qt6-", "gtk3", "gtk4", "libadwaita",
        "xdg-desktop-portal", "wayland", "xorg-xwayland", "sddm", "plymouth", "kf6-", "swaync", "libinput"])
    {
        Group::Desktop
    } else if starts(&["pipewire", "lib32-pipewire", "wireplumber", "alsa-", "lib32-alsa", "pulseaudio", "sof-firmware"]) {
        Group::Sound
    } else if starts(&["networkmanager", "iwd", "wpa_supplicant", "bluez", "blueman", "openssh", "nm-connection-editor"]) {
        Group::Network
    } else {
        Group::Other
    }
}

/// Kernel, firmware, microcode, systemd and the C library take effect after a
/// restart; the compositor, the shell and their graphics stack after a new login.
pub fn restart(package: &str) -> Option<Restart> {
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| package.starts_with(p));
    if group(package) == Group::Kernel
        || starts(&["linux-firmware", "intel-ucode", "amd-ucode", "nvidia", "lib32-nvidia"])
        || matches!(package, "systemd" | "systemd-libs" | "glibc" | "lib32-glibc")
    {
        Some(Restart::Reboot)
    } else if starts(&["hyprland", "aquamarine", "hyprutils", "hyprgraphics", "hyprlang", "hyprcursor", "quickshell",
        "mesa", "qt6-base", "qt6-declarative", "qt6-wayland", "windoesnot-plugins", "windoesnot-components"])
    {
        Some(Restart::Relogin)
    } else {
        None
    }
}

/// The program behind an installed package: the first visible application
/// entry it ships, preferring one named after the package.
pub fn installed_app(package: &str, version: &str, language: &str) -> Option<App> {
    app_from(Path::new(LOCAL_DB), Path::new("/"), package, version, language)
}

fn app_from(local_db: &Path, root: &Path, package: &str, version: &str, language: &str) -> Option<App> {
    let files = fs::read_to_string(local_db.join(format!("{package}-{version}")).join("files")).ok()?;
    let mut entries: Vec<&str> = files
        .lines()
        .filter(|line| line.starts_with(APPLICATIONS) && line.ends_with(".desktop"))
        .collect();
    let stem = |path: &&str| path.rsplit('/').next().unwrap_or("").trim_end_matches(".desktop").to_lowercase();
    entries.sort_by_key(|path| !stem(path).contains(&package.to_lowercase()));
    entries
        .into_iter()
        .find_map(|path| parse_entry(&fs::read_to_string(root.join(path)).ok()?, language))
}

/// Name (localized when available) and icon of a visible application entry.
fn parse_entry(text: &str, language: &str) -> Option<App> {
    let mut in_main = false;
    let (mut name, mut localized, mut icon) = (None, None, None);
    let localized_key = format!("Name[{language}]");
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "Name" => name = Some(value.to_string()),
            k if k == localized_key => localized = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()).filter(|v| !v.is_empty()),
            "NoDisplay" | "Hidden" if value.eq_ignore_ascii_case("true") => return None,
            "Type" if value != "Application" => return None,
            _ => {}
        }
    }
    Some(App { name: localized.or(name)?, icon })
}

/// Two-letter UI language from the environment, e.g. "de" for de_DE.UTF-8.
pub fn language() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
        .and_then(|value| value.get(..2).map(str::to_string))
        .unwrap_or_else(|| "en".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_fall_into_plain_groups() {
        assert_eq!(group("linux-cachyos"), Group::Kernel);
        assert_eq!(group("linux-cachyos-lts-headers"), Group::Kernel);
        assert_eq!(group("linux-firmware-intel"), Group::Drivers);
        assert_eq!(group("lib32-mesa"), Group::Drivers);
        assert_eq!(group("hyprland"), Group::Desktop);
        assert_eq!(group("wireplumber"), Group::Sound);
        assert_eq!(group("networkmanager"), Group::Network);
        assert_eq!(group("7zip"), Group::Other);
    }

    #[test]
    fn restart_follows_what_was_replaced() {
        assert_eq!(restart("linux-cachyos"), Some(Restart::Reboot));
        assert_eq!(restart("intel-ucode"), Some(Restart::Reboot));
        assert_eq!(restart("glibc"), Some(Restart::Reboot));
        assert_eq!(restart("hyprland"), Some(Restart::Relogin));
        assert_eq!(restart("windoesnot-plugins"), Some(Restart::Relogin));
        assert_eq!(restart("firefox"), None);
        assert_eq!(restart("linux-api-headers"), None);
    }

    #[test]
    fn windoesnot_and_apps_are_recognised() {
        let app = App { name: "Firefox".into(), icon: None };
        assert_eq!(category("windoesnot-desktop", None), Category::Windoesnot);
        assert_eq!(category("firefox", Some(&app)), Category::App);
        assert_eq!(category("glib2", None), Category::System);
    }

    #[test]
    fn desktop_entries_give_localized_name_and_icon() {
        let entry = "[Desktop Entry]\nType=Application\nName=Image Viewer\nName[de]=Bildbetrachter\nIcon=viewer\n\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(parse_entry(entry, "de"), Some(App { name: "Bildbetrachter".into(), icon: Some("viewer".into()) }));
        assert_eq!(parse_entry(entry, "fr").unwrap().name, "Image Viewer");
        assert!(parse_entry("[Desktop Entry]\nName=Helper\nNoDisplay=true\n", "de").is_none());
        assert!(parse_entry("[Desktop Entry]\nType=Link\nName=Site\n", "de").is_none());
    }

    #[test]
    fn app_comes_from_the_installed_files_list() {
        let root = std::env::temp_dir().join(format!("wdn-classify-{}", std::process::id()));
        let db = root.join("local");
        fs::create_dir_all(db.join("gimp-3.0.6-1")).unwrap();
        fs::write(db.join("gimp-3.0.6-1/files"), "%FILES%\nusr/\nusr/share/applications/helper.desktop\nusr/share/applications/gimp.desktop\n").unwrap();
        fs::create_dir_all(root.join("usr/share/applications")).unwrap();
        fs::write(root.join("usr/share/applications/helper.desktop"), "[Desktop Entry]\nName=Helper\nIcon=h\n").unwrap();
        fs::write(root.join("usr/share/applications/gimp.desktop"), "[Desktop Entry]\nName=GNU Image Manipulation Program\nName[de]=GIMP\nIcon=gimp\n").unwrap();
        assert_eq!(app_from(&db, &root, "gimp", "3.0.6-1", "de"), Some(App { name: "GIMP".into(), icon: Some("gimp".into()) }));
        assert_eq!(app_from(&db, &root, "missing", "1-1", "de"), None);
        let _ = fs::remove_dir_all(root);
    }
}

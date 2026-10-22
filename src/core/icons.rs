//! Icons for packages that are not installed yet, taken from the AppStream
//! catalogues already on the system: the Arch catalogue
//! (`archlinux-appstream-data`, icons named `<package>_<app-id>.png`) and the
//! Flathub catalogue (icons named after the app id). AUR packages usually have
//! no icon of their own, so their base name (`localsend-bin` -> `localsend`)
//! is matched against Flathub app names.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FLATHUB: &str = "/var/lib/flatpak/appstream/flathub/x86_64/active";
const SWCATALOG_ICONS: &str = "/usr/share/swcatalog/icons";
const SIZES: [&str; 2] = ["128x128", "64x64"];

pub fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// The package name followed by the names left after removing AUR suffixes.
pub fn base_names(package: &str) -> Vec<String> {
    let mut names = vec![package.to_string()];
    let mut current = package;
    for suffix in ["-git", "-bin", "-appimage", "-stable", "-desktop"] {
        if let Some(stripped) = current.strip_suffix(suffix) {
            current = stripped;
            names.push(current.to_string());
        }
    }
    names
}

/// Cached icon file for `package`, if any catalogue has one.
pub fn icon_file(package: &str, flathub_id: Option<&str>) -> Option<PathBuf> {
    if let Some(id) = flathub_id {
        if let Some(path) = flathub_icon_by_id(id) {
            return Some(path);
        }
    }
    let names = base_names(package);
    if let Some(path) = names.iter().find_map(|name| arch_index().get(name)) {
        return Some(path.clone());
    }
    names.iter().find_map(|name| flathub_index().get(&normalize(name)).cloned())
}

/// Builds both indexes off the UI thread so the first tiles do not wait.
pub fn warm_up() {
    std::thread::spawn(|| {
        arch_index();
        flathub_index();
    });
}

fn flathub_icon_by_id(id: &str) -> Option<PathBuf> {
    SIZES
        .iter()
        .map(|size| Path::new(FLATHUB).join("icons").join(size).join(format!("{id}.png")))
        .find(|path| path.is_file())
}

fn arch_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        let Ok(origins) = std::fs::read_dir(SWCATALOG_ICONS) else {
            return index;
        };
        for origin in origins.flatten() {
            // Larger size first: the first match for a package wins.
            for size in SIZES {
                let Ok(files) = std::fs::read_dir(origin.path().join(size)) else {
                    continue;
                };
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().to_string();
                    if let Some((package, _)) = name.split_once('_') {
                        index.entry(package.to_string()).or_insert_with(|| file.path());
                    }
                }
            }
        }
        index
    })
}

fn flathub_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        let Ok(xml) = std::fs::read_to_string(Path::new(FLATHUB).join("appstream.xml")) else {
            return index;
        };
        for component in xml.split("<component").skip(1) {
            let Some(id) = between(component, "<id>", "</id>") else {
                continue;
            };
            let Some(path) = flathub_icon_by_id(id) else {
                continue;
            };
            let last_segment = id.rsplit('.').next().unwrap_or(id);
            let mut keys = vec![normalize(id), normalize(last_segment)];
            // The untranslated name is the first plain <name> element.
            if let Some(name) = between(component, "<name>", "</name>") {
                keys.push(normalize(name));
            }
            for key in keys.into_iter().filter(|key| !key.is_empty()) {
                index.entry(key).or_insert_with(|| path.clone());
            }
        }
        index
    })
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(text[from..to].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aur_suffixes_are_stripped_in_order() {
        assert_eq!(base_names("localsend-bin"), ["localsend-bin", "localsend"]);
        assert_eq!(base_names("foo-bin-git"), ["foo-bin-git", "foo-bin", "foo"]);
        assert_eq!(normalize("LocalSend"), "localsend");
        assert_eq!(normalize("localsend_app"), "localsendapp");
    }
}

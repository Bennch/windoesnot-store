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
    if let Some(path) = names.iter().find_map(|name| flathub_index().get(&normalize(name))) {
        return Some(path.clone());
    }
    names.iter().find_map(|name| papirus_index().get(&normalize(name)).cloned())
}

/// Icon of the Flathub app whose project page is `url` (same host and first path segment).
pub fn icon_for_homepage(url: &str) -> Option<PathBuf> {
    homepage_key(url).and_then(|key| homepage_index().get(&key).cloned())
}

/// `https://github.com/hyprwm/Hyprland/` -> `github.com/hyprwm/hyprland`; plain hosts keep only the host.
pub fn homepage_key(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let rest = rest.split(['?', '#']).next()?.trim_end_matches('/');
    let mut parts = rest.split('/');
    let host = parts.next()?.trim_start_matches("www.").to_lowercase();
    if host.is_empty() { return None; }
    let forge = ["github.com", "gitlab.com", "codeberg.org", "invent.kde.org", "gitlab.gnome.org", "sr.ht"];
    if forge.contains(&host.as_str()) {
        let owner = parts.next()?;
        let repo = parts.next().unwrap_or("");
        return Some(format!("{host}/{}/{}", owner.to_lowercase(), repo.trim_end_matches(".git").to_lowercase()));
    }
    Some(host)
}

fn homepage_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        let Ok(xml) = std::fs::read_to_string(Path::new(FLATHUB).join("appstream.xml")) else {
            return index;
        };
        for component in xml.split("<component").skip(1) {
            let (Some(id), Some(home)) = (between(component, "<id>", "</id>"), between(component, "<url type=\"homepage\">", "</url>")) else {
                continue;
            };
            if let (Some(path), Some(key)) = (flathub_icon_by_id(id), homepage_key(home)) {
                // A bare host is shared by many projects: only exact repository/project keys are reliable.
                if key.contains('/') { index.entry(key).or_insert(path); }
            }
        }
        index
    })
}

/// Papirus app icons by normalized name (installed with the icon theme; the ISO ships it).
fn papirus_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for size in ["64x64", "48x48", "128x128"] {
            let Ok(files) = std::fs::read_dir(Path::new("/usr/share/icons/Papirus").join(size).join("apps")) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                let is_icon = matches!(path.extension().and_then(|e| e.to_str()), Some("svg" | "png"));
                // Skip symlinks that point outside the theme directory.
                if !is_icon || !path.canonicalize().map_or(false, |p| p.starts_with("/usr/share/icons/Papirus")) { continue; }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    index.entry(normalize(stem)).or_insert(path);
                }
            }
        }
        index
    })
}

/// Builds both indexes off the UI thread so the first tiles do not wait.
pub fn warm_up() {
    std::thread::spawn(|| {
        arch_index();
        flathub_index();
        papirus_index();
        homepage_index();
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
    fn homepage_keys() {
        assert_eq!(homepage_key("https://github.com/hyprwm/Hyprland/").as_deref(), Some("github.com/hyprwm/hyprland"));
        assert_eq!(homepage_key("https://github.com/o/r.git").as_deref(), Some("github.com/o/r"));
        assert_eq!(homepage_key("https://www.example.org/path?x=1").as_deref(), Some("example.org"));
        assert_eq!(homepage_key("not a url"), None);
    }

    #[test]
    fn aur_suffixes_are_stripped_in_order() {
        assert_eq!(base_names("localsend-bin"), ["localsend-bin", "localsend"]);
        assert_eq!(base_names("foo-bin-git"), ["foo-bin-git", "foo-bin", "foo"]);
        assert_eq!(normalize("LocalSend"), "localsend");
        assert_eq!(normalize("localsend_app"), "localsendapp");
    }
}

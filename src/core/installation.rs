//! Whether and how an app is installed, across all sources: a native package
//! (repository or AUR) or a Flatpak. Home, search and details use the same
//! answer, so an app installed as Flatpak is never offered again as package.
//!
//! Native packages and Flatpaks are linked through the AppStream component ids
//! in Arch's catalogue (`<pkgname>gimp</pkgname>` ↔ `org.gimp.GIMP`), plus
//! Flatpak ids that callers already know (curated home apps).
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use crate::core::models::PackageSource;
use crate::core::providers::{FlatpakProvider, PacmanProvider};

const ARCH_CATALOGUE: &str = "/usr/share/swcatalog/xml";
/// Common AUR names for the same app.
const NATIVE_VARIANTS: [&str; 4] = ["", "-git", "-bin", "-appimage"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub source: PackageSource,
    /// Package name or Flatpak app id, as needed to remove it.
    pub name: String,
    pub version: String,
}

/// Snapshot of everything installed. Cheap to query; reload after transactions.
#[derive(Debug, Clone, Default)]
pub struct InstalledApps {
    native: HashMap<String, (String, bool)>,
    flatpaks: Vec<(String, String)>,
}

impl InstalledApps {
    pub fn load(pacman: &dyn PacmanProvider, flatpak: Option<&dyn FlatpakProvider>) -> Self {
        let native = pacman
            .list_installed()
            .unwrap_or_default()
            .into_iter()
            .map(|pkg| (pkg.name, (pkg.version, pkg.source == PackageSource::Aur)))
            .collect();
        let flatpaks = flatpak
            .and_then(|flatpak| flatpak.list_installed().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|pkg| (pkg.name, pkg.version))
            .collect();
        Self { native, flatpaks }
    }

    /// All installations of the app behind `name` (a package name or a Flatpak
    /// id), with an optional known Flatpak id. Native installations come first.
    pub fn find(&self, name: &str, flatpak_id: Option<&str>) -> Vec<Installation> {
        let mut native_names: Vec<String> = Vec::new();
        let mut flatpak_ids: Vec<String> = flatpak_id.map(normalize_id).into_iter().collect();
        if looks_like_app_id(name) {
            flatpak_ids.push(normalize_id(name));
            native_names.extend(packages_for_id(name).iter().cloned());
        } else {
            native_names.extend(NATIVE_VARIANTS.iter().map(|suffix| format!("{name}{suffix}")));
            flatpak_ids.extend(ids_for_package(name).iter().cloned());
        }
        if let Some(id) = flatpak_id {
            native_names.extend(packages_for_id(id).iter().cloned());
        }

        // Package names that are also the last part of an installed Flatpak id
        // (discord ↔ com.discordapp.Discord), for apps the catalogue does not link.
        let short_name = (!looks_like_app_id(name)).then(|| name.to_lowercase());

        let mut found = Vec::new();
        for package in native_names {
            if let Some((version, foreign)) = self.native.get(&package) {
                let source = if *foreign { PackageSource::Aur } else { PackageSource::Repo };
                push_unique(&mut found, Installation { source, name: package, version: version.clone() });
            }
        }
        for (id, version) in &self.flatpaks {
            let installed = normalize_id(id);
            // A known id also covers branch variants such as org.mozilla.firefox.ESR.
            let by_id = flatpak_ids.iter().any(|wanted| installed == *wanted || installed.starts_with(&format!("{wanted}.")));
            let by_name = short_name.as_deref().is_some_and(|short| installed.rsplit('.').next() == Some(short));
            if by_id || by_name {
                push_unique(&mut found, Installation { source: PackageSource::Flatpak, name: id.clone(), version: version.clone() });
            }
        }
        found
    }
}

fn push_unique(found: &mut Vec<Installation>, installation: Installation) {
    if !found.contains(&installation) {
        found.push(installation);
    }
}

/// Flatpak ids are reverse-DNS names with at least two dots; package names never contain one.
fn looks_like_app_id(name: &str) -> bool {
    name.matches('.').count() >= 2
}

fn normalize_id(id: &str) -> String {
    id.trim().trim_end_matches(".desktop").to_lowercase()
}

fn ids_for_package(package: &str) -> &'static [String] {
    catalogue().0.get(package).map(Vec::as_slice).unwrap_or(&[])
}

fn packages_for_id(id: &str) -> &'static [String] {
    catalogue().1.get(&normalize_id(id)).map(Vec::as_slice).unwrap_or(&[])
}

/// package → component ids, and component id → packages, from Arch's AppStream data.
fn catalogue() -> &'static (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    static INDEX: OnceLock<(HashMap<String, Vec<String>>, HashMap<String, Vec<String>>)> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut by_package: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_id: HashMap<String, Vec<String>> = HashMap::new();
        let Ok(files) = std::fs::read_dir(ARCH_CATALOGUE) else {
            return (by_package, by_id);
        };
        for file in files.flatten() {
            let path = file.path();
            if !path.to_string_lossy().ends_with(".xml.gz") {
                continue;
            }
            let Ok(compressed) = std::fs::File::open(&path) else { continue };
            let mut xml = String::new();
            if flate2::read::GzDecoder::new(compressed).read_to_string(&mut xml).is_err() {
                continue;
            }
            for (package, id) in parse_catalogue(&xml) {
                by_package.entry(package.clone()).or_default().push(id.clone());
                by_id.entry(id).or_default().push(package);
            }
        }
        (by_package, by_id)
    })
}

/// (package, normalized component id) pairs of one AppStream catalogue.
fn parse_catalogue(xml: &str) -> Vec<(String, String)> {
    xml.split("<component")
        .skip(1)
        .filter_map(|component| {
            let id = between(component, "<id>", "</id>")?;
            let package = between(component, "<pkgname>", "</pkgname>")?;
            Some((package.to_string(), normalize_id(id)))
        })
        .collect()
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(text[from..to].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(native: &[(&str, &str, bool)], flatpaks: &[(&str, &str)]) -> InstalledApps {
        InstalledApps {
            native: native.iter().map(|(n, v, f)| (n.to_string(), (v.to_string(), *f))).collect(),
            flatpaks: flatpaks.iter().map(|(id, v)| (id.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn known_flatpak_id_finds_a_flatpak_install_of_a_package() {
        let installed = apps(&[], &[("org.gimp.GIMP", "3.0.4")]);
        let found = installed.find("gimp", Some("org.gimp.GIMP"));
        assert_eq!(found, [Installation { source: PackageSource::Flatpak, name: "org.gimp.GIMP".into(), version: "3.0.4".into() }]);
    }

    #[test]
    fn native_sources_and_aur_variants_are_distinguished() {
        let installed = apps(&[("vlc", "3.0.24-1", false), ("localsend-bin", "1.17-1", true)], &[]);
        assert_eq!(installed.find("vlc", None)[0].source, PackageSource::Repo);
        let aur = installed.find("localsend", None);
        assert_eq!((aur[0].source, aur[0].name.as_str()), (PackageSource::Aur, "localsend-bin"));
        assert!(installed.find("gimp", None).is_empty());
    }

    #[test]
    fn flatpak_ids_match_case_insensitively_and_branch_variants() {
        let installed = apps(&[], &[("org.videolan.VLC", "3.0.21"), ("org.mozilla.firefox.ESR", "128")]);
        assert_eq!(installed.find("org.videolan.vlc", None).len(), 1);
        assert_eq!(installed.find("firefox", Some("org.mozilla.firefox")).len(), 1);
        assert!(installed.find("thunderbird", Some("org.mozilla.Thunderbird")).is_empty());
    }

    #[test]
    fn package_name_matches_the_last_part_of_a_flatpak_id() {
        let installed = apps(&[], &[("com.discordapp.Discord", "1.0.164")]);
        assert_eq!(installed.find("discord", None).len(), 1);
        assert!(installed.find("discord-canary", None).is_empty());
        assert!(installed.find("com.discordapp", None).is_empty());
    }

    #[test]
    fn both_installations_are_reported_native_first() {
        let installed = apps(&[("gimp", "3.2.6-2", false)], &[("org.gimp.GIMP", "3.0.4")]);
        let found = installed.find("gimp", Some("org.gimp.GIMP"));
        let sources: Vec<_> = found.iter().map(|i| i.source).collect();
        assert_eq!(sources, [PackageSource::Repo, PackageSource::Flatpak]);
    }

    #[test]
    fn catalogue_links_packages_to_component_ids() {
        let xml = "<components><component type=\"desktop-application\"><id>org.gimp.GIMP</id>\
                   <pkgname>gimp</pkgname></component><component><id>code-oss.desktop</id>\
                   <pkgname>code</pkgname></component><component><id>no.package</id></component></components>";
        assert_eq!(parse_catalogue(xml), [("gimp".into(), "org.gimp.gimp".into()), ("code".into(), "code-oss".into())]);
    }

    /// Reads this machine's real packages, Flatpaks and AppStream catalogue:
    /// `cargo test -- --ignored this_system --nocapture`
    #[test]
    #[ignore]
    fn this_system() {
        use crate::core::providers::{flatpak::Flatpak, pacman::Pacman};
        let installed = InstalledApps::load(&Pacman, Some(&Flatpak));
        for (name, flatpak_id) in [
            ("gimp", Some("org.gimp.GIMP")), ("gimp", None), ("org.gimp.GIMP", None),
            ("vlc", None), ("obs-studio", None), ("steam", None), ("discord", Some("com.discordapp.Discord")),
            ("discord", None), ("code", Some("com.visualstudio.code")), ("firefox", None), ("htop", None),
        ] {
            println!("{name:<16} {:<26} {:?}", flatpak_id.unwrap_or("-"), installed.find(name, flatpak_id));
        }
    }
}

//! Where an app can be installed from: the distribution's repositories, Flathub
//! or the AUR. The details page lists every source that offers the app, like
//! other app stores do, so people can choose instead of getting whichever
//! search hit they clicked.
use std::process::Command;

use crate::core::installation::{ids_for_package, looks_like_app_id, normalize_id, packages_for_id};
use crate::core::models::PackageSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOption {
    pub source: PackageSource,
    /// Package name or Flatpak application id, as needed to install it.
    pub name: String,
    pub version: String,
    pub size: Option<String>,
    /// Repository of a native package (e.g. "extra"), when known.
    pub origin: Option<String>,
}

/// What the lookups return; injected so the matching logic is testable.
pub struct Lookups<'a> {
    pub repo: &'a dyn Fn(&str) -> Option<SourceOption>,
    pub flathub: &'a dyn Fn(&str) -> Vec<(String, String)>,
    /// None while the AUR is switched off.
    pub aur: Option<&'a dyn Fn(&str) -> Option<SourceOption>>,
}

/// All sources for the app behind `name` (package name or Flatpak id), in the
/// order offered: repositories, Flathub, AUR. `known` is the entry the page was
/// opened from and is always kept, even if a lookup fails.
pub fn find(name: &str, flatpak_id: Option<&str>, known: SourceOption, lookups: &Lookups) -> Vec<SourceOption> {
    let mut packages: Vec<String> = Vec::new();
    let mut ids: Vec<String> = flatpak_id.map(normalize_id).into_iter().collect();
    if looks_like_app_id(name) {
        ids.push(normalize_id(name));
        packages.extend(packages_for_id(name).iter().cloned());
    } else {
        packages.push(name.to_string());
        ids.extend(ids_for_package(name).iter().cloned());
    }

    let mut options = vec![known.clone()];
    if known.source != PackageSource::Repo {
        if let Some(found) = packages.iter().find_map(|package| (lookups.repo)(package)) {
            options.push(found);
        }
    }
    if known.source != PackageSource::Flatpak {
        let query = packages.first().cloned().unwrap_or_else(|| name.to_string());
        if let Some((id, version)) = matching_flatpak(&(lookups.flathub)(&query), &ids, &packages) {
            options.push(SourceOption { source: PackageSource::Flatpak, name: id, version, size: None, origin: Some("flathub".into()) });
        }
    }
    if known.source != PackageSource::Aur {
        if let Some(aur) = lookups.aur {
            if let Some(found) = packages.first().and_then(|package| aur(package)) {
                options.push(found);
            }
        }
    }
    options.sort_by_key(|option| match option.source {
        PackageSource::Repo => 0,
        PackageSource::Flatpak => 1,
        PackageSource::Aur => 2,
    });
    options
}

/// The Flathub app for these AppStream ids, or whose id ends in the package
/// name (org.videolan.VLC for vlc). Plugins and manuals of the app don't match.
fn matching_flatpak(hits: &[(String, String)], ids: &[String], packages: &[String]) -> Option<(String, String)> {
    hits.iter()
        .find(|(id, _)| ids.contains(&normalize_id(id)))
        .or_else(|| {
            hits.iter().find(|(id, _)| {
                let last = id.rsplit('.').next().unwrap_or("").to_lowercase();
                packages.iter().any(|package| *package == last)
            })
        })
        .cloned()
}

/// Flathub search results as (application id, version), from the local
/// appstream cache; no network access of its own.
pub fn flathub_search(query: &str) -> Vec<(String, String)> {
    let Ok(output) = Command::new("timeout")
        .args(["10s", "flatpak", "search", "--columns=application,version,remotes", query])
        .env("LC_ALL", "C")
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let (id, version, remotes) = (columns.next()?, columns.next()?, columns.next()?);
            remotes.split(',').any(|remote| remote.trim() == "flathub").then(|| (id.to_string(), version.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(source: PackageSource, name: &str) -> SourceOption {
        SourceOption { source, name: name.into(), version: "1".into(), size: None, origin: None }
    }

    fn hits() -> Vec<(String, String)> {
        vec![("org.gimp.GIMP.Manual".into(), "2.10".into()), ("org.gimp.GIMP".into(), "3.2.6".into())]
    }

    #[test]
    fn flathub_app_matches_by_appstream_id_not_its_plugins() {
        let found = matching_flatpak(&hits(), &["org.gimp.gimp".into()], &["gimp".into()]);
        assert_eq!(found, Some(("org.gimp.GIMP".into(), "3.2.6".into())));
    }

    #[test]
    fn flathub_app_matches_by_last_id_part() {
        let hits = vec![("org.videolan.VLC".into(), "3.0.21".into())];
        assert_eq!(matching_flatpak(&hits, &[], &["vlc".into()]).unwrap().0, "org.videolan.VLC");
        assert!(matching_flatpak(&hits, &[], &["vlc-git".into()]).is_none());
    }

    #[test]
    fn all_sources_are_offered_repo_first_and_aur_only_when_enabled() {
        let repo = |name: &str| (name == "gimp").then(|| option(PackageSource::Repo, "gimp"));
        let flathub = |_: &str| hits();
        let aur = |name: &str| Some(option(PackageSource::Aur, name));
        let known = option(PackageSource::Flatpak, "org.gimp.GIMP");
        let with_aur = Lookups { repo: &repo, flathub: &flathub, aur: Some(&aur) };
        let sources: Vec<_> = find("gimp", None, option(PackageSource::Repo, "gimp"), &with_aur).into_iter().map(|o| o.source).collect();
        assert_eq!(sources, [PackageSource::Repo, PackageSource::Flatpak, PackageSource::Aur]);
        let without_aur = Lookups { repo: &repo, flathub: &flathub, aur: None };
        let sources: Vec<_> = find("gimp", None, option(PackageSource::Repo, "gimp"), &without_aur).into_iter().map(|o| o.source).collect();
        assert_eq!(sources, [PackageSource::Repo, PackageSource::Flatpak]);
        // Opened from the Flatpak tile: whatever the catalogue maps, the clicked source stays.
        assert!(find("org.gimp.GIMP", None, known.clone(), &without_aur).contains(&known));
    }

    #[test]
    fn the_clicked_source_survives_failed_lookups() {
        let none = |_: &str| None;
        let empty = |_: &str| Vec::new();
        let lookups = Lookups { repo: &none, flathub: &empty, aur: None };
        let known = option(PackageSource::Aur, "localsend-bin");
        assert_eq!(find("localsend-bin", None, known.clone(), &lookups), vec![known]);
    }
}

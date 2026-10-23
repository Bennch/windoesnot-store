//! System update status for the desktop settings (`windoesnot-store --status-json`):
//! pending updates from the user-owned sync copy, unread Arch news, and whether a
//! restart or a new login is needed. Reading only; nothing is installed.
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::core::news::{self, NewsItem};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PendingUpdate {
    pub name: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Serialize)]
pub struct NewsEntry {
    pub title: String,
    pub link: String,
    pub intervention: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub unreleased: bool,
    #[serde(default)]
    pub de: Vec<String>,
    #[serde(default)]
    pub en: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WinDoesNot {
    /// Installed desktop package version; None on a non-packaged (developer) setup.
    pub installed: Option<String>,
    /// Version in the configured WinDoesNot repository, if any.
    pub available: Option<String>,
    pub update: bool,
    /// What the update brings, or what is new in the installed version.
    pub changes: Vec<Release>,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub windoesnot: WinDoesNot,
    /// Unix seconds of this check.
    pub checked: u64,
    pub updates: Vec<PendingUpdate>,
    pub news: Vec<NewsEntry>,
    /// The news feed could not be loaded (offline); updates still work.
    pub news_unavailable: bool,
    /// The running kernel's modules are gone: an upgrade replaced the kernel.
    pub reboot: bool,
    /// Hyprland or Quickshell were replaced while running.
    pub relogin: bool,
}

pub fn collect(refresh: bool) -> Status {
    if refresh {
        if let Err(err) = crate::core::syncdb::refresh_now() {
            eprintln!("windoesnot-store: package databases not refreshed: {err}");
        }
    }
    let (news, news_unavailable) = match news::unread() {
        Ok(items) => (items.iter().map(entry).collect(), false),
        Err(_) => (Vec::new(), true),
    };
    Status {
        windoesnot: windoesnot(),
        checked: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        updates: pending_updates(),
        news,
        news_unavailable,
        reboot: kernel_replaced(Path::new("/proc/sys/kernel/osrelease"), Path::new("/usr/lib/modules")),
        relogin: session_replaced(),
    }
}

/// The running kernel was replaced by an upgrade: restart to use the new one.
pub fn needs_reboot() -> bool {
    kernel_replaced(Path::new("/proc/sys/kernel/osrelease"), Path::new("/usr/lib/modules"))
}

/// Hyprland or Quickshell were replaced: log in again to use the new ones.
pub fn needs_relogin() -> bool {
    session_replaced()
}

const VERSION_FILE: &str = "/usr/share/windoesnot/VERSION";
const LOCAL_CHANGELOG: &str = "/usr/share/windoesnot/changelog.json";
/// Changelog of the newest WinDoesNot release on GitHub (assumed repository
/// name; the project is not uploaded yet, until then this 404s and the
/// installed changelog is used).
const REMOTE_CHANGELOG: &str = "https://raw.githubusercontent.com/Bennch/windoesnot/main/packaging/changelog.json";

fn windoesnot() -> WinDoesNot {
    let installed = std::fs::read_to_string(VERSION_FILE).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let available = repository_version();
    // GitHub knows the changes of a version before it is installed.
    let releases = remote_changelog().or_else(|| {
        std::fs::read_to_string(LOCAL_CHANGELOG).ok().and_then(|text| parse_changelog(&text))
    }).unwrap_or_default();
    let update = match (&installed, &available) {
        (Some(installed), Some(available)) => vercmp(available, installed) > 0,
        _ => false,
    };
    let changes = select_changes(&releases, installed.as_deref(), available.as_deref().filter(|_| update), vercmp);
    WinDoesNot { installed, available, update, changes }
}

/// `pacman -Si` from the user-owned sync copy; only set once the WinDoesNot
/// repository is configured.
fn repository_version() -> Option<String> {
    let output = Command::new("pacman")
        .args(["-Si", "--dbpath"])
        .arg(crate::core::syncdb::query_dbpath())
        .arg("windoesnot-desktop")
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Version").map(|rest| rest.trim_start_matches([' ', ':']).trim().to_string()))
        .filter(|v| !v.is_empty())
}

fn remote_changelog() -> Option<Vec<Release>> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(6)).build();
    let text = agent.get(REMOTE_CHANGELOG).call().ok()?.into_string().ok()?;
    parse_changelog(&text)
}

fn parse_changelog(text: &str) -> Option<Vec<Release>> {
    #[derive(serde::Deserialize)]
    struct File {
        releases: Vec<Release>,
    }
    serde_json::from_str::<File>(text).ok().map(|file| file.releases)
}

/// With an update: the released versions after the installed one up to the
/// available one. Without: what is new in the installed version.
fn select_changes(releases: &[Release], installed: Option<&str>, available: Option<&str>, cmp: fn(&str, &str) -> i32) -> Vec<Release> {
    let Some(installed) = installed else { return Vec::new() };
    releases
        .iter()
        .filter(|release| !release.unreleased)
        .filter(|release| match available {
            Some(available) => cmp(&release.version, installed) > 0 && cmp(&release.version, available) <= 0,
            None => cmp(&release.version, installed) == 0,
        })
        .cloned()
        .collect()
}

/// pacman's version comparison (epochs, pkgrel).
fn vercmp(a: &str, b: &str) -> i32 {
    Command::new("vercmp")
        .args([a, b])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

fn entry(item: &NewsItem) -> NewsEntry {
    NewsEntry { title: item.title.clone(), link: item.link.clone(), intervention: item.needs_intervention() }
}

fn pending_updates() -> Vec<PendingUpdate> {
    let output = Command::new("pacman")
        .args(["-Qu", "--dbpath"])
        .arg(crate::core::syncdb::query_dbpath())
        .env("LC_ALL", "C")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    parse_pending(&output)
}

/// `pacman -Qu` lines: "name old -> new" (ignored packages end with "[ignored]").
fn parse_pending(output: &str) -> Vec<PendingUpdate> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let old = parts.next()?.to_string();
            (parts.next()? == "->").then_some(())?;
            let new = parts.next()?.to_string();
            Some(PendingUpdate { name, old, new })
        })
        .collect()
}

fn kernel_replaced(osrelease: &Path, modules: &Path) -> bool {
    std::fs::read_to_string(osrelease)
        .map(|release| !modules.join(release.trim()).is_dir())
        .unwrap_or(false)
}

/// After an upgrade the running program's file is replaced; /proc then
/// reports its executable as "(deleted)".
fn session_replaced() -> bool {
    let uid = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir("/proc") else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        let owned = std::fs::metadata(&path).map(|m| std::os::unix::fs::MetadataExt::uid(&m) == uid).unwrap_or(false);
        let name = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
        owned
            && matches!(name.trim(), "Hyprland" | "quickshell" | "qs")
            && std::fs::read_link(path.join("exe")).is_ok_and(|exe| exe.to_string_lossy().ends_with(" (deleted)"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_qu_lines_are_parsed() {
        let output = "linux-cachyos 7.2.7-1 -> 7.2.11-1\nhyprland 0.56.2-4 -> 0.57.0-1 [ignored]\nbroken line\n";
        let updates = parse_pending(output);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0], PendingUpdate { name: "linux-cachyos".into(), old: "7.2.7-1".into(), new: "7.2.11-1".into() });
    }

    #[test]
    fn missing_modules_of_the_running_kernel_need_a_reboot() {
        let root = std::env::temp_dir().join(format!("wdn-status-{}", std::process::id()));
        let modules = root.join("modules");
        std::fs::create_dir_all(modules.join("7.2.11-1-cachyos")).unwrap();
        let release = root.join("osrelease");
        std::fs::write(&release, "7.2.11-1-cachyos\n").unwrap();
        assert!(!kernel_replaced(&release, &modules));
        std::fs::write(&release, "7.2.7-1-cachyos\n").unwrap();
        assert!(kernel_replaced(&release, &modules));
        let _ = std::fs::remove_dir_all(root);
    }

    fn release(version: &str, unreleased: bool) -> Release {
        Release { version: version.into(), date: None, unreleased, de: vec![], en: vec![] }
    }

    #[test]
    fn changes_cover_the_update_or_the_installed_version() {
        let releases = [release("0.1.0-17", true), release("0.1.0-16", false), release("0.1.0-15", false), release("0.1.0-14", false)];
        let versions = |list: Vec<Release>| list.into_iter().map(|r| r.version).collect::<Vec<_>>();
        assert_eq!(versions(select_changes(&releases, Some("0.1.0-14"), Some("0.1.0-16"), vercmp)), ["0.1.0-16", "0.1.0-15"]);
        assert_eq!(versions(select_changes(&releases, Some("0.1.0-15"), None, vercmp)), ["0.1.0-15"]);
        assert!(select_changes(&releases, None, Some("0.1.0-16"), vercmp).is_empty());
    }

    #[test]
    fn project_changelog_parses() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../packaging/changelog.json"));
        if let Ok(text) = text {
            let releases = parse_changelog(&text).expect("valid changelog");
            assert!(releases.iter().all(|r| !r.de.is_empty() && r.de.len() == r.en.len()));
        }
    }
}

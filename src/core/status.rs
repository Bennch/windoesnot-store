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

#[derive(Debug, Serialize)]
pub struct Status {
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
}

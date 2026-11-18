//! Starting an installed app from the details page.
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::core::installation::Installation;
use crate::core::models::PackageSource;

/// A launchable target: a Flatpak app id or the `.desktop` file shipped by a native package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launcher {
    Flatpak(String),
    Desktop(PathBuf),
}

/// Desktop entries of an installed package: `/usr/share/applications/*.desktop` out of `pacman -Ql`
/// that are meant to be shown (no NoDisplay/Hidden, type Application).
pub fn find_launcher(installation: &Installation) -> Option<Launcher> {
    match installation.source {
        PackageSource::Flatpak => Some(Launcher::Flatpak(installation.name.clone())),
        PackageSource::Repo | PackageSource::Aur => {
            let out = Command::new("pacman").args(["-Ql", &installation.name]).env("LC_ALL", "C").output().ok()?;
            if !out.status.success() {
                return None;
            }
            let listing = String::from_utf8_lossy(&out.stdout);
            desktop_files(&listing)
                .into_iter()
                .find(|path| std::fs::read_to_string(path).map_or(false, |text| is_launchable_entry(&text)))
                .map(Launcher::Desktop)
        }
    }
}

/// `pkg /usr/share/applications/x.desktop` lines -> paths of top-level desktop files.
pub fn desktop_files(pacman_ql: &str) -> Vec<PathBuf> {
    pacman_ql
        .lines()
        .filter_map(|line| line.split_once(' ').map(|(_, path)| path.trim()))
        .filter(|path| path.starts_with("/usr/share/applications/") && path.ends_with(".desktop"))
        .filter(|path| !path[24..].contains('/'))
        .map(PathBuf::from)
        .collect()
}

pub fn is_launchable_entry(text: &str) -> bool {
    let mut kind_application = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line != "[Desktop Entry]" {
            break; // actions and other groups
        }
        match line.split_once('=') {
            Some(("Type", "Application")) => kind_application = true,
            Some(("NoDisplay", "true")) | Some(("Hidden", "true")) => return false,
            _ => {}
        }
    }
    kind_application
}

pub fn launch(launcher: &Launcher) -> std::io::Result<()> {
    let mut command = match launcher {
        Launcher::Flatpak(id) => {
            let mut c = Command::new("flatpak");
            c.args(["run", id]);
            c
        }
        Launcher::Desktop(path) => {
            let mut c = Command::new("gio");
            c.arg("launch").arg(path);
            c
        }
    };
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_top_level_desktop_files_only() {
        let listing = "foo /usr/share/applications/\nfoo /usr/share/applications/foo.desktop\nfoo /usr/share/applications/sub/x.desktop\nfoo /usr/bin/foo\nfoo /usr/share/icons/foo.desktop\n";
        assert_eq!(desktop_files(listing), vec![PathBuf::from("/usr/share/applications/foo.desktop")]);
    }

    #[test]
    fn hidden_entries_are_not_launchers() {
        assert!(is_launchable_entry("[Desktop Entry]\nType=Application\nName=A\nExec=a\n"));
        assert!(!is_launchable_entry("[Desktop Entry]\nType=Application\nNoDisplay=true\n"));
        assert!(!is_launchable_entry("[Desktop Entry]\nType=Link\nName=A\n"));
        assert!(is_launchable_entry("[Desktop Entry]\nType=Application\n[Desktop Action x]\nNoDisplay=true\n"));
    }
}

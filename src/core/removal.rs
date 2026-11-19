//! What a removal would take with it. `pacman -Rns` also removes every dependency nothing else needs; one of them can be a
//! program the user relies on (flatpak was marked as a dependency of malcontent), so the Store shows the list first.

use std::process::Command;

pub struct RemovalPreview {
    /// Packages besides the requested one that the removal would take along.
    pub extras: Vec<String>,
    /// Installed Flatpak apps (only counted when flatpak itself is among the extras).
    pub flatpak_apps: usize,
}

pub fn parse_preview(output: &str, requested: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != requested && !line.contains(' '))
        .map(str::to_string)
        .collect()
}

fn flatpak_app_count() -> usize {
    let mut paths = vec![std::path::PathBuf::from("/var/lib/flatpak/app")];
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(std::path::PathBuf::from(home).join(".local/share/flatpak/app"));
    }
    paths
        .iter()
        .filter_map(|path| std::fs::read_dir(path).ok())
        .map(|entries| entries.filter_map(Result::ok).count())
        .sum()
}

/// Read-only: `pacman -Rs --print` needs no root (-n cannot be combined with --print; it only affects config backups).
/// If the preview cannot be made the list is empty and the caller keeps the safer behaviour of its own.
pub fn removal_preview(name: &str) -> RemovalPreview {
    let output = Command::new("pacman")
        .args(["-Rs", "--print", "--print-format", "%n", name])
        .env("LC_ALL", "C")
        .output();
    let extras = match output {
        Ok(out) if out.status.success() => parse_preview(&String::from_utf8_lossy(&out.stdout), name),
        _ => Vec::new(),
    };
    let flatpak_apps = if extras.iter().any(|package| package == "flatpak") { flatpak_app_count() } else { 0 };
    RemovalPreview { extras, flatpak_apps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_other_packages_are_extras() {
        let out = "malcontent\nlibgsystemservice\nflatpak\nostree\ncomposefs\n";
        assert_eq!(parse_preview(out, "malcontent"), vec!["libgsystemservice", "flatpak", "ostree", "composefs"]);
        assert!(parse_preview("vlc\n", "vlc").is_empty());
        assert!(parse_preview("", "vlc").is_empty());
    }

    #[test]
    fn messages_are_not_taken_for_packages() {
        assert!(parse_preview("error: target not found: x\n", "x").is_empty());
    }
}

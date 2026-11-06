//! Apps picked in the installer that were not part of the offline vault.
//!
//! The installer stores only app IDs (`/var/lib/windoesnot/pending-apps.json`); each ID is
//! resolved here against the signed catalogue shipped with the desktop, so nothing
//! user-supplied ever becomes a package name. Flatpak apps are normally installed by the
//! `windoesnot-pending-apps` session service; repository and AUR apps wait for the user here,
//! because they need a password (and, for AUR, the PKGBUILD review).
use std::path::PathBuf;

use serde_json::Value;

use super::models::PackageSource;

/// One app that is still waiting, with the actions that install it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingApp {
    pub id: String,
    pub name: String,
    /// (package or Flatpak id, source, Flatpak origin).
    pub actions: Vec<(String, PackageSource, Option<String>)>,
}

fn env_path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(default))
}

fn state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(dir).join("windoesnot");
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state/windoesnot")
}

fn read_json(path: &PathBuf) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Same character set the catalogue validator allows; anything else is dropped.
fn safe_ref(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c))
        && !value.starts_with(['-', '.'])
}

/// Waiting apps from the given files (testable), without the installed check.
pub fn load_from(pending: &PathBuf, catalog: &PathBuf, done: &PathBuf, german: bool) -> Vec<PendingApp> {
    let Some(wanted) = read_json(pending) else { return Vec::new() };
    let Some(catalog) = read_json(catalog) else { return Vec::new() };
    let finished: Vec<String> = read_json(done)
        .and_then(|v| v.get("done").cloned())
        .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()))
        .unwrap_or_default();
    let apps = catalog.get("apps").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut result = Vec::new();
    for id in wanted.get("pending").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if finished.iter().any(|f| f == id) {
            continue;
        }
        let Some(app) = apps.iter().find(|a| a.get("id").and_then(Value::as_str) == Some(id)) else { continue };
        let name = app["name"][if german { "de" } else { "en" }].as_str().unwrap_or(id).to_string();
        let mut actions = Vec::new();
        match app.get("source").and_then(Value::as_str).unwrap_or("offline") {
            "flatpak" => {
                if let Some(flatpak) = app.get("flatpak_id").and_then(Value::as_str).filter(|v| safe_ref(v)) {
                    actions.push((flatpak.to_string(), PackageSource::Flatpak, Some("flathub".to_string())));
                }
            }
            "repo" => {
                for package in app.get("repo_packages").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    if safe_ref(package) {
                        actions.push((package.to_string(), PackageSource::Repo, None));
                    }
                }
            }
            "aur" => {
                if let Some(package) = app.get("aur_package").and_then(Value::as_str).filter(|v| safe_ref(v)) {
                    actions.push((package.to_string(), PackageSource::Aur, None));
                }
            }
            _ => {}
        }
        if !actions.is_empty() {
            result.push(PendingApp { id: id.to_string(), name, actions });
        }
    }
    result
}

/// Waiting apps of this installation (default paths, overridable for tests).
pub fn load(german: bool) -> Vec<PendingApp> {
    load_from(
        &env_path("WINDOESNOT_PENDING", "/var/lib/windoesnot/pending-apps.json"),
        &env_path("WINDOESNOT_CATALOG", "/usr/share/windoesnot/iso/software/catalog.json"),
        &state_dir().join("pending-apps-done.json"),
        german,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_fixed_references_and_skips_done_apps() {
        let dir = std::env::temp_dir().join(format!("wdn-pending-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pending, catalog, done) = (dir.join("p.json"), dir.join("c.json"), dir.join("d.json"));
        std::fs::write(&catalog, r#"{"apps":[
            {"id":"ollama","name":{"de":"Ollama","en":"Ollama"},"source":"repo","repo_packages":["ollama"]},
            {"id":"codex-cli","name":{"de":"Codex","en":"Codex CLI"},"source":"aur","aur_package":"openai-codex-bin"},
            {"id":"chrome","name":{"de":"Chrome","en":"Chrome"},"source":"flatpak","flatpak_id":"com.google.Chrome"},
            {"id":"evil","name":{"de":"x","en":"x"},"source":"repo","repo_packages":["--noconfirm; rm"]},
            {"id":"firefox","name":{"de":"Firefox","en":"Firefox"},"source":"offline"}]}"#).unwrap();
        std::fs::write(&pending, r#"{"pending":["ollama","codex-cli","chrome","evil","firefox","unknown"]}"#).unwrap();
        std::fs::write(&done, r#"{"done":["chrome"]}"#).unwrap();
        let apps = load_from(&pending, &catalog, &done, false);
        let names: Vec<_> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(names, ["ollama", "codex-cli"]);
        assert_eq!(apps[1].actions, vec![("openai-codex-bin".to_string(), PackageSource::Aur, None)]);
        std::fs::remove_dir_all(dir).ok();
    }
}

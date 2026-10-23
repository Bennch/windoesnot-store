use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use directories::ProjectDirs;

use crate::core::models::Settings;

pub fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("me", "windoesnot", "windoesnot-store").expect("Project dirs")
}

pub fn cache_dir() -> PathBuf {
    project_dirs().cache_dir().to_path_buf()
}

pub fn screenshots_dir() -> PathBuf {
    cache_dir().join("screenshots")
}

pub fn config_dir() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}

pub fn load_settings() -> Settings {
    let path = config_dir().join("settings.json");
    let mut settings: Settings = fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default();
    if let Some(helper) = settings.aur_helper.installed() {
        settings.aur_helper = helper;
    }
    settings
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("settings.json");
    let data = serde_json::to_string_pretty(settings)?;
    fs::write(path, data)?;
    Ok(())
}

/// This command is deliberately separate from update checking: toggling AUR
/// only changes a user preference, never refreshes pacman's system database.
pub fn aur_cli_value(args: &[String]) -> Result<Option<bool>> {
    if !args.iter().any(|arg| matches!(arg.as_str(), "--set-aur" | "--enable-aur" | "--disable-aur")) {
        return Ok(None);
    }
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--enable-aur"] | ["--set-aur", "true" | "1" | "on"] => Ok(Some(true)),
        ["--disable-aur"] | ["--set-aur", "false" | "0" | "off"] => Ok(Some(false)),
        _ => anyhow::bail!("Usage: windoesnot-store --set-aur true|false"),
    }
}

pub fn set_aur_enabled(enabled: bool) -> Result<()> {
    update_aur_file(&config_dir().join("settings.json"), enabled)
}

fn update_aur_file(path: &std::path::Path, enabled: bool) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut value = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error.into()),
    };
    let object = value.as_object_mut().ok_or_else(|| anyhow::anyhow!("Settings must be a JSON object"))?;
    object.insert("aur_enabled".into(), enabled.into());
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("aur-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec_pretty(&value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

pub fn watch_aur_setting(directory: &std::path::Path, changed: impl Fn(bool) + 'static) -> Result<gio::FileMonitor> {
    use gio::prelude::*;
    fs::create_dir_all(directory)?;
    let monitor = gio::File::for_path(directory).monitor_directory(
        gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)?;
    let path = directory.join("settings.json");
    monitor.connect_changed(move |_, _, _, _| {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                changed(value.get("aur_enabled").and_then(|v| v.as_bool()).unwrap_or(false));
            }
        }
    });
    Ok(monitor)
}

#[cfg(test)]
mod aur_settings_tests {
    use super::*;

    #[test]
    fn running_store_observes_atomic_external_toggle() {
        use gio::prelude::*;
        use std::cell::Cell;
        use std::rc::Rc;
        let dir = std::env::temp_dir().join(format!("wdn-aur-watch-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        update_aur_file(&path, false).unwrap();
        let context = glib::MainContext::new();
        context.with_thread_default(|| {
            let seen = Rc::new(Cell::new(false));
            let callback = seen.clone();
            let monitor = watch_aur_setting(&dir, move |value| callback.set(value)).unwrap();
            update_aur_file(&path, true).unwrap();
            let start = Instant::now();
            while !seen.get() && start.elapsed() < Duration::from_secs(3) {
                context.iteration(false);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(seen.get(), "External atomic update must reach the running Store");
            monitor.cancel();
        }).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_aur_arguments_only() {
        for value in ["true", "1", "on"] {
            assert_eq!(aur_cli_value(&["--set-aur".into(), value.into()]).unwrap(), Some(true));
        }
        assert_eq!(aur_cli_value(&["--disable-aur".into()]).unwrap(), Some(false));
        for args in [vec!["--set-aur"], vec!["--set-aur", "typo"], vec!["--enable-aur", "--disable-aur"]] {
            assert!(aur_cli_value(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_err());
        }
        assert!(!Settings::default().aur_enabled);
        let old: Settings = serde_json::from_str(r#"{"theme":"Dark"}"#).unwrap();
        assert!(!old.aur_enabled);
    }

    #[test]
    fn aur_toggle_preserves_other_settings_and_rejects_corruption() {
        let dir = std::env::temp_dir().join(format!("wdn-aur-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"theme":"Dark","custom":{"keep":42},"aur_enabled":false}"#).unwrap();
        update_aur_file(&path, true).unwrap();
        let data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(data["aur_enabled"], true);
        assert_eq!(data["custom"]["keep"], 42);
        assert_eq!(data["theme"], "Dark");
        fs::write(&path, "broken json").unwrap();
        assert!(update_aur_file(&path, false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken json");
        fs::remove_dir_all(dir).unwrap();
    }
}

pub fn ensure_cache_dirs() -> Result<()> {
    fs::create_dir_all(screenshots_dir())?;
    Ok(())
}

pub fn clear_screenshots_cache() -> Result<()> {
    let dir = screenshots_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(())
}

pub fn find_logo_path() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("assets/logo.svg"),
        PathBuf::from("assets/logo.png"),
        PathBuf::from("/usr/share/aurora/assets/logo.svg"),
        PathBuf::from("/usr/share/aurora/assets/logo.png"),
        PathBuf::from("/usr/share/icons/hicolor/scalable/apps/io.github.ahmoodio.aurora.svg"),
        PathBuf::from("/usr/share/icons/hicolor/scalable/apps/io.github.ahmoodio.aurora.png"),
        PathBuf::from("/usr/share/icons/hicolor/256x256/apps/io.github.ahmoodio.aurora.png"),
    ];

    for path in candidates {
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// TTL-based cache for the set of installed package names from a given
/// provider. `pacman -Q` and `flatpak list` are both surprisingly slow
/// (the latter is ~1-2s even on a warm daemon), and the installed set
/// changes at most once per second in practice. Caching cuts a second
/// or two off every keystroke-driven search.
#[derive(Default)]
pub struct InstalledCache {
    inner: Mutex<Option<CacheEntry>>,
    ttl: Duration,
}

struct CacheEntry {
    fetched_at: Instant,
    names: HashSet<String>,
}

impl InstalledCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(None),
            ttl,
        }
    }

    /// Returns a fresh installed-set if the cache is stale (or empty);
    /// otherwise returns a clone of the cached set.
    pub fn get_or_refresh<F>(&self, loader: F) -> HashSet<String>
    where
        F: FnOnce() -> Result<HashSet<String>>,
    {
        if let Some(entry) = self.inner.lock().unwrap().as_ref() {
            if entry.fetched_at.elapsed() < self.ttl {
                return entry.names.clone();
            }
        }
        let names = match loader() {
            Ok(set) => set,
            Err(_) => HashSet::new(),
        };
        *self.inner.lock().unwrap() = Some(CacheEntry {
            fetched_at: Instant::now(),
            names: names.clone(),
        });
        names
    }

    pub fn invalidate(&self) {
        *self.inner.lock().unwrap() = None;
    }
}

/// True if the given binary exists on PATH and is executable.
pub fn command_exists(name: &str) -> bool {
    if name.is_empty() || name.contains('/') {
        return false;
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(name);
        let Ok(meta) = std::fs::metadata(&candidate) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

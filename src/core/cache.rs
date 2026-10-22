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

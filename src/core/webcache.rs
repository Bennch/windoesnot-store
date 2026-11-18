//! Small on-disk cache for public JSON lists (Flathub collections, pkgstats) to save data and start faster.
//! Fresh entries are used without a request; when the network fails an older entry is still better than nothing.
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn cache_file(url: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(base.join("windoesnot-store/web").join(format!("{:016x}.json", fnv1a(url))))
}

/// FNV-1a: stable across runs (std's hasher is not).
pub fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3))
}

pub fn age(path: &std::path::Path) -> Option<Duration> {
    SystemTime::now().duration_since(std::fs::metadata(path).ok()?.modified().ok()?).ok()
}

/// JSON from `url`, cached for `ttl`.
pub fn get_json(url: &str, ttl: Duration, timeout: Duration) -> Option<serde_json::Value> {
    let path = cache_file(url);
    let read = |p: &PathBuf| std::fs::read_to_string(p).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    if let Some(p) = &path {
        if age(p).map_or(false, |a| a < ttl) {
            if let Some(json) = read(p) {
                return Some(json);
            }
        }
    }
    let fetched = ureq::get(url).timeout(timeout).call().ok()
        .and_then(|response| response.into_string().ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok().map(|json| (text, json)));
    match fetched {
        Some((text, json)) => {
            if let Some(p) = &path {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(p, text);
            }
            Some(json)
        }
        None => path.as_ref().and_then(read),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        assert_eq!(fnv1a("a"), 0xaf63dc4c8601ec8c);
        assert_ne!(fnv1a("https://x/1"), fnv1a("https://x/2"));
    }
}

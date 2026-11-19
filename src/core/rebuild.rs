//! CachyOS ships CPU-optimised rebuilds (`-v3`) of nearly every package: same upstream version, the
//! package release gains a `.N` suffix (`0.57.2-1` -> `0.57.2-1.1`). They are not news for the user.

/// `(epoch+version, pkgrel)`: everything before the last dash and the part after it.
fn split(version: &str) -> (&str, &str) {
    version.rsplit_once('-').unwrap_or((version, ""))
}

/// True when `new` is the same upstream version as `old` and only the release gained `.N` digits.
pub fn is_cpu_rebuild(old: &str, new: &str) -> bool {
    let ((old_version, old_release), (new_version, new_release)) = (split(old), split(new));
    if old_version != new_version || old_release.is_empty() || new_release.len() <= old_release.len() {
        return false;
    }
    match new_release.strip_prefix(old_release).and_then(|rest| rest.strip_prefix('.')) {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// One `pacman -Qu` line: `name old -> new` (an optional `[ignored]` suffix is fine).
pub fn is_cpu_rebuild_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let (Some(_), Some(old), Some("->"), Some(new)) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    is_cpu_rebuild(old, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_dot_suffix_on_the_release_counts() {
        assert!(is_cpu_rebuild("0.57.2-1", "0.57.2-1.1"));
        assert!(is_cpu_rebuild("1:2.4-3", "1:2.4-3.2"));
        assert!(!is_cpu_rebuild("0.57.2-1", "0.57.3-1.1"));
        assert!(!is_cpu_rebuild("0.57.2-1", "0.57.2-2"));
        assert!(!is_cpu_rebuild("0.57.2-1.1", "0.57.2-1"));
        assert!(!is_cpu_rebuild("0.57.2-1", "0.57.2-1.x"));
        assert!(!is_cpu_rebuild("2-1", "2-1."));
    }

    #[test]
    fn pacman_lines_are_parsed() {
        assert!(is_cpu_rebuild_line("quickshell 0.3.2-1 -> 0.3.2-1.1"));
        assert!(is_cpu_rebuild_line("quickshell 0.3.2-1 -> 0.3.2-1.1 [ignored]"));
        assert!(!is_cpu_rebuild_line("firefox 150.0-1 -> 151.0-1"));
        assert!(!is_cpu_rebuild_line("garbage"));
    }
}

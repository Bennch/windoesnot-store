//! Older versions of installed repository packages that are still in the pacman cache.
//! Pure parsing and ordering live here; the transaction itself goes through the privileged helper (`pacman -U`).
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const CACHE_DIR: &str = "/var/cache/pacman/pkg";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub version: String,
    pub path: PathBuf,
}

/// `name-[epoch:]version-release-arch.pkg.tar.zst` -> (name, version-with-release). Epoch may be percent-encoded.
pub fn parse_filename(file: &str) -> Option<(String, String)> {
    let base = [".pkg.tar.zst", ".pkg.tar.xz", ".pkg.tar.gz", ".pkg.tar"]
        .iter()
        .find_map(|ext| file.strip_suffix(ext))?;
    let base = base.replace("%3A", ":").replace("%3a", ":");
    let (rest, arch) = base.rsplit_once('-')?;
    if arch.is_empty() || arch.contains(':') {
        return None;
    }
    let (rest, release) = rest.rsplit_once('-')?;
    let (name, version) = rest.rsplit_once('-')?;
    if name.is_empty() || version.is_empty() || release.is_empty() {
        return None;
    }
    Some((name.to_string(), format!("{version}-{release}")))
}

fn split_evr(v: &str) -> (u64, &str, Option<&str>) {
    let (epoch, rest) = match v.split_once(':') {
        Some((e, r)) if !e.is_empty() && e.chars().all(|c| c.is_ascii_digit()) => (e.parse().unwrap_or(0), r),
        _ => (0, v),
    };
    match rest.rsplit_once('-') {
        Some((ver, rel)) => (epoch, ver, Some(rel)),
        None => (epoch, rest, None),
    }
}

/// Port of libalpm's rpmvercmp (same control flow, so edge cases match `vercmp`).
fn rpmvercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut one, mut two) = (0usize, 0usize);
    let (mut p1, mut p2) = (0usize, 0usize);
    while one < a.len() && two < b.len() {
        while one < a.len() && !a[one].is_ascii_alphanumeric() { one += 1; }
        while two < b.len() && !b[two].is_ascii_alphanumeric() { two += 1; }
        if one >= a.len() || two >= b.len() { break; }
        if one - p1 != two - p2 {
            return if one - p1 < two - p2 { Ordering::Less } else { Ordering::Greater };
        }
        p1 = one;
        p2 = two;
        let isnum = a[p1].is_ascii_digit();
        if isnum {
            while p1 < a.len() && a[p1].is_ascii_digit() { p1 += 1; }
            while p2 < b.len() && b[p2].is_ascii_digit() { p2 += 1; }
        } else {
            while p1 < a.len() && a[p1].is_ascii_alphabetic() { p1 += 1; }
            while p2 < b.len() && b[p2].is_ascii_alphabetic() { p2 += 1; }
        }
        if one == p1 {
            return Ordering::Less;
        }
        if two == p2 {
            return if isnum { Ordering::Greater } else { Ordering::Less };
        }
        let (x, y) = (&a[one..p1], &b[two..p2]);
        let ord = if isnum {
            let (x, y) = (trim_zeros(x), trim_zeros(y));
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        } else {
            x.cmp(y)
        };
        if ord != Ordering::Equal {
            return ord;
        }
        one = p1;
        two = p2;
    }
    if one >= a.len() && two >= b.len() {
        return Ordering::Equal;
    }
    let alpha = |s: &[u8], i: usize| i < s.len() && s[i].is_ascii_alphabetic();
    if (one >= a.len() && !alpha(b, two)) || alpha(a, one) { Ordering::Less } else { Ordering::Greater }
}

fn trim_zeros(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|c| **c == b'0').count();
    &s[n..]
}

/// pacman's vercmp for `[epoch:]version[-release]` strings.
pub fn vercmp(a: &str, b: &str) -> Ordering {
    let (ea, va, ra) = split_evr(a);
    let (eb, vb, rb) = split_evr(b);
    ea.cmp(&eb)
        .then_with(|| rpmvercmp(va, vb))
        .then_with(|| match (ra, rb) {
            (Some(x), Some(y)) => rpmvercmp(x, y),
            _ => Ordering::Equal,
        })
}

/// Cached archives of `package` that are strictly older than `installed`, newest first.
pub fn older_candidates(package: &str, installed: &str, files: &[(String, PathBuf)]) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = files
        .iter()
        .filter_map(|(file, path)| {
            let (name, version) = parse_filename(file)?;
            (name == package && vercmp(&version, installed) == Ordering::Less)
                .then(|| Candidate { version, path: path.clone() })
        })
        .collect();
    found.sort_by(|a, b| vercmp(&b.version, &a.version));
    found.dedup_by(|a, b| a.version == b.version);
    found
}

pub fn read_cache(dir: &Path) -> Vec<(String, PathBuf)> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let file = e.file_name().to_string_lossy().to_string();
                    let path = e.path();
                    (path.is_file() && !file.ends_with(".sig")).then_some((file, path))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn installed_version(package: &str) -> Option<String> {
    let out = std::process::Command::new("pacman").args(["-Q", package]).env("LC_ALL", "C").output().ok()?;
    if !out.status.success() { return None; }
    String::from_utf8(out.stdout).ok()?.split_whitespace().nth(1).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cache_file_names() {
        assert_eq!(parse_filename("hyprland-0.57.2-1-x86_64.pkg.tar.zst"), Some(("hyprland".into(), "0.57.2-1".into())));
        assert_eq!(parse_filename("lib32-libffi-3.8.0-1.1-x86_64.pkg.tar.zst"), Some(("lib32-libffi".into(), "3.8.0-1.1".into())));
        assert_eq!(parse_filename("vim-runtime-1%3A9.1-2-any.pkg.tar.xz"), Some(("vim-runtime".into(), "1:9.1-2".into())));
        assert_eq!(parse_filename("foo-1.0-1-x86_64.pkg.tar.zst.sig"), None);
        assert_eq!(parse_filename("notapackage.txt"), None);
    }

    #[test]
    fn vercmp_matches_pacman() {
        let l = Ordering::Less;
        assert_eq!(vercmp("1.0-1", "1.0-2"), l);
        assert_eq!(vercmp("1.0-1", "1.0-1.1"), l);
        assert_eq!(vercmp("160.0-1.1", "160.0.1-1"), l);
        assert_eq!(vercmp("2.0-1", "1:1.0-1"), l);
        assert_eq!(vercmp("1.0a-1", "1.0-1"), l);
        assert_eq!(vercmp("1.0-1", "1.0.1-1"), l);
        assert_eq!(vercmp("1.0rc1-1", "1.0-1"), l);
        assert_eq!(vercmp("1.10-1", "1.9-1"), Ordering::Greater);
        assert_eq!(vercmp("1.01-1", "1.1-1"), Ordering::Equal);
        assert_eq!(vercmp("7.3.3-1", "7.3.3-1"), Ordering::Equal);
    }

    #[test]
    fn only_older_versions_sorted_newest_first() {
        let files: Vec<(String, PathBuf)> = ["pkg-1.0-1", "pkg-1.2-1", "pkg-1.1-3", "pkg-1.1-2", "other-0.1-1", "pkg-1.3-1"]
            .iter()
            .map(|n| (format!("{n}-x86_64.pkg.tar.zst"), PathBuf::from(format!("/var/cache/pacman/pkg/{n}-x86_64.pkg.tar.zst"))))
            .collect();
        let found = older_candidates("pkg", "1.2-1", &files);
        let versions: Vec<&str> = found.iter().map(|c| c.version.as_str()).collect();
        assert_eq!(versions, ["1.1-3", "1.1-2", "1.0-1"]);
    }

    /// Compares against the real `vercmp` (run with `cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn agrees_with_system_vercmp() {
        let samples = ["1.0-1", "1.0-1.1", "1.0.1-1", "1.0a-1", "1.0rc1-1", "1.10-1", "1.9-1", "1.01-1", "2:0.1-1", "1:9.9-1", "160.0-1.1", "160.0.1-1",
            "6.18.60-2", "7.3.3-1", "7.3.4-1", "0.57.1-1", "0.57.2-1", "3.8.0-1.1", "3.8.1-1", "1.0_beta-1", "1.0+git20260101-1", "20260101-1", "r123.abc-1", "2.47-4", "2.47-2", "1.2.3.4-5", "1.2.3-5.1"];
        for a in samples { for b in samples {
            let out = std::process::Command::new("vercmp").args([a, b]).output().unwrap();
            let expected = String::from_utf8(out.stdout).unwrap().trim().parse::<i32>().unwrap();
            let got = match vercmp(a, b) { Ordering::Less => -1, Ordering::Equal => 0, Ordering::Greater => 1 };
            assert_eq!(got, expected, "{a} vs {b}");
        }}
    }
}

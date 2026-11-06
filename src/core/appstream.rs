use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

use anyhow::{anyhow, Result};
use serde_json::Value;
use url::Url;

use crate::core::cache::{ensure_cache_dirs, screenshots_dir};

#[derive(Debug, Clone)]
pub struct AppStreamComponent {
    pub id: String,
    pub summary: Option<String>,
    pub icon_name: Option<String>,
    pub screenshots: Vec<String>,
}

/// What Flathub knows about an app; `description` is already Pango markup.
#[derive(Debug, Clone, Default)]
pub struct FlathubInfo {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub developer: Option<String>,
    pub categories: Vec<String>,
    pub screenshots: Vec<String>,
    /// The official logo, downloaded to the cache.
    pub icon_path: Option<PathBuf>,
}

/// AppStream descriptions (p, ul/ol/li, em, code) as Pango markup.
pub fn appstream_markup(html: &str) -> String {
    let mut out = String::new();
    let mut ordered: Option<u32> = None;
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&text_chunk(&rest[..start]));
        let Some(end) = rest[start..].find('>') else { break };
        let tag = rest[start + 1..start + end].trim().to_lowercase();
        let name = tag.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_string();
        let closing = tag.starts_with('/');
        match (name.as_str(), closing) {
            ("p", true) => out.push_str("\n\n"),
            ("ul", false) => ordered = None,
            ("ol", false) => ordered = Some(0),
            ("ul", true) | ("ol", true) => {
                ordered = None;
                out.push('\n');
            }
            ("li", false) => match ordered.as_mut() {
                Some(n) => {
                    *n += 1;
                    out.push_str(&format!("  {n}. "));
                }
                None => out.push_str("  \u{2022} "),
            },
            ("li", true) => out.push('\n'),
            ("em", false) | ("i", false) => out.push_str("<i>"),
            ("em", true) | ("i", true) => out.push_str("</i>"),
            ("b", false) | ("strong", false) => out.push_str("<b>"),
            ("b", true) | ("strong", true) => out.push_str("</b>"),
            ("code", false) => out.push_str("<tt>"),
            ("code", true) => out.push_str("</tt>"),
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(&text_chunk(rest));
    let mut cleaned = out.trim().to_string();
    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }
    cleaned
}

/// Text between tags: collapse whitespace, decode the few entities, escape for Pango.
fn text_chunk(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }
    let decoded = collapsed
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let escaped = decoded.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    // Whitespace next to a tag was trimmed by split_whitespace; keep a separating space.
    let lead = raw.starts_with(char::is_whitespace) && !raw.trim().is_empty();
    let trail = raw.ends_with(char::is_whitespace);
    format!("{}{}{}", if lead { " " } else { "" }, escaped, if trail { " " } else { "" })
}

#[derive(Debug, Clone, Default)]
pub struct AppStreamClient;

impl AppStreamClient {
    pub fn search_component(&self, name: &str) -> Option<AppStreamComponent> {
        let output = Command::new("appstreamcli")
            .args(["search", name, "--format=json"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Self::component_from_json(&v);
        }
        Self::component_from_text(&text)
    }

    pub fn get_component(&self, id: &str) -> Option<AppStreamComponent> {
        let output = Command::new("appstreamcli")
            .args(["get", id, "--format=json"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Self::component_from_json(&v);
        }
        None
    }

    fn component_from_json(v: &Value) -> Option<AppStreamComponent> {
        let comps = v.get("components").and_then(|c| c.as_array())?;
        let comp = comps.first()?;
        let id = comp.get("id").and_then(|v| v.as_str())?.to_string();
        let summary = comp.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());

        let mut icon_name = None;
        if let Some(icons) = comp.get("icons").and_then(|v| v.as_array()) {
            for icon in icons {
                if let Some(name) = icon.get("name").and_then(|v| v.as_str()) {
                    icon_name = Some(name.to_string());
                    break;
                }
            }
        }

        let mut screenshots = Vec::new();
        if let Some(shots) = comp.get("screenshots").and_then(|v| v.as_array()) {
            for shot in shots {
                if let Some(images) = shot.get("images").and_then(|v| v.as_array()) {
                    if let Some(url) = images
                        .iter()
                        .filter_map(|img| img.get("url").and_then(|v| v.as_str()))
                        .next()
                    {
                        screenshots.push(url.to_string());
                    }
                }
            }
        }

        Some(AppStreamComponent {
            id,
            summary,
            icon_name,
            screenshots,
        })
    }

    fn component_from_text(text: &str) -> Option<AppStreamComponent> {
        for line in text.lines() {
            let line = line.trim();
            if line.contains('.') && (line.contains(".desktop") || line.contains(".metainfo")) {
                return Some(AppStreamComponent {
                    id: line.to_string(),
                    summary: None,
                    icon_name: None,
                    screenshots: Vec::new(),
                });
            }
        }
        None
    }

    /// Flathub's own appstream entry for an app id: the source for names,
    /// formatted descriptions and screenshots when local metadata is thin.
    /// None when the id is unknown to Flathub or the network is unavailable.
    pub fn flathub_info(app_id: &str) -> Option<FlathubInfo> {
        let id = app_id.trim_end_matches(".desktop");
        if id.is_empty() || id.contains('/') || !id.contains('.') {
            return None;
        }
        let url = format!("https://flathub.org/api/v2/appstream/{id}");
        let response = ureq::get(&url).timeout(std::time::Duration::from_secs(8)).call().ok()?;
        let json: Value = serde_json::from_str(&response.into_string().ok()?).ok()?;
        let text = |key: &str| json.get(key).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_owned);
        let screenshots = json
            .get("screenshots")
            .and_then(|v| v.as_array())
            .map(|shots| {
                shots
                    .iter()
                    .filter_map(|shot| {
                        shot.get("sizes")?
                            .as_array()?
                            .iter()
                            .filter_map(|size| {
                                let width: i64 = size.get("width")?.as_str()?.parse().ok()?;
                                Some(((width - 624).abs(), size.get("src")?.as_str()?.to_string()))
                            })
                            .min_by_key(|(distance, _)| *distance)
                            .map(|(_, src)| src)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(FlathubInfo {
            title: text("name"),
            summary: text("summary"),
            description: text("description").map(|html| appstream_markup(&html)).filter(|m| !m.is_empty()),
            homepage: json.get("urls").and_then(|u| u.get("homepage")).and_then(|v| v.as_str()).map(str::to_owned),
            license: text("project_license"),
            developer: text("developer_name"),
            categories: json
                .get("categories")
                .and_then(|v| v.as_array())
                .map(|list| list.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            screenshots,
            icon_path: json.get("icon").and_then(|v| v.as_str()).and_then(Self::ensure_cached),
        })
    }

    pub fn download_screenshots_async(&self, urls: Vec<String>) {
        let _ = ensure_cache_dirs();
        thread::spawn(move || {
            for url in urls {
                let _ = Self::download_one(&url);
            }
        });
    }

    pub fn cached_path_for_url(url: &str) -> Option<PathBuf> {
        let parsed = Url::parse(url).ok()?;
        // The whole path: Flathub names every app's first image "image-1_...png".
        let filename = parsed.path_segments()?.collect::<Vec<_>>().join("_");
        Some(screenshots_dir().join(filename))
    }

    pub fn ensure_cached(url: &str) -> Option<PathBuf> {
        let _ = ensure_cache_dirs();
        let path = Self::cached_path_for_url(url)?;
        if path.exists() {
            return Some(path);
        }
        if Self::download_one(url).is_ok() && path.exists() {
            return Some(path);
        }
        None
    }

    fn download_one(url: &str) -> Result<()> {
        let path = Self::cached_path_for_url(url).ok_or_else(|| anyhow!("invalid url"))?;
        if path.exists() {
            return Ok(());
        }
        let response = ureq::get(url).call()?;
        let mut reader = response.into_reader();
        let mut file = fs::File::create(path)?;
        let _ = std::io::copy(&mut reader, &mut file)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::appstream_markup;

    #[test]
    fn description_becomes_markup_with_lists() {
        let html = "<p>Audacity is <em>great</em> &amp; free.</p><p>You can:</p><ul><li>Record</li><li>Edit <code>WAV</code></li></ul>";
        assert_eq!(
            appstream_markup(html),
            "Audacity is <i>great</i> &amp; free.\n\nYou can:\n\n  \u{2022} Record\n  \u{2022} Edit <tt>WAV</tt>"
        );
    }
}

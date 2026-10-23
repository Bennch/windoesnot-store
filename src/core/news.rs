//! Arch Linux news before a system upgrade. Arch asks users to read the news
//! before updating, because some upgrades need a manual intervention. The
//! review dialog shows unread items; executing marks them as read.
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;

const FEED: &str = "https://archlinux.org/feeds/news/";
/// Older items were relevant to upgrades long past.
const MAX_AGE_DAYS: i64 = 45;
const MAX_ITEMS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    /// Days since 1970-01-01, None if the date could not be read.
    pub day: Option<i64>,
}

impl NewsItem {
    /// Arch marks these explicitly in the title.
    pub fn needs_intervention(&self) -> bool {
        let title = self.title.to_lowercase();
        title.contains("manual intervention") || title.contains("requires manual")
    }
}

fn seen_path() -> PathBuf {
    crate::core::cache::cache_dir().join("arch-news-seen.txt")
}

/// Unread recent items, newest first. Network errors are returned to the
/// caller, which shows a note but never blocks the upgrade.
pub fn unread() -> Result<Vec<NewsItem>> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).build();
    let feed = agent.get(FEED).call()?.into_string()?;
    let seen: HashSet<String> = std::fs::read_to_string(seen_path())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    Ok(select_unread(parse_feed(&feed), &seen, today()))
}

pub fn mark_read(items: &[NewsItem]) {
    if items.is_empty() {
        return;
    }
    let mut seen = std::fs::read_to_string(seen_path()).unwrap_or_default();
    for item in items {
        if !seen.lines().any(|line| line == item.link) {
            seen.push_str(&item.link);
            seen.push('\n');
        }
    }
    let _ = std::fs::create_dir_all(crate::core::cache::cache_dir());
    let _ = std::fs::write(seen_path(), seen);
}

fn today() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86_400).unwrap_or(0)
}

fn select_unread(items: Vec<NewsItem>, seen: &HashSet<String>, today: i64) -> Vec<NewsItem> {
    items
        .into_iter()
        .filter(|item| !seen.contains(&item.link))
        .filter(|item| item.day.is_none_or(|day| today - day <= MAX_AGE_DAYS))
        .take(MAX_ITEMS)
        .collect()
}

fn parse_feed(xml: &str) -> Vec<NewsItem> {
    xml.split("<item>")
        .skip(1)
        .filter_map(|item| {
            let title = unescape(between(item, "<title>", "</title>")?);
            let link = between(item, "<link>", "</link>")?.to_string();
            let day = between(item, "<pubDate>", "</pubDate>").and_then(parse_rfc2822_day);
            Some(NewsItem { title, link, day })
        })
        .collect()
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(text[from..to].trim())
}

fn unescape(text: &str) -> String {
    text.trim_start_matches("<![CDATA[")
        .trim_end_matches("]]>")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// "Sat, 21 Jun 2026 10:12:13 +0000" → days since 1970-01-01.
fn parse_rfc2822_day(date: &str) -> Option<i64> {
    let mut parts = date.split_whitespace().skip(1);
    let day: i64 = parts.next()?.parse().ok()?;
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        .iter()
        .position(|name| Some(*name) == parts.clone().next())? as i64
        + 1;
    parts.next();
    let year: i64 = parts.next()?.parse().ok()?;
    Some(days_from_civil(year, month, day))
}

/// Days since 1970-01-01 for a Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED_SAMPLE: &str = r#"<rss><channel><title>Arch Linux: Recent news updates</title>
<item><title>Active AUR malicious packages incident</title><link>https://archlinux.org/news/aur-incident/</link>
<pubDate>Tue, 16 Jun 2026 09:00:00 +0000</pubDate></item>
<item><title>linux-firmware &gt;= 20250508 upgrade requires manual intervention</title>
<link>https://archlinux.org/news/firmware/</link><pubDate>Mon, 01 Jun 2026 08:00:00 +0000</pubDate></item>
<item><title>Old news</title><link>https://archlinux.org/news/old/</link><pubDate>Fri, 02 Jan 2026 08:00:00 +0000</pubDate></item>
</channel></rss>"#;

    #[test]
    fn dates_are_counted_from_the_unix_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(parse_rfc2822_day("Tue, 16 Jun 2026 09:00:00 +0000"), Some(days_from_civil(2026, 6, 16)));
        assert_eq!(parse_rfc2822_day("garbage"), None);
    }

    #[test]
    fn feed_items_are_parsed_and_unescaped() {
        let items = parse_feed(FEED_SAMPLE);
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].title, "linux-firmware >= 20250508 upgrade requires manual intervention");
        assert!(items[1].needs_intervention());
        assert!(!items[0].needs_intervention());
    }

    #[test]
    fn only_recent_unread_items_are_shown() {
        let items = parse_feed(FEED_SAMPLE);
        let today = days_from_civil(2026, 6, 20);
        let seen: HashSet<String> = ["https://archlinux.org/news/aur-incident/".to_string()].into();
        let unread = select_unread(items, &seen, today);
        let titles: Vec<_> = unread.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["linux-firmware >= 20250508 upgrade requires manual intervention"]);
    }
}

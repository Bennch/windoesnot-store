use std::ffi::OsStr;
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

use crate::core::models::{PackageDetails, PackageSource, PackageSummary, Settings};
use crate::core::providers::AurProvider;

#[derive(Debug, Clone)]
pub struct Aur {
    settings: Arc<Mutex<Settings>>,
}

impl Aur {
    pub fn new(settings: Arc<Mutex<Settings>>) -> Self {
        Self { settings }
    }

    fn helper_bin(&self) -> Result<String> {
        self.settings.lock().unwrap().aur_helper.installed()
            .map(|helper| helper.as_str().to_string())
            .ok_or_else(|| anyhow!(crate::i18n::t("No AUR helper installed. Install paru or yay to use AUR.")))
    }

    fn run_capture<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let helper = self.helper_bin()?;
        let output = Command::new(&helper)
            .args(args)
            .env("LC_ALL", "C")
            .output()?;
        if !output.status.success() {
            return Err(anyhow!("{} failed with status {}", helper, output.status));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_search_output(output: &str) -> Vec<PackageSummary> {
        // yay/paru also list repo packages; those come from the pacman provider.
        crate::core::providers::parse_search_hits(output)
            .into_iter()
            .filter(|hit| hit.repo == "aur")
            .map(|hit| PackageSummary {
                name: hit.name,
                summary: hit.summary,
                version: hit.version,
                source: PackageSource::Aur,
                installed: false,
                origin: None,
            })
            .collect()
    }

    fn parse_info(output: &str) -> PackageDetails {
        let mut name = String::new();
        let mut version = String::new();
        let mut desc = String::new();
        let mut summary = String::new();
        let mut size = None;
        let mut home = None;

        for line in output.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let value = v.trim();
                match key {
                    "Name" => name = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Description" => {
                        desc = value.to_string();
                        summary = value.to_string();
                    }
                    "Installed Size" | "Download Size" => size = Some(value.to_string()),
                    "URL" => home = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        PackageDetails {
            name,
            summary,
            description: desc,
            version,
            source: PackageSource::Aur,
            installed: false,
            size,
            home,
            screenshots: Vec::new(),
            icon_name: None,
        }
    }
}

impl AurProvider for Aur {
    fn search(&self, query: &str) -> Result<Vec<PackageSummary>> {
        let mut args = vec!["-Ss".to_string()];
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        args.extend(terms);
        let output = self.run_capture(args)?;
        Ok(Self::parse_search_output(&output))
    }

    fn info(&self, name: &str) -> Result<PackageDetails> {
        let output = self.run_capture(["-Si", name])?;
        Ok(Self::parse_info(&output))
    }
}

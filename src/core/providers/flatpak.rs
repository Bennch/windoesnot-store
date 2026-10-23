use std::process::Command;

use anyhow::{anyhow, Result};

use crate::core::models::{PackageDetails, PackageSource, PackageSummary};
use crate::core::providers::FlatpakProvider;

#[derive(Debug, Default)]
pub struct Flatpak;

impl Flatpak {
    fn run_capture(args: &[&str]) -> Result<String> {
        let output = Command::new("flatpak")
            .args(args)
            .env("LC_ALL", "C")
            .output()?;
        if !output.status.success() {
            return Err(anyhow!("flatpak failed with status {}", output.status));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_search(output: &str) -> Vec<PackageSummary> {
        let mut results = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let app_id = cols.get(0).unwrap_or(&"").trim().to_string();
            if app_id.is_empty() {
                continue;
            }
            let summary = cols.get(1).unwrap_or(&"").trim().to_string();
            let version = cols.get(2).unwrap_or(&"").trim().to_string();
            let branch = cols.get(3).unwrap_or(&"").trim();
            let remote = cols.get(4).unwrap_or(&"").trim().to_string();
            let display_version = if !version.is_empty() {
                version
            } else {
                branch.to_string()
            };
            results.push(PackageSummary {
                name: app_id,
                summary,
                version: display_version,
                source: PackageSource::Flatpak,
                installed: false,
                origin: if remote.is_empty() { None } else { Some(remote) },
                installed_via: None,
            });
        }
        results
    }

    fn parse_list(output: &str) -> Vec<PackageSummary> {
        let mut results = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let app_id = cols.get(0).unwrap_or(&"").trim().to_string();
            if app_id.is_empty() {
                continue;
            }
            let summary = cols.get(1).unwrap_or(&"").trim().to_string();
            let version = cols.get(2).unwrap_or(&"").trim().to_string();
            let branch = cols.get(3).unwrap_or(&"").trim();
            let display_version = if !version.is_empty() {
                version
            } else {
                branch.to_string()
            };
            results.push(PackageSummary {
                name: app_id,
                summary,
                version: display_version,
                source: PackageSource::Flatpak,
                installed: true,
                origin: None,
                installed_via: Some(PackageSource::Flatpak),
            });
        }
        results
    }

    fn parse_info(output: &str) -> PackageDetails {
        let mut name = String::new();
        let mut version = String::new();
        let mut desc = String::new();
        let mut summary = String::new();
        let mut home = None;
        let mut size = None;

        for line in output.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let value = v.trim();
                match key {
                    "Name" | "Application" => name = value.to_string(),
                    "Summary" => summary = value.to_string(),
                    "Description" => desc = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Website" | "URL" => home = Some(value.to_string()),
                    "Installed Size" => size = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        if summary.is_empty() {
            summary = desc.clone();
        }

        PackageDetails {
            name,
            summary,
            description: desc,
            version,
            source: PackageSource::Flatpak,
            installed: true,
            size,
            home,
            screenshots: Vec::new(),
            icon_name: None,
        }
    }
}

impl FlatpakProvider for Flatpak {
    fn search(&self, query: &str) -> Result<Vec<PackageSummary>> {
        let output = Self::run_capture(&[
            "search",
            "--columns=application,description,version,branch,remote",
            query,
        ])?;
        Ok(Self::parse_search(&output))
    }

    fn info(&self, name: &str) -> Result<PackageDetails> {
        let output = Self::run_capture(&["info", name])?;
        Ok(Self::parse_info(&output))
    }

    fn list_installed(&self) -> Result<Vec<PackageSummary>> {
        let output =
            Self::run_capture(&["list", "--app", "--columns=application,description,version,branch"])?;
        Ok(Self::parse_list(&output))
    }
}

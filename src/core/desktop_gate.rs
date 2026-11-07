//! Check ABI pins from the local sync database before a full upgrade.
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub package: String,
    pub installed_version: String,
    pub target_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub owner: String,
    pub package: String,
    pub operator: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Ok,
    Blocked { package: String, target_version: String, reason: String },
}

pub fn parse_updates(output: &str) -> Vec<Update> {
    output.lines().filter_map(|line| {
        let mut words = line.split_whitespace();
        let (package, installed_version, arrow, target_version) =
            (words.next()?, words.next()?, words.next()?, words.next()?);
        (arrow == "->").then(|| Update {
            package: package.into(), installed_version: installed_version.into(), target_version: target_version.into(),
        })
    }).collect()
}

/// Only versioned dependencies emitted by `_abi` are relevant. libstdc++ is
/// provided by gcc-libs, so its version is checked against that package.
pub fn parse_dependencies(owner: &str, info: &str) -> Vec<Dependency> {
    let mut depends = String::new();
    let mut in_depends = false;
    for line in info.lines() {
        if let Some((key, value)) = line.split_once(':') {
            in_depends = key.trim() == "Depends On";
            if in_depends { depends.push_str(value); depends.push(' '); }
        } else if in_depends {
            depends.push_str(line);
            depends.push(' ');
        }
    }
    depends.split_whitespace().filter_map(|item| {
        let (package, operator, version) = if let Some((name, version)) = item.split_once(">=") {
            (name, ">=", version)
        } else if let Some((name, version)) = item.split_once('<') {
            (name, "<", version)
        } else { return None };
        let allowed = match owner {
            "windoesnot-plugins" => matches!(package, "hyprland" | "libstdc++"),
            "windoesnot-components" => matches!(package, "qt6-base" | "qtermwidget"),
            _ => false,
        };
        allowed.then(|| Dependency { owner: owner.into(), package: package.into(), operator: operator.into(), version: version.into() })
    }).collect()
}

pub fn check_with(updates: &[Update], dependencies: &[Dependency], mut vercmp: impl FnMut(&str, &str) -> i32) -> Gate {
    for update in updates {
        for dep in dependencies {
            let updated_package = if dep.package == "libstdc++" { "gcc-libs" } else { &dep.package };
            if update.package != updated_package { continue; }
            let order = vercmp(&update.target_version, &dep.version);
            let valid = match dep.operator.as_str() { ">=" => order >= 0, "<" => order < 0, _ => true };
            if !valid {
                return Gate::Blocked {
                    package: update.package.clone(), target_version: update.target_version.clone(),
                    reason: dep.owner.clone(),
                };
            }
        }
    }
    Gate::Ok
}

fn vercmp(a: &str, b: &str) -> i32 {
    Command::new("vercmp").args([a, b]).output().ok()
        .and_then(|o| o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).flatten())
        .unwrap_or(0)
}

pub fn check(updates: &[Update], dependencies: &[Dependency]) -> Gate {
    check_with(updates, dependencies, vercmp)
}

fn pacman_output(args: &[&str]) -> Option<String> {
    let output = Command::new("pacman").args(args).env("LC_ALL", "C").output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn offered_dependencies(owner: &str) -> Option<Vec<Dependency>> {
    let db = crate::core::syncdb::local_dbpath();
    let output = Command::new("pacman").args(["-Si", "--dbpath"]).arg(db).arg(owner)
        .env("LC_ALL", "C").output().ok()?;
    output.status.success().then(|| parse_dependencies(owner, &String::from_utf8_lossy(&output.stdout)))
}

pub fn pending_gate() -> Gate {
    let db = crate::core::syncdb::local_dbpath();
    let output = Command::new("pacman").args(["-Qu", "--dbpath"]).arg(db)
        .env("LC_ALL", "C").output();
    let Ok(output) = output else { return Gate::Ok };
    if !output.status.success() { return Gate::Ok; }
    let updates = parse_updates(&String::from_utf8_lossy(&output.stdout));
    let mut deps = Vec::new();
    // A components mismatch cannot be resolved by removing the plugins.
    for owner in ["windoesnot-components", "windoesnot-plugins"] {
        if pacman_output(&["-Qq", owner]).is_some() {
            if let Some(offered) = offered_dependencies(owner) { deps.extend(offered); }
        }
    }
    check(&updates, &deps)
}

pub fn plugin_can_install() -> bool {
    if pacman_output(&["-Qq", "windoesnot-plugins"]).is_some() { return false; }
    let Some(deps) = offered_dependencies("windoesnot-plugins") else { return false; };
    if deps.is_empty() { return false; }
    deps.iter().all(|dep| {
        let package = if dep.package == "libstdc++" { "gcc-libs" } else { &dep.package };
        let Some(installed) = pacman_output(&["-Q", package]) else { return false; };
        let Some(version) = installed.split_whitespace().nth(1) else { return false; };
        let order = vercmp(version, &dep.version);
        match dep.operator.as_str() { ">=" => order >= 0, "<" => order < 0, _ => false }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_outputs_and_wrapped_dependencies() {
        assert_eq!(parse_updates("hyprland 0.57-1 -> 0.58-1\n")[0].target_version, "0.58-1");
        let deps = parse_dependencies("windoesnot-plugins", "Depends On      : hyprland>=0.57-1  hyprland<0.57-2\n                  libstdc++>=15.2-1  libstdc++<15.2-2\nDescription     : x");
        assert_eq!(deps.len(), 4);
        assert_eq!(deps[2].package, "libstdc++");
    }

    #[test]
    fn blocks_new_release_and_checks_gcc_provider() {
        let deps = parse_dependencies("windoesnot-plugins", "Depends On : hyprland>=0.57-1 hyprland<0.57-2 libstdc++>=15.2-1 libstdc++<15.2-2");
        let cmp = |a: &str, b: &str| a.cmp(b) as i32;
        let hypr = parse_updates("hyprland 0.57-1 -> 0.58-1");
        assert!(matches!(check_with(&hypr, &deps, cmp), Gate::Blocked { package, .. } if package == "hyprland"));
        let gcc = parse_updates("gcc-libs 15.2-1 -> 15.2-2");
        assert!(matches!(check_with(&gcc, &deps, cmp), Gate::Blocked { package, .. } if package == "gcc-libs"));
    }

    #[test]
    fn cpu_suffix_within_abi_range_and_components_pin() {
        let deps = parse_dependencies("windoesnot-plugins", "Depends On : hyprland>=0.57-1 hyprland<0.57-2");
        let updates = parse_updates("hyprland 0.57-1 -> 0.57-1.1");
        assert_eq!(check(&updates, &deps), Gate::Ok);
        let qt = parse_dependencies("windoesnot-components", "Depends On : qt6-base>=6.9-1 qt6-base<6.9-2 qtermwidget>=2-1 qtermwidget<2-2");
        assert_eq!(qt.len(), 4);
        assert!(matches!(check(&parse_updates("qt6-base 6.9-1 -> 6.9-2"), &qt), Gate::Blocked { .. }));
    }
}

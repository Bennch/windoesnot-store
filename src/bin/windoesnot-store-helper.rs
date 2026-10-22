use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

fn main() {
    if let Err(err) = run() {
        eprintln!("windoesnot-store-helper error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    ensure_root()?;

    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err(anyhow!("no command supplied"));
    }

    let target = args.remove(0);
    match target.as_str() {
        "pacman" => run_pacman(args),
        "clear-pacman-lock" => clear_pacman_lock(),
        _ => Err(anyhow!("unsupported target: {target}")),
    }
}

fn ensure_root() -> Result<()> {
    let uid = unsafe { libc::geteuid() };
    if uid != 0 {
        return Err(anyhow!("must be run as root via pkexec"));
    }
    Ok(())
}

fn run_pacman(mut args: Vec<String>) -> Result<()> {
    // Some helper invocations include "pacman" twice:
    // windoesnot-store-helper pacman pacman -S ...
    if matches!(args.first(), Some(arg) if arg == "pacman") {
        args.remove(0);
    }

    let args = prepare_pacman_args(args)?;

    let status = Command::new("pacman")
        .args(&args)
        .env("LC_ALL", "C")
        .status()?;

    std::process::exit(status.code().unwrap_or(1));
}

fn prepare_pacman_args(mut args: Vec<String>) -> Result<Vec<String>> {
    // AUR helpers can request native dependencies through plain -S. Enforce
    // the same full-upgrade contract there, including callers older than the UI.
    if matches!(args.first().map(String::as_str), Some("-S" | "--sync")) {
        args[0] = "-Syu".to_string();
    }
    validate_pacman(&args)?;
    Ok(args)
}

fn clear_pacman_lock() -> Result<()> {
    ensure_no_package_manager_running()?;

    let lock_path = "/var/lib/pacman/db.lck";
    if !Path::new(lock_path).exists() {
        println!("No pacman lock file present.");
        return Ok(());
    }

    fs::remove_file(lock_path)?;
    println!("Removed stale pacman lock: {lock_path}");
    Ok(())
}

fn ensure_no_package_manager_running() -> Result<()> {
    let mut running = Vec::new();
    let candidates = ["pacman", "yay", "paru", "pamac", "pkcon", "packagekitd"];

    for proc_name in candidates {
        match Command::new("pgrep").arg("-x").arg(proc_name).status() {
            Ok(status) if status.success() => running.push(proc_name),
            Ok(_) => {}
            Err(err) => {
                return Err(anyhow!("failed to run pgrep safety check: {err}"));
            }
        }
    }

    if running.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "package manager appears active: {}",
            running.join(", ")
        ))
    }
}

fn validate_pacman(raw_args: &[String]) -> Result<()> {
    if raw_args.is_empty() {
        return Err(anyhow!("missing pacman args"));
    }

    // Normalize the operation:
    // 1. Map long-form ops (--remove → -R, --sync → -S, etc.)
    // 2. Split compound flags like -Rns into -R + separate -n -s flags
    let (op, args): (String, Vec<String>) = {
        let first = &raw_args[0];
        match first.as_str() {
            "--remove" => {
                let mut a: Vec<String> = vec!["-R".into()];
                a.extend_from_slice(&raw_args[1..]);
                ("-R".into(), a)
            }
            "--sync" => {
                let mut a: Vec<String> = vec!["-S".into()];
                a.extend_from_slice(&raw_args[1..]);
                ("-S".into(), a)
            }
            "--upgrade" => {
                let mut a: Vec<String> = vec!["-U".into()];
                a.extend_from_slice(&raw_args[1..]);
                ("-U".into(), a)
            }
            "--database" => {
                let mut a: Vec<String> = vec!["-D".into()];
                a.extend_from_slice(&raw_args[1..]);
                ("-D".into(), a)
            }
            s if s.starts_with("-R") && s.len() > 2 => {
                let mut a: Vec<String> = vec!["-R".into()];
                for ch in s[2..].chars() {
                    a.push(format!("-{ch}"));
                }
                a.extend_from_slice(&raw_args[1..]);
                ("-R".into(), a)
            }
            _ => (first.clone(), raw_args.to_vec()),
        }
    };

    let allowed_ops = ["-S", "-Syu", "-R", "-U", "-D"];
    if !allowed_ops.contains(&op.as_str()) {
        return Err(anyhow!("operation not allowed: {op}"));
    }

    let mut allow_flags = true;
    let mut pkgs = Vec::new();
    let mut pkg_files = Vec::new();
    let mut i = 1usize;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            allow_flags = false;
            i += 1;
            continue;
        }
        if arg.starts_with('-') {
            if !allow_flags {
                return Err(anyhow!("flags after packages are not allowed"));
            }

            if is_allowed_flag(arg) {
                i += 1;
                continue;
            }

            if flag_takes_value(arg) {
                let Some(value) = args.get(i + 1) else {
                    return Err(anyhow!("flag requires value: {arg}"));
                };
                if value.starts_with('-') {
                    return Err(anyhow!("flag requires value: {arg}"));
                }
                validate_flag_value(arg, value)?;
                i += 2;
                continue;
            }

            if let Some((flag, value)) = split_flag_value(arg) {
                validate_flag_value(flag, value)?;
                i += 1;
                continue;
            }

            if !is_allowed_flag(arg) {
                return Err(anyhow!("flag not allowed: {arg}"));
            }
        } else {
            allow_flags = false;
            if op == "-U" {
                if !is_safe_pkgfile(arg) {
                    return Err(anyhow!("invalid package file: {arg}"));
                }
                pkg_files.push(arg);
            } else {
                if !is_safe_pkg(arg) {
                    return Err(anyhow!("invalid package name: {arg}"));
                }
                pkgs.push(arg);
            }
        }
        i += 1;
    }

    if op == "-S" || op == "-R" || op == "-D" {
        if pkgs.is_empty() {
            return Err(anyhow!("no packages supplied"));
        }
    }

    if op == "-U" && pkg_files.is_empty() {
        return Err(anyhow!("no package files supplied"));
    }

    if pkgs.len() > 200 {
        return Err(anyhow!("too many packages"));
    }

    Ok(())
}

fn is_allowed_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--noconfirm"
            | "--needed"
            | "--noprogressbar"
            | "--asdeps"
            | "--asexplicit"
            | "--nosave"
            | "-n"
            | "--downloadonly"
            | "--print"
            | "--quiet"
            | "-q"
            | "--verbose"
            | "-v"
            // short forms of pacman flags that are safe within the
            // already-locked-down op whitelist (-S / -Syu / -R / -U / -D)
            | "-c" // --clean           (-S / -R)
            | "-u" // --upgrades / --unneeded (-S / -R)
            | "-s" // --search / --recursive  (-S / -R)
            | "-i" // --info            (-S / -R)
            | "-l" // --list            (-S)
            | "-g" // --groups          (-S)
            | "-y" // --refresh         (-S)
    )
}

fn flag_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--config"
            | "--color"
            | "--overwrite"
            | "--dbpath"
            | "--root"
            | "--sysroot"
            | "--cachedir"
            | "--logfile"
            | "--hookdir"
            | "--gpgdir"
            | "--arch"
            | "--print-format"
    )
}

fn split_flag_value(flag: &str) -> Option<(&str, &str)> {
    let (name, value) = flag.split_once('=')?;
    if !flag_takes_value(name) {
        return None;
    }
    Some((name, value))
}

fn validate_flag_value(flag: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(anyhow!("invalid value for {flag}"));
    }

    match flag {
        "--color" => {
            if !matches!(value, "always" | "auto" | "never") {
                return Err(anyhow!("invalid value for --color: {value}"));
            }
        }
        "--config" => {
            if value != "/etc/pacman.conf" {
                return Err(anyhow!("unsafe --config path: {value}"));
            }
        }
        // pacman runs as root here: caller-chosen roots, databases, caches, keyrings
        // or hook directories would let a caller run code as root. The store and
        // AUR helpers never need them.
        "--dbpath" | "--root" | "--sysroot" | "--cachedir" | "--logfile" | "--hookdir"
        | "--gpgdir" | "--arch" | "--overwrite" => {
            return Err(anyhow!("flag not allowed: {flag}"));
        }
        "--ignore" | "--ignore-group" => {
            // pacman accepts comma-separated package / group names. Reuse the
            // same character set as is_safe_pkg so a hostile value can't smuggle
            // in shell metacharacters if the helper is ever invoked from a
            // shell wrapper.
            for token in value.split(',') {
                if token.is_empty()
                    || token.len() > 128
                    || !token
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-._@".contains(c))
                {
                    return Err(anyhow!("invalid value for {flag}: {token}"));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn is_safe_pkg(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || "+-._@".contains(c))
}

fn is_safe_pkgfile(path: &str) -> bool {
    if path.is_empty() || path.len() > 512 {
        return false;
    }

    let mut candidate = Path::new(path).to_path_buf();
    if !candidate.is_absolute() {
        if let Ok(cwd) = env::current_dir() {
            candidate = cwd.join(candidate);
        }
    }

    let canon = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if !canon.is_file() {
        return false;
    }

    let allowed_prefixes = ["/home/", "/tmp/", "/var/cache/"];
    let canon_str = canon.to_string_lossy();
    if !allowed_prefixes.iter().any(|p| canon_str.starts_with(p)) {
        return false;
    }

    let name = canon.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let allowed_exts = [
        ".pkg.tar.zst",
        ".pkg.tar.xz",
        ".pkg.tar.gz",
        ".pkg.tar",
    ];
    allowed_exts.iter().any(|ext| name.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare(args: &[&str]) -> Result<Vec<String>> {
        prepare_pacman_args(args.iter().map(|arg| arg.to_string()).collect())
    }

    #[test]
    fn full_upgrade_accepts_package_targets() {
        assert!(prepare(&["-Syu", "--needed", "--noconfirm", "firefox", "vlc"]).is_ok());
        assert!(prepare(&["-Syu", "--noconfirm"]).is_ok());
    }

    #[test]
    fn legacy_and_aur_dependency_installs_are_upgraded_at_helper_boundary() {
        for op in ["-S", "--sync"] {
            let args = prepare(&[op, "--asdeps", "--needed", "gtk4"]).unwrap();
            assert_eq!(args[0], "-Syu");
            assert_eq!(args.last().unwrap(), "gtk4");
        }
    }

    #[test]
    fn refresh_without_full_upgrade_is_not_an_allowed_operation() {
        assert!(prepare(&["-Sy", "firefox"]).is_err());
        assert!(prepare(&["-Syy", "firefox"]).is_err());
    }

    #[test]
    fn dependency_and_package_ignore_bypasses_are_rejected() {
        for flag in ["--nodeps", "-d", "-dd", "--nocheck", "--ignore=hyprland", "--ignore-group=base", "--assume-installed=hyprland"] {
            assert!(prepare(&["-Syu", flag, "firefox"]).is_err(), "accepted {flag}");
        }
        assert!(prepare(&["-Syu", "--ignore", "hyprland", "firefox"]).is_err());
    }

    #[test]
    fn end_of_options_and_invalid_targets_cannot_inject_options() {
        assert!(prepare(&["-Syu", "--", "firefox"]).is_ok());
        assert!(prepare(&["-Syu", "--", "--nodeps"]).is_err());
        assert!(prepare(&["-Syu", "firefox", "--needed"]).is_err());
        assert!(prepare(&["-Syu", "firefox;reboot"]).is_err());
    }

    #[test]
    fn root_relocation_and_hook_flags_are_rejected() {
        for flag in ["--hookdir", "--root", "--sysroot", "--dbpath", "--cachedir", "--gpgdir", "--logfile", "--arch", "--overwrite"] {
            assert!(prepare(&["-Syu", flag, "/tmp/x", "firefox"]).is_err(), "accepted {flag}");
            assert!(prepare(&["-Syu", &format!("{flag}=/tmp/x"), "firefox"]).is_err(), "accepted {flag}=");
        }
        assert!(prepare(&["-Syu", "--config", "/etc/pacman.conf", "firefox"]).is_ok());
        assert!(prepare(&["-Syu", "--config", "/etc/pacman.d/evil.conf", "firefox"]).is_err());
    }

    #[test]
    fn normal_remove_and_dependency_marking_still_work() {
        assert!(prepare(&["-Rns", "--noconfirm", "vlc"]).is_ok());
        assert!(prepare(&["-D", "--asdeps", "gtk4"]).is_ok());
    }
}

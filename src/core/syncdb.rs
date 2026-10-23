//! A user-owned copy of the pacman sync databases for searching and update
//! checks (the principle of `checkupdates`). Refreshing it never touches
//! `/var/lib/pacman`, so searching cannot cause a partial upgrade. The system
//! databases are only refreshed by full upgrades (`-Syu`) through the helper.
//! A freshly installed WinDoesNot has no system sync data at all, so without
//! this copy the store could not search before the first update.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Result};

const SYSTEM_DB: &str = "/var/lib/pacman";
const MAX_AGE: Duration = Duration::from_secs(3 * 60 * 60);
static SYNC_LOCK: Mutex<()> = Mutex::new(());

pub fn dir() -> PathBuf {
    crate::core::cache::cache_dir().join("syncdb")
}

/// `--dbpath` for repository queries: the user copy once it holds sync data
/// (refreshed when older than three hours), otherwise the system databases.
pub fn query_dbpath() -> PathBuf {
    let copy = dir();
    if let Err(err) = refresh(&copy, Path::new(SYSTEM_DB), false) {
        eprintln!("windoesnot-store: search database not refreshed: {err}");
    }
    if has_sync_data(&copy) { copy } else { PathBuf::from(SYSTEM_DB) }
}

/// Refreshes the copy now, e.g. for an explicit update check.
pub fn refresh_now() -> Result<()> {
    refresh(&dir(), Path::new(SYSTEM_DB), true)
}

fn refresh(copy: &Path, system: &Path, force: bool) -> Result<()> {
    let _guard = SYNC_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    prepare(copy, system)?;
    if !force && is_fresh(&copy.join("sync"), MAX_AGE) {
        return Ok(());
    }
    // fakeroot lets pacman write the copy as the normal user, like checkupdates;
    // pacman 7's download sandbox cannot drop to its own user under fakeroot.
    let output = Command::new("fakeroot")
        .args(["--", "pacman", "-Sy", "--disable-sandbox-filesystem", "--dbpath"])
        .arg(copy)
        .args(["--logfile", "/dev/null"])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(anyhow!("{}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(())
}

/// Creates the copy: installed state comes from the real local database, and
/// existing system sync data seeds the first refresh so less is downloaded.
fn prepare(copy: &Path, system: &Path) -> Result<()> {
    let sync = copy.join("sync");
    fs::create_dir_all(&sync)?;
    let local = copy.join("local");
    if fs::symlink_metadata(&local).is_err() {
        std::os::unix::fs::symlink(system.join("local"), &local)?;
    }
    let seed = !has_sync_data(copy);
    if let Ok(entries) = fs::read_dir(system.join("sync")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if seed && name.ends_with(".db") {
                let _ = fs::copy(&path, sync.join(&name));
            }
            // Repositories with required database signatures need the .sig next
            // to the database; pacman does not refetch it for an unchanged
            // database, and one invalid database fails every query. Take the
            // matching pair from the system when the copy lacks the signature.
            if name.ends_with(".db.sig") && !sync.join(&name).exists() {
                let database = name.trim_end_matches(".sig");
                let _ = fs::copy(system.join("sync").join(database), sync.join(database));
                let _ = fs::copy(&path, sync.join(&name));
            }
        }
    }
    Ok(())
}

/// When package data was last downloaded: the newer of the user copy and the
/// system databases. None on a fresh system that never went online.
pub fn last_refreshed() -> Option<SystemTime> {
    [dir().join("sync"), Path::new(SYSTEM_DB).join("sync")]
        .iter()
        .flat_map(|sync| database_files(sync).collect::<Vec<_>>())
        .filter_map(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .max()
}

fn has_sync_data(copy: &Path) -> bool {
    database_files(&copy.join("sync")).next().is_some()
}

fn is_fresh(sync: &Path, max_age: Duration) -> bool {
    let newest = database_files(sync)
        .filter_map(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .max();
    newest
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_some_and(|age| age < max_age)
}

fn database_files(sync: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(sync)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wdn-syncdb-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copy_links_local_db_and_seeds_from_system_sync_data() {
        let root = temp_dir("seed");
        let (system, copy) = (root.join("system"), root.join("copy"));
        fs::create_dir_all(system.join("local")).unwrap();
        fs::create_dir_all(system.join("sync")).unwrap();
        fs::write(system.join("sync/extra.db"), b"db").unwrap();
        fs::write(system.join("sync/extra.files"), b"files").unwrap();
        fs::write(system.join("sync/signed.db"), b"system db").unwrap();
        fs::write(system.join("sync/signed.db.sig"), b"sig").unwrap();

        prepare(&copy, &system).unwrap();
        assert_eq!(fs::read_link(copy.join("local")).unwrap(), system.join("local"));
        assert!(copy.join("sync/extra.db").is_file());
        assert!(copy.join("sync/signed.db.sig").is_file(), "signatures are seeded");
        assert!(!copy.join("sync/extra.files").exists(), "only sync databases are seeded");
        assert!(has_sync_data(&copy));
        assert!(is_fresh(&copy.join("sync"), MAX_AGE));
        assert!(!is_fresh(&copy.join("sync"), Duration::ZERO));
        // Preparing again keeps the existing copy and link.
        prepare(&copy, &system).unwrap();
        // A copy without the signature (made by an older version) gets the
        // matching database and signature from the system.
        fs::write(copy.join("sync/signed.db"), b"refreshed db").unwrap();
        fs::remove_file(copy.join("sync/signed.db.sig")).unwrap();
        prepare(&copy, &system).unwrap();
        assert_eq!(fs::read(copy.join("sync/signed.db")).unwrap(), b"system db");
        assert!(copy.join("sync/signed.db.sig").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_system_leaves_no_sync_data() {
        let root = temp_dir("empty");
        let (system, copy) = (root.join("system"), root.join("copy"));
        fs::create_dir_all(system.join("local")).unwrap();
        prepare(&copy, &system).unwrap();
        assert!(!has_sync_data(&copy));
        assert!(!is_fresh(&copy.join("sync"), MAX_AGE));
        let _ = fs::remove_dir_all(root);
    }
}

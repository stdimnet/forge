//! File | Manage IDE Settings | Restore Default Settings…: the current configuration is
//! copied to a timestamped backup folder next to it, then the settings files are removed on
//! the next start, so Forge starts with its defaults.
//!
//! Connections and accounts (AI providers, code hosts, data sources, SSH configurations) and
//! the recent projects are no IDE settings; they stay.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::dirs::ForgeDirs;

/// Files of `<config dir>/forge` that Restore Default Settings removes.
pub const RESET_FILES: [&str; 3] = [
    "settings.toml",
    forge_core::settings::NEW_PROJECT_SETTINGS_FILE,
    "dialog-windows.json",
];

/// `20261023-142501` for a Unix time in seconds (UTC).
pub fn timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copy `<config dir>/forge` to `<config dir>/forge-backup-<timestamp>` and return the backup
/// folder. A second backup in the same second gets a suffix.
pub fn backup_settings(dirs: &ForgeDirs, now_secs: u64) -> Result<PathBuf, String> {
    let config = dirs.config.as_ref().ok_or("no user config directory")?;
    let source = config.join("forge");
    let stamp = timestamp(now_secs);
    let mut target = config.join(format!("forge-backup-{stamp}"));
    let mut n = 1;
    while target.exists() {
        n += 1;
        target = config.join(format!("forge-backup-{stamp}-{n}"));
    }
    if source.is_dir() {
        copy_dir(&source, &target).map_err(|e| format!("cannot back up the settings: {e}"))?;
    } else {
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    }
    Ok(target)
}

/// Remove the settings files (run at startup, before they are loaded). Returns the removed
/// files.
pub fn reset_settings(dirs: &ForgeDirs) -> Vec<PathBuf> {
    let Some(config) = dirs.forge_config() else {
        return Vec::new();
    };
    RESET_FILES
        .iter()
        .map(|f| config.join(f))
        .filter(|p| fs::remove_file(p).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(0), "19700101-000000");
        assert_eq!(timestamp(1_792_765_501), "20261023-142501");
        assert_eq!(timestamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn backs_up_then_resets_only_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = ForgeDirs::under(tmp.path());
        let forge = tmp.path().join("config/forge");
        fs::create_dir_all(forge.join("scratches")).unwrap();
        fs::write(forge.join("settings.toml"), "[editor]\nx = 1\n").unwrap();
        fs::write(forge.join("new-project-settings.toml"), "a = 1\n").unwrap();
        fs::write(forge.join("ssh.toml"), "").unwrap();
        fs::write(forge.join("scratches/scratch.php"), "<?php").unwrap();

        let backup = backup_settings(&dirs, 1_792_765_501).unwrap();
        assert!(backup.ends_with("forge-backup-20261023-142501"));
        assert!(backup.join("settings.toml").exists());
        assert!(backup.join("scratches/scratch.php").exists());
        let second = backup_settings(&dirs, 1_792_765_501).unwrap();
        assert!(second.ends_with("forge-backup-20261023-142501-2"));

        let removed = reset_settings(&dirs);
        assert_eq!(removed.len(), 2);
        assert!(!forge.join("settings.toml").exists());
        assert!(forge.join("ssh.toml").exists());
        assert!(forge.join("scratches/scratch.php").exists());
    }
}

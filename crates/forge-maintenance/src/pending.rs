//! Work for the next start: Invalidate Caches and Restore Default Settings write a marker
//! file (`<data dir>/forge/pending-maintenance.json`) and restart Forge; the new process runs
//! the marker's work first thing, before settings, indexes or the webview open any file.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::caches::{cleanup_plan, run_cleanup, CacheOption, CleanupReport};
use crate::dirs::ForgeDirs;
use crate::restore::reset_settings;

/// What the next start does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWork {
    /// Invalidate the caches with these options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalidate: Option<Vec<CacheOption>>,
    /// Remove the settings files (they were backed up already).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restore_defaults: bool,
}

/// What the start did, for the log.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReport {
    pub caches: Option<CleanupReport>,
    pub reset_files: Vec<PathBuf>,
}

pub fn marker_path(dirs: &ForgeDirs) -> Option<PathBuf> {
    dirs.forge_data()
        .map(|d| d.join("pending-maintenance.json"))
}

/// The work already scheduled.
pub fn scheduled(dirs: &ForgeDirs) -> PendingWork {
    marker_path(dirs)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Add work for the next start (merged with what is scheduled already).
pub fn schedule(dirs: &ForgeDirs, change: impl FnOnce(&mut PendingWork)) -> Result<(), String> {
    let path = marker_path(dirs).ok_or("no user data directory")?;
    let mut work = scheduled(dirs);
    change(&mut work);
    if let Some(options) = work.invalidate.as_mut() {
        options.sort();
        options.dedup();
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&work).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Run the scheduled work, if any. The marker is removed first, so a failing cleanup never
/// repeats on every start.
pub fn run_pending(dirs: &ForgeDirs) -> Option<PendingReport> {
    let path = marker_path(dirs)?;
    if !path.exists() {
        return None;
    }
    let work = scheduled(dirs);
    let _ = fs::remove_file(&path);
    let mut report = PendingReport::default();
    if let Some(options) = &work.invalidate {
        report.caches = Some(run_cleanup(cleanup_plan(dirs, options)));
    }
    if work.restore_defaults {
        report.reset_files = reset_settings(dirs);
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_work_runs_once_on_the_next_start() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = ForgeDirs::under(tmp.path());
        let index = tmp.path().join("cache/forge/php-index/x.json");
        let stubs = tmp.path().join("cache/forge/php-stubs/core.php");
        let settings = tmp.path().join("config/forge/settings.toml");
        for p in [&index, &stubs, &settings] {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        assert_eq!(run_pending(&dirs), None);

        schedule(&dirs, |w| {
            w.invalidate = Some(vec![CacheOption::SharedIndexes, CacheOption::SharedIndexes])
        })
        .unwrap();
        schedule(&dirs, |w| w.restore_defaults = true).unwrap();
        let work = scheduled(&dirs);
        assert_eq!(work.invalidate, Some(vec![CacheOption::SharedIndexes]));
        assert!(work.restore_defaults);
        // Nothing happens before the restart.
        assert!(index.exists() && settings.exists());

        let report = run_pending(&dirs).unwrap();
        assert_eq!(report.caches.unwrap().deleted.len(), 2);
        assert_eq!(report.reset_files, vec![settings.clone()]);
        assert!(!index.exists() && !stubs.exists() && !settings.exists());
        assert!(!marker_path(&dirs).unwrap().exists());
        assert_eq!(run_pending(&dirs), None);
    }
}

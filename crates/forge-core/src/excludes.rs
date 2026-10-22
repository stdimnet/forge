//! The project's excluded paths, shared by everything that walks or watches the project
//! (Find in Files, Go to File, the TODO scan, language indexes, the file watcher, the
//! search index). The rules are computed by [`forge_scopes::structure::ExcludeSet`] from
//! the project settings (Settings | Directories, "Mark Directory as" › Excluded).

use std::sync::{Arc, RwLock};

pub use forge_scopes::structure::{
    ExcludeReason, ExcludeSet, FolderKind, FolderMark, StructureConfig,
};

/// A handle to the current [`ExcludeSet`]; clones share it, so a change reaches every
/// clone of a [`crate::Workspace`] at once.
#[derive(Clone, Default)]
pub struct Excludes(Arc<RwLock<Current>>);

#[derive(Default)]
struct Current {
    config: StructureConfig,
    set: Arc<ExcludeSet>,
}

impl Excludes {
    /// The rules in effect now.
    pub fn get(&self) -> Arc<ExcludeSet> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).set.clone()
    }

    /// The configuration the rules come from.
    pub fn config(&self) -> StructureConfig {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .config
            .clone()
    }

    /// Replace the rules; true when they exclude something else now (indexes need a
    /// refresh).
    pub fn set(&self, config: StructureConfig) -> bool {
        let set = ExcludeSet::new(&config);
        let mut guard = self.0.write().unwrap_or_else(|e| e.into_inner());
        let changed = *guard.set != set;
        *guard = Current {
            config,
            set: Arc::new(set),
        };
        changed
    }
}

impl std::fmt::Debug for Excludes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Excludes").field(&self.get()).finish()
    }
}

/// Whether a walker (descending from the project root) skips `entry`.
pub(crate) fn skips_entry(
    set: &ExcludeSet,
    root: &std::path::Path,
    entry: &ignore::DirEntry,
) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
    let rel = entry
        .path()
        .strip_prefix(root)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    set.skips(&rel, &name, is_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_changes() {
        let a = Excludes::default();
        let b = a.clone();
        assert!(!b.get().excludes("logs/x.log", false));
        let config = StructureConfig {
            folders: vec![FolderMark {
                path: "logs".into(),
                kind: FolderKind::Excluded,
            }],
            ..StructureConfig::default()
        };
        assert!(a.set(config.clone()));
        assert!(!a.set(config.clone()));
        assert_eq!(b.config(), config);
        assert!(b.get().excludes("logs/x.log", false));
    }
}

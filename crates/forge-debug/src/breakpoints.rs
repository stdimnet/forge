//! Project breakpoints, persisted in `<project>/.forge/breakpoints.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DebugError, DebugResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BreakpointKind {
    #[default]
    Line,
    Exception,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    pub id: u64,
    #[serde(default)]
    pub kind: BreakpointKind,
    /// Project relative file (line breakpoints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based line (line breakpoints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Exception class (exception breakpoints), `*` for any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception: Option<String>,
    /// PHP expression; the breakpoint only suspends when it is truthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Breakpoint {
    pub fn condition(&self) -> Option<&str> {
        self.condition.as_deref().map(str::trim).filter(|c| !c.is_empty())
    }
}

/// Fields of a new breakpoint (ids are assigned by the store).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointSpec {
    #[serde(default)]
    pub kind: BreakpointKind,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub exception: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointsSnapshot {
    pub breakpoints: Vec<Breakpoint>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Default)]
pub struct BreakpointStore {
    path: Option<PathBuf>,
    data: BreakpointsSnapshot,
}

/// File holding a project's breakpoints.
pub fn breakpoints_path(root: &Path) -> PathBuf {
    root.join(".forge").join("breakpoints.json")
}

impl BreakpointStore {
    /// In-memory store (nothing is written).
    pub fn memory() -> Self {
        Self::default()
    }

    /// Load `<root>/.forge/breakpoints.json` (missing file = no breakpoints).
    pub fn load(root: &Path) -> Self {
        let path = breakpoints_path(root);
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| match serde_json::from_str::<BreakpointsSnapshot>(&s) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!("ignoring malformed {}: {e}", path.display());
                    None
                }
            })
            .unwrap_or_default();
        Self {
            path: Some(path),
            data,
        }
    }

    fn save(&self) -> DebugResult<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if self.data.breakpoints.is_empty() && !self.data.muted && !path.exists() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| DebugError::Other(e.to_string()))?;
        std::fs::write(path, json + "\n")?;
        Ok(())
    }

    pub fn snapshot(&self) -> BreakpointsSnapshot {
        self.data.clone()
    }

    pub fn list(&self) -> &[Breakpoint] {
        &self.data.breakpoints
    }

    pub fn muted(&self) -> bool {
        self.data.muted
    }

    pub fn get(&self, id: u64) -> Option<&Breakpoint> {
        self.data.breakpoints.iter().find(|b| b.id == id)
    }

    pub fn find_line(&self, file: &str, line: u32) -> Option<&Breakpoint> {
        self.data.breakpoints.iter().find(|b| {
            b.kind == BreakpointKind::Line
                && b.file.as_deref() == Some(file)
                && b.line == Some(line)
        })
    }

    pub fn add(&mut self, spec: BreakpointSpec) -> DebugResult<Breakpoint> {
        match spec.kind {
            BreakpointKind::Line => {
                let (Some(file), Some(line)) = (&spec.file, spec.line) else {
                    return Err(DebugError::Other(
                        "a line breakpoint needs a file and a line".into(),
                    ));
                };
                if let Some(existing) = self.find_line(file, line) {
                    return Ok(existing.clone());
                }
            }
            BreakpointKind::Exception => {
                if spec.exception.as_deref().unwrap_or("").trim().is_empty() {
                    return Err(DebugError::Other(
                        "an exception breakpoint needs an exception class".into(),
                    ));
                }
            }
        }
        let id = self.data.breakpoints.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let bp = Breakpoint {
            id,
            kind: spec.kind,
            file: spec.file,
            line: spec.line,
            exception: spec.exception.map(|e| e.trim().to_string()),
            condition: spec.condition,
            enabled: spec.enabled.unwrap_or(true),
        };
        self.data.breakpoints.push(bp.clone());
        self.save()?;
        Ok(bp)
    }

    /// Replace a breakpoint (matched by id). Returns the previous version.
    pub fn update(&mut self, bp: Breakpoint) -> DebugResult<Breakpoint> {
        let slot = self
            .data
            .breakpoints
            .iter_mut()
            .find(|b| b.id == bp.id)
            .ok_or_else(|| DebugError::NotFound(format!("no breakpoint {}", bp.id)))?;
        let old = std::mem::replace(slot, bp);
        self.save()?;
        Ok(old)
    }

    pub fn remove(&mut self, id: u64) -> DebugResult<Option<Breakpoint>> {
        let idx = self.data.breakpoints.iter().position(|b| b.id == id);
        let removed = idx.map(|i| self.data.breakpoints.remove(i));
        self.save()?;
        Ok(removed)
    }

    pub fn set_muted(&mut self, muted: bool) -> DebugResult<()> {
        self.data.muted = muted;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(file: &str, line: u32) -> BreakpointSpec {
        BreakpointSpec {
            file: Some(file.into()),
            line: Some(line),
            ..Default::default()
        }
    }

    #[test]
    fn crud_persists_to_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = BreakpointStore::load(dir.path());
        assert!(store.list().is_empty());
        let a = store.add(line("app/A.php", 3)).unwrap();
        let again = store.add(line("app/A.php", 3)).unwrap();
        assert_eq!(a.id, again.id, "same line is not added twice");
        let mut b = store
            .add(BreakpointSpec {
                kind: BreakpointKind::Exception,
                exception: Some(" RuntimeException ".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(b.exception.as_deref(), Some("RuntimeException"));
        b.enabled = false;
        store.update(b.clone()).unwrap();
        store.set_muted(true).unwrap();

        let reloaded = BreakpointStore::load(dir.path());
        assert_eq!(reloaded.list().len(), 2);
        assert!(reloaded.muted());
        assert!(!reloaded.get(b.id).unwrap().enabled);

        let mut store = reloaded;
        store.remove(a.id).unwrap();
        assert!(store.find_line("app/A.php", 3).is_none());
        assert!(store.add(BreakpointSpec::default()).is_err());
    }

    #[test]
    fn no_file_is_written_without_breakpoints() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = BreakpointStore::load(dir.path());
        store.remove(1).unwrap();
        assert!(!breakpoints_path(dir.path()).exists());
    }
}

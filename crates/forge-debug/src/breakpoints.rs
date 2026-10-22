//! Project breakpoints, persisted in `<project>/.forge/breakpoints.json`.

use std::path::{Path, PathBuf};

use forge_core::{LocalFiles, SharedFiles};

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
    /// "Suspend execution": off makes a logpoint that logs and lets the script run on.
    #[serde(default = "yes")]
    pub suspend: bool,
    /// "Log: 'Breakpoint hit' message".
    #[serde(default, skip_serializing_if = "is_false")]
    pub log_message: bool,
    /// "Log: Stack trace".
    #[serde(default, skip_serializing_if = "is_false")]
    pub log_stack: bool,
    /// "Evaluate and log": a PHP expression whose value is printed to the console.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_expression: Option<String>,
    /// "Remove once hit" (temporary breakpoints, Ctrl+Alt+Shift+F8).
    #[serde(default, skip_serializing_if = "is_false")]
    pub temporary: bool,
    /// "Pass count": suspend only from the n-th hit on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_count: Option<u32>,
}

fn yes() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Breakpoint {
    pub fn condition(&self) -> Option<&str> {
        self.condition
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
    }

    pub fn log_expression(&self) -> Option<&str> {
        self.log_expression
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
    }

    /// The pass count when it changes anything (more than one pass).
    pub fn pass_count(&self) -> Option<u32> {
        self.pass_count.filter(|n| *n > 1)
    }

    /// Whether a hit writes something to the console.
    pub fn logs(&self) -> bool {
        self.log_message || self.log_stack || self.log_expression().is_some()
    }

    /// Whether a hit needs Forge's attention beyond suspending (logging, removal, or
    /// resuming a logpoint).
    pub fn acts_on_hit(&self) -> bool {
        self.logs() || self.temporary || !self.suspend
    }

    /// Whether the engine reported exception `name` for this exception breakpoint.
    pub fn matches_exception(&self, name: &str) -> bool {
        let want = self.exception.as_deref().unwrap_or("*").trim();
        let want = want.trim_start_matches('\\');
        want == "*" || want.eq_ignore_ascii_case(name.trim().trim_start_matches('\\'))
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
    #[serde(default)]
    pub suspend: Option<bool>,
    #[serde(default)]
    pub log_message: bool,
    #[serde(default)]
    pub log_stack: bool,
    #[serde(default)]
    pub log_expression: Option<String>,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub pass_count: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointsSnapshot {
    pub breakpoints: Vec<Breakpoint>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Default)]
pub struct BreakpointStore {
    /// Where the project's files are (`None`: memory only).
    files: Option<SharedFiles>,
    /// The file existed when loaded (an empty store is not written otherwise).
    existed: bool,
    data: BreakpointsSnapshot,
}

/// The breakpoints file relative to the project root.
pub const BREAKPOINTS_FILE: &str = ".forge/breakpoints.json";

/// File holding a project's breakpoints.
pub fn breakpoints_path(root: &Path) -> PathBuf {
    root.join(BREAKPOINTS_FILE)
}

impl BreakpointStore {
    /// In-memory store (nothing is written).
    pub fn memory() -> Self {
        Self::default()
    }

    /// Load `<root>/.forge/breakpoints.json` (missing file = no breakpoints).
    pub fn load(root: &Path) -> Self {
        Self::load_files(LocalFiles::shared(root))
    }

    /// Load the project's breakpoints through `files` (a remote project's live on its host).
    pub fn load_files(files: SharedFiles) -> Self {
        let text = files.read(BREAKPOINTS_FILE).unwrap_or_else(|e| {
            tracing::warn!("cannot read {}: {e}", files.describe(BREAKPOINTS_FILE));
            None
        });
        let existed = text.is_some();
        let data = text
            .and_then(|s| match serde_json::from_str::<BreakpointsSnapshot>(&s) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!(
                        "ignoring malformed {}: {e}",
                        files.describe(BREAKPOINTS_FILE)
                    );
                    None
                }
            })
            .unwrap_or_default();
        Self {
            files: Some(files),
            existed,
            data,
        }
    }

    fn save(&mut self) -> DebugResult<()> {
        let Some(files) = &self.files else {
            return Ok(());
        };
        if self.data.breakpoints.is_empty() && !self.data.muted && !self.existed {
            return Ok(());
        }
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| DebugError::Other(e.to_string()))?;
        files
            .write(BREAKPOINTS_FILE, &(json + "\n"))
            .map_err(|e| DebugError::Other(e.to_string()))?;
        self.existed = true;
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

    /// The enabled breakpoint the engine stopped at: an exception breakpoint for
    /// `exception` (the exact class first, else any, since the engine also reports
    /// subclasses), otherwise the line breakpoint at `file:line`.
    pub fn find_hit(
        &self,
        file: Option<&str>,
        line: u32,
        exception: Option<&str>,
    ) -> Option<&Breakpoint> {
        let enabled = || self.data.breakpoints.iter().filter(|b| b.enabled);
        match exception {
            Some(name) => {
                let exceptions = || enabled().filter(|b| b.kind == BreakpointKind::Exception);
                exceptions()
                    .find(|b| b.matches_exception(name))
                    .or_else(|| exceptions().next())
            }
            None => {
                let file = file?;
                enabled().find(|b| {
                    b.kind == BreakpointKind::Line
                        && b.file.as_deref() == Some(file)
                        && b.line == Some(line)
                })
            }
        }
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
        let id = self
            .data
            .breakpoints
            .iter()
            .map(|b| b.id)
            .max()
            .unwrap_or(0)
            + 1;
        let bp = Breakpoint {
            id,
            kind: spec.kind,
            file: spec.file,
            line: spec.line,
            exception: spec.exception.map(|e| e.trim().to_string()),
            condition: spec.condition,
            enabled: spec.enabled.unwrap_or(true),
            suspend: spec.suspend.unwrap_or(true),
            log_message: spec.log_message,
            log_stack: spec.log_stack,
            log_expression: spec.log_expression,
            temporary: spec.temporary,
            pass_count: spec.pass_count,
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
    fn logpoints_temporary_and_pass_count_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = BreakpointStore::load(dir.path());
        let lp = store
            .add(BreakpointSpec {
                suspend: Some(false),
                log_message: true,
                log_expression: Some(" $order->id ".into()),
                ..line("app/A.php", 7)
            })
            .unwrap();
        assert!(!lp.suspend && lp.logs() && lp.acts_on_hit());
        assert_eq!(lp.log_expression(), Some("$order->id"));
        let tmp = store
            .add(BreakpointSpec {
                temporary: true,
                pass_count: Some(3),
                ..line("app/A.php", 9)
            })
            .unwrap();
        assert_eq!(tmp.pass_count(), Some(3));
        let json = std::fs::read_to_string(breakpoints_path(dir.path())).unwrap();
        assert!(json.contains("\"logExpression\""), "{json}");
        assert!(
            !json.contains("logStack"),
            "defaults are not written: {json}"
        );
        let reloaded = BreakpointStore::load(dir.path());
        assert_eq!(reloaded.get(lp.id), Some(&lp));
        assert!(reloaded.get(tmp.id).unwrap().temporary);
        // Files of older versions have no suspend flag: they suspend.
        let old: Breakpoint = serde_json::from_str(r#"{"id":1,"file":"a.php","line":2}"#).unwrap();
        assert!(old.suspend && old.enabled && !old.acts_on_hit());
    }

    #[test]
    fn hits_are_found_by_location_or_exception() {
        let mut store = BreakpointStore::memory();
        let a = store.add(line("a.php", 3)).unwrap();
        let mut off = store.add(line("a.php", 4)).unwrap();
        off.enabled = false;
        store.update(off).unwrap();
        let any = store
            .add(BreakpointSpec {
                kind: BreakpointKind::Exception,
                exception: Some("\\LogicException".into()),
                ..Default::default()
            })
            .unwrap();
        let rt = store
            .add(BreakpointSpec {
                kind: BreakpointKind::Exception,
                exception: Some("RuntimeException".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store.find_hit(Some("a.php"), 3, None).map(|b| b.id),
            Some(a.id)
        );
        assert!(store.find_hit(Some("a.php"), 4, None).is_none(), "disabled");
        assert!(store.find_hit(None, 3, None).is_none());
        let hit = |e: &str| store.find_hit(None, 0, Some(e)).map(|b| b.id);
        assert_eq!(hit("RuntimeException"), Some(rt.id));
        assert_eq!(hit("logicexception"), Some(any.id));
        // A subclass the engine matched by hierarchy.
        assert_eq!(hit("App\\OrderFailed"), Some(any.id));
    }

    #[test]
    fn no_file_is_written_without_breakpoints() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = BreakpointStore::load(dir.path());
        store.remove(1).unwrap();
        assert!(!breakpoints_path(dir.path()).exists());
    }
}

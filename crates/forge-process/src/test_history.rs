//! Test History of the Run tool window's test runner: the results of the last test runs of
//! a project, kept outside the project in Forge's data dir
//! (`<data dir>/forge/test-history/<project key>/<id>.json`, compact JSON), newest first and
//! at most [`MAX_RUNS`] per project. A stored run opens in a Run tab again (its tree is
//! rebuilt with `TestTree::import`) and reruns with its configuration.

use std::fs;
use std::path::{Path, PathBuf};

use forge_core::project_state::project_key;
use forge_core::{ForgeError, Result};
use forge_output::{StoredNode, TestCounts};
use serde::{Deserialize, Serialize};

use crate::config::RunConfiguration;

/// Runs kept per project.
pub const MAX_RUNS: usize = 10;

/// One finished test run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunRecord {
    /// File-name safe, sortable by time (`<millis>-<n>`).
    pub id: String,
    /// The configuration's name (the Run tab's title).
    pub name: String,
    pub configuration: RunConfiguration,
    /// Start time, milliseconds since the Unix epoch.
    pub at: u64,
    /// "Tests failed: 1, passed: 3 of 4 tests – 51 ms".
    pub summary: String,
    pub counts: TestCounts,
    pub tree: StoredNode,
}

/// A run as the Test History menu lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub at: u64,
    pub summary: String,
    pub counts: TestCounts,
}

impl From<&TestRunRecord> for TestRunEntry {
    fn from(r: &TestRunRecord) -> Self {
        TestRunEntry {
            id: r.id.clone(),
            name: r.name.clone(),
            type_id: r.configuration.type_id.clone(),
            at: r.at,
            summary: r.summary.clone(),
            counts: r.counts,
        }
    }
}

fn safe_id(id: &str) -> Result<&str> {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(id)
    } else {
        Err(ForgeError::Other(format!("invalid test run id {id:?}")))
    }
}

fn folder(dir: &Path, root: &str) -> PathBuf {
    dir.join(project_key(root))
}

fn read_records(dir: &Path, root: &str) -> Vec<TestRunRecord> {
    let Ok(entries) = fs::read_dir(folder(dir, root)) else {
        return Vec::new();
    };
    let mut out: Vec<TestRunRecord> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// The stored runs of a project, newest first.
pub fn list(dir: &Path, root: &str) -> Vec<TestRunEntry> {
    read_records(dir, root).iter().map(Into::into).collect()
}

pub fn load(dir: &Path, root: &str, id: &str) -> Result<TestRunRecord> {
    let path = folder(dir, root).join(format!("{}.json", safe_id(id)?));
    let text = fs::read_to_string(&path).map_err(|e| ForgeError::io(&path, e))?;
    serde_json::from_str(&text).map_err(|e| ForgeError::Other(e.to_string()))
}

/// Store a run (its `id` is chosen here when empty) and drop the oldest beyond
/// [`MAX_RUNS`].
pub fn save(dir: &Path, root: &str, mut record: TestRunRecord) -> Result<TestRunEntry> {
    let folder = folder(dir, root);
    fs::create_dir_all(&folder).map_err(|e| ForgeError::io(&folder, e))?;
    if record.id.is_empty() {
        let mut n = 0;
        record.id = loop {
            let id = format!("{}-{n}", record.at);
            if !folder.join(format!("{id}.json")).exists() {
                break id;
            }
            n += 1;
        };
    }
    let path = folder.join(format!("{}.json", safe_id(&record.id)?));
    let text = serde_json::to_string(&record).map_err(|e| ForgeError::Other(e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| ForgeError::io(&tmp, e))?;
    fs::rename(&tmp, &path).map_err(|e| ForgeError::io(&path, e))?;
    for old in read_records(dir, root).into_iter().skip(MAX_RUNS) {
        let _ = fs::remove_file(folder.join(format!("{}.json", old.id)));
    }
    Ok((&record).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_output::links::LinkContext;
    use forge_output::{parse_service_message, TestTree};

    fn record(at: u64) -> TestRunRecord {
        let mut tree = TestTree::new("Test Results", LinkContext::default());
        tree.apply(&parse_service_message("##teamcity[testStarted name='a']").unwrap());
        tree.apply(&parse_service_message("##teamcity[testFinished name='a']").unwrap());
        tree.finish(Some(0));
        TestRunRecord {
            id: String::new(),
            name: "UserTest".into(),
            configuration: RunConfiguration {
                name: "UserTest".into(),
                type_id: "phpunit".into(),
                ..RunConfiguration::default()
            },
            at,
            summary: tree.summary().text,
            counts: tree.counts(),
            tree: tree.export(),
        }
    }

    #[test]
    fn keeps_the_newest_runs_per_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = "/home/me/shop";
        for at in 1..=(MAX_RUNS as u64 + 2) {
            save(dir.path(), root, record(at)).unwrap();
        }
        let entries = list(dir.path(), root);
        assert_eq!(entries.len(), MAX_RUNS);
        assert_eq!(entries[0].at, MAX_RUNS as u64 + 2);
        assert_eq!(entries[0].type_id, "phpunit");
        assert_eq!(entries[0].summary, "Tests passed: 1 of 1 test");
        let back = load(dir.path(), root, &entries[0].id).unwrap();
        assert_eq!(back.tree.children[0].name, "a");
        assert!(list(dir.path(), "/other").is_empty());
        assert!(load(dir.path(), root, "../x").is_err());
        let same = save(dir.path(), root, record(5)).unwrap();
        assert_eq!(same.id, "5-1");
    }
}

//! Per-request review state, kept in Forge's cache folder (never in the project):
//! `<cache dir>/forge/code-review/<host-server-repo>/<number>.json`.
//!
//! - viewed files, stored with a digest of their diff: when a push changes a file, its
//!   digest changes and the file shows as unviewed again;
//! - pending review comments (drafts);
//! - the last AI summary and AI review, valid for the head SHA they were made for.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use forge_forges::{ChangedFile, RepoRef};
use forge_vcs::LineKind;
use serde::{Deserialize, Serialize};

use crate::anchor::Finding;
use crate::batch::Draft;
use crate::parse::Summary;

/// A short digest of a file's diff (FNV-1a over path, hunk positions and lines).
pub fn digest(file: &ChangedFile) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    feed(file.path.as_bytes());
    for hk in &file.hunks {
        feed(format!("@{},{}", hk.old_start, hk.new_start).as_bytes());
        for l in &hk.lines {
            feed(match l.kind {
                LineKind::Added => b"\n+",
                LineKind::Removed => b"\n-",
                LineKind::Context => b"\n ",
            });
            feed(l.text.as_bytes());
        }
    }
    format!("{h:016x}")
}

/// The AI summary of one head commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CachedSummary {
    pub head_sha: String,
    pub summary: Summary,
    pub provider: String,
    pub model: Option<String>,
    /// Files left out by `.aiignore`.
    pub excluded: Vec<String>,
    /// Files only named (generated or over the size limit).
    pub omitted: Vec<String>,
    /// Unix seconds.
    pub created_at: u64,
}

/// The AI review of one head commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CachedFindings {
    pub head_sha: String,
    pub findings: Vec<Finding>,
    /// Finding ids the user dismissed or turned into a draft.
    pub handled: BTreeSet<String>,
    pub provider: String,
    pub model: Option<String>,
    pub excluded: Vec<String>,
    pub omitted: Vec<String>,
    pub created_at: u64,
}

impl CachedFindings {
    /// The findings still waiting for a decision.
    pub fn open(&self) -> Vec<Finding> {
        self.findings
            .iter()
            .filter(|f| !self.handled.contains(&f.id))
            .cloned()
            .collect()
    }
}

/// Everything Forge remembers about reviewing one request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewRecord {
    /// Path → digest of the diff that was viewed.
    pub viewed: BTreeMap<String, String>,
    pub drafts: Vec<Draft>,
    pub summary: Option<CachedSummary>,
    pub findings: Option<CachedFindings>,
    /// Counter for draft ids.
    pub next_id: u64,
}

impl ReviewRecord {
    /// Whether `file` was viewed in its current form.
    pub fn is_viewed(&self, file: &ChangedFile) -> bool {
        self.viewed.get(&file.path) == Some(&digest(file))
    }

    pub fn set_viewed(&mut self, path: &str, digest: &str, viewed: bool) {
        if viewed {
            self.viewed.insert(path.to_string(), digest.to_string());
        } else {
            self.viewed.remove(path);
        }
    }

    /// Add a draft (empty id) or replace the one with the same id; returns its id.
    pub fn save_draft(&mut self, mut draft: Draft) -> String {
        if let Some(d) = self
            .drafts
            .iter_mut()
            .find(|d| !draft.id.is_empty() && d.id == draft.id)
        {
            *d = draft;
            return d.id.clone();
        }
        self.next_id += 1;
        draft.id = format!("d{}", self.next_id);
        let id = draft.id.clone();
        self.drafts.push(draft);
        id
    }

    /// Remove a draft; a draft made from a finding brings the finding back.
    pub fn delete_draft(&mut self, id: &str) {
        let finding = self
            .drafts
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.finding.clone());
        self.drafts.retain(|d| d.id != id);
        if let (Some(f), Some(c)) = (finding, self.findings.as_mut()) {
            c.handled.remove(&f);
        }
    }

    pub fn summary_for(&self, head_sha: &str) -> Option<&CachedSummary> {
        self.summary.as_ref().filter(|s| s.head_sha == head_sha)
    }

    pub fn findings_for(&self, head_sha: &str) -> Option<&CachedFindings> {
        self.findings.as_ref().filter(|s| s.head_sha == head_sha)
    }

    /// Mark a finding as handled (dismissed or drafted).
    pub fn handle_finding(&mut self, id: &str) {
        if let Some(c) = self.findings.as_mut() {
            c.handled.insert(id.to_string());
        }
    }
}

/// Reads and writes [`ReviewRecord`]s below one folder.
#[derive(Debug, Clone)]
pub struct ReviewStore {
    dir: PathBuf,
}

impl ReviewStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `<cache dir>/forge/code-review`.
    pub fn default_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("forge").join("code-review"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A file-name-safe key for a repository: `<kind>-<host>-<path>`.
    pub fn repo_key(repo: &RepoRef) -> String {
        let host = repo
            .server
            .split("://")
            .nth(1)
            .unwrap_or(&repo.server)
            .trim_end_matches('/');
        format!("{}-{host}-{}", repo.kind.id(), repo.path)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    fn path(&self, repo: &RepoRef, number: u64) -> PathBuf {
        self.dir
            .join(Self::repo_key(repo))
            .join(format!("{number}.json"))
    }

    /// The record of a request; a missing or unreadable file yields an empty one.
    pub fn load(&self, repo: &RepoRef, number: u64) -> ReviewRecord {
        std::fs::read_to_string(self.path(repo, number))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, repo: &RepoRef, number: u64, record: &ReviewRecord) -> std::io::Result<()> {
        let path = self.path(repo, number);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(record)?)?;
        std::fs::rename(tmp, path)
    }

    /// Load, change and save in one step.
    pub fn update<T>(
        &self,
        repo: &RepoRef,
        number: u64,
        f: impl FnOnce(&mut ReviewRecord) -> T,
    ) -> std::io::Result<T> {
        let mut record = self.load(repo, number);
        let out = f(&mut record);
        self.save(repo, number, &record)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::{file, hunk};
    use forge_forges::HostKind;

    fn repo() -> RepoRef {
        RepoRef {
            kind: HostKind::Gitlab,
            server: "https://git.acme.dev:8443".into(),
            path: "team/sub/shop".into(),
            remote: "origin".into(),
        }
    }

    #[test]
    fn viewed_state_follows_the_diff() {
        let v1 = file("a.php", vec![hunk(1, &[("+", "a")])]);
        let v2 = file("a.php", vec![hunk(1, &[("+", "b")])]);
        assert_ne!(digest(&v1), digest(&v2));
        let mut r = ReviewRecord::default();
        r.set_viewed("a.php", &digest(&v1), true);
        assert!(r.is_viewed(&v1));
        assert!(!r.is_viewed(&v2), "a new push makes it unviewed");
        r.set_viewed("a.php", "", false);
        assert!(!r.is_viewed(&v1));
    }

    #[test]
    fn drafts_and_findings() {
        let mut r = ReviewRecord::default();
        let a = r.save_draft(Draft {
            body: "a".into(),
            ..Draft::default()
        });
        let b = r.save_draft(Draft {
            body: "b".into(),
            finding: Some("f1".into()),
            ..Draft::default()
        });
        assert_eq!((a.as_str(), b.as_str()), ("d1", "d2"));
        r.save_draft(Draft {
            id: a.clone(),
            body: "a2".into(),
            ..Draft::default()
        });
        assert_eq!(r.drafts[0].body, "a2");
        r.findings = Some(CachedFindings {
            head_sha: "h1".into(),
            findings: vec![],
            ..CachedFindings::default()
        });
        r.handle_finding("f1");
        r.delete_draft(&b);
        assert!(r.findings.as_ref().unwrap().handled.is_empty());
        assert!(r.findings_for("h1").is_some() && r.findings_for("h2").is_none());
    }

    #[test]
    fn round_trips_in_the_cache_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ReviewStore::new(tmp.path());
        assert_eq!(
            ReviewStore::repo_key(&repo()),
            "gitlab-git.acme.dev_8443-team_sub_shop"
        );
        assert_eq!(store.load(&repo(), 7), ReviewRecord::default());
        store
            .update(&repo(), 7, |r| {
                r.summary = Some(CachedSummary {
                    head_sha: "abc".into(),
                    ..CachedSummary::default()
                })
            })
            .unwrap();
        let back = store.load(&repo(), 7);
        assert!(back.summary_for("abc").is_some());
        assert!(back.summary_for("def").is_none());
        assert!(tmp
            .path()
            .join("gitlab-git.acme.dev_8443-team_sub_shop/7.json")
            .exists());
    }
}

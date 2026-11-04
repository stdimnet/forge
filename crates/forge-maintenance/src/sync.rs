//! Settings Sync: the settings groups of Export / Import Settings kept in step between
//! machines through a folder the user chooses (a cloud-drive folder) or a git repository the
//! user provides. Forge runs no sync service of its own.
//!
//! The shared folder holds one file per group plus a metadata file:
//!
//! ```text
//! <folder>/forge-settings-sync/sync.json               { format, components: { id: { sha256, modifiedMs, machine } } }
//! <folder>/forge-settings-sync/components/<id>.data    the group: TOML of its settings, or the config file
//! <folder>/forge-settings-sync/backups/<ms>/<id>.data  remote versions that lost a conflict
//! ```
//!
//! Each group is compared with the hash it had at the last sync (kept in
//! `<config dir>/forge/settings-sync-state.json`): only one side changed → that side wins;
//! both changed → the newer change wins (last writer wins per group) and the losing version
//! is backed up (local ones to `<config dir>/forge/settings-sync-backups/<ms>/`). Secrets
//! are never part of it: they live in the OS keychain.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::archive::{ImportPlan, SettingsArchive};
use crate::groups::{group, group_of_key, settings_groups};

/// Folder inside the sync folder or repository.
pub const SYNC_DIR: &str = "forge-settings-sync";
const META: &str = "sync.json";
const FORMAT: u32 = 1;
/// The sync state in the Forge config dir.
pub const STATE_FILE: &str = "settings-sync-state.json";
/// Local backups of groups that lost a conflict, in the Forge config dir.
pub const LOCAL_BACKUPS: &str = "settings-sync-backups";

/// One group of the local settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalComponent {
    pub bytes: Vec<u8>,
    /// When it last changed (milliseconds since the Unix epoch): the settings file's or the
    /// config file's modification time.
    pub modified_ms: u64,
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The chosen groups of the local settings. `values` is the user layer; `files` maps config
/// file names to their content and modification time. Groups without values are left out.
pub fn local_components(
    values: &BTreeMap<String, Value>,
    settings_modified_ms: u64,
    files: &BTreeMap<String, (Vec<u8>, u64)>,
    groups: &[String],
) -> Result<BTreeMap<String, LocalComponent>, String> {
    let mut out = BTreeMap::new();
    for g in settings_groups()
        .iter()
        .filter(|g| groups.iter().any(|x| x == g.id))
    {
        match g.file {
            Some(name) => {
                if let Some((bytes, modified_ms)) = files.get(name) {
                    out.insert(
                        g.id.to_string(),
                        LocalComponent {
                            bytes: bytes.clone(),
                            modified_ms: *modified_ms,
                        },
                    );
                }
            }
            None => {
                let selected: BTreeMap<String, Value> = values
                    .iter()
                    .filter(|(k, _)| group_of_key(k) == g.id)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if selected.is_empty() {
                    continue;
                }
                let toml = forge_core::settings_to_toml(&selected).map_err(|e| e.to_string())?;
                out.insert(
                    g.id.to_string(),
                    LocalComponent {
                        bytes: toml.into_bytes(),
                        modified_ms: settings_modified_ms,
                    },
                );
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComponent {
    pub sha256: String,
    pub modified_ms: u64,
    /// Name of the machine that wrote it.
    #[serde(default)]
    pub machine: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    format: u32,
    #[serde(default)]
    components: BTreeMap<String, RemoteComponent>,
}

/// What this machine remembers about the last sync.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    /// Hash of every group at the last sync (both sides were equal then).
    #[serde(default)]
    pub components: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_report: Option<SyncReport>,
}

impl SyncState {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Push,
    Pull,
    /// Both sides changed; the newer one wins.
    Conflict(Side),
}

/// What to do with one group: `(hash, modified_ms)` of each side and the hash at the last
/// sync.
pub fn decide(
    local: Option<(&str, u64)>,
    remote: Option<(&str, u64)>,
    base: Option<&str>,
) -> Action {
    match (local, remote) {
        (None, None) => Action::Nothing,
        (Some(_), None) => Action::Push,
        (None, Some(_)) => Action::Pull,
        (Some((l, lt)), Some((r, rt))) => {
            if l == r {
                return Action::Nothing;
            }
            let local_changed = base != Some(l);
            let remote_changed = base != Some(r);
            match (local_changed, remote_changed) {
                (true, false) => Action::Push,
                (false, true) => Action::Pull,
                _ => Action::Conflict(if lt >= rt { Side::Local } else { Side::Remote }),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub group: String,
    pub winner: Side,
    /// Where the losing version was saved.
    pub backup: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Groups written to the shared folder.
    pub pushed: Vec<String>,
    /// Groups taken from the shared folder (to apply locally).
    pub pulled: Vec<String>,
    pub conflicts: Vec<ConflictInfo>,
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
}

impl SyncReport {
    /// Whether the shared folder changed (a git repository needs a commit).
    pub fn changed_remote(&self) -> bool {
        !self.pushed.is_empty() || !self.conflicts.is_empty()
    }
}

/// A group taken from the shared folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulled {
    pub group: String,
    pub bytes: Vec<u8>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Everything one sync needs to know besides the folder.
pub struct SyncInput<'a> {
    pub local: &'a BTreeMap<String, LocalComponent>,
    /// The groups to sync.
    pub groups: &'a [String],
    pub machine: &'a str,
    pub now_ms: u64,
    /// `<config dir>/forge/settings-sync-backups`.
    pub local_backups: &'a Path,
}

/// Sync the groups with `folder`: writes what changed here into it, returns what changed
/// there (for the caller to apply, e.g. with [`apply_plan`]) and updates `state`.
pub fn sync_folder(
    folder: &Path,
    input: &SyncInput,
    state: &mut SyncState,
) -> Result<(SyncReport, Vec<Pulled>), String> {
    if !folder.is_dir() {
        return Err(format!(
            "the sync folder {} does not exist",
            folder.display()
        ));
    }
    let root = folder.join(SYNC_DIR);
    let meta_path = root.join(META);
    let mut meta: Meta = match std::fs::read(&meta_path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", meta_path.display()))?
        }
        Err(_) => Meta {
            format: FORMAT,
            ..Meta::default()
        },
    };
    if meta.format > FORMAT {
        return Err(format!(
            "the sync folder was written by a newer Forge (format {}); update Forge",
            meta.format
        ));
    }
    let component_path = |id: &str| root.join("components").join(format!("{id}.data"));
    let mut report = SyncReport {
        at_ms: input.now_ms,
        ..SyncReport::default()
    };
    let mut pulled = Vec::new();
    for id in input.groups {
        if group(id).is_none() && id != crate::groups::OTHER {
            continue;
        }
        let local = input.local.get(id);
        let local_hash = local.map(|c| sha(&c.bytes));
        let remote = meta.components.get(id).cloned();
        let action = decide(
            local_hash.as_deref().zip(local.map(|c| c.modified_ms)),
            remote.as_ref().map(|r| (r.sha256.as_str(), r.modified_ms)),
            state.components.get(id).map(String::as_str),
        );
        let read_remote = || -> Result<Vec<u8>, String> {
            let path = component_path(id);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            match &remote {
                Some(r) if sha(&bytes) != r.sha256 => Err(format!(
                    "{} does not match sync.json (is the folder still syncing?)",
                    path.display()
                )),
                _ => Ok(bytes),
            }
        };
        let push = |meta: &mut Meta, report: &mut SyncReport| -> Result<(), String> {
            let c = local.expect("push needs a local component");
            write_atomic(&component_path(id), &c.bytes)?;
            meta.components.insert(
                id.clone(),
                RemoteComponent {
                    sha256: sha(&c.bytes),
                    modified_ms: c.modified_ms.max(1),
                    machine: input.machine.to_string(),
                },
            );
            report.pushed.push(id.clone());
            Ok(())
        };
        match action {
            Action::Nothing => {}
            Action::Push => push(&mut meta, &mut report)?,
            Action::Pull => {
                pulled.push(Pulled {
                    group: id.clone(),
                    bytes: read_remote()?,
                });
                report.pulled.push(id.clone());
            }
            Action::Conflict(Side::Local) => {
                let backup = root
                    .join("backups")
                    .join(input.now_ms.to_string())
                    .join(format!("{id}.data"));
                write_atomic(&backup, &read_remote()?)?;
                push(&mut meta, &mut report)?;
                report.conflicts.push(ConflictInfo {
                    group: id.clone(),
                    winner: Side::Local,
                    backup: backup.display().to_string(),
                });
            }
            Action::Conflict(Side::Remote) => {
                let backup = input
                    .local_backups
                    .join(input.now_ms.to_string())
                    .join(format!("{id}.data"));
                write_atomic(&backup, &local.expect("conflict has a local side").bytes)?;
                pulled.push(Pulled {
                    group: id.clone(),
                    bytes: read_remote()?,
                });
                report.pulled.push(id.clone());
                report.conflicts.push(ConflictInfo {
                    group: id.clone(),
                    winner: Side::Remote,
                    backup: backup.display().to_string(),
                });
            }
        }
        match meta.components.get(id) {
            Some(r) => state.components.insert(id.clone(), r.sha256.clone()),
            None => state.components.remove(id),
        };
    }
    if !report.pushed.is_empty() {
        let json = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
        write_atomic(&meta_path, &json)?;
    }
    state.last_report = Some(report.clone());
    Ok((report, pulled))
}

/// The settings changes and config files that apply pulled groups (each group replaces the
/// local one as a whole, like Import Settings).
pub fn apply_plan(
    pulled: &[Pulled],
    current: &BTreeMap<String, Value>,
) -> Result<ImportPlan, String> {
    let mut archive = SettingsArchive::default();
    let mut ids = Vec::new();
    for p in pulled {
        ids.push(p.group.clone());
        match group(&p.group).and_then(|g| g.file) {
            Some(name) => {
                archive.files.insert(name.to_string(), p.bytes.clone());
            }
            None => {
                let src = std::str::from_utf8(&p.bytes)
                    .map_err(|_| format!("synced group {} is not UTF-8", p.group))?;
                let values = forge_core::parse_settings_toml(src).map_err(|e| e.to_string())?;
                // Only keys of the group itself, whatever the file says.
                archive.values.extend(
                    values
                        .into_iter()
                        .filter(|(k, _)| group_of_key(k) == p.group),
                );
            }
        }
    }
    Ok(archive.plan(current, &ids))
}

/// A git repository the user provides as the shared folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemote {
    pub url: String,
    pub branch: String,
}

impl GitRemote {
    fn check(&self) -> Result<(), String> {
        if self.url.trim().is_empty() || self.url.starts_with('-') {
            return Err("enter the URL of a git repository for Settings Sync".into());
        }
        let ok_branch = !self.branch.is_empty()
            && !self.branch.starts_with('-')
            && !self.branch.contains("..")
            && self
                .branch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b));
        if !ok_branch {
            return Err(format!("`{}` is not a valid branch name", self.branch));
        }
        Ok(())
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Clone the repository into `clone_dir` (Forge's cache) or bring the clone up to date with
/// the remote branch. The clone only ever holds Forge's own commits, so it is reset hard.
pub fn prepare_git(remote: &GitRemote, clone_dir: &Path) -> Result<(), String> {
    remote.check()?;
    if !clone_dir.join(".git").exists() {
        let _ = std::fs::remove_dir_all(clone_dir);
        std::fs::create_dir_all(clone_dir).map_err(|e| format!("{}: {e}", clone_dir.display()))?;
        git(clone_dir, &["init", "-q"])?;
        git(clone_dir, &["remote", "add", "origin", "--", &remote.url])?;
    } else {
        git(
            clone_dir,
            &["remote", "set-url", "origin", "--", &remote.url],
        )?;
    }
    git(clone_dir, &["fetch", "-q", "origin"])?;
    let tracking = format!("origin/{}", remote.branch);
    git(clone_dir, &["checkout", "-q", "-B", &remote.branch])?;
    if git(clone_dir, &["rev-parse", "--verify", "-q", &tracking]).is_ok() {
        git(clone_dir, &["reset", "-q", "--hard", &tracking])?;
    }
    Ok(())
}

/// Commit the sync folder and push it; `Err` when the push was rejected.
pub fn publish_git(remote: &GitRemote, clone_dir: &Path, machine: &str) -> Result<(), String> {
    git(clone_dir, &["add", "-A", "--", SYNC_DIR])?;
    if git(clone_dir, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(());
    }
    let message = format!("Sync Forge settings from {machine}");
    git(
        clone_dir,
        &[
            "-c",
            "user.name=Forge Settings Sync",
            "-c",
            "user.email=settings-sync@forge.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            &message,
        ],
    )?;
    git(
        clone_dir,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/heads/{}", remote.branch),
        ],
    )
    .map(|_| ())
}

/// Sync through a git repository: pull, sync the clone's folder, commit and push. A push
/// that loses a race with another machine is retried once on top of its commit.
pub fn sync_git(
    remote: &GitRemote,
    clone_dir: &Path,
    input: &SyncInput,
    state: &mut SyncState,
) -> Result<(SyncReport, Vec<Pulled>), String> {
    let before = state.clone();
    let mut last_error = String::new();
    for _ in 0..2 {
        prepare_git(remote, clone_dir)?;
        let mut attempt = before.clone();
        let (report, pulled) = sync_folder(clone_dir, input, &mut attempt)?;
        if !report.changed_remote() {
            *state = attempt;
            return Ok((report, pulled));
        }
        match publish_git(remote, clone_dir, input.machine) {
            Ok(()) => {
                *state = attempt;
                return Ok((report, pulled));
            }
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

/// `<cache dir>/forge/settings-sync-git`: where the repository is cloned (never a project).
pub fn default_clone_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("forge").join("settings-sync-git"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comp(text: &str, t: u64) -> LocalComponent {
        LocalComponent {
            bytes: text.as_bytes().to_vec(),
            modified_ms: t,
        }
    }

    #[test]
    fn decisions() {
        assert_eq!(decide(None, None, None), Action::Nothing);
        assert_eq!(decide(Some(("a", 1)), None, None), Action::Push);
        assert_eq!(decide(None, Some(("a", 1)), None), Action::Pull);
        assert_eq!(
            decide(Some(("a", 1)), Some(("a", 5)), None),
            Action::Nothing
        );
        assert_eq!(
            decide(Some(("b", 9)), Some(("a", 5)), Some("a")),
            Action::Push
        );
        assert_eq!(
            decide(Some(("a", 9)), Some(("b", 5)), Some("a")),
            Action::Pull
        );
        assert_eq!(
            decide(Some(("b", 9)), Some(("c", 5)), Some("a")),
            Action::Conflict(Side::Local)
        );
        assert_eq!(
            decide(Some(("b", 3)), Some(("c", 5)), None),
            Action::Conflict(Side::Remote)
        );
    }

    #[test]
    fn components_per_group() {
        let values = BTreeMap::from([
            ("keymap.active".to_string(), json!("macOS")),
            ("editor.font.size".to_string(), json!(15)),
            ("editor.colorScheme.active".to_string(), json!("Ember")),
        ]);
        let files = BTreeMap::from([("ssh.toml".to_string(), (b"x".to_vec(), 7))]);
        let groups: Vec<String> = ["keymap", "colorSchemes", "liveTemplates", "ssh"]
            .map(String::from)
            .to_vec();
        let c = local_components(&values, 42, &files, &groups).unwrap();
        assert_eq!(
            c.keys().collect::<Vec<_>>(),
            ["colorSchemes", "keymap", "ssh"]
        );
        assert_eq!(c["keymap"].modified_ms, 42);
        assert_eq!(c["ssh"].modified_ms, 7);
        assert!(String::from_utf8_lossy(&c["keymap"].bytes).contains("macOS"));
    }

    fn input<'a>(
        local: &'a BTreeMap<String, LocalComponent>,
        groups: &'a [String],
        now_ms: u64,
        machine: &'a str,
        local_backups: &'a Path,
    ) -> SyncInput<'a> {
        SyncInput {
            local,
            groups,
            machine,
            now_ms,
            local_backups,
        }
    }

    /// Two machines share a folder: changes travel both ways, a conflict keeps the newer
    /// side and backs up the other.
    #[test]
    fn two_machines_share_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("Dropbox");
        std::fs::create_dir_all(&shared).unwrap();
        let backups_a = dir.path().join("a-backups");
        let backups_b = dir.path().join("b-backups");
        let groups: Vec<String> = ["keymap", "editor"].map(String::from).to_vec();
        let (mut state_a, mut state_b) = (SyncState::default(), SyncState::default());

        let mut a = BTreeMap::from([(
            "keymap".to_string(),
            comp("[keymap]\nactive = \"macOS\"\n", 10),
        )]);

        let (r, pulled) = sync_folder(
            &shared,
            &input(&a, &groups, 100, "a", &backups_a),
            &mut state_a,
        )
        .unwrap();
        assert_eq!(r.pushed, ["keymap"]);
        assert!(pulled.is_empty());

        // B has nothing yet: it pulls the keymap.
        let b = BTreeMap::new();
        let (r, pulled) = sync_folder(
            &shared,
            &input(&b, &groups, 200, "b", &backups_b),
            &mut state_b,
        )
        .unwrap();
        assert_eq!(r.pulled, ["keymap"]);
        assert_eq!(pulled[0].bytes, a["keymap"].bytes);
        let mut b = BTreeMap::from([(
            "keymap".to_string(),
            comp("[keymap]\nactive = \"macOS\"\n", 200),
        )]);

        // Nothing changed: nothing happens.
        let (r, _) = sync_folder(
            &shared,
            &input(&a, &groups, 300, "a", &backups_a),
            &mut state_a,
        )
        .unwrap();
        assert!(r.pushed.is_empty() && r.pulled.is_empty() && r.conflicts.is_empty());

        // Both change the keymap; B's change is newer and wins, A's is backed up locally.
        a.insert(
            "keymap".into(),
            comp("[keymap]\nactive = \"VSCode\"\n", 400),
        );
        b.insert("keymap".into(), comp("[keymap]\nactive = \"Emacs\"\n", 500));
        let (r, _) = sync_folder(
            &shared,
            &input(&b, &groups, 600, "b", &backups_b),
            &mut state_b,
        )
        .unwrap();
        assert_eq!(r.pushed, ["keymap"]);
        let (r, pulled) = sync_folder(
            &shared,
            &input(&a, &groups, 700, "a", &backups_a),
            &mut state_a,
        )
        .unwrap();
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].winner, Side::Remote);
        assert!(String::from_utf8_lossy(&pulled[0].bytes).contains("Emacs"));
        let backup = std::fs::read_to_string(&r.conflicts[0].backup).unwrap();
        assert!(backup.contains("VSCode"));

        // A newer local change wins over an older remote one; the remote copy is backed up
        // in the shared folder.
        a.insert(
            "keymap".into(),
            comp("[keymap]\nactive = \"Eclipse\"\n", 900),
        );
        b.insert(
            "keymap".into(),
            comp("[keymap]\nactive = \"Sublime\"\n", 800),
        );
        sync_folder(
            &shared,
            &input(&b, &groups, 850, "b", &backups_b),
            &mut state_b,
        )
        .unwrap();
        let (r, _) = sync_folder(
            &shared,
            &input(&a, &groups, 950, "a", &backups_a),
            &mut state_a,
        )
        .unwrap();
        assert_eq!(r.conflicts[0].winner, Side::Local);
        assert!(r.conflicts[0]
            .backup
            .starts_with(&shared.display().to_string()));
        assert!(std::fs::read_to_string(&r.conflicts[0].backup)
            .unwrap()
            .contains("Sublime"));

        // State round trip.
        let path = dir.path().join("state.json");
        state_a.save(&path).unwrap();
        assert_eq!(SyncState::load(&path), state_a);
        assert!(sync_folder(
            &dir.path().join("missing"),
            &input(&a, &groups, 1, "a", &backups_a),
            &mut state_a
        )
        .is_err());
    }

    #[test]
    fn pulled_groups_replace_local_ones() {
        let current = BTreeMap::from([
            ("keymap.active".to_string(), json!("macOS")),
            ("keymap.custom".to_string(), json!([])),
            ("editor.font.size".to_string(), json!(12)),
        ]);
        let pulled = vec![
            Pulled {
                group: "keymap".into(),
                // A foreign key in the file is ignored.
                bytes: b"[keymap]\nactive = \"VSCode\"\n[editor.font]\nsize = 30\n".to_vec(),
            },
            Pulled {
                group: "ssh".into(),
                bytes: b"[[configs]]\n".to_vec(),
            },
        ];
        let plan = apply_plan(&pulled, &current).unwrap();
        assert_eq!(
            plan.changes,
            vec![
                ("keymap.custom".to_string(), Value::Null),
                ("keymap.active".to_string(), json!("VSCode")),
            ]
        );
        assert_eq!(plan.files[0].0, "ssh.toml");
    }

    fn has_git() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[test]
    fn syncs_through_a_local_git_repository() {
        if !has_git() {
            eprintln!("git not installed: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("settings.git");
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]).unwrap();
        let remote = GitRemote {
            url: bare.display().to_string(),
            branch: "main".into(),
        };
        let groups: Vec<String> = vec!["keymap".into()];
        let backups = dir.path().join("backups");
        let a = BTreeMap::from([(
            "keymap".to_string(),
            comp("[keymap]\nactive = \"macOS\"\n", 10),
        )]);
        let mut state_a = SyncState::default();
        let input_a = SyncInput {
            local: &a,
            groups: &groups,
            machine: "a",
            now_ms: 100,
            local_backups: &backups,
        };
        let (r, _) =
            sync_git(&remote, &dir.path().join("clone-a"), &input_a, &mut state_a).unwrap();
        assert_eq!(r.pushed, ["keymap"]);

        let b = BTreeMap::new();
        let mut state_b = SyncState::default();
        let input_b = SyncInput {
            local: &b,
            groups: &groups,
            machine: "b",
            now_ms: 200,
            local_backups: &backups,
        };
        let (r, pulled) =
            sync_git(&remote, &dir.path().join("clone-b"), &input_b, &mut state_b).unwrap();
        assert_eq!(r.pulled, ["keymap"]);
        assert_eq!(pulled[0].bytes, a["keymap"].bytes);
        let log = git(&bare, &["log", "--format=%s", "main"]).unwrap();
        assert!(log.contains("Sync Forge settings from a"), "{log}");
        let bad = GitRemote {
            url: "--upload-pack=evil".into(),
            branch: "main".into(),
        };
        assert!(prepare_git(&bad, &dir.path().join("clone-c")).is_err());
    }
}

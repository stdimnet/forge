//! What a guest may do, and the [`Policy`] that decides every guest request **on the host**.
//!
//! The guest's client is untrusted: its UI hides what a permission does not allow, but only
//! the host's decision counts. The four levels are PHPStorm's: Read-only, Edit files, Full
//! access and Custom (one value per area).

use serde::{Deserialize, Serialize};

use crate::excludes::Excludes;
use crate::protocol::GuestMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilesAccess {
    ReadOnly,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalAccess {
    None,
    ReadOnly,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunAccess {
    None,
    ReadOnly,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VcsAccess {
    ReadOnly,
    Full,
}

/// One value per area (the Custom Permissions dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub files: FilesAccess,
    pub terminal: TerminalAccess,
    pub run: RunAccess,
    pub vcs: VcsAccess,
}

/// The presets of the Start Session popup (setting `codeWithMe.defaultPermission`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionLevel {
    ReadOnly,
    EditFiles,
    FullAccess,
    Custom,
}

impl Permissions {
    /// Guests can view files, the terminal and run output.
    pub const READ_ONLY: Self = Self {
        files: FilesAccess::ReadOnly,
        terminal: TerminalAccess::ReadOnly,
        run: RunAccess::ReadOnly,
        vcs: VcsAccess::ReadOnly,
    };
    /// Guests can edit files, but cannot use the terminal or run code.
    pub const EDIT_FILES: Self = Self {
        files: FilesAccess::Edit,
        terminal: TerminalAccess::None,
        run: RunAccess::None,
        vcs: VcsAccess::ReadOnly,
    };
    /// Guests can edit files, use the terminal and run code on the host's machine.
    pub const FULL_ACCESS: Self = Self {
        files: FilesAccess::Edit,
        terminal: TerminalAccess::Full,
        run: RunAccess::Full,
        vcs: VcsAccess::Full,
    };

    /// The permissions of a preset; `None` for Custom, which has no fixed table.
    pub fn preset(level: PermissionLevel) -> Option<Self> {
        match level {
            PermissionLevel::ReadOnly => Some(Self::READ_ONLY),
            PermissionLevel::EditFiles => Some(Self::EDIT_FILES),
            PermissionLevel::FullAccess => Some(Self::FULL_ACCESS),
            PermissionLevel::Custom => None,
        }
    }

    /// The preset these permissions are, or Custom.
    pub fn level(&self) -> PermissionLevel {
        [
            PermissionLevel::ReadOnly,
            PermissionLevel::EditFiles,
            PermissionLevel::FullAccess,
        ]
        .into_iter()
        .find(|l| Self::preset(*l) == Some(*self))
        .unwrap_or(PermissionLevel::Custom)
    }

    pub fn can_edit_files(&self) -> bool {
        self.files == FilesAccess::Edit
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::READ_ONLY
    }
}

/// Why the host refused a request. Shown to the guest and written to the session log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Denied(pub String);

/// The host's rules for one participant.
#[derive(Debug, Clone)]
pub struct Policy {
    pub permissions: Permissions,
    pub excludes: Excludes,
}

impl Policy {
    pub fn new(permissions: Permissions, excludes: Excludes) -> Self {
        Self {
            permissions,
            excludes,
        }
    }

    /// Whether a guest may see this workspace-relative path at all: inside the project,
    /// not in `.git/` and not excluded. File system checks (symlinks leaving the project)
    /// happen where the file is read.
    pub fn check_path(&self, path: &str) -> Result<(), Denied> {
        if !is_plain_relative(path) {
            return Err(Denied(format!("\"{path}\" is not a path in the project")));
        }
        if path == ".git" || path.starts_with(".git/") || path.contains("/.git/") {
            return Err(Denied("Git's internal files are not shared".into()));
        }
        if self.excludes.is_excluded(path) {
            return Err(Denied(format!("\"{path}\" is not shared with guests")));
        }
        Ok(())
    }

    /// Decide a message from an admitted guest before the host acts on it.
    pub fn check(&self, msg: &GuestMessage) -> Result<(), Denied> {
        match msg {
            GuestMessage::Knock { .. } => Err(Denied("already in the session".into())),
            GuestMessage::DocOpen { path, .. } => self.check_path(path),
            GuestMessage::DocUpdate { .. } | GuestMessage::DocSave { .. } => {
                if self.permissions.can_edit_files() {
                    Ok(())
                } else {
                    Err(Denied("Your permission is Read-only".into()))
                }
            }
            GuestMessage::Awareness { .. }
            | GuestMessage::DocClose { .. }
            | GuestMessage::Leave => Ok(()),
            // Project calls are decoded and checked by the host (`Policy::check_call`).
            GuestMessage::Call { .. } => Ok(()),
        }
    }
}

/// A non-empty `/`-separated relative path without `.`/`..` segments, backslashes, drive
/// letters or control characters.
pub fn is_plain_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DocId;

    #[test]
    fn presets_match_the_plan_table() {
        assert_eq!(
            Permissions::READ_ONLY,
            Permissions {
                files: FilesAccess::ReadOnly,
                terminal: TerminalAccess::ReadOnly,
                run: RunAccess::ReadOnly,
                vcs: VcsAccess::ReadOnly
            }
        );
        assert_eq!(Permissions::EDIT_FILES.terminal, TerminalAccess::None);
        assert_eq!(Permissions::EDIT_FILES.run, RunAccess::None);
        assert_eq!(Permissions::EDIT_FILES.vcs, VcsAccess::ReadOnly);
        assert_eq!(Permissions::FULL_ACCESS.vcs, VcsAccess::Full);
        assert_eq!(Permissions::default(), Permissions::READ_ONLY);
    }

    #[test]
    fn levels_round_trip_and_custom_is_detected() {
        for l in [
            PermissionLevel::ReadOnly,
            PermissionLevel::EditFiles,
            PermissionLevel::FullAccess,
        ] {
            assert_eq!(Permissions::preset(l).unwrap().level(), l);
        }
        let custom = Permissions {
            terminal: TerminalAccess::Full,
            ..Permissions::EDIT_FILES
        };
        assert_eq!(custom.level(), PermissionLevel::Custom);
        assert_eq!(Permissions::preset(PermissionLevel::Custom), None);
        assert_eq!(
            serde_json::to_string(&PermissionLevel::EditFiles).unwrap(),
            "\"editFiles\""
        );
    }

    fn policy(p: Permissions) -> Policy {
        Policy::new(p, Excludes::defaults())
    }

    fn update() -> GuestMessage {
        GuestMessage::DocUpdate {
            doc: DocId(1),
            update: serde_bytes::ByteBuf::from(vec![0]),
        }
    }

    #[test]
    fn edits_and_saves_need_files_edit() {
        let save = GuestMessage::DocSave {
            request: 1,
            doc: DocId(1),
        };
        for (p, allowed) in [
            (Permissions::READ_ONLY, false),
            (Permissions::EDIT_FILES, true),
            (Permissions::FULL_ACCESS, true),
        ] {
            assert_eq!(policy(p).check(&update()).is_ok(), allowed);
            assert_eq!(policy(p).check(&save).is_ok(), allowed);
        }
    }

    #[test]
    fn every_level_may_look_and_leave() {
        let awareness = GuestMessage::Awareness {
            doc: DocId(1),
            update: serde_bytes::ByteBuf::new(),
        };
        let ro = policy(Permissions::READ_ONLY);
        assert!(ro.check(&awareness).is_ok());
        assert!(ro.check(&GuestMessage::DocClose { doc: DocId(1) }).is_ok());
        assert!(ro.check(&GuestMessage::Leave).is_ok());
        assert!(ro
            .check(&GuestMessage::DocOpen {
                request: 1,
                path: "app/Models/User.php".into()
            })
            .is_ok());
    }

    #[test]
    fn paths_outside_the_project_git_and_secrets_are_refused() {
        let p = policy(Permissions::FULL_ACCESS);
        for path in [
            "",
            "/etc/passwd",
            "../secret",
            "app/../../x",
            "./app",
            "app//x",
            "C:\\Windows",
            "c:/x",
            "a\u{0}b",
            ".git",
            ".git/config",
            "vendor/pkg/.git/HEAD",
            ".env",
            "config/.env.local",
            "storage/oauth-private.key",
        ] {
            let open = GuestMessage::DocOpen {
                request: 1,
                path: path.into(),
            };
            assert!(p.check(&open).is_err(), "allowed {path:?}");
        }
        assert!(p.check_path(".env.example").is_ok());
        assert!(p.check_path(".gitignore").is_ok());
    }

    #[test]
    fn a_second_knock_is_refused() {
        let knock = GuestMessage::Knock {
            protocol: 1,
            session: serde_bytes::ByteBuf::new(),
            name: "x".into(),
            proof: serde_bytes::ByteBuf::new(),
        };
        assert!(policy(Permissions::FULL_ACCESS).check(&knock).is_err());
    }
}

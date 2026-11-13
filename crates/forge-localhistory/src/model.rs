//! The event model: change sets of file entries (state before and after), labels, and the
//! limits that decide what is recorded.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The state of a path at one point in time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum FileState {
    /// Not recorded (the file was never seen, or it is too large or binary).
    #[default]
    Unknown,
    /// The path did not exist.
    Absent,
    /// A folder.
    Dir,
    /// A file with the content stored under this hash.
    File(String),
}

impl FileState {
    /// Compact form used in the change log: `?`, `-`, `/` or the content hash.
    pub fn code(&self) -> &str {
        match self {
            FileState::Unknown => "?",
            FileState::Absent => "-",
            FileState::Dir => "/",
            FileState::File(h) => h,
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "?" | "" => FileState::Unknown,
            "-" => FileState::Absent,
            "/" => FileState::Dir,
            h => FileState::File(h.to_string()),
        }
    }

    pub fn kind(&self) -> StateKind {
        match self {
            FileState::Unknown => StateKind::Unknown,
            FileState::Absent => StateKind::Absent,
            FileState::Dir => StateKind::Dir,
            FileState::File(_) => StateKind::File,
        }
    }

    pub fn hash(&self) -> Option<&str> {
        match self {
            FileState::File(h) => Some(h),
            _ => None,
        }
    }
}

impl Serialize for FileState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for FileState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = String::deserialize(d)?;
        Ok(FileState::from_code(&code))
    }
}

/// [`FileState`] without the hash, for the UI. `Current` means "as it is now" (nothing
/// changed the path since the revision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateKind {
    Unknown,
    Absent,
    Dir,
    File,
    Current,
}

/// What caused a change set. The UI shows the set's name; the kind picks the icon and
/// decides what is listed (baselines never are).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    /// A file was saved.
    Save,
    /// Editor changes recorded by a feature (e.g. a refactoring in open editors).
    Edit,
    Refactoring,
    /// Changes the file watcher found that Forge did not make.
    External,
    /// Update, checkout, rollback, unshelve and other VCS operations.
    Vcs,
    Create,
    Delete,
    Rename,
    Move,
    Copy,
    /// Local History's own Revert.
    Revert,
    /// A label (Put Label…): a named point in time across the project, without entries.
    Label,
    /// The content of a file when Forge first saw it (opened it); never listed, only used
    /// as the "before" of later changes.
    Baseline,
}

/// One path in a change set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "p")]
    pub path: String,
    #[serde(rename = "b")]
    pub before: FileState,
    #[serde(rename = "a")]
    pub after: FileState,
    /// Renames and moves: where the path came from (the old path has its own entry, with
    /// `after` absent).
    #[serde(rename = "f", default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// One event: a save, a refactoring, an external change, a label, ...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub id: u64,
    /// Milliseconds since the Unix epoch.
    pub time: i64,
    pub kind: ChangeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Entry>,
}

impl ChangeSet {
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }
}

/// Content handed to the recorder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Content {
    /// Not known to the caller; the recorder uses the last state it knows.
    #[default]
    Unknown,
    Absent,
    Dir,
    Bytes(Vec<u8>),
}

impl Content {
    /// Text from the UI: `None` = the file does not exist.
    pub fn text(text: Option<String>) -> Self {
        text.map_or(Content::Absent, |t| Content::Bytes(t.into_bytes()))
    }
}

/// A path's change handed to the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    pub path: String,
    pub before: Content,
    pub after: Content,
    pub from: Option<String>,
}

impl NewEntry {
    pub fn new(path: impl Into<String>, before: Content, after: Content) -> Self {
        NewEntry {
            path: path.into(),
            before,
            after,
            from: None,
        }
    }
}

/// A change handed to the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChange {
    pub kind: ChangeKind,
    pub name: String,
    pub time: i64,
    /// Changes of one operation (a refactoring, an update from VCS) share a group and are
    /// merged into one change set.
    pub group: Option<u64>,
    pub entries: Vec<NewEntry>,
}

/// What is recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    /// Larger files are not recorded (bytes).
    pub max_file_size: u64,
    /// Folder names (or project-relative folder paths) whose files are never recorded.
    pub excluded: Vec<String>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_file_size: 1024 * 1024,
            excluded: [".git", ".hg", ".svn", "node_modules", "vendor", ".idea"]
                .map(String::from)
                .to_vec(),
        }
    }
}

impl Limits {
    /// The path is inside an excluded folder (or is one).
    pub fn is_excluded(&self, path: &str) -> bool {
        let path = path.trim_matches('/');
        self.excluded.iter().any(|ex| {
            let ex = ex.trim_matches('/');
            if ex.is_empty() {
                return false;
            }
            if ex.contains('/') {
                path == ex || path.starts_with(&format!("{ex}/"))
            } else {
                path.split('/').any(|part| part == ex)
            }
        })
    }

    /// The content is small enough and not binary.
    pub fn accepts(&self, bytes: &[u8]) -> bool {
        bytes.len() as u64 <= self.max_file_size && !is_binary(bytes)
    }
}

/// A NUL byte in the first 8000 bytes, like git's heuristic.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// `path` is `dir` or below it; `""` is the project root.
pub fn is_under(path: &str, dir: &str) -> bool {
    dir.is_empty() || path == dir || path.starts_with(&format!("{dir}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_through_the_log_form() {
        for s in [
            FileState::Unknown,
            FileState::Absent,
            FileState::Dir,
            FileState::File("ab12".into()),
        ] {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(serde_json::from_str::<FileState>(&json).unwrap(), s);
        }
        let e = Entry {
            path: "a.php".into(),
            before: FileState::Absent,
            after: FileState::File("ff".into()),
            from: None,
        };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"p":"a.php","b":"-","a":"ff"}"#
        );
    }

    #[test]
    fn excluded_folders_and_binary_files() {
        let l = Limits::default();
        assert!(l.is_excluded("vendor/laravel/x.php"));
        assert!(l.is_excluded("web/node_modules/a.js"));
        assert!(l.is_excluded(".git"));
        assert!(!l.is_excluded("app/vendorish/x.php"));
        let l2 = Limits {
            excluded: vec!["storage/logs".into()],
            ..Limits::default()
        };
        assert!(l2.is_excluded("storage/logs/laravel.log"));
        assert!(!l2.is_excluded("storage/app/a.txt"));
        assert!(l.accepts(b"<?php echo 1;"));
        assert!(!l.accepts(b"\x89PNG\0\0"));
        let small = Limits {
            max_file_size: 3,
            ..Limits::default()
        };
        assert!(!small.accepts(b"four"));
        assert!(is_under("app/A.php", "app"));
        assert!(is_under("app/A.php", ""));
        assert!(!is_under("application/A.php", "app"));
    }
}

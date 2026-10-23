//! State Forge keeps per project outside the project folder:
//!
//! - the window layout (open tool windows, where each tool window sits, their sizes and tabs,
//!   open editor tabs) in Forge's state dir (`<state dir>/forge/layouts/<project key>.json`;
//!   the state dir is `$XDG_STATE_HOME` on Linux and the local data dir elsewhere), plus the
//!   layout stored with Window | Store Current Layout as Default (`layouts/_default.json`);
//! - the AI chat history in the user data dir
//!   (`<data dir>/forge/chat-history/<project key>/<id>.json`);
//! - the Test History of the Run tool window in the user data dir
//!   (`<data dir>/forge/test-history/<project key>/<id>.json`);
//! - small per-project lists such as the Run Anything history in the state dir
//!   (`<state dir>/forge/<kind>/<project key>.json`, see [`load_list`]).
//!
//! Nothing is written into the project itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ForgeError, Result};

/// A stable, file-name safe key for a project root: its folder name plus a hash of the
/// whole root (`shop-3f2a…`), so two projects called `app` do not share state.
pub fn project_key(root: &str) -> String {
    let name: String = root
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    // FNV-1a, stable across Rust versions (unlike `DefaultHasher`).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in root.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    if name.is_empty() {
        format!("{hash:016x}")
    } else {
        format!("{name}-{hash:016x}")
    }
}

// ---- Layout --------------------------------------------------------------------------

/// Size limits of the tool window areas (pixels), as the UI enforces them when resizing.
const SIDE_SIZE: (u32, u32) = (180, 900);
const BOTTOM_SIZE: (u32, u32) = (100, 700);
/// Most editor tabs restored.
const MAX_OPEN_FILES: usize = 50;
/// Most tool window placements kept.
const MAX_PLACEMENTS: usize = 200;
const ANCHORS: [&str; 3] = ["left", "right", "bottom"];
/// Where a tool window can sit, like PHPStorm's New UI: the primary and the split ("secondary")
/// group of each anchor. `left`/`leftSplit` are Left Top/Left Bottom, `right`/`rightSplit` Right
/// Top/Right Bottom, `bottom`/`bottomSplit` Bottom Left/Bottom Right.
pub const SLOTS: [&str; 6] = [
    "left",
    "leftSplit",
    "right",
    "rightSplit",
    "bottom",
    "bottomSplit",
];
/// Share of the first tool window when an area shows two (per mille).
const SPLIT_RATIO: (u32, u32) = (100, 900);

/// Where the user put a tool window (the registration supplies the default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolWindowPlacement {
    /// One of [`SLOTS`].
    pub slot: String,
    /// Position inside its stripe group (lower first).
    #[serde(default)]
    pub order: u32,
    /// Removed from the sidebar: no stripe button until the window is opened another way.
    #[serde(default)]
    pub removed: bool,
}

/// The workbench layout of one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectLayout {
    /// Visible tool window per slot (see [`SLOTS`]); `None` = nothing open there.
    pub active: BTreeMap<String, Option<String>>,
    /// Size of each anchor's area in pixels.
    pub sizes: BTreeMap<String, u32>,
    /// Share of the first tool window per anchor when it shows two (per mille, 100–900).
    pub splits: BTreeMap<String, u32>,
    /// Tool windows the user moved, reordered or removed from the sidebar (id → placement).
    /// Layouts saved before placements existed have none: every window uses its default.
    pub tool_windows: BTreeMap<String, ToolWindowPlacement>,
    /// Selected tab inside tool windows that have tabs (tool window id → tab id).
    pub tabs: BTreeMap<String, String>,
    /// Open editor tabs (workspace-relative paths) in order.
    pub open_files: Vec<String>,
    /// The active editor tab.
    pub active_file: Option<String>,
}

impl ProjectLayout {
    /// Drop what cannot be restored: unknown anchors and slots, out-of-range sizes and split
    /// ratios, placements of odd ids, absolute paths or `..` escapes, duplicate tabs.
    pub fn sanitize(&mut self) {
        self.active.retain(|slot, _| SLOTS.contains(&slot.as_str()));
        self.splits
            .retain(|anchor, _| ANCHORS.contains(&anchor.as_str()));
        for ratio in self.splits.values_mut() {
            *ratio = (*ratio).clamp(SPLIT_RATIO.0, SPLIT_RATIO.1);
        }
        self.tool_windows
            .retain(|id, p| !id.is_empty() && id.len() <= 128 && SLOTS.contains(&p.slot.as_str()));
        while self.tool_windows.len() > MAX_PLACEMENTS {
            self.tool_windows.pop_last();
        }
        self.sizes
            .retain(|anchor, _| ANCHORS.contains(&anchor.as_str()));
        for (anchor, size) in self.sizes.iter_mut() {
            let (min, max) = if anchor == "bottom" {
                BOTTOM_SIZE
            } else {
                SIDE_SIZE
            };
            *size = (*size).clamp(min, max);
        }
        let mut seen = std::collections::HashSet::new();
        self.open_files
            .retain(|p| is_relative_path(p) && seen.insert(p.clone()));
        self.open_files.truncate(MAX_OPEN_FILES);
        if self
            .active_file
            .as_ref()
            .is_some_and(|a| !self.open_files.contains(a))
        {
            self.active_file = self.open_files.last().cloned();
        }
    }
}

fn is_relative_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.starts_with('\\')
        && !p.contains(':')
        && !p.split(['/', '\\']).any(|s| s == "..")
}

/// `<state dir>/forge/layouts`: `~/.local/state` on Linux, the local data dir on macOS and
/// Windows (which have no separate state dir).
pub fn default_layout_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|d| d.join("forge").join("layouts"))
}

pub fn load_layout(dir: &Path, root: &str) -> Option<ProjectLayout> {
    let text = fs::read_to_string(dir.join(format!("{}.json", project_key(root)))).ok()?;
    let mut layout: ProjectLayout = serde_json::from_str(&text).ok()?;
    layout.sanitize();
    Some(layout)
}

pub fn save_layout(dir: &Path, root: &str, layout: &ProjectLayout) -> Result<()> {
    let mut layout = layout.clone();
    layout.sanitize();
    write_json(&dir.join(format!("{}.json", project_key(root))), &layout)
}

/// The file of the default layout. Project keys always end in a hex hash, so the name never
/// clashes with a project's layout.
fn default_layout_file(dir: &Path) -> PathBuf {
    dir.join("_default.json")
}

/// Only the tool window part of a layout counts as the default: placements, open tool
/// windows, sizes and split ratios, but no editor tabs or tool window tabs.
fn window_part(mut layout: ProjectLayout) -> ProjectLayout {
    layout.sanitize();
    layout.tabs.clear();
    layout.open_files.clear();
    layout.active_file = None;
    layout
}

/// The layout stored with Window | Store Current Layout as Default, for every project.
pub fn load_default_layout(dir: &Path) -> Option<ProjectLayout> {
    let text = fs::read_to_string(default_layout_file(dir)).ok()?;
    Some(window_part(serde_json::from_str(&text).ok()?))
}

pub fn save_default_layout(dir: &Path, layout: &ProjectLayout) -> Result<()> {
    write_json(&default_layout_file(dir), &window_part(layout.clone()))
}

// ---- Chat history --------------------------------------------------------------------

/// Conversations kept per project; older ones are deleted.
pub const MAX_CHATS: usize = 100;

/// A saved AI chat. `conversation` is the UI's conversation model, stored as it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRecord {
    pub id: String,
    pub agent_id: String,
    /// Milliseconds since the Unix epoch of the last change.
    pub at: i64,
    #[serde(default)]
    pub title: String,
    pub conversation: Value,
}

/// A history list entry (without the conversation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    pub id: String,
    pub agent_id: String,
    pub at: i64,
    pub title: String,
    pub turns: usize,
}

/// The title of a conversation: the first line of its first prompt.
pub fn chat_title(conversation: &Value) -> String {
    let prompt = conversation["turns"][0]["prompt"].as_str().unwrap_or("");
    let line = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let title: String = line.trim().chars().take(80).collect();
    if title.is_empty() {
        "New chat".into()
    } else {
        title
    }
}

fn turns(conversation: &Value) -> usize {
    conversation["turns"].as_array().map_or(0, Vec::len)
}

/// `<data dir>/forge/test-history` (the Run tool window's Test History, see
/// `forge_process::test_history`).
pub fn default_test_history_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("test-history"))
}

/// `<data dir>/forge/chat-history`.
pub fn default_chat_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("chat-history"))
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
        Err(ForgeError::Other(format!("invalid chat id {id:?}")))
    }
}

/// The chats of one project, newest first.
pub fn list_chats(dir: &Path, root: &str) -> Vec<ChatSummary> {
    let Ok(entries) = fs::read_dir(dir.join(project_key(root))) else {
        return Vec::new();
    };
    let mut out: Vec<ChatSummary> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str::<ChatRecord>(&t).ok())
        .map(|r| ChatSummary {
            turns: turns(&r.conversation),
            title: if r.title.is_empty() {
                chat_title(&r.conversation)
            } else {
                r.title
            },
            id: r.id,
            agent_id: r.agent_id,
            at: r.at,
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    out
}

pub fn load_chat(dir: &Path, root: &str, id: &str) -> Result<ChatRecord> {
    let path = dir
        .join(project_key(root))
        .join(format!("{}.json", safe_id(id)?));
    let text = fs::read_to_string(&path).map_err(|e| ForgeError::io(&path, e))?;
    serde_json::from_str(&text).map_err(|e| ForgeError::Other(e.to_string()))
}

/// Save a chat (empty conversations are not kept) and prune the oldest beyond
/// [`MAX_CHATS`]. Returns the stored summary.
pub fn save_chat(dir: &Path, root: &str, mut record: ChatRecord) -> Result<Option<ChatSummary>> {
    safe_id(&record.id)?;
    if turns(&record.conversation) == 0 {
        delete_chat(dir, root, &record.id)?;
        return Ok(None);
    }
    if record.title.trim().is_empty() {
        record.title = chat_title(&record.conversation);
    }
    let folder = dir.join(project_key(root));
    write_json(&folder.join(format!("{}.json", record.id)), &record)?;
    for old in list_chats(dir, root).into_iter().skip(MAX_CHATS) {
        let _ = fs::remove_file(folder.join(format!("{}.json", old.id)));
    }
    Ok(Some(ChatSummary {
        turns: turns(&record.conversation),
        id: record.id,
        agent_id: record.agent_id,
        at: record.at,
        title: record.title,
    }))
}

pub fn delete_chat(dir: &Path, root: &str, id: &str) -> Result<()> {
    let path = dir
        .join(project_key(root))
        .join(format!("{}.json", safe_id(id)?));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ForgeError::io(&path, e)),
    }
}

// ---- Small per-project lists ---------------------------------------------------------

/// `<state dir>/forge/<kind>` (e.g. `run-anything`), next to the layouts.
pub fn default_list_dir(kind: &str) -> Option<PathBuf> {
    default_layout_dir().and_then(|d| d.parent().map(|p| p.join(kind)))
}

/// A list stored for the project at `root` (the Run Anything history); empty when missing or
/// unreadable.
pub fn load_list<T: serde::de::DeserializeOwned>(dir: &Path, root: &str) -> Vec<T> {
    fs::read_to_string(dir.join(format!("{}.json", project_key(root))))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_list<T: Serialize>(dir: &Path, root: &str, items: &[T]) -> Result<()> {
    write_json(&dir.join(format!("{}.json", project_key(root))), &items)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| ForgeError::Other(e.to_string()))?;
    // Write a sibling file first so a crash never leaves half a file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| ForgeError::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| ForgeError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lists_are_stored_per_project() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_list::<String>(dir.path(), "/p/shop").is_empty());
        save_list(dir.path(), "/p/shop", &["npm run dev".to_string()]).unwrap();
        assert_eq!(load_list::<String>(dir.path(), "/p/shop"), ["npm run dev"]);
        assert!(load_list::<String>(dir.path(), "/p/other").is_empty());
        assert!(default_list_dir("run-anything").is_none_or(|d| d.ends_with("forge/run-anything")));
    }

    #[test]
    fn project_keys_are_stable_and_distinct() {
        let a = project_key("/home/me/work/app");
        assert!(a.starts_with("app-"), "{a}");
        assert_eq!(a, project_key("/home/me/work/app"));
        assert_ne!(a, project_key("/home/me/other/app"));
        assert!(project_key("me@host:/srv/my shop").starts_with("my_shop-"));
        assert_eq!(project_key("/").len(), 16);
    }

    #[test]
    fn layout_round_trips_and_is_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let mut layout = ProjectLayout::default();
        layout.active.insert("left".into(), Some("project".into()));
        layout.active.insert("bottom".into(), None);
        layout.active.insert("middle".into(), Some("x".into()));
        layout.sizes.insert("left".into(), 5000);
        layout.sizes.insert("bottom".into(), 10);
        layout.tabs.insert("git".into(), "log".into());
        layout.open_files = vec![
            "app/User.php".into(),
            "../secret".into(),
            "/etc/passwd".into(),
            "app/User.php".into(),
            "routes/web.php".into(),
        ];
        layout.active_file = Some("gone.php".into());
        save_layout(dir.path(), "/p", &layout).unwrap();
        let back = load_layout(dir.path(), "/p").unwrap();
        assert_eq!(back.active.len(), 2);
        assert_eq!(back.active["bottom"], None);
        assert_eq!(back.sizes["left"], 900);
        assert_eq!(back.sizes["bottom"], 100);
        assert_eq!(back.tabs["git"], "log");
        assert_eq!(back.open_files, vec!["app/User.php", "routes/web.php"]);
        assert_eq!(back.active_file.as_deref(), Some("routes/web.php"));
        assert_eq!(load_layout(dir.path(), "/other"), None);
        // The JSON uses camelCase keys.
        let text = serde_json::to_string(&back).unwrap();
        assert!(text.contains("\"openFiles\""), "{text}");
    }

    #[test]
    fn tool_window_placements_and_splits_are_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let mut layout = ProjectLayout::default();
        layout
            .active
            .insert("leftSplit".into(), Some("structure".into()));
        layout
            .active
            .insert("bottomSplit".into(), Some("terminal".into()));
        layout.active.insert("top".into(), Some("x".into()));
        layout.splits.insert("left".into(), 20);
        layout.splits.insert("bottom".into(), 600);
        layout.splits.insert("leftSplit".into(), 500);
        let place = |slot: &str, order, removed| ToolWindowPlacement {
            slot: slot.into(),
            order,
            removed,
        };
        layout
            .tool_windows
            .insert("terminal".into(), place("right", 20, false));
        layout
            .tool_windows
            .insert("todo".into(), place("bottom", 0, true));
        layout
            .tool_windows
            .insert("odd".into(), place("middle", 0, false));
        layout
            .tool_windows
            .insert(String::new(), place("left", 0, false));
        save_layout(dir.path(), "/p", &layout).unwrap();
        let back = load_layout(dir.path(), "/p").unwrap();
        assert_eq!(
            back.active.keys().collect::<Vec<_>>(),
            vec!["bottomSplit", "leftSplit"]
        );
        assert_eq!(back.splits.len(), 2);
        assert_eq!(back.splits["left"], 100);
        assert_eq!(back.splits["bottom"], 600);
        assert_eq!(back.tool_windows.len(), 2);
        assert_eq!(back.tool_windows["terminal"], place("right", 20, false));
        assert!(back.tool_windows["todo"].removed);
        let text = serde_json::to_string(&back).unwrap();
        assert!(text.contains("\"toolWindows\""), "{text}");
    }

    #[test]
    fn layouts_saved_before_placements_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{"active":{"left":"project","bottom":null},"sizes":{"left":320},
            "tabs":{},"openFiles":["a.php"],"activeFile":"a.php"}"#;
        fs::write(dir.path().join(format!("{}.json", project_key("/p"))), old).unwrap();
        let back = load_layout(dir.path(), "/p").unwrap();
        assert_eq!(back.active["left"].as_deref(), Some("project"));
        assert_eq!(back.sizes["left"], 320);
        assert!(back.tool_windows.is_empty());
        assert!(back.splits.is_empty());
        // A placement without order or removed flag uses the defaults.
        let p: ToolWindowPlacement = serde_json::from_str(r#"{"slot":"right"}"#).unwrap();
        assert_eq!((p.order, p.removed), (0, false));
    }

    #[test]
    fn default_layout_keeps_only_the_tool_windows() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_default_layout(dir.path()), None);
        let mut layout = ProjectLayout::default();
        layout
            .active
            .insert("right".into(), Some("terminal".into()));
        layout.tool_windows.insert(
            "terminal".into(),
            ToolWindowPlacement {
                slot: "right".into(),
                order: 5,
                removed: false,
            },
        );
        layout.tabs.insert("git".into(), "log".into());
        layout.open_files = vec!["a.php".into()];
        layout.active_file = Some("a.php".into());
        save_default_layout(dir.path(), &layout).unwrap();
        let back = load_default_layout(dir.path()).unwrap();
        assert_eq!(back.active["right"].as_deref(), Some("terminal"));
        assert_eq!(back.tool_windows["terminal"].order, 5);
        assert!(back.tabs.is_empty() && back.open_files.is_empty());
        assert_eq!(back.active_file, None);
        // Not mistaken for a project's layout.
        assert_eq!(load_layout(dir.path(), "_default"), None);
    }

    fn conv(prompt: &str) -> Value {
        json!({ "conversationId": "abc", "turns": [{ "prompt": prompt, "blocks": [] }] })
    }

    #[test]
    fn chats_are_saved_listed_loaded_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let save = |id: &str, at: i64, c: Value| {
            save_chat(
                dir.path(),
                "/p",
                ChatRecord {
                    id: id.into(),
                    agent_id: "claude-code".into(),
                    at,
                    title: String::new(),
                    conversation: c,
                },
            )
        };
        let s = save("c1", 10, conv("\n  Explain the User model\nmore"))
            .unwrap()
            .unwrap();
        assert_eq!(s.title, "Explain the User model");
        save("c2", 20, conv("Second")).unwrap();
        assert!(save("c3", 30, json!({ "turns": [] })).unwrap().is_none());
        let list = list_chats(dir.path(), "/p");
        assert_eq!(
            list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["c2", "c1"]
        );
        assert_eq!(list[0].turns, 1);
        assert_eq!(
            load_chat(dir.path(), "/p", "c1").unwrap().conversation["conversationId"],
            "abc"
        );
        assert!(list_chats(dir.path(), "/q").is_empty());
        delete_chat(dir.path(), "/p", "c1").unwrap();
        delete_chat(dir.path(), "/p", "c1").unwrap();
        assert_eq!(list_chats(dir.path(), "/p").len(), 1);
        assert!(load_chat(dir.path(), "/p", "../x").is_err());
    }

    #[test]
    fn keeps_at_most_max_chats() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(MAX_CHATS + 3) {
            save_chat(
                dir.path(),
                "/p",
                ChatRecord {
                    id: format!("c{i}"),
                    agent_id: "a".into(),
                    at: i as i64,
                    title: "t".into(),
                    conversation: conv("x"),
                },
            )
            .unwrap();
        }
        let list = list_chats(dir.path(), "/p");
        assert_eq!(list.len(), MAX_CHATS);
        assert_eq!(list.last().unwrap().id, "c3");
    }
}

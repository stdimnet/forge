//! The terminal layout of a project, like PHPStorm's workspace state: the tabs of the
//! Terminal tool window with their names, working directories and shell profiles, and which
//! one is selected. It is restored when the project opens again.
//!
//! - The layout is workspace state, outside the project:
//!   `<state dir>/forge/terminal-layouts/<project key>.json`.
//! - The scrollback of each tab is optional (setting `terminal.restoreScrollback`), capped at
//!   [`MAX_SCROLLBACK_BYTES`] and kept in the user cache dir:
//!   `<cache dir>/forge/terminal-scrollback/<project key>/<tab id>.txt`. Never in the project.
//!
//! Tabs that only make sense while their session runs (SSH sessions, `docker exec` from the
//! Services tool window, Code With Me terminals) are not part of the layout: the UI leaves
//! them out and [`TerminalLayout::sanitize`] drops what does not fit.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use forge_core::project_state::project_key;
use forge_core::{ForgeError, Result};

/// Most tabs restored.
pub const MAX_TABS: usize = 30;
/// Longest tab title kept.
const MAX_TITLE: usize = 120;
/// Longest tab id (ids name the scrollback files).
const MAX_ID: usize = 64;
/// Scrollback kept per tab (the end of the output).
pub const MAX_SCROLLBACK_BYTES: usize = 256 * 1024;

/// A shell profile of the "+" menu a tab was opened with ("WSL: Ubuntu", "PowerShell").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalProfile {
    /// The profile's label, so the restored tab follows a profile that changed since.
    pub label: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
}

/// One terminal tab of the layout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTabState {
    /// The tab's id, also its session id and the name of its scrollback file.
    pub id: String,
    pub title: String,
    /// Renamed by the user: the title stays, whatever the program sets.
    #[serde(default)]
    pub renamed: bool,
    /// Start directory: relative to the project root, or absolute (the directory the shell
    /// was in when the project closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Opened with a shell profile instead of the default shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<TerminalProfile>,
    /// The saved scrollback, filled by [`load`] (never stored in the layout file itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scrollback: Option<String>,
}

/// The Terminal tool window of a project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayout {
    pub tabs: Vec<TerminalTabState>,
    /// The selected tab's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

/// File-name safe tab ids: letters, digits, `-` and `_`.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl TerminalLayout {
    /// Keep what can be restored: tabs with a valid, unique id and a title (cut to length),
    /// at most [`MAX_TABS`]; empty directories and profiles without a program are dropped,
    /// and `active` names a kept tab (else the first one).
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.tabs
            .retain(|t| valid_id(&t.id) && seen.insert(t.id.clone()));
        self.tabs.truncate(MAX_TABS);
        for tab in &mut self.tabs {
            tab.title = tab.title.trim().chars().take(MAX_TITLE).collect();
            if tab.title.is_empty() {
                tab.title = "Local".into();
            }
            if tab.cwd.as_deref().is_some_and(|d| d.trim().is_empty()) {
                tab.cwd = None;
            }
            if tab
                .profile
                .as_ref()
                .is_some_and(|p| p.program.trim().is_empty())
            {
                tab.profile = None;
            }
            tab.scrollback = None;
        }
        if !self
            .active
            .as_ref()
            .is_some_and(|a| self.tabs.iter().any(|t| &t.id == a))
        {
            self.active = self.tabs.first().map(|t| t.id.clone());
        }
    }
}

/// `<state dir>/forge/terminal-layouts` (the state dir is `$XDG_STATE_HOME` on Linux and the
/// local data dir elsewhere).
pub fn default_layout_dir() -> Option<PathBuf> {
    forge_core::project_state::default_list_dir("terminal-layouts")
}

/// `<cache dir>/forge/terminal-scrollback`.
pub fn default_scrollback_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("forge").join("terminal-scrollback"))
}

/// The saved layout of the project at `root`, with each tab's scrollback from
/// `scrollback_dir` when one is given.
pub fn load(dir: &Path, scrollback_dir: Option<&Path>, root: &str) -> Option<TerminalLayout> {
    let text = fs::read_to_string(dir.join(format!("{}.json", project_key(root)))).ok()?;
    let mut layout: TerminalLayout = serde_json::from_str(&text).ok()?;
    layout.sanitize();
    if let Some(sb) = scrollback_dir {
        let folder = sb.join(project_key(root));
        for tab in &mut layout.tabs {
            tab.scrollback = fs::read_to_string(folder.join(format!("{}.txt", tab.id)))
                .ok()
                .filter(|s| !s.is_empty());
        }
    }
    Some(layout)
}

/// Store the layout of the project at `root` (sanitized; an empty layout removes the file).
pub fn save(dir: &Path, root: &str, layout: &TerminalLayout) -> Result<()> {
    let mut layout = layout.clone();
    layout.sanitize();
    let path = dir.join(format!("{}.json", project_key(root)));
    if layout.tabs.is_empty() {
        return remove(&path, fs::remove_file(&path));
    }
    fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
    let json =
        serde_json::to_string_pretty(&layout).map_err(|e| ForgeError::Other(e.to_string()))?;
    fs::write(&path, json).map_err(|e| ForgeError::io(&path, e))
}

/// A removal that found nothing to remove is fine.
fn remove(path: &Path, result: std::io::Result<()>) -> Result<()> {
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(ForgeError::io(path, e)),
        _ => Ok(()),
    }
}

/// The end of `text`, at most [`MAX_SCROLLBACK_BYTES`], starting at a line when it was cut.
pub fn cap_scrollback(text: &str) -> &str {
    if text.len() <= MAX_SCROLLBACK_BYTES {
        return text;
    }
    let mut start = text.len() - MAX_SCROLLBACK_BYTES;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let rest = &text[start..];
    match rest.find('\n') {
        Some(at) => &rest[at + 1..],
        None => rest,
    }
}

/// Replace the saved scrollback of the project at `root` with `tabs` (`(tab id, output)`),
/// each capped with [`cap_scrollback`]. Tabs with an invalid id or no output are skipped.
pub fn save_scrollback(dir: &Path, root: &str, tabs: &[(String, String)]) -> Result<()> {
    let folder = dir.join(project_key(root));
    clear_scrollback(dir, root)?;
    let mut created = false;
    for (id, text) in tabs {
        if !valid_id(id) || text.is_empty() {
            continue;
        }
        if !created {
            fs::create_dir_all(&folder).map_err(|e| ForgeError::io(&folder, e))?;
            created = true;
        }
        let file = folder.join(format!("{id}.txt"));
        fs::write(&file, cap_scrollback(text)).map_err(|e| ForgeError::io(&file, e))?;
    }
    Ok(())
}

/// Forget the saved scrollback of the project at `root`.
pub fn clear_scrollback(dir: &Path, root: &str) -> Result<()> {
    let folder = dir.join(project_key(root));
    remove(&folder, fs::remove_dir_all(&folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &str, title: &str, cwd: Option<&str>) -> TerminalTabState {
        TerminalTabState {
            id: id.into(),
            title: title.into(),
            cwd: cwd.map(Into::into),
            ..TerminalTabState::default()
        }
    }

    fn two_tabs() -> TerminalLayout {
        let mut wsl = tab("term-2", "Ubuntu", Some("/home/me/shop/app"));
        wsl.renamed = true;
        wsl.profile = Some(TerminalProfile {
            label: "WSL: Ubuntu".into(),
            program: "wsl.exe".into(),
            args: vec!["-d".into(), "Ubuntu".into()],
            env: vec![],
        });
        TerminalLayout {
            tabs: vec![tab("term-1", "Local", Some("app")), wsl],
            active: Some("term-2".into()),
        }
    }

    #[test]
    fn round_trips_a_layout_per_project_with_scrollback() {
        let state = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let layout = two_tabs();
        save(state.path(), "/p/shop", &layout).unwrap();
        save_scrollback(
            cache.path(),
            "/p/shop",
            &[("term-1".into(), "$ ls\r\nartisan\r\n".into())],
        )
        .unwrap();
        // Another project keeps its own.
        save(
            state.path(),
            "/p/blog",
            &TerminalLayout {
                tabs: vec![tab("term-9", "Local", None)],
                active: None,
            },
        )
        .unwrap();

        let loaded = load(state.path(), Some(cache.path()), "/p/shop").unwrap();
        assert_eq!(loaded.active.as_deref(), Some("term-2"));
        assert_eq!(loaded.tabs.len(), 2);
        assert_eq!(loaded.tabs[0].cwd.as_deref(), Some("app"));
        assert_eq!(
            loaded.tabs[0].scrollback.as_deref(),
            Some("$ ls\r\nartisan\r\n")
        );
        assert_eq!(loaded.tabs[1].scrollback, None);
        assert_eq!(loaded.tabs[1].profile, layout.tabs[1].profile);
        assert!(loaded.tabs[1].renamed);
        // Without the scrollback dir (the option is off) no history comes back.
        assert_eq!(
            load(state.path(), None, "/p/shop").unwrap().tabs[0].scrollback,
            None
        );
        assert_eq!(
            load(state.path(), None, "/p/blog").unwrap().tabs[0].id,
            "term-9"
        );
        assert!(load(state.path(), None, "/p/other").is_none());

        // An empty layout removes the file; scrollback can be cleared.
        save(state.path(), "/p/shop", &TerminalLayout::default()).unwrap();
        assert!(load(state.path(), None, "/p/shop").is_none());
        clear_scrollback(cache.path(), "/p/shop").unwrap();
        assert!(!cache.path().join(project_key("/p/shop")).exists());
    }

    #[test]
    fn sanitize_keeps_what_can_be_restored() {
        let mut layout = TerminalLayout {
            tabs: vec![
                tab("../../etc", "Evil", None),
                tab("term-1", "  ", Some("  ")),
                tab("term-1", "Duplicate", None),
                TerminalTabState {
                    profile: Some(TerminalProfile::default()),
                    scrollback: Some("not stored".into()),
                    ..tab("term-2", &"x".repeat(500), None)
                },
            ],
            active: Some("gone".into()),
        };
        layout.sanitize();
        let ids: Vec<_> = layout.tabs.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["term-1", "term-2"]);
        assert_eq!(layout.tabs[0].title, "Local");
        assert_eq!(layout.tabs[0].cwd, None);
        assert_eq!(layout.tabs[1].title.len(), MAX_TITLE);
        assert_eq!(layout.tabs[1].profile, None);
        assert_eq!(layout.tabs[1].scrollback, None);
        assert_eq!(layout.active.as_deref(), Some("term-1"));

        let mut many = TerminalLayout {
            tabs: (0..50)
                .map(|i| tab(&format!("t{i}"), "Local", None))
                .collect(),
            active: None,
        };
        many.sanitize();
        assert_eq!(many.tabs.len(), MAX_TABS);
    }

    #[test]
    fn caps_the_scrollback_at_a_line() {
        let line = "0123456789abcdef\n";
        let text = line.repeat(MAX_SCROLLBACK_BYTES / line.len() + 100);
        let capped = cap_scrollback(&text);
        assert!(capped.len() <= MAX_SCROLLBACK_BYTES);
        assert!(capped.starts_with("0123"));
        assert!(text.ends_with(capped));
        assert_eq!(cap_scrollback("short"), "short");
        // Multi-byte text is cut at a character boundary.
        let wide = "ä".repeat(MAX_SCROLLBACK_BYTES);
        assert!(cap_scrollback(&wide).len() <= MAX_SCROLLBACK_BYTES);
    }
}

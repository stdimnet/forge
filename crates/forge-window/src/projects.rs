//! Several project windows, like PHPStorm: every project window is a Forge process of its own
//! with its own project state (workspace, settings, language servers, terminals). The running
//! instances find each other through a small registry file and a local control port per
//! instance, which answers "who are you" and "come to the front" requests.
//!
//! This module holds the pure parts: the registry ([`ProjectWindows`]: order, Next/Previous
//! Project Window, pruning), the control protocol ([`control_line`], [`parse_control_line`])
//! and where Open Project puts a project ([`open_target`]). The Tauri shell does the I/O.

use serde::{Deserialize, Serialize};

/// One running project window (a Forge instance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWindowEntry {
    /// Unique per instance (`<pid>-<start time>`).
    pub id: String,
    pub pid: u32,
    /// Local control port (`127.0.0.1`).
    pub port: u16,
    /// Secret the control port expects in every request.
    pub token: String,
    /// When the window opened (milliseconds since the Unix epoch); orders the windows.
    pub opened: u64,
}

/// The registry of running project windows, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectWindows {
    #[serde(default)]
    pub windows: Vec<ProjectWindowEntry>,
}

/// Most windows the registry keeps (older entries of crashed instances are pruned anyway).
const MAX_WINDOWS: usize = 64;

impl ProjectWindows {
    /// Parse the registry file; a missing or broken file is an empty registry.
    pub fn from_json(text: &str) -> Self {
        let mut windows: Self = serde_json::from_str(text).unwrap_or_default();
        windows.normalize();
        windows
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    fn normalize(&mut self) {
        self.windows.retain(|w| !w.id.is_empty() && w.port != 0);
        self.windows
            .sort_by(|a, b| a.opened.cmp(&b.opened).then_with(|| a.id.cmp(&b.id)));
        let mut seen = std::collections::HashSet::new();
        self.windows.retain(|w| seen.insert(w.id.clone()));
        while self.windows.len() > MAX_WINDOWS {
            self.windows.remove(0);
        }
    }

    /// Add a window, or update the entry with the same id.
    pub fn register(&mut self, entry: ProjectWindowEntry) {
        self.windows.retain(|w| w.id != entry.id);
        self.windows.push(entry);
        self.normalize();
    }

    /// Remove a window; `false` when it was not registered.
    pub fn unregister(&mut self, id: &str) -> bool {
        let before = self.windows.len();
        self.windows.retain(|w| w.id != id);
        before != self.windows.len()
    }

    /// Keep only the windows `alive` confirms (e.g. their control port answers).
    pub fn retain_alive(&mut self, mut alive: impl FnMut(&ProjectWindowEntry) -> bool) -> bool {
        let before = self.windows.len();
        self.windows.retain(|w| alive(w));
        before != self.windows.len()
    }

    pub fn get(&self, id: &str) -> Option<&ProjectWindowEntry> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// Next Project Window (`delta = 1`) / Previous Project Window (`delta = -1`) from the
    /// window `current`, wrapping around. `None` when there is no other window.
    pub fn cycle(&self, current: &str, delta: i32) -> Option<&ProjectWindowEntry> {
        let n = self.windows.len() as i64;
        if n < 2 {
            return None;
        }
        let at = self.windows.iter().position(|w| w.id == current)? as i64;
        let next = (at + i64::from(delta)).rem_euclid(n) as usize;
        Some(&self.windows[next]).filter(|w| w.id != current)
    }
}

/// What the control port of a window is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRequest {
    /// Answer with the window's [`ProjectWindowInfo`] as one JSON line.
    Info,
    /// Bring the window to the front.
    Focus,
}

impl ControlRequest {
    fn verb(self) -> &'static str {
        match self {
            ControlRequest::Info => "info",
            ControlRequest::Focus => "focus",
        }
    }
}

/// The request line sent to a window's control port.
pub fn control_line(token: &str, request: ControlRequest) -> String {
    format!("forge-window {token} {}\n", request.verb())
}

/// Parse a request line; `None` for anything else, including a wrong token.
pub fn parse_control_line(line: &str, token: &str) -> Option<ControlRequest> {
    let mut parts = line.trim_end_matches(['\r', '\n']).split(' ');
    if parts.next()? != "forge-window" || token.is_empty() || parts.next()? != token {
        return None;
    }
    let request = match parts.next()? {
        "info" => ControlRequest::Info,
        "focus" => ControlRequest::Focus,
        _ => return None,
    };
    parts.next().is_none().then_some(request)
}

/// A project window as Window | Next Project Window and the window list show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWindowInfo {
    pub id: String,
    /// The open project's name; `None` on the Welcome screen.
    #[serde(default)]
    pub name: Option<String>,
    /// The open project's root as the UI shows it (`user@host:/path` for remote projects).
    #[serde(default)]
    pub root: Option<String>,
    /// This is the window that asked.
    #[serde(default)]
    pub current: bool,
}

/// Where Open Project puts a project (Settings | System Settings | Project: "Open project in").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenTarget {
    /// Ask with the "Open Project" dialog (New Window / This Window).
    Ask,
    NewWindow,
    ThisWindow,
}

/// The target for the setting value (`ask | newWindow | thisWindow`). A window without a
/// project (the Welcome screen) always opens the project itself.
pub fn open_target(setting: &str, has_project: bool) -> OpenTarget {
    if !has_project {
        return OpenTarget::ThisWindow;
    }
    match setting {
        "newWindow" => OpenTarget::NewWindow,
        "thisWindow" | "currentWindow" => OpenTarget::ThisWindow,
        _ => OpenTarget::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, opened: u64) -> ProjectWindowEntry {
        ProjectWindowEntry {
            id: id.into(),
            pid: 1,
            port: 4000,
            token: "t".into(),
            opened,
        }
    }

    #[test]
    fn registry_keeps_windows_in_opening_order() {
        let mut reg = ProjectWindows::default();
        reg.register(entry("b", 20));
        reg.register(entry("a", 10));
        reg.register(entry("c", 30));
        let ids = |r: &ProjectWindows| r.windows.iter().map(|w| w.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&reg), ["a", "b", "c"]);
        // Registering again updates the entry instead of adding one.
        let mut moved = entry("a", 10);
        moved.port = 5000;
        reg.register(moved);
        assert_eq!(ids(&reg), ["a", "b", "c"]);
        assert_eq!(reg.get("a").unwrap().port, 5000);
        assert!(reg.unregister("b"));
        assert!(!reg.unregister("b"));
        assert_eq!(ids(&reg), ["a", "c"]);
    }

    #[test]
    fn registry_round_trips_and_tolerates_broken_files() {
        let mut reg = ProjectWindows::default();
        reg.register(entry("a", 1));
        let back = ProjectWindows::from_json(&reg.to_json());
        assert_eq!(back, reg);
        assert!(ProjectWindows::from_json("not json").windows.is_empty());
        assert!(ProjectWindows::from_json("").windows.is_empty());
        // Entries without id or port and duplicates are dropped.
        let text = r#"{"windows":[
            {"id":"","pid":1,"port":1,"token":"t","opened":1},
            {"id":"x","pid":1,"port":0,"token":"t","opened":1},
            {"id":"y","pid":1,"port":9,"token":"t","opened":2},
            {"id":"y","pid":1,"port":9,"token":"t","opened":2}]}"#;
        assert_eq!(ProjectWindows::from_json(text).windows.len(), 1);
    }

    #[test]
    fn dead_windows_are_pruned() {
        let mut reg = ProjectWindows::default();
        reg.register(entry("a", 1));
        reg.register(entry("b", 2));
        assert!(reg.retain_alive(|w| w.id == "b"));
        assert!(!reg.retain_alive(|_| true));
        assert_eq!(reg.windows.len(), 1);
    }

    #[test]
    fn next_and_previous_project_window_wrap_around() {
        let mut reg = ProjectWindows::default();
        assert_eq!(reg.cycle("a", 1), None);
        reg.register(entry("a", 1));
        assert_eq!(reg.cycle("a", 1), None, "a single window has no next one");
        reg.register(entry("b", 2));
        reg.register(entry("c", 3));
        assert_eq!(reg.cycle("a", 1).unwrap().id, "b");
        assert_eq!(reg.cycle("c", 1).unwrap().id, "a");
        assert_eq!(reg.cycle("a", -1).unwrap().id, "c");
        assert_eq!(reg.cycle("b", -1).unwrap().id, "a");
        assert_eq!(reg.cycle("unknown", 1), None);
    }

    #[test]
    fn control_lines_need_the_token() {
        let line = control_line("s3cret", ControlRequest::Focus);
        assert_eq!(line, "forge-window s3cret focus\n");
        assert_eq!(
            parse_control_line(&line, "s3cret"),
            Some(ControlRequest::Focus)
        );
        assert_eq!(
            parse_control_line(&control_line("s3cret", ControlRequest::Info), "s3cret"),
            Some(ControlRequest::Info)
        );
        assert_eq!(parse_control_line(&line, "other"), None);
        assert_eq!(parse_control_line(&line, ""), None);
        assert_eq!(
            parse_control_line("forge-window s3cret quit\n", "s3cret"),
            None
        );
        assert_eq!(
            parse_control_line("forge-window s3cret focus extra", "s3cret"),
            None
        );
        assert_eq!(parse_control_line("GET / HTTP/1.1", "s3cret"), None);
    }

    #[test]
    fn open_target_follows_the_setting() {
        assert_eq!(open_target("ask", true), OpenTarget::Ask);
        assert_eq!(open_target("newWindow", true), OpenTarget::NewWindow);
        assert_eq!(open_target("thisWindow", true), OpenTarget::ThisWindow);
        assert_eq!(open_target("whatever", true), OpenTarget::Ask);
        // The Welcome screen opens the project itself.
        assert_eq!(open_target("newWindow", false), OpenTarget::ThisWindow);
        assert_eq!(
            serde_json::to_string(&OpenTarget::NewWindow).unwrap(),
            "\"newWindow\""
        );
    }
}

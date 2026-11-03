//! Hot Reload of `dotnet watch` runs: what its output says about the application (hot
//! reload enabled, a file changed, changes applied, a rude edit that needs a restart, a
//! failed build), for the Hot Reload status and the Apply Changes action. Works with and
//! without the emoji prefixes (`DOTNET_WATCH_SUPPRESS_EMOJIS`) and with ANSI colors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotReloadState {
    /// Building or starting the application.
    Starting,
    /// Hot reload is enabled and the application runs.
    Watching,
    /// A file changed; changes are being applied.
    Applying,
    /// Changes were applied to the running application.
    Applied,
    /// Nothing to apply (no C# changes).
    NoChanges,
    /// A rude edit: the application is restarted (or asks to be).
    RudeEdit,
    Restarting,
    /// The build failed; `dotnet watch` waits for a file change.
    BuildFailed,
    /// The application exited; `dotnet watch` waits for a file change.
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    pub state: HotReloadState,
    /// The line without the `dotnet watch` prefix.
    pub message: String,
    /// Changed files (`File changed: ./Program.cs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// `dotnet watch` asks whether to restart (answer `y` on its input).
    #[serde(default)]
    pub prompt: bool,
}

/// The events of a chunk of `dotnet watch` output.
pub fn parse_watch_output(text: &str) -> Vec<WatchEvent> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = crate::aspire::strip_ansi(raw);
        let Some(rest) = line.trim().strip_prefix("dotnet watch") else {
            continue;
        };
        // Drop the emoji / symbol and an optional `[App (net9.0)]`.
        let msg: String = rest
            .trim_start_matches(|c: char| !c.is_ascii_alphanumeric() && c != '[')
            .to_string();
        let msg = match msg.strip_prefix('[') {
            Some(r) => r
                .split_once(']')
                .map(|(_, m)| m.trim().to_string())
                .unwrap_or(msg.clone()),
            None => msg.trim().to_string(),
        };
        let l = msg.to_ascii_lowercase();
        let mut files = Vec::new();
        let state = if l.starts_with("file changed:") || l.starts_with("files changed:") {
            files = msg
                .split_once(':')
                .map(|(_, f)| f)
                .unwrap_or("")
                .split(',')
                .map(|f| {
                    f.trim()
                        .trim_end_matches('.')
                        .trim_start_matches("./")
                        .to_string()
                })
                .filter(|f| !f.is_empty())
                .collect();
            HotReloadState::Applying
        } else if l.contains("hot reload enabled") || l.starts_with("started") {
            HotReloadState::Watching
        } else if l.contains("hot reload") && (l.contains("succeeded") || l.contains("applied"))
            || l.contains("changes applied")
        {
            HotReloadState::Applied
        } else if l.contains("no c# changes") || l.contains("no hot reload changes") {
            HotReloadState::NoChanges
        } else if l.contains("rude edit")
            || l.contains("requires restarting")
            || l.contains("restart is needed")
            || l.contains("do you want to restart")
        {
            HotReloadState::RudeEdit
        } else if l.starts_with("restarting") || l.contains("restart requested") {
            HotReloadState::Restarting
        } else if l.starts_with("build failed")
            || l.contains("build failed")
            || l.contains("error cs")
        {
            HotReloadState::BuildFailed
        } else if l.starts_with("waiting for a file to change") || l.starts_with("exited") {
            HotReloadState::Waiting
        } else if l.starts_with("building") {
            HotReloadState::Starting
        } else {
            continue;
        };
        out.push(WatchEvent {
            prompt: l.contains("do you want to restart"),
            state,
            message: msg,
            files,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotnet_watch() {
        let out = "dotnet watch 🔨 Building...\n  Bookstore.Api -> /src/bin/Debug/net8.0/Bookstore.Api.dll\ndotnet watch 🔥 Hot reload enabled. For a list of supported edits, see https://aka.ms/dotnet/hot-reload.\n  💡 Press \"Ctrl + R\" to restart.\ndotnet watch ⌚ File changed: ./Endpoints/Books.cs.\ndotnet watch 🔥 Hot reload of changes succeeded.\ndotnet watch ⌚ File changed: ./Program.cs.\ndotnet watch ⚠ [Bookstore.Api (net8.0)] Unable to apply hot reload due to rude edit.\ndotnet watch ❔ Do you want to restart your app - Yes (y) / No (n) / Always (a) / Never (v)?\ndotnet watch ❌ Build failed: CS1002\n";
        let e = parse_watch_output(out);
        let states: Vec<HotReloadState> = e.iter().map(|e| e.state).collect();
        use HotReloadState::*;
        assert_eq!(
            states,
            [
                Starting,
                Watching,
                Applying,
                Applied,
                Applying,
                RudeEdit,
                RudeEdit,
                BuildFailed
            ]
        );
        assert_eq!(e[2].files, ["Endpoints/Books.cs"]);
        assert!(e[6].prompt);
        assert_eq!(e[5].message, "Unable to apply hot reload due to rude edit.");
    }

    #[test]
    fn net9_and_plain_output() {
        let e = parse_watch_output("dotnet watch 🔥 [Bookstore.Api (net9.0)] Hot reload succeeded.\ndotnet watch : Waiting for a file to change before restarting dotnet...\n\u{1b}[33mdotnet watch 🔄 Restarting\u{1b}[0m\nnot ours\n");
        let states: Vec<HotReloadState> = e.iter().map(|e| e.state).collect();
        assert_eq!(
            states,
            [
                HotReloadState::Applied,
                HotReloadState::Waiting,
                HotReloadState::Restarting
            ]
        );
    }
}

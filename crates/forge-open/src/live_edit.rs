//! Live Edit through the Browser Companion: changes to style sheets are pushed into the
//! open pages without a reload (CSS hot swap), other web files reload the pages that use
//! them. Forge records the events here; the paired extension polls
//! `GET /api/forge/live-edit?since=<seq>` and applies them in the tabs whose page loaded
//! the changed file.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Events kept for slow pollers.
const KEEP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveEditKind {
    /// Replace the style sheet in place: with `text` (unsaved editor content) or by
    /// reloading the `<link>` with a cache-busting query.
    Css,
    /// Reload the page.
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEditEvent {
    pub seq: u64,
    pub kind: LiveEditKind,
    /// Project-relative path of the changed file.
    pub file: String,
    /// Its file name, which the extension matches against the page's resources.
    pub name: String,
    /// The style sheet's text for a hot swap while typing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// What Live Edit does for a file: `None` for files a page never loads.
pub fn classify(path: &str) -> Option<LiveEditKind> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "css" => Some(LiveEditKind::Css),
        "html" | "htm" | "js" | "mjs" | "jsx" | "ts" | "tsx" | "vue" | "svelte" | "astro"
        | "php" | "twig" | "json" | "svg" | "png" | "jpg" | "jpeg" | "gif" | "webp" => {
            Some(LiveEditKind::Reload)
        }
        _ if name.ends_with(".blade.php") || name.ends_with(".html.twig") => {
            Some(LiveEditKind::Reload)
        }
        _ => None,
    }
}

/// The answer of a poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    /// The latest sequence number; poll again with it.
    pub seq: u64,
    pub events: Vec<LiveEditEvent>,
}

#[derive(Default)]
struct Inner {
    seq: u64,
    events: VecDeque<LiveEditEvent>,
}

/// The events of the running session.
#[derive(Default)]
pub struct LiveEditHub {
    inner: Mutex<Inner>,
}

impl LiveEditHub {
    /// Record a change. `text` is sent along for style sheets (hot swap while typing).
    /// Returns the event, or `None` for files Live Edit ignores.
    pub fn push(&self, file: &str, text: Option<String>) -> Option<LiveEditEvent> {
        let kind = classify(file)?;
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.seq += 1;
        let event = LiveEditEvent {
            seq: inner.seq,
            kind,
            file: file.to_string(),
            name: file.rsplit('/').next().unwrap_or(file).to_string(),
            text: text.filter(|_| kind == LiveEditKind::Css),
        };
        // A newer change of the same style sheet replaces the older one.
        inner
            .events
            .retain(|e| !(e.kind == LiveEditKind::Css && e.file == event.file));
        inner.events.push_back(event.clone());
        while inner.events.len() > KEEP {
            inner.events.pop_front();
        }
        Some(event)
    }

    /// Events after `since`. A poller that starts (`since` 0) only learns the current
    /// sequence number, so old changes are not replayed into fresh pages.
    pub fn since(&self, since: u64) -> Poll {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let events = if since == 0 {
            Vec::new()
        } else {
            inner
                .events
                .iter()
                .filter(|e| e.seq > since)
                .cloned()
                .collect()
        };
        Poll {
            seq: inner.seq,
            events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_files() {
        assert_eq!(classify("public/css/app.css"), Some(LiveEditKind::Css));
        assert_eq!(
            classify("resources/views/home.blade.php"),
            Some(LiveEditKind::Reload)
        );
        assert_eq!(classify("src/App.vue"), Some(LiveEditKind::Reload));
        assert_eq!(classify("README.md"), None);
        // Sass is compiled first (File Watchers); its .css output is what pages load.
        assert_eq!(classify("assets/app.scss"), None);
    }

    #[test]
    fn records_and_replays_events() {
        let hub = LiveEditHub::default();
        assert_eq!(
            hub.since(0),
            Poll {
                seq: 0,
                events: vec![]
            }
        );
        hub.push("css/app.css", Some("body{}".into())).unwrap();
        hub.push("index.html", Some("<html>".into())).unwrap();
        assert!(hub.push("notes.txt", None).is_none());
        let p = hub.since(0);
        assert_eq!((p.seq, p.events.len()), (2, 0));
        let e = hub
            .push("css/app.css", Some("body{color:red}".into()))
            .unwrap();
        let p = hub.since(1);
        assert_eq!(p.seq, 3);
        assert_eq!(
            p.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(e.text.as_deref(), Some("body{color:red}"));
        assert_eq!(p.events[0].text, None, "pages reload; no text for HTML");
        assert_eq!(p.events[1].name, "app.css");
        assert!(hub.since(3).events.is_empty());
    }
}

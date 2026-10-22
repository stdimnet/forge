//! Payloads of the OSC (operating system command) sequences Forge's terminal understands
//! beyond what xterm.js handles itself. xterm.js hands the UI the payload after
//! `ESC ] <code> ;`; the UI passes it here (through WebAssembly, synchronously) and renders
//! the result.
//!
//! - OSC 9;4 progress (ConEmu, Windows Terminal, Ghostty; emitted by Claude Code, winget,
//!   systemd, ...): `ESC ] 9 ; 4 ; <state> ; <percent> ST`.
//! - OSC 9 notifications (iTerm2): `ESC ] 9 ; <message> ST`, and ConEmu's `9;9;<cwd>`.
//! - OSC 777 notifications (rxvt, Ghostty, foot): `ESC ] 777 ; notify ; <title> ; <body> ST`.
//! - OSC 7 working directory: `ESC ] 7 ; file://<host>/<path> ST`.
//! - OSC 133 shell integration marks (FinalTerm): `A` prompt start, `D` command finished.
//!
//! OSC 8 hyperlink targets are checked by [`link_target`].

use serde::{Deserialize, Serialize};

use crate::links::to_workspace_path;

/// State of an OSC 9;4 progress report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressState {
    Normal,
    Error,
    Indeterminate,
    Paused,
}

/// Progress reported by a terminal program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub state: ProgressState,
    /// 0–100. Indeterminate progress keeps the last known value.
    pub percent: u8,
}

/// What an OSC sequence asks the terminal to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OscAction {
    /// Show (`Some`) or remove (`None`) the program's progress.
    Progress { progress: Option<Progress> },
    /// Show a notification.
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        body: String,
    },
    /// The shell's current directory (absolute, on the shell's machine).
    Cwd { path: String },
    /// A prompt started or a command finished: progress the last program left is stale.
    CommandBoundary,
    /// Not understood, malformed or not for Forge.
    Ignored,
}

/// Parse the payload of OSC `code` (7, 9, 133 or 777). `previous` is the progress the tab
/// shows now; error and paused reports without a percentage keep its value.
pub fn parse_osc(code: u32, data: &str, previous: Option<&Progress>) -> OscAction {
    match code {
        7 => parse_osc7(data).map_or(OscAction::Ignored, |path| OscAction::Cwd { path }),
        9 => parse_osc9(data, previous),
        133 if data.starts_with('A') || data.starts_with('D') => OscAction::CommandBoundary,
        777 => parse_osc777(data).unwrap_or(OscAction::Ignored),
        _ => OscAction::Ignored,
    }
}

/// Parse the part after `9;4;`: `<state>[;<percent>]`. `Some(None)` removes the progress
/// (state 0), `None` means the input is not a valid progress report.
///
/// Like Windows Terminal, error and paused without a percentage keep `previous`'s value, and
/// normal without a percentage means 0.
pub fn parse_progress(args: &str, previous: Option<&Progress>) -> Option<Option<Progress>> {
    let mut parts = args.split(';');
    let state = match parts.next().unwrap_or("").trim() {
        "0" => return Some(None),
        "1" => ProgressState::Normal,
        "2" => ProgressState::Error,
        "3" => ProgressState::Indeterminate,
        "4" => ProgressState::Paused,
        _ => return None,
    };
    let raw = parts.next().unwrap_or("").trim();
    let percent = if raw.is_empty() {
        match state {
            ProgressState::Normal => 0,
            _ => previous.map_or(0, |p| p.percent),
        }
    } else {
        if !is_decimal(raw) {
            return None;
        }
        raw.parse::<f64>().ok()?.round().clamp(0.0, 100.0) as u8
    };
    Some(Some(Progress { state, percent }))
}

/// `123` or `12.5`: digits with an optional fraction, no sign or exponent.
fn is_decimal(s: &str) -> bool {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    digits(int) && frac.is_none_or(digits)
}

/// OSC 9: progress (`4;…`), ConEmu cwd (`9;…`), other ConEmu sub-commands (ignored) or an
/// iTerm2 notification.
fn parse_osc9(data: &str, previous: Option<&Progress>) -> OscAction {
    let digits = data.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &data[digits..];
    // ConEmu uses numeric sub-commands (1 = sleep, 2 = message box, 4 = progress, 9 = cwd …).
    if digits > 0 && (rest.is_empty() || rest.starts_with(';')) {
        let args = rest.strip_prefix(';').unwrap_or(rest);
        return match &data[..digits] {
            "4" => parse_progress(args, previous).map_or(OscAction::Ignored, |progress| {
                OscAction::Progress { progress }
            }),
            "9" if !args.is_empty() => OscAction::Cwd {
                path: unquote(args).to_string(),
            },
            _ => OscAction::Ignored,
        };
    }
    match data.trim() {
        "" => OscAction::Ignored,
        body => OscAction::Notify {
            title: None,
            body: body.to_string(),
        },
    }
}

/// OSC 777; only `notify;<title>;<body>` is supported.
fn parse_osc777(data: &str) -> Option<OscAction> {
    let mut parts = data.splitn(3, ';');
    if parts.next() != Some("notify") {
        return None;
    }
    let title = parts.next().unwrap_or("").trim();
    let body = parts.next().unwrap_or("").trim();
    match (title.is_empty(), body.is_empty()) {
        (true, true) => None,
        (_, true) => Some(OscAction::Notify {
            title: None,
            body: title.to_string(),
        }),
        _ => Some(OscAction::Notify {
            title: (!title.is_empty()).then(|| title.to_string()),
            body: body.to_string(),
        }),
    }
}

/// OSC 7 (`file://host/path`) into a path on the shell's machine.
pub fn parse_osc7(data: &str) -> Option<String> {
    let data = data.trim();
    let scheme = data.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") || data.contains('\n') {
        return None;
    }
    let after_host = &data[7..];
    let path = &after_host[after_host.find('/')?..];
    let path = percent_decode(path)?;
    // file:///C:/Users/... on Windows
    let b = path.as_bytes();
    if b.len() >= 4 && b[1].is_ascii_alphabetic() && b[2] == b':' && b[3] == b'/' {
        return Some(path[1..].to_string());
    }
    Some(path)
}

/// Decode `%XX` escapes; `None` for a malformed escape or invalid UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn unquote(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Where an OSC 8 hyperlink may lead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkTarget {
    /// A web or mail link, opened in the browser.
    Url { url: String },
    /// A project file, opened in the editor.
    File {
        /// Workspace-relative path.
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
}

/// What an OSC 8 hyperlink points to. Only web and mail links open in the browser and
/// `file://` links to files of the project (`root`) open in the editor; everything else
/// (e.g. `javascript:`) is refused with `None`.
pub fn link_target(uri: &str, root: Option<&str>) -> Option<LinkTarget> {
    let u = uri.trim();
    let lower = u.to_ascii_lowercase();
    if ["http:", "https:", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return Some(LinkTarget::Url { url: u.to_string() });
    }
    if !lower.starts_with("file://") {
        return None;
    }
    let (base, fragment) = u.split_once('#').unwrap_or((u, ""));
    let path = to_workspace_path(&parse_osc7(base)?, root, "")?;
    let fragment = fragment.strip_prefix(['L', 'l']).unwrap_or(fragment);
    let digits = fragment.bytes().take_while(u8::is_ascii_digit).count();
    let line = fragment[..digits].parse().ok();
    Some(LinkTarget::File { path, line })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(state: ProgressState, percent: u8) -> Progress {
        Progress { state, percent }
    }

    #[test]
    fn parses_every_progress_state() {
        use ProgressState::*;
        assert_eq!(parse_progress("1;42", None), Some(Some(p(Normal, 42))));
        assert_eq!(parse_progress("2;60", None), Some(Some(p(Error, 60))));
        assert_eq!(parse_progress("3", None), Some(Some(p(Indeterminate, 0))));
        assert_eq!(
            parse_progress("3;10", None),
            Some(Some(p(Indeterminate, 10)))
        );
        assert_eq!(parse_progress("4;75", None), Some(Some(p(Paused, 75))));
    }

    #[test]
    fn clears_progress_with_state_0() {
        assert_eq!(parse_progress("0", None), Some(None));
        assert_eq!(parse_progress("0;0", None), Some(None));
        assert_eq!(
            parse_osc(9, "4;0", None),
            OscAction::Progress { progress: None }
        );
    }

    #[test]
    fn clamps_and_rounds_the_percentage() {
        assert_eq!(parse_progress("1;150", None).unwrap().unwrap().percent, 100);
        assert_eq!(parse_progress("1;33.6", None).unwrap().unwrap().percent, 34);
    }

    #[test]
    fn keeps_the_previous_value_for_error_and_paused() {
        use ProgressState::*;
        let prev = p(Normal, 40);
        assert_eq!(parse_progress("2", Some(&prev)), Some(Some(p(Error, 40))));
        assert_eq!(parse_progress("4;", Some(&prev)), Some(Some(p(Paused, 40))));
        assert_eq!(parse_progress("1", Some(&prev)), Some(Some(p(Normal, 0))));
    }

    #[test]
    fn rejects_invalid_progress() {
        for bad in ["", "5;10", "x;10", "1;-5", "1;abc", "1;1e2", "1;.5", "1;5."] {
            assert_eq!(parse_progress(bad, None), None, "{bad:?}");
        }
        assert_eq!(parse_osc(9, "4", None), OscAction::Ignored);
        assert_eq!(parse_osc(9, "4;7;1", None), OscAction::Ignored);
    }

    #[test]
    fn tells_osc9_notifications_from_conemu_sub_commands() {
        let notify = |body: &str| OscAction::Notify {
            title: None,
            body: body.into(),
        };
        assert_eq!(
            parse_osc(9, "Claude is waiting for your input", None),
            notify("Claude is waiting for your input")
        );
        assert_eq!(
            parse_osc(9, "42 tests passed", None),
            notify("42 tests passed")
        );
        assert_eq!(
            parse_osc(9, "9;\"C:\\Users\\dev\"", None),
            OscAction::Cwd {
                path: "C:\\Users\\dev".into()
            }
        );
        assert_eq!(parse_osc(9, "1;500", None), OscAction::Ignored);
        assert_eq!(parse_osc(9, "9", None), OscAction::Ignored);
        assert_eq!(parse_osc(9, "   ", None), OscAction::Ignored);
    }

    #[test]
    fn parses_osc777_notifications() {
        let n = |title: Option<&str>, body: &str| OscAction::Notify {
            title: title.map(Into::into),
            body: body.into(),
        };
        assert_eq!(
            parse_osc(777, "notify;Claude Code;Task finished", None),
            n(Some("Claude Code"), "Task finished")
        );
        assert_eq!(
            parse_osc(777, "notify;Build;done; 3 warnings", None),
            n(Some("Build"), "done; 3 warnings")
        );
        assert_eq!(
            parse_osc(777, "notify;Only a title", None),
            n(None, "Only a title")
        );
        assert_eq!(parse_osc(777, "notify;;Body", None), n(None, "Body"));
        assert_eq!(parse_osc(777, "preexec", None), OscAction::Ignored);
        assert_eq!(parse_osc(777, "notify;;", None), OscAction::Ignored);
    }

    #[test]
    fn parses_osc7_working_directories() {
        let cwd = |s: &str| parse_osc7(s);
        assert_eq!(
            cwd("file://forge/home/dev/Projects/shop").as_deref(),
            Some("/home/dev/Projects/shop")
        );
        assert_eq!(
            cwd("file:///home/dev/My%20Project").as_deref(),
            Some("/home/dev/My Project")
        );
        assert_eq!(
            cwd("FILE://pc/C:/Users/dev").as_deref(),
            Some("C:/Users/dev")
        );
        assert_eq!(cwd("/home/dev"), None);
        assert_eq!(cwd("file://host"), None);
        assert_eq!(cwd("file://host/%E0%A4%A"), None);
        assert_eq!(cwd("file://host/%FF"), None);
        assert_eq!(
            parse_osc(7, "file:///srv/app", None),
            OscAction::Cwd {
                path: "/srv/app".into()
            }
        );
    }

    #[test]
    fn recognizes_osc133_command_boundaries() {
        assert_eq!(parse_osc(133, "A", None), OscAction::CommandBoundary);
        assert_eq!(parse_osc(133, "D;0", None), OscAction::CommandBoundary);
        assert_eq!(parse_osc(133, "B", None), OscAction::Ignored);
        assert_eq!(parse_osc(133, "C", None), OscAction::Ignored);
        assert_eq!(parse_osc(52, "c;aGk=", None), OscAction::Ignored);
    }

    #[test]
    fn opens_only_safe_link_targets() {
        let root = Some("/home/dev/shop");
        assert_eq!(
            link_target("https://laravel.com/docs", root),
            Some(LinkTarget::Url {
                url: "https://laravel.com/docs".into()
            })
        );
        assert!(matches!(
            link_target("MAILTO:dev@example.com", root),
            Some(LinkTarget::Url { .. })
        ));
        assert_eq!(
            link_target("file://forge/home/dev/shop/app/Models/Order.php#L30", root),
            Some(LinkTarget::File {
                path: "app/Models/Order.php".into(),
                line: Some(30)
            })
        );
        assert_eq!(
            link_target("file:///home/dev/shop/README.md", root),
            Some(LinkTarget::File {
                path: "README.md".into(),
                line: None
            })
        );
        // Outside the project, or not a web, mail or file link.
        assert_eq!(link_target("file:///etc/passwd", root), None);
        assert_eq!(link_target("javascript:alert(1)", root), None);
        assert_eq!(link_target("vscode://file/x", root), None);
    }

    #[test]
    fn serializes_for_the_ui() {
        let json = serde_json::to_string(&parse_osc(9, "4;1;42", None)).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"progress","progress":{"state":"normal","percent":42}}"#
        );
        let json = serde_json::to_string(&parse_osc(133, "A", None)).unwrap();
        assert_eq!(json, r#"{"kind":"commandBoundary"}"#);
    }
}

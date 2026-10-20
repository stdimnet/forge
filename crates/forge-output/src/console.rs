//! Console lines built from styled segments, sent to the UI as deltas.
//!
//! `\n` starts a new line; after a `\r` the next text overwrites the current line (progress
//! bars), while `\r\n` is a plain line break. Only the current (last) line can still change,
//! so the model keeps just that line and every delta says whether it replaces the UI's last
//! line.

use serde::{Deserialize, Serialize};

use crate::ansi::{Segment, Style};
use crate::links::{find_links, FileLink, LinkContext};

/// A run of text with one style, optionally a link (file reference or URL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<SpanLink>,
}

/// Where a link span leads: a project file (`target`, 1-based line/column) or a URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanLink {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub url: bool,
    /// Workspace-relative path, or the URL.
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl SpanLink {
    fn of(l: &FileLink) -> Option<Self> {
        Some(SpanLink {
            url: l.url,
            target: if l.url {
                l.path.clone()
            } else {
                l.target.clone()?
            },
            line: l.line,
            column: l.column,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleLine {
    pub segments: Vec<Span>,
}

impl ConsoleLine {
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }

    /// A line of plain text in one style, with links.
    pub fn plain(text: &str, style: Style, ctx: &LinkContext) -> Self {
        let segs = if text.is_empty() {
            Vec::new()
        } else {
            vec![Segment {
                text: text.to_string(),
                style,
            }]
        };
        render(&segs, ctx)
    }
}

/// Lines to apply to the UI's list: when `replace_last` is set the first line replaces the
/// last line shown, the others are appended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleDelta {
    pub replace_last: bool,
    pub lines: Vec<ConsoleLine>,
}

impl ConsoleDelta {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Apply to a line list, as the UI does.
    pub fn apply_to(&self, lines: &mut Vec<ConsoleLine>) {
        if self.replace_last && !self.lines.is_empty() {
            lines.pop();
        }
        lines.extend(self.lines.iter().cloned());
    }
}

/// Split segments at link boundaries.
fn render(segments: &[Segment], ctx: &LinkContext) -> ConsoleLine {
    let text: String = segments.iter().map(|s| s.text.as_str()).collect();
    let links = if text.contains(['/', '\\', '.']) {
        find_links(&text, ctx)
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    let mut pos = 0;
    for seg in segments {
        let seg_end = pos + seg.text.len();
        let mut cursor = pos;
        while cursor < seg_end {
            let link = links.iter().find(|l| l.start <= cursor && cursor < l.end);
            let next = match link {
                Some(l) => l.end.min(seg_end),
                None => links
                    .iter()
                    .filter(|l| l.start > cursor)
                    .map(|l| l.start)
                    .min()
                    .unwrap_or(seg_end)
                    .min(seg_end),
            };
            out.push(Span {
                text: seg.text[cursor - pos..next - pos].to_string(),
                style: seg.style.clone(),
                link: link.and_then(SpanLink::of),
            });
            cursor = next;
        }
        pos = seg_end;
    }
    ConsoleLine { segments: out }
}

/// The current line of a console and whether the UI already shows it.
#[derive(Debug, Default)]
pub struct ConsoleModel {
    current: Vec<Segment>,
    cr: bool,
    shown: bool,
    links: LinkContext,
}

impl ConsoleModel {
    pub fn new(links: LinkContext) -> Self {
        ConsoleModel {
            links,
            ..Self::default()
        }
    }

    pub fn links(&self) -> &LinkContext {
        &self.links
    }

    /// Append segments (with `extra` as the base style, e.g. red for stderr).
    pub fn append(&mut self, segments: &[Segment], extra: Option<&Style>) -> ConsoleDelta {
        let mut delta = ConsoleDelta {
            replace_last: self.shown,
            lines: Vec::new(),
        };
        let mut touched = false;
        for seg in segments {
            let style = match extra {
                Some(base) => seg.style.over(base),
                None => seg.style.clone(),
            };
            for (idx, part) in seg.text.split('\n').enumerate() {
                if idx > 0 {
                    delta.lines.push(render(&self.current, &self.links));
                    self.current.clear();
                    self.cr = false;
                }
                touched = true;
                for (k, piece) in part.split('\r').enumerate() {
                    if k > 0 {
                        self.cr = true;
                    }
                    if piece.is_empty() {
                        continue;
                    }
                    if self.cr {
                        self.current.clear();
                        self.cr = false;
                    }
                    match self.current.last_mut() {
                        Some(last) if last.style == style => last.text.push_str(piece),
                        _ => self.current.push(Segment {
                            text: piece.to_string(),
                            style: style.clone(),
                        }),
                    }
                }
            }
        }
        if touched {
            delta.lines.push(render(&self.current, &self.links));
            self.shown = true;
        }
        delta
    }

    /// The UI cleared its lines: the current line starts over.
    pub fn clear(&mut self) {
        self.current.clear();
        self.cr = false;
        self.shown = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::AnsiParser;

    fn texts(lines: &[ConsoleLine]) -> Vec<String> {
        lines.iter().map(ConsoleLine::text).collect()
    }

    #[test]
    fn splits_lines_and_handles_carriage_returns() {
        let mut lines = Vec::new();
        let mut p = AnsiParser::new();
        let mut m = ConsoleModel::default();
        for chunk in [
            "first\nprogress 10%\rprogress 50%",
            "\rprogress 100%\r",
            "\nlast\r\nend",
        ] {
            m.append(&p.feed(chunk), None).apply_to(&mut lines);
        }
        assert_eq!(texts(&lines), ["first", "progress 100%", "last", "end"]);
    }

    #[test]
    fn deltas_replace_the_current_line() {
        let mut m = ConsoleModel::default();
        let seg = |t: &str| {
            vec![Segment {
                text: t.into(),
                style: Style::default(),
            }]
        };
        let d = m.append(&seg("a"), None);
        assert!(!d.replace_last);
        let d = m.append(&seg("b\nc"), Some(&Style::fg("var(--danger)")));
        assert!(d.replace_last);
        assert_eq!(texts(&d.lines), ["ab", "c"]);
        assert_eq!(
            d.lines[1].segments[0].style.fg.as_deref(),
            Some("var(--danger)")
        );
    }

    #[test]
    fn marks_links_that_resolve_into_the_project() {
        let ctx = LinkContext::new(Some("/p"), None);
        let mut m = ConsoleModel::new(ctx);
        let mut p = AnsiParser::new();
        let d = m.append(
            &p.feed("at \x1b[1m/p/app/User.php:42\x1b[0m and /etc/x.conf:1, see https://a.io/x.\n"),
            None,
        );
        let spans = &d.lines[0].segments;
        let link = spans.iter().find_map(|s| s.link.clone()).unwrap();
        assert_eq!(link.target, "app/User.php");
        assert_eq!(link.line, Some(42));
        assert!(spans.iter().any(|s| s
            .link
            .as_ref()
            .is_some_and(|l| l.url && l.target == "https://a.io/x")));
        assert!(spans
            .iter()
            .all(|s| !s.text.contains("x.conf") || s.link.is_none()));
    }
}

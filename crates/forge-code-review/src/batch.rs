//! Pending review comments ("drafts") and how they become one review. Drafts live in the
//! cache until the user submits them; nothing is posted before that.

use forge_forges::{ChangedFile, HostKind, NewReview, Position, ReviewComment, ReviewEvent};
use forge_vcs::LineKind;
use serde::{Deserialize, Serialize};

use crate::anchor::Finding;
use crate::parse::Severity;

/// A pending review comment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Draft {
    /// Empty for a new draft; the store assigns one.
    pub id: String,
    /// Markdown.
    pub body: String,
    /// `None`: a general comment on the request.
    pub position: Option<Position>,
    /// First line of a multi-line comment.
    pub start_line: Option<u32>,
    /// The AI finding it was made from.
    pub finding: Option<String>,
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Info => "Note",
    }
}

/// The comment text for a finding. GitHub and GitLab render a `suggestion` block as an
/// applicable change; Forgejo shows a plain code block.
pub fn finding_body(f: &Finding, kind: HostKind) -> String {
    let mut body = String::new();
    if f.title.trim().is_empty() {
        body.push_str(&format!("**{}:** ", severity_word(f.severity)));
    } else {
        body.push_str(&format!(
            "**{}: {}**\n\n",
            severity_word(f.severity),
            f.title.trim()
        ));
    }
    body.push_str(f.explanation.trim());
    if f.position.is_none() {
        body = format!("`{}` line {}: {body}", f.path, f.line);
    }
    if let Some(s) = f.suggestion.as_deref().filter(|s| !s.trim().is_empty()) {
        let fence = if s.contains("```") { "````" } else { "```" };
        match (kind, f.position.is_some()) {
            (HostKind::Github | HostKind::Gitlab, true) => {
                body.push_str(&format!("\n\n{fence}suggestion\n{}\n{fence}", s.trim_end()));
            }
            _ => body.push_str(&format!(
                "\n\nSuggested change:\n\n{fence}\n{}\n{fence}",
                s.trim_end()
            )),
        }
    }
    body
}

/// A draft made from a finding (inline when the finding is placed on the diff).
pub fn draft_from_finding(f: &Finding, kind: HostKind) -> Draft {
    Draft {
        id: String::new(),
        body: finding_body(f, kind),
        position: f.position.clone(),
        start_line: f.start_line,
        finding: Some(f.id.clone()),
    }
}

/// Is the position's line still shown in the diff (it may have moved after a push)?
fn on_diff(p: &Position, files: &[ChangedFile]) -> bool {
    let Some(file) = files
        .iter()
        .find(|f| f.path == p.path || f.old_path.as_deref() == Some(p.path.as_str()))
    else {
        return false;
    };
    file.hunks.iter().any(|h| {
        let (mut o, mut n) = (h.old_start, h.new_start);
        h.lines.iter().any(|l| {
            let hit = match l.kind {
                LineKind::Added => p.new_line == Some(n),
                LineKind::Removed => p.new_line.is_none() && p.old_line == Some(o),
                LineKind::Context => {
                    p.new_line == Some(n) || (p.new_line.is_none() && p.old_line == Some(o))
                }
            };
            match l.kind {
                LineKind::Added => n += 1,
                LineKind::Removed => o += 1,
                LineKind::Context => {
                    o += 1;
                    n += 1;
                }
            }
            hit
        })
    })
}

/// Batch `drafts` into one review. Inline drafts whose line is no longer in the diff and
/// general drafts are added to the review body, so nothing written is lost.
pub fn build_review(
    drafts: &[Draft],
    event: ReviewEvent,
    body: &str,
    files: &[ChangedFile],
) -> Result<NewReview, String> {
    let mut text = body.trim().to_string();
    let mut comments = Vec::new();
    for d in drafts.iter().filter(|d| !d.body.trim().is_empty()) {
        match &d.position {
            Some(p) if on_diff(p, files) => comments.push(ReviewComment {
                body: d.body.trim().to_string(),
                position: p.clone(),
                start_line: d
                    .start_line
                    .filter(|s| p.new_line.or(p.old_line).is_some_and(|e| *s < e)),
            }),
            other => {
                if !text.is_empty() {
                    text.push_str("\n\n---\n\n");
                }
                if let Some(p) = other {
                    let line = p.new_line.or(p.old_line).unwrap_or(0);
                    text.push_str(&format!("`{}` line {line}:\n\n", p.path));
                }
                text.push_str(d.body.trim());
            }
        }
    }
    if event != ReviewEvent::Approve && text.is_empty() && comments.is_empty() {
        return Err(match event {
            ReviewEvent::RequestChanges => "Describe the changes you are requesting".into(),
            _ => "Add a comment before submitting the review".into(),
        });
    }
    Ok(NewReview {
        event,
        body: text,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::{file, hunk};

    fn pos(new_line: Option<u32>, old_line: Option<u32>) -> Option<Position> {
        Some(Position {
            path: "a.php".into(),
            new_line,
            old_line,
            ..Position::default()
        })
    }

    fn draft(body: &str, position: Option<Position>, start_line: Option<u32>) -> Draft {
        Draft {
            id: "d".into(),
            body: body.into(),
            position,
            start_line,
            finding: None,
        }
    }

    #[test]
    fn batches_drafts_into_one_review() {
        let files = vec![file(
            "a.php",
            vec![hunk(5, &[(" ", "a"), ("-", "b"), ("+", "c"), ("+", "d")])],
        )];
        let drafts = vec![
            draft("On c", pos(Some(6), None), None),
            draft("Range", pos(Some(7), None), Some(5)),
            draft("Removed b", pos(None, Some(6)), None),
            draft("Moved away", pos(Some(90), None), None),
            draft("General", None, None),
            draft("   ", pos(Some(6), None), None),
        ];
        let r = build_review(&drafts, ReviewEvent::RequestChanges, "Please fix", &files).unwrap();
        assert_eq!(r.event, ReviewEvent::RequestChanges);
        let bodies: Vec<&str> = r.comments.iter().map(|c| c.body.as_str()).collect();
        assert_eq!(bodies, ["On c", "Range", "Removed b"]);
        assert_eq!(r.comments[1].start_line, Some(5));
        assert_eq!(
            r.body,
            "Please fix\n\n---\n\n`a.php` line 90:\n\nMoved away\n\n---\n\nGeneral"
        );
    }

    #[test]
    fn empty_reviews_need_a_verdict() {
        assert!(build_review(&[], ReviewEvent::Comment, " ", &[]).is_err());
        assert!(build_review(&[], ReviewEvent::RequestChanges, "", &[]).is_err());
        let ok = build_review(&[], ReviewEvent::Approve, "", &[]).unwrap();
        assert!(ok.comments.is_empty() && ok.body.is_empty());
    }

    #[test]
    fn finding_bodies_use_suggestions_where_hosts_apply_them() {
        let f = Finding {
            id: "f1".into(),
            path: "a.php".into(),
            line: 6,
            end_line: None,
            severity: Severity::Error,
            title: "Null role".into(),
            explanation: "The cast fails for NULL.".into(),
            suggestion: Some("$x ?? null;".into()),
            position: pos(Some(6), None),
            start_line: None,
        };
        let gh = finding_body(&f, HostKind::Github);
        assert_eq!(
            gh,
            "**Error: Null role**\n\nThe cast fails for NULL.\n\n```suggestion\n$x ?? null;\n```"
        );
        assert!(finding_body(&f, HostKind::Forgejo).contains("Suggested change:\n\n```\n$x"));
        let unplaced = Finding {
            position: None,
            ..f.clone()
        };
        let body = finding_body(&unplaced, HostKind::Github);
        assert!(body.starts_with("`a.php` line 6: **Error: Null role**"));
        assert!(!body.contains("```suggestion"));
        let d = draft_from_finding(&f, HostKind::Gitlab);
        assert_eq!((d.finding.as_deref(), d.position), (Some("f1"), f.position));
    }
}

//! Commit message inspections (Settings | Version Control | Commit): the subject line
//! length, a blank line between subject and body and the right margin of the body.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MessageRules {
    /// Longest subject line (0 = no limit).
    pub subject_max_length: usize,
    pub blank_line_after_subject: bool,
    /// Longest body line (0 = no limit).
    pub body_right_margin: usize,
}

impl Default for MessageRules {
    fn default() -> Self {
        Self {
            subject_max_length: 72,
            blank_line_after_subject: true,
            body_right_margin: 72,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageProblemKind {
    SubjectTooLong,
    MissingBlankLine,
    BodyLineTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageProblem {
    pub kind: MessageProblemKind,
    /// 0-based line of the message.
    pub line: usize,
    /// Character columns (0-based, end exclusive) to underline.
    pub from: usize,
    pub to: usize,
    pub message: String,
}

/// Check a commit message. Lines starting with `#` are comments and skipped.
pub fn inspect_message(message: &str, rules: &MessageRules) -> Vec<MessageProblem> {
    let lines: Vec<(usize, &str)> = message
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .enumerate()
        .filter(|(_, l)| !l.starts_with('#'))
        .collect();
    let mut out = Vec::new();
    let Some(subject_at) = lines.iter().position(|(_, l)| !l.trim().is_empty()) else {
        return out;
    };
    let (subject_line, subject) = lines[subject_at];
    let len = subject.chars().count();
    if rules.subject_max_length > 0 && len > rules.subject_max_length {
        out.push(MessageProblem {
            kind: MessageProblemKind::SubjectTooLong,
            line: subject_line,
            from: rules.subject_max_length,
            to: len,
            message: format!(
                "Subject line is too long ({len} characters, at most {})",
                rules.subject_max_length
            ),
        });
    }
    if rules.blank_line_after_subject {
        if let Some((line, text)) = lines.get(subject_at + 1) {
            if !text.trim().is_empty() {
                out.push(MessageProblem {
                    kind: MessageProblemKind::MissingBlankLine,
                    line: *line,
                    from: 0,
                    to: text.chars().count(),
                    message: "Missing blank line between subject and body".into(),
                });
            }
        }
    }
    if rules.body_right_margin > 0 {
        for (line, text) in lines.iter().skip(subject_at + 1) {
            let len = text.chars().count();
            if len > rules.body_right_margin {
                out.push(MessageProblem {
                    kind: MessageProblemKind::BodyLineTooLong,
                    line: *line,
                    from: rules.body_right_margin,
                    to: len,
                    message: format!(
                        "Body line is longer than {} characters",
                        rules.body_right_margin
                    ),
                });
            }
        }
    }
    out
}

/// Workspace-relative paths whose staged text uses CRLF line separators ("Warn if CRLF line
/// separators are about to be committed"). `files` are (path, content) pairs.
pub fn files_with_crlf<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<String> {
    files
        .into_iter()
        .filter(|(_, bytes)| !bytes.contains(&0) && bytes.windows(2).any(|w| w == b"\r\n"))
        .map(|(p, _)| p.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_good_message() {
        let msg = "Add routes tool window\n\nShows every route of the project.\n# comment line that is long enough to exceed the margin of seventy-two\n";
        assert!(inspect_message(msg, &MessageRules::default()).is_empty());
        assert!(inspect_message("", &MessageRules::default()).is_empty());
    }

    #[test]
    fn reports_each_rule() {
        let subject = "x".repeat(80);
        let body = "y".repeat(75);
        let msg = format!("{subject}\nbody right after\n{body}");
        let problems = inspect_message(&msg, &MessageRules::default());
        let kinds: Vec<_> = problems.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MessageProblemKind::SubjectTooLong,
                MessageProblemKind::MissingBlankLine,
                MessageProblemKind::BodyLineTooLong
            ]
        );
        assert_eq!((problems[0].from, problems[0].to), (72, 80));
        assert_eq!(problems[2].line, 2);
        let off = MessageRules {
            subject_max_length: 0,
            blank_line_after_subject: false,
            body_right_margin: 0,
        };
        assert!(inspect_message(&msg, &off).is_empty());
    }

    #[test]
    fn finds_crlf_files() {
        let files: Vec<(&str, &[u8])> = vec![
            ("a.txt", b"a\r\nb"),
            ("b.txt", b"a\nb"),
            ("c.bin", b"\0\r\n"),
        ];
        assert_eq!(files_with_crlf(files), vec!["a.txt".to_string()]);
    }
}

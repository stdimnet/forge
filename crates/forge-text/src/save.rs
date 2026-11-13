//! What happens to a document when it is saved (Settings | Editor | General | On Save):
//! trailing spaces are removed from modified lines or all lines (never on a line with the
//! caret, so typing is not disturbed) and the file gets a final line break.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// `editor.stripTrailingSpaces`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StripMode {
    None,
    #[default]
    Modified,
    All,
}

impl StripMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "modified" => Some(Self::Modified),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SaveOptions {
    pub strip_trailing_spaces: StripMode,
    pub ensure_newline_at_eof: bool,
}

/// A save request: the editor text (`\n` separators), the text as last loaded or saved (to
/// find modified lines) and the 0-based lines that hold a caret.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SaveRequest {
    pub text: String,
    pub saved: Option<String>,
    pub caret_lines: Vec<usize>,
    pub options: SaveOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub text: String,
    /// The text differs from the request's text (the editor applies it).
    pub changed: bool,
}

/// The line separator a text uses: `\r\n`, `\r` (only when there is no `\n`) or `\n`.
fn separator(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\r') && !text.contains('\n') {
        "\r"
    } else {
        "\n"
    }
}

/// `\r\n` and `\r` line ends as `\n`.
fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Apply the on-save cleanup. Texts with `\r\n` or `\r` line ends (files of providers that
/// keep them, such as scratches and remote files) are cleaned up like `\n` texts and keep
/// their separator, also in the final newline.
pub fn prepare_save(req: &SaveRequest) -> SaveResult {
    let sep = separator(&req.text);
    if sep == "\n" {
        return prepare_lf(req);
    }
    let lf = SaveRequest {
        text: to_lf(&req.text),
        saved: req.saved.as_deref().map(to_lf),
        ..req.clone()
    };
    let out = prepare_lf(&lf).text.replace('\n', sep);
    SaveResult {
        changed: out != req.text,
        text: out,
    }
}

fn prepare_lf(req: &SaveRequest) -> SaveResult {
    let text = req.text.as_str();
    let mut out = match req.options.strip_trailing_spaces {
        StripMode::None => text.to_string(),
        mode => {
            let modified = match (mode, &req.saved) {
                (StripMode::Modified, Some(saved)) => Some(modified_lines(saved, text)),
                // Without a known saved state every line counts as modified.
                _ => None,
            };
            let keep: HashSet<usize> = req.caret_lines.iter().copied().collect();
            strip_lines(text, |i| {
                !keep.contains(&i) && modified.as_ref().is_none_or(|m| m.contains(&i))
            })
        }
    };
    if req.options.ensure_newline_at_eof && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    SaveResult {
        changed: out != text,
        text: out,
    }
}

fn strip_lines(text: &str, strip: impl Fn(usize) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if strip(i) {
            out.push_str(line.trim_end_matches([' ', '\t']));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Above this many line pairs in the changed middle, every middle line counts as modified
/// instead of running the quadratic diff.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// 0-based lines of `current` that are new or changed compared with `saved`.
pub fn modified_lines(saved: &str, current: &str) -> HashSet<usize> {
    let a: Vec<&str> = saved.split('\n').collect();
    let b: Vec<&str> = current.split('\n').collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let a_mid = &a[prefix..a.len() - suffix];
    let b_mid = &b[prefix..b.len() - suffix];
    let mut out = HashSet::new();
    if b_mid.is_empty() {
        return out;
    }
    if a_mid.is_empty() || a_mid.len() * b_mid.len() > MAX_DIFF_CELLS {
        out.extend(prefix..prefix + b_mid.len());
        return out;
    }
    // Longest common subsequence; lines of `current` outside it are modified.
    let (n, m) = (a_mid.len(), b_mid.len());
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] = if a_mid[i] == b_mid[j] {
                lcs[at(i + 1, j + 1)] + 1
            } else {
                lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while j < m {
        if i < n && a_mid[i] == b_mid[j] {
            i += 1;
            j += 1;
        } else if i < n && lcs[at(i + 1, j)] >= lcs[at(i, j + 1)] {
            i += 1;
        } else {
            out.insert(prefix + j);
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(
        text: &str,
        saved: Option<&str>,
        carets: &[usize],
        strip: StripMode,
        eof: bool,
    ) -> String {
        prepare_save(&SaveRequest {
            text: text.into(),
            saved: saved.map(String::from),
            caret_lines: carets.to_vec(),
            options: SaveOptions {
                strip_trailing_spaces: strip,
                ensure_newline_at_eof: eof,
            },
        })
        .text
    }

    #[test]
    fn strips_all_lines_except_caret_lines() {
        assert_eq!(
            save("a  \nb\t\nc ", None, &[], StripMode::All, false),
            "a\nb\nc"
        );
        assert_eq!(
            save("a  \nb  ", None, &[1], StripMode::All, false),
            "a\nb  "
        );
        assert_eq!(
            save("a  \nb  ", None, &[], StripMode::None, false),
            "a  \nb  "
        );
    }

    #[test]
    fn strips_only_modified_lines() {
        let saved = "keep  \nold\nend  ";
        let text = "keep  \nnew  \nadded \t\nend  ";
        assert_eq!(
            save(text, Some(saved), &[], StripMode::Modified, false),
            "keep  \nnew\nadded\nend  "
        );
        // No saved state: everything counts as modified.
        assert_eq!(save("x  ", None, &[], StripMode::Modified, false), "x");
    }

    #[test]
    fn keeps_crlf_and_cr_line_ends() {
        assert_eq!(
            save("a  \r\nb\t\r\nc ", None, &[], StripMode::All, true),
            "a\r\nb\r\nc\r\n"
        );
        assert_eq!(save("a \rb", None, &[], StripMode::All, true), "a\rb\r");
        assert_eq!(save("a\r", None, &[], StripMode::None, true), "a\r");
        // Only the modified line, found across the separators.
        assert_eq!(
            save(
                "x  \r\ny  ",
                Some("x  \r\nz"),
                &[],
                StripMode::Modified,
                false
            ),
            "x  \r\ny"
        );
    }

    #[test]
    fn ensures_a_final_newline() {
        assert_eq!(save("a", None, &[], StripMode::None, true), "a\n");
        assert_eq!(save("a\n", None, &[], StripMode::None, true), "a\n");
        assert_eq!(save("", None, &[], StripMode::None, true), "");
        let r = prepare_save(&SaveRequest {
            text: "a\n".into(),
            ..Default::default()
        });
        assert!(!r.changed);
    }

    #[test]
    fn finds_modified_lines() {
        let m = modified_lines("a\nb\nc\nd", "a\nB\nc\nx\nd");
        let mut v: Vec<_> = m.into_iter().collect();
        v.sort();
        assert_eq!(v, vec![1, 3]);
        assert!(modified_lines("same\ntext", "same\ntext").is_empty());
        let mut v: Vec<_> = modified_lines("", "a\nb").into_iter().collect();
        v.sort();
        assert_eq!(v, vec![0, 1]);
    }

    #[test]
    fn parses_modes() {
        assert_eq!(StripMode::parse("all"), Some(StripMode::All));
        assert_eq!(StripMode::parse("x"), None);
    }
}

//! Emmet actions on existing markup: Go to Next/Previous Edit Point, Update Tag with Emmet
//! and Remove Tag. Offsets are bytes here; `lib.rs` converts to UTF-16.

use crate::abbr::{self, Kind};
use crate::html::{attributes, element_at};
use crate::text::{line_end, line_start};

/// A text change (byte offsets in the original text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteEdit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// Edit points: between empty quotes (`href="|"`), between an empty element's tags
/// (`<p>|</p>`) and on empty lines.
pub fn edit_points(text: &str) -> Vec<usize> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for i in 1..b.len() {
        let pair = (b[i - 1], b[i]);
        let empty_value =
            (pair == (b'"', b'"') || pair == (b'\'', b'\'')) && i >= 2 && b[i - 2] == b'=';
        let empty_element = pair == (b'>', b'<') && b.get(i + 1) == Some(&b'/');
        if empty_value || empty_element {
            out.push(i);
        }
    }
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        // A line of whitespace between other lines (not the document's last empty line).
        if line.trim().is_empty() && end < text.len() && start > 0 {
            out.push(end);
        }
        start = end + 1;
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The next (or previous) edit point from `byte`.
pub fn edit_point(text: &str, byte: usize, forward: bool) -> Option<usize> {
    let points = edit_points(text);
    if forward {
        points.into_iter().find(|p| *p > byte)
    } else {
        points.into_iter().rev().find(|p| *p < byte)
    }
}

/// Update the element around `byte` from an abbreviation: a name renames the tag, `#id`
/// sets the id, `.class` adds classes and `[a=b]` sets attributes.
pub fn update_tag(text: &str, byte: usize, abbreviation: &str) -> Result<Vec<ByteEdit>, String> {
    let tree = abbr::parse(abbreviation.trim())?;
    let root = tree.root();
    if root.children.len() != 1 || tree.nodes.len() != 2 {
        return Err("Update Tag takes a single element".into());
    }
    let Kind::Element(spec) = &tree.nodes[root.children[0]].kind else {
        return Err("Update Tag takes a single element".into());
    };
    let el = element_at(text, byte).ok_or("No tag at the caret")?;
    let mut edits = Vec::new();
    let (open_from, open_to) = el.open;
    if !spec.name.is_empty() && spec.name != el.name {
        let name_from = open_from + 1;
        edits.push(ByteEdit {
            from: name_from,
            to: name_from + el.name.len(),
            text: spec.name.clone(),
        });
        if let Some((close_from, _)) = el.close {
            edits.push(ByteEdit {
                from: close_from + 2,
                to: close_from + 2 + el.name.len(),
                text: spec.name.clone(),
            });
        }
    }
    let existing = attributes(text, open_from, open_to);
    // New attributes go before `>` / `/>`.
    let insert_at = if text[..open_to].ends_with("/>") {
        let mut i = open_to - 2;
        while i > open_from && text.as_bytes()[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i
    } else {
        open_to - 1
    };
    let mut appended = String::new();
    for a in &spec.attrs {
        let value = a.value.clone().unwrap_or_default();
        let found = existing
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&a.name));
        match found {
            Some(e) if a.name == "class" => {
                let Some((vs, ve)) = e.value else {
                    edits.push(ByteEdit {
                        from: e.name_range.1,
                        to: e.name_range.1,
                        text: format!("=\"{value}\""),
                    });
                    continue;
                };
                let current: Vec<&str> = text[vs..ve].split_whitespace().collect();
                let add: Vec<&str> = value
                    .split_whitespace()
                    .filter(|c| !current.contains(c))
                    .collect();
                if !add.is_empty() {
                    let sep = if current.is_empty() { "" } else { " " };
                    edits.push(ByteEdit {
                        from: ve,
                        to: ve,
                        text: format!("{sep}{}", add.join(" ")),
                    });
                }
            }
            Some(e) => match e.value {
                Some((vs, ve)) => edits.push(ByteEdit {
                    from: vs,
                    to: ve,
                    text: value,
                }),
                None if !a.boolean => edits.push(ByteEdit {
                    from: e.name_range.1,
                    to: e.name_range.1,
                    text: format!("=\"{value}\""),
                }),
                None => {}
            },
            None if a.boolean => appended.push_str(&format!(" {}", a.name)),
            None => appended.push_str(&format!(" {}=\"{value}\"", a.name)),
        }
    }
    if !appended.is_empty() {
        edits.push(ByteEdit {
            from: insert_at,
            to: insert_at,
            text: appended,
        });
    }
    if edits.is_empty() {
        return Err("Nothing to update".into());
    }
    edits.sort_by_key(|e| e.from);
    Ok(edits)
}

/// Remove the tags of the element around `byte`, keeping its content. A tag alone on its
/// line removes the line; content between such lines loses one level of indentation.
pub fn remove_tag(text: &str, byte: usize, indent: &str) -> Result<Vec<ByteEdit>, String> {
    let el = element_at(text, byte).ok_or("No tag at the caret")?;
    let own_line = |from: usize, to: usize| {
        let ls = line_start(text, from);
        let le = line_end(text, to);
        (text[ls..from].trim().is_empty() && text[to..le].trim().is_empty()).then_some((ls, le))
    };
    let removal = |from: usize, to: usize| match own_line(from, to) {
        Some((ls, le)) => {
            let end = if le < text.len() { le + 1 } else { le };
            let start = if end == le && ls > 0 { ls - 1 } else { ls };
            ByteEdit {
                from: start,
                to: end,
                text: String::new(),
            }
        }
        None => ByteEdit {
            from,
            to,
            text: String::new(),
        },
    };
    let mut edits = vec![removal(el.open.0, el.open.1)];
    let Some((close_from, close_to)) = el.close else {
        return Ok(edits);
    };
    edits.push(removal(close_from, close_to));
    if own_line(el.open.0, el.open.1).is_some() && own_line(close_from, close_to).is_some() {
        // Dedent the lines between the tags.
        let mut ls = line_end(text, el.open.1) + 1;
        let close_line = line_start(text, close_from);
        while ls < close_line {
            let le = line_end(text, ls);
            let line = &text[ls..le];
            let cut = if line.starts_with(indent) && !indent.is_empty() {
                indent.len()
            } else if line.starts_with('\t') {
                1
            } else {
                line.len() - line.trim_start_matches(' ').len()
            }
            .min(line.len() - line.trim_start().len());
            if cut > 0 {
                edits.push(ByteEdit {
                    from: ls,
                    to: ls + cut,
                    text: String::new(),
                });
            }
            ls = le + 1;
        }
    }
    edits.sort_by_key(|e| e.from);
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edits: &[ByteEdit]) -> String {
        let mut out = text.to_string();
        for e in edits.iter().rev() {
            out.replace_range(e.from..e.to, &e.text);
        }
        out
    }

    #[test]
    fn edit_points_in_order() {
        let t = "<a href=\"\"></a>\n<div>\n  \n</div>";
        let pts = edit_points(t);
        assert_eq!(pts, vec![9, 11, 24]);
        assert_eq!(edit_point(t, 0, true), Some(9));
        assert_eq!(edit_point(t, 9, true), Some(11));
        assert_eq!(edit_point(t, 11, true), Some(24));
        assert_eq!(edit_point(t, 24, true), None);
        assert_eq!(edit_point(t, 24, false), Some(11));
        assert_eq!(edit_point(t, 9, false), None);
    }

    #[test]
    fn updates_tags() {
        let t = "<div class=\"a\" id=\"x\">\n  <p>hi</p>\n</div>";
        let edits = update_tag(t, 3, "section#main.b.a[role=main]").unwrap();
        assert_eq!(
            apply(t, &edits),
            "<section class=\"a b\" id=\"main\" role=\"main\">\n  <p>hi</p>\n</section>"
        );
        let t = "<img src=\"a.png\" />";
        let edits = update_tag(t, 2, ".logo").unwrap();
        assert_eq!(apply(t, &edits), "<img src=\"a.png\" class=\"logo\" />");
        assert!(update_tag("text", 1, "div").is_err());
        assert!(update_tag("<p></p>", 1, "a>b").is_err());
    }

    #[test]
    fn removes_tags() {
        let t = "<div>\n  <ul>\n    <li>x</li>\n  </ul>\n</div>";
        let at = t.find("<ul>").unwrap() + 1;
        let edits = remove_tag(t, at, "  ").unwrap();
        assert_eq!(apply(t, &edits), "<div>\n  <li>x</li>\n</div>");
        let t = "<p>Hello <b>world</b>!</p>";
        let edits = remove_tag(t, t.find("world").unwrap(), "  ").unwrap();
        assert_eq!(apply(t, &edits), "<p>Hello world!</p>");
        let t = "a<br>b";
        assert_eq!(apply(t, &remove_tag(t, 2, "  ").unwrap()), "ab");
        assert!(remove_tag("plain", 2, "  ").is_err());
    }
}

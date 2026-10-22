//! YAML: indentation by nesting (with "Indent sequence value"), spaces inside flow
//! collections, preferred quotes and blank lines. Block scalars (`|`, `>`) move as a whole.

use super::Opts;

/// A line ends with a block scalar indicator (`key: |`, `- >-`).
fn starts_block_scalar(content: &str) -> bool {
    let t = content.trim_end();
    let t = t.split(" #").next().unwrap_or(t).trim_end();
    let last = t.rsplit([' ', ':']).next().unwrap_or("");
    !last.is_empty()
        && (last.starts_with('|') || last.starts_with('>'))
        && last[1..]
            .chars()
            .all(|c| c == '-' || c == '+' || c.is_ascii_digit())
        && (t.len() == last.len()
            || t[..t.len() - last.len()].trim_end().ends_with(':')
            || t[..t.len() - last.len()].trim_end().ends_with('-'))
}

fn width(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Byte ranges of block scalar contents.
pub fn block_scalars(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut block: Option<(usize, usize)> = None; // (parent width, start)
    for line in text.split_inclusive('\n') {
        if let Some((parent, start)) = block {
            if !line.trim().is_empty() && width(line) <= parent {
                out.push((start, at));
                block = None;
            }
        }
        if block.is_none() && starts_block_scalar(line) {
            block = Some((width(line), at + line.len()));
        }
        at += line.len();
    }
    if let Some((_, start)) = block {
        out.push((start, text.len()));
    }
    out
}

/// The `- ` prefix of a sequence item line (`- `, `- - `), in bytes.
fn dash_prefix(t: &str) -> usize {
    let mut n = 0;
    let mut rest = t;
    while let Some(r) = rest.strip_prefix('-') {
        if !(r.starts_with(' ') || r.is_empty()) {
            break;
        }
        let spaces = r.len() - r.trim_start_matches(' ').len();
        n += 1 + spaces;
        rest = &r[spaces..];
    }
    n
}

/// Spaces inside `{…}` / `[…]` and the preferred quotes, outside strings and comments.
fn fix_flow(line: &str, o: &Opts<'_>) -> String {
    let braces = o.bool("spaces.withinBraces");
    let brackets = o.bool("spaces.withinBrackets");
    let single = o.bool("other.singleQuote");
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '#' && (i == 0 || chars[i - 1] == ' ') {
            out.extend(&chars[i..]);
            break;
        }
        if c == '"' || c == '\'' {
            let end = (i + 1..chars.len()).find(|&j| chars[j] == c && chars[j - 1] != '\\');
            let Some(end) = end else {
                out.extend(&chars[i..]);
                break;
            };
            let inner: String = chars[i + 1..end].iter().collect();
            let want = if single { '\'' } else { '"' };
            if c != want && !inner.contains(want) && !inner.contains('\\') {
                out.push(want);
                out.push_str(&inner);
                out.push(want);
            } else {
                out.extend(&chars[i..=end]);
            }
            i = end + 1;
            continue;
        }
        let at_value = depth > 0
            || (i > 0 && (out.ends_with(": ") || out.ends_with("- ") || out.trim().is_empty()));
        match c {
            '{' | '[' if at_value => {
                depth += 1;
                out.push(c);
                let space = if c == '{' { braces } else { brackets };
                let mut j = i + 1;
                while j < chars.len() && chars[j] == ' ' {
                    j += 1;
                }
                let empty = chars.get(j) == Some(&if c == '{' { '}' } else { ']' });
                if space && !empty {
                    out.push(' ');
                }
                i = j;
                continue;
            }
            '}' | ']' if depth > 0 => {
                depth -= 1;
                let open = if c == '}' { '{' } else { '[' };
                while out.ends_with(' ') {
                    out.pop();
                }
                let space = if c == '}' { braces } else { brackets };
                if space && !out.ends_with(open) {
                    out.push(' ');
                }
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Lay out a YAML document (`layout`), or only fix flow collections and quotes.
pub fn format(text: &str, o: &Opts<'_>, layout: bool) -> String {
    let unit = o.int("indent").max(1);
    let indent_seq = o.bool("indentSequenceValue");
    let keep = o.int("blankLines.keepMax");
    let blocks = block_scalars(text);
    let mut out = String::with_capacity(text.len());
    // (original width, new width, line was `key:` without value, line is a sequence item)
    let mut stack: Vec<(usize, usize, bool, bool)> = Vec::new();
    let mut blank = 0;
    let mut at = 0;
    let mut block_shift: Option<(usize, usize)> = None; // (original first width, new base)
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let start = at;
        at += raw.len();
        if blocks.iter().any(|(s, e)| *s <= start && start < *e) {
            if line.trim().is_empty() {
                out.push('\n');
                continue;
            }
            let (first, base) = *block_shift.get_or_insert((width(line), 0));
            let w = width(line);
            out.push_str(&" ".repeat(base + w.saturating_sub(first)));
            out.push_str(line.trim_start());
            out.push('\n');
            continue;
        }
        block_shift = None;
        if line.trim().is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            for _ in 0..blank.min(keep) {
                out.push('\n');
            }
        }
        blank = 0;
        let w = width(line);
        let t = line.trim_start();
        let is_item = dash_prefix(t) > 0;
        let new = if !layout || t.starts_with("---") || t.starts_with("...") {
            stack.clear();
            w
        } else {
            while let Some(&(ow, _, key, item)) = stack.last() {
                // A sequence at the same column as its key belongs to the key.
                if ow > w
                    || (ow == w && !(is_item && key && !item))
                    || (ow == w && item && !is_item)
                {
                    stack.pop();
                } else {
                    break;
                }
            }
            match stack.last() {
                Some(&(ow, nw, key, item)) if ow < w || (ow == w && key) => {
                    let parent_line_item = item;
                    if parent_line_item && !key {
                        // Content after `- ` keeps its offset from the dash.
                        nw + (w - ow)
                    } else if is_item && key {
                        nw + if indent_seq { unit } else { 0 }
                    } else {
                        nw + unit
                    }
                }
                Some(&(_, nw, _, _)) => nw,
                None => 0,
            }
        };
        let content = if layout {
            fix_flow(t, o)
        } else {
            t.to_string()
        };
        let key_only = {
            let c = content.split(" #").next().unwrap_or(&content).trim_end();
            c.ends_with(':') && !c.starts_with('#')
        };
        if !t.starts_with('#') {
            stack.push((w, new, key_only, is_item));
            // Keys inside a sequence item (`- name:`) are nested at the item's content.
            if is_item {
                let p = dash_prefix(t);
                let rest_key = content[p..]
                    .split(" #")
                    .next()
                    .unwrap_or("")
                    .trim_end()
                    .ends_with(':');
                if rest_key {
                    stack.push((w + p, new + p, true, false));
                }
            }
        }
        if starts_block_scalar(&content) {
            // The block content is indented one level below this line.
            let base = new + if is_item { dash_prefix(t) } else { unit };
            block_shift = Some((block_first_width(text, at), base));
        }
        out.push_str(&" ".repeat(new));
        out.push_str(&content);
        out.push('\n');
    }
    if !text.ends_with('\n') {
        out.pop();
    }
    out
}

fn block_first_width(text: &str, from: usize) -> usize {
    text[from..]
        .lines()
        .find(|l| !l.trim().is_empty())
        .map_or(0, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::model;
    use serde_json::json;

    #[test]
    fn reindents_and_fixes_flow_collections() {
        let mut v = model().defaults();
        let src = "a:\n    b:\n    - x\n    - y: 1\n      z: [1,2]\n    c: {k: 'v'}\n    d: |\n        keep\n          this\nlast: \"q\"\n";
        let o = Opts::new("yaml", &v);
        assert_eq!(
            format(src, &o, true),
            "a:\n  b:\n    - x\n    - y: 1\n      z: [1,2]\n  c: { k: \"v\" }\n  d: |\n    keep\n      this\nlast: \"q\"\n"
        );
        v.insert("yaml.indentSequenceValue".into(), json!(false));
        v.insert("yaml.other.singleQuote".into(), json!(true));
        let o = Opts::new("yaml", &v);
        assert_eq!(format("a:\n  - \"x\"\n", &o, true), "a:\n- 'x'\n");
        assert_eq!(block_scalars("a: |\n  x\n\n  y\nb: 1\n"), vec![(5, 14)]);
    }
}

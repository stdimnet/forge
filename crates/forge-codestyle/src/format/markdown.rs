//! Markdown: one space after header, list and blockquote markers, and the blank lines
//! between paragraphs, around headers and around block elements (fenced code, tables,
//! blockquotes). Fenced code and front matter are never changed.

use super::Opts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Text,
    Header,
    Code,
    Table,
    Quote,
    FrontMatter,
}

fn fence_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with("```") {
        Some("```")
    } else if t.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn is_header(line: &str) -> bool {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && line.len() - t.len() < 4
}

fn fix_line(line: &str, o: &Opts<'_>) -> String {
    let lead = &line[..line.len() - line.trim_start().len()];
    let t = line.trim_start();
    if is_header(line) && o.bool("spaces.afterHeaderSymbol") {
        let hashes = t.chars().take_while(|c| *c == '#').count();
        let rest = t[hashes..].trim_start();
        if !rest.is_empty() {
            return format!("{lead}{} {rest}", &t[..hashes]);
        }
    }
    if o.bool("spaces.afterListMarker") {
        let marker_len = if t.starts_with(['-', '*', '+']) {
            1
        } else {
            let digits = t.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 && t[digits..].starts_with(['.', ')']) {
                digits + 1
            } else {
                0
            }
        };
        if marker_len > 0 {
            let rest = &t[marker_len..];
            let spaces = rest.len() - rest.trim_start_matches(' ').len();
            if spaces >= 1 && !rest.trim().is_empty() {
                return format!(
                    "{lead}{} {}",
                    &t[..marker_len],
                    rest.trim_start_matches(' ')
                );
            }
        }
    }
    if o.bool("spaces.afterBlockquoteMarker") && t.starts_with('>') {
        let markers = t.chars().take_while(|c| *c == '>').count();
        let rest = t[markers..].trim_start();
        if !rest.is_empty() {
            return format!("{lead}{} {rest}", &t[..markers]);
        }
    }
    line.to_string()
}

/// Format a Markdown document.
pub fn format(text: &str, o: &Opts<'_>) -> String {
    // Group the lines into blocks separated by blank lines.
    let mut groups: Vec<(Block, Vec<String>, usize)> = Vec::new(); // (kind, lines, blank lines before)
    let mut blank = 0usize;
    let mut fence: Option<&str> = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(f) = fence {
            groups
                .last_mut()
                .expect("code block")
                .1
                .push(line.to_string());
            if line.trim_start().starts_with(f) {
                fence = None;
            }
            i += 1;
            continue;
        }
        if i == 0 && line.trim_end() == "---" {
            let end = lines[1..]
                .iter()
                .position(|l| l.trim_end() == "---")
                .map_or(lines.len(), |p| p + 2);
            groups.push((
                Block::FrontMatter,
                lines[..end.min(lines.len())]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                0,
            ));
            i = end;
            continue;
        }
        if line.trim().is_empty() {
            blank += 1;
            i += 1;
            continue;
        }
        let kind = if let Some(f) = fence_of(line) {
            fence = Some(f);
            Block::Code
        } else if is_header(line) {
            Block::Header
        } else if line.trim_start().starts_with('|') {
            Block::Table
        } else if line.trim_start().starts_with('>') {
            Block::Quote
        } else {
            Block::Text
        };
        let fixed = if matches!(kind, Block::Code) {
            line.to_string()
        } else {
            fix_line(line, o)
        };
        let joins = blank == 0
            && kind != Block::Header
            && kind != Block::Code
            && groups.last().is_some_and(|(k, _, _)| *k == kind);
        if joins {
            groups.last_mut().expect("group").1.push(fixed);
        } else {
            groups.push((kind, vec![fixed], blank));
        }
        blank = 0;
        i += 1;
    }
    let keep = o.int("blankLines.keepMax");
    let around_header = o.int("blankLines.aroundHeader");
    let around_block = o.int("blankLines.aroundBlockElements");
    let mut out = String::with_capacity(text.len());
    for (n, (kind, lines, before)) in groups.iter().enumerate() {
        if n > 0 {
            let prev = groups[n - 1].0;
            let mut b = (*before).min(keep.max(1));
            if *kind == Block::Header || prev == Block::Header {
                b = b.max(around_header);
            }
            let block = |k: Block| matches!(k, Block::Code | Block::Table | Block::Quote);
            if block(*kind) || block(prev) {
                b = b.max(around_block);
            }
            if prev == Block::FrontMatter {
                b = *before;
            }
            for _ in 0..b {
                out.push('\n');
            }
        }
        for l in lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::model;

    #[test]
    fn spaces_and_blank_lines() {
        let values = model().defaults();
        let o = Opts::new("markdown", &values);
        let src = "#Title\nText\n-  a\n*   b\n\n\n\n>q\n```\n#not a header\n\n\n\nx\n```\n## Next\n| a |\n";
        assert_eq!(
            format(src, &o),
            "# Title\n\nText\n- a\n* b\n\n> q\n\n```\n#not a header\n\n\n\nx\n```\n\n## Next\n\n| a |\n"
        );
        assert_eq!(format("---\nt: 1\n---\n#A\n", &o), "---\nt: 1\n---\n# A\n");
    }
}

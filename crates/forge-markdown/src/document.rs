//! The Markdown preview of a file: GitHub-flavored blocks with the source lines of every
//! top-level block (synchronized scrolling), the front matter and the footnotes.

use serde::{Deserialize, Serialize};

use crate::ast::{Block, Inline};
use crate::parse::{parse_ranges, Flavor};

/// One `key: value` entry of the front matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontMatterEntry {
    pub key: String,
    pub value: String,
}

/// The YAML (`---`) or TOML (`+++`) block at the start of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontMatter {
    /// `yaml` or `toml`.
    pub format: String,
    pub entries: Vec<FrontMatterEntry>,
    /// 0-based line of the closing fence.
    pub end_line: u32,
}

/// A footnote definition, in the order of first reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Footnote {
    pub label: String,
    pub index: u32,
    pub blocks: Vec<Block>,
    /// 0-based line of the definition.
    pub line: u32,
}

/// 0-based first and last source line of a top-level block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// What the preview renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub blocks: Vec<Block>,
    /// The source lines of `blocks[i]`.
    pub lines: Vec<LineRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_matter: Option<FrontMatter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<Footnote>,
}

/// Split off the front matter: its parsed form and the source with the front matter blanked
/// out (the lines stay, so line numbers keep matching the editor).
fn front_matter(src: &str) -> (Option<FrontMatter>, String) {
    let mut lines = src.split('\n');
    let fence = match lines.next().map(str::trim_end) {
        Some("---") => "---",
        Some("+++") => "+++",
        _ => return (None, src.to_string()),
    };
    let mut body = Vec::new();
    let mut end = None;
    for (i, line) in lines.enumerate() {
        let t = line.trim_end();
        if t == fence || (fence == "---" && t == "...") {
            end = Some(i + 1);
            break;
        }
        body.push(line);
    }
    let Some(end_line) = end else {
        return (None, src.to_string());
    };
    // `---` followed by a blank line is a thematic break, not front matter.
    if body.first().is_some_and(|l| l.trim().is_empty()) && body.iter().all(|l| l.trim().is_empty())
    {
        return (None, src.to_string());
    }
    let sep = if fence == "---" { ':' } else { '=' };
    let mut entries: Vec<FrontMatterEntry> = Vec::new();
    for line in &body {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let top_level = !line.starts_with([' ', '\t', '-']);
        match line.split_once(sep) {
            Some((k, v)) if top_level && !k.trim().is_empty() && !k.contains(' ') => {
                entries.push(FrontMatterEntry {
                    key: k.trim().to_string(),
                    value: unquote(v.trim()).to_string(),
                });
            }
            _ => {
                // Continuation (nested map, list item): add to the previous value.
                if let Some(last) = entries.last_mut() {
                    let item = line.trim().trim_start_matches("- ").trim();
                    if !last.value.is_empty() {
                        last.value.push_str(", ");
                    }
                    last.value.push_str(unquote(item));
                }
            }
        }
    }
    let mut rest: Vec<&str> = src.split('\n').collect();
    for l in rest.iter_mut().take(end_line + 1) {
        *l = "";
    }
    (
        Some(FrontMatter {
            format: if fence == "---" { "yaml" } else { "toml" }.into(),
            entries,
            end_line: end_line as u32,
        }),
        rest.join("\n"),
    )
}

fn unquote(v: &str) -> &str {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return &v[1..v.len() - 1];
        }
    }
    v
}

/// Byte offset to 0-based line.
struct LineIndex(Vec<usize>);

impl LineIndex {
    fn new(src: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex(starts)
    }

    fn line(&self, offset: usize) -> u32 {
        (self.0.partition_point(|&s| s <= offset).max(1) - 1) as u32
    }
}

/// A Markdown file for the preview.
pub fn parse_document(src: &str) -> Document {
    let src = src.replace("\r\n", "\n");
    let (front_matter, body) = front_matter(&src);
    let parsed = parse_ranges(&body, Flavor::Preview);
    let index = LineIndex::new(&body);
    let lines = parsed
        .ranges
        .iter()
        .map(|r| {
            let start = index.line(r.start);
            // The range may end after trailing blank lines.
            let text = body.get(r.clone()).unwrap_or_default();
            let end = index.line(r.start + text.trim_end().len().saturating_sub(1));
            LineRange { start, end }
        })
        .collect();
    let mut footnotes: Vec<Footnote> = parsed
        .footnotes
        .into_iter()
        .filter_map(|f| {
            let index_of = *parsed.footnote_numbers.get(&f.label)?;
            Some(Footnote {
                line: index.line(f.range.start),
                label: f.label,
                index: index_of,
                blocks: f.blocks,
            })
        })
        .collect();
    footnotes.sort_by_key(|f| f.index);
    footnotes.dedup_by_key(|f| f.index);
    Document {
        blocks: parsed.blocks,
        lines,
        front_matter,
        footnotes,
    }
}

/// The 0-based line of the heading with the anchor `id` (`#getting-started`), for links
/// into a Markdown file.
pub fn anchor_line(src: &str, id: &str) -> Option<u32> {
    let id = id.trim_start_matches('#').to_lowercase();
    let doc = parse_document(src);
    doc.blocks
        .iter()
        .zip(&doc.lines)
        .find_map(|(b, l)| match b {
            Block::Heading { id: Some(h), .. } if *h == id => Some(l.start),
            _ => None,
        })
}

impl Document {
    /// Plain text of the headings (tests, outlines).
    pub fn headings(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { inline, .. } => Some(Inline::text_of(inline)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Inline {
        Inline::Text { text: t.into() }
    }

    #[test]
    fn maps_blocks_to_source_lines() {
        let doc = parse_document("# Title\n\nFirst\nparagraph.\n\n- a\n- b\n\n```php\n$x;\n```\n");
        assert_eq!(
            doc.lines,
            [
                LineRange { start: 0, end: 0 },
                LineRange { start: 2, end: 3 },
                LineRange { start: 5, end: 6 },
                LineRange { start: 8, end: 10 },
            ]
        );
    }

    #[test]
    fn gfm_tables_task_lists_strikethrough_and_autolinks() {
        let doc = parse_document(
            "| Name | Qty |\n|:-----|----:|\n| *Pen* | 2 |\n\n- [x] ~~done~~\n\nSee https://forge.dev, www.php.net and mail@acme.test.",
        );
        let Block::Table { align, head, rows } = &doc.blocks[0] else {
            panic!("{:?}", doc.blocks[0])
        };
        assert_eq!(align, &["left", "right"]);
        assert_eq!(head[1], [text("Qty")]);
        assert_eq!(
            rows[0][0],
            [Inline::Em {
                children: vec![text("Pen")]
            }]
        );
        let Block::List { items, .. } = &doc.blocks[1] else {
            panic!()
        };
        assert_eq!(items[0].checked, Some(true));
        let Block::Paragraph { inline } = &doc.blocks[2] else {
            panic!()
        };
        let hrefs: Vec<&str> = inline
            .iter()
            .filter_map(|i| match i {
                Inline::Link { href, .. } => Some(href.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            hrefs,
            [
                "https://forge.dev",
                "http://www.php.net",
                "mailto:mail@acme.test"
            ]
        );
    }

    #[test]
    fn numbers_footnotes_in_reference_order() {
        let doc = parse_document(
            "Second[^b] and first[^a].\n\n[^a]: Alpha.\n[^b]: Beta.\n[^unused]: Nope.\n",
        );
        let Block::Paragraph { inline } = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(
            inline[1],
            Inline::FootnoteRef {
                label: "b".into(),
                index: 1
            }
        );
        assert_eq!(doc.footnotes.len(), 2);
        assert_eq!(doc.footnotes[0].label, "b");
        assert_eq!(doc.footnotes[0].line, 3);
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn headings_get_github_anchors() {
        let doc = parse_document("# Getting Started!\n\n## Getting started\n\n### API: `run()`\n");
        let ids: Vec<_> = doc
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading { id, .. } => id.clone().unwrap_or_default(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(ids, ["getting-started", "getting-started-1", "api-run"]);
        assert_eq!(
            anchor_line("# A\n\ntext\n\n## Install it\n", "#install-it"),
            Some(4)
        );
        assert_eq!(anchor_line("# A\n", "missing"), None);
    }

    #[test]
    fn images_from_markdown_and_html() {
        let doc = parse_document(
            "![Logo](docs/logo.png \"The logo\")\n\n<p align=\"center\"><img src=\"art.svg\" alt=\"Art\" width=\"120\"></p>\n",
        );
        let Block::Paragraph { inline } = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(
            inline[0],
            Inline::Image {
                src: "docs/logo.png".into(),
                alt: "Logo".into(),
                title: Some("The logo".into()),
                width: None,
                height: None
            }
        );
        let Block::Paragraph { inline } = &doc.blocks[1] else {
            panic!("{:?}", doc.blocks)
        };
        assert_eq!(
            inline,
            &[Inline::Image {
                src: "art.svg".into(),
                alt: "Art".into(),
                title: None,
                width: Some("120".into()),
                height: None
            }]
        );
    }

    #[test]
    fn sanitizes_raw_html() {
        let doc = parse_document(
            "<!-- hidden\ncomment -->\n<script>alert(1)</script>\n\nA <b>bold</b> <a href=\"docs/x.md\">link</a> &amp; <span onclick=\"x()\">text</span><br>next\n",
        );
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("hidden"), "{json}");
        assert!(!json.contains("onclick"), "{json}");
        assert!(!json.contains("<"), "{json}");
        let Block::Paragraph { inline } = doc.blocks.last().unwrap() else {
            panic!()
        };
        assert_eq!(
            inline[1],
            Inline::Strong {
                children: vec![text("bold")]
            }
        );
        assert_eq!(
            inline[3],
            Inline::Link {
                href: "docs/x.md".into(),
                children: vec![text("link")]
            }
        );
        assert!(inline.contains(&Inline::Break));
        assert!(Inline::text_of(inline).contains("& text"));
    }

    #[test]
    fn front_matter_becomes_a_table_and_keeps_line_numbers() {
        let doc = parse_document(
            "---\ntitle: \"Release notes\"\ntags:\n  - php\n  - forge\n---\n# Heading\n",
        );
        let fm = doc.front_matter.clone().unwrap();
        assert_eq!(fm.format, "yaml");
        assert_eq!(fm.end_line, 5);
        assert_eq!(
            fm.entries,
            [
                FrontMatterEntry {
                    key: "title".into(),
                    value: "Release notes".into()
                },
                FrontMatterEntry {
                    key: "tags".into(),
                    value: "php, forge".into()
                },
            ]
        );
        assert_eq!(doc.headings(), ["Heading"]);
        assert_eq!(doc.lines[0].start, 6);
        // A thematic break at the start is not front matter.
        assert!(parse_document("---\n\ntext\n").front_matter.is_none());
    }

    #[test]
    fn github_alerts_and_mermaid_fences() {
        let doc = parse_document("> [!WARNING]\n> Careful.\n\n```mermaid\ngraph TD; A-->B\n```\n");
        assert!(matches!(&doc.blocks[0], Block::Quote { kind: Some(k), .. } if k == "warning"));
        assert!(matches!(&doc.blocks[1], Block::Code { lang, .. } if lang == "mermaid"));
    }
}

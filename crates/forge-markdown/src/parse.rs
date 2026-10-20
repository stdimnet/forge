//! pulldown-cmark events into the [`Block`] tree.

use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::ast::{Block, Inline, ListItem};

/// Where the Markdown comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flavor {
    /// Assistant messages: raw HTML stays literal text.
    #[default]
    Chat,
    /// Language server documentation: `<br>`/`<p>` are line breaks, other tags are dropped,
    /// and bare URLs and fully qualified class names are marked.
    Docs,
}

enum Container {
    Root,
    Item {
        checked: Option<bool>,
    },
    Quote,
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
}

struct Frame {
    container: Container,
    blocks: Vec<Block>,
}

/// An open inline container (paragraph, heading, emphasis, link, ...).
enum Span {
    Paragraph,
    Heading(u8),
    /// Text directly in a tight list item.
    Plain,
    Strong,
    Em,
    Del,
    Link(String),
}

struct Builder<'a> {
    src: &'a str,
    flavor: Flavor,
    frames: Vec<Frame>,
    spans: Vec<(Span, Vec<Inline>)>,
    code: Option<(String, String, Range<usize>)>,
}

fn level(h: HeadingLevel) -> u8 {
    match h {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn push_text(out: &mut Vec<Inline>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text { text: last }) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Inline::Text {
            text: text.to_string(),
        });
    }
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"</?[a-zA-Z][^>]*>").expect("tag regex"))
}

fn break_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^<\s*(br|p|/p)\s*/?>$").expect("break regex"))
}

fn linkable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"https?://[^\s<>()]+[^\s<>().,;:!?]|\\?[A-Za-z_][A-Za-z0-9_]*(?:\\[A-Za-z_][A-Za-z0-9_]*)+")
            .expect("linkable regex")
    })
}

/// Split documentation text into text, URLs and fully qualified class names.
fn autolink(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut last = 0;
    for m in linkable_re().find_iter(text) {
        push_text(&mut out, &text[last..m.start()]);
        let t = m.as_str().to_string();
        out.push(if t.starts_with("http") {
            Inline::Url { text: t }
        } else {
            Inline::Fqcn { text: t }
        });
        last = m.end();
    }
    push_text(&mut out, &text[last..]);
    out
}

fn autolink_all(items: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::new();
    for i in items {
        match i {
            Inline::Text { text } => {
                for x in autolink(&text) {
                    match x {
                        Inline::Text { text } => push_text(&mut out, &text),
                        other => out.push(other),
                    }
                }
            }
            Inline::Strong { children } => out.push(Inline::Strong {
                children: autolink_all(children),
            }),
            Inline::Em { children } => out.push(Inline::Em {
                children: autolink_all(children),
            }),
            Inline::Del { children } => out.push(Inline::Del {
                children: autolink_all(children),
            }),
            other => out.push(other),
        }
    }
    out
}

impl Builder<'_> {
    fn blocks(&mut self) -> &mut Vec<Block> {
        &mut self.frames.last_mut().expect("root frame").blocks
    }

    fn inlines(&mut self) -> &mut Vec<Inline> {
        if self.spans.is_empty() {
            // Text directly inside a tight list item.
            self.spans.push((Span::Plain, Vec::new()));
        }
        &mut self.spans.last_mut().expect("span").1
    }

    /// Close a pending tight-item text before a nested block starts.
    fn close_plain(&mut self) {
        if matches!(self.spans.last(), Some((Span::Plain, _))) && self.spans.len() == 1 {
            let (_, inline) = self.spans.pop().expect("plain");
            let inline = self.finish_inline(inline);
            if !inline.is_empty() {
                self.blocks().push(Block::Plain { inline });
            }
        }
    }

    fn finish_inline(&self, items: Vec<Inline>) -> Vec<Inline> {
        let mut items = match self.flavor {
            Flavor::Docs => autolink_all(items),
            Flavor::Chat => items,
        };
        // Trim the outer whitespace left by soft breaks and dropped tags.
        if let Some(Inline::Text { text }) = items.first_mut() {
            *text = text.trim_start().to_string();
        }
        if let Some(Inline::Text { text }) = items.last_mut() {
            *text = text.trim_end().to_string();
        }
        items.retain(|i| !matches!(i, Inline::Text { text } if text.is_empty()));
        items
    }

    fn html(&mut self, html: &str) {
        match self.flavor {
            Flavor::Chat => push_text(self.inlines(), html),
            Flavor::Docs => {
                let mut last = 0;
                for m in tag_re().find_iter(html) {
                    let before = html[last..m.start()].to_string();
                    push_text(self.inlines(), &before);
                    if break_tag_re().is_match(m.as_str()) {
                        self.inlines().push(Inline::Break);
                    }
                    last = m.end();
                }
                let rest = html[last..].to_string();
                push_text(self.inlines(), &rest);
            }
        }
    }

    fn start(&mut self, tag: Tag, range: Range<usize>) {
        match tag {
            Tag::Paragraph => {
                self.close_plain();
                self.spans.push((Span::Paragraph, Vec::new()));
            }
            Tag::Heading { level: l, .. } => {
                self.close_plain();
                self.spans.push((Span::Heading(level(l)), Vec::new()));
            }
            Tag::BlockQuote(_) => {
                self.close_plain();
                self.frames.push(Frame {
                    container: Container::Quote,
                    blocks: Vec::new(),
                });
            }
            Tag::CodeBlock(kind) => {
                self.close_plain();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
                        .next()
                        .unwrap_or_default()
                        .to_lowercase(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new(), range));
            }
            Tag::List(start) => {
                self.close_plain();
                self.frames.push(Frame {
                    container: Container::List {
                        ordered: start.is_some(),
                        start: start.unwrap_or(1),
                        items: Vec::new(),
                    },
                    blocks: Vec::new(),
                });
            }
            Tag::Item => self.frames.push(Frame {
                container: Container::Item { checked: None },
                blocks: Vec::new(),
            }),
            Tag::Emphasis => self.spans.push((Span::Em, Vec::new())),
            Tag::Strong => self.spans.push((Span::Strong, Vec::new())),
            Tag::Strikethrough => self.spans.push((Span::Del, Vec::new())),
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                let _ = self.inlines();
                self.spans
                    .push((Span::Link(dest_url.to_string()), Vec::new()));
            }
            Tag::HtmlBlock => {
                self.close_plain();
                self.spans.push((Span::Paragraph, Vec::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::HtmlBlock => {
                let Some((span, inline)) = self.spans.pop() else {
                    return;
                };
                let inline = self.finish_inline(inline);
                if inline.is_empty() && !matches!(span, Span::Heading(_)) {
                    return;
                }
                let block = match span {
                    Span::Heading(level) => Block::Heading { level, inline },
                    _ => Block::Paragraph { inline },
                };
                self.blocks().push(block);
            }
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Image => {
                let Some((span, children)) = self.spans.pop() else {
                    return;
                };
                let inline = match span {
                    Span::Strong => Inline::Strong { children },
                    Span::Em => Inline::Em { children },
                    Span::Del => Inline::Del { children },
                    Span::Link(href) => {
                        let children = if children.is_empty() {
                            vec![Inline::Text { text: href.clone() }]
                        } else {
                            children
                        };
                        Inline::Link { href, children }
                    }
                    _ => return,
                };
                self.inlines().push(inline);
            }
            TagEnd::CodeBlock => {
                if let Some((lang, mut text, range)) = self.code.take() {
                    if text.ends_with('\n') {
                        text.pop();
                    }
                    let open = !fence_closed(&self.src[range]);
                    self.blocks().push(Block::Code { lang, text, open });
                }
            }
            TagEnd::Item => {
                self.close_plain();
                let frame = self.frames.pop().expect("item frame");
                let Container::Item { checked } = frame.container else {
                    return;
                };
                if let Some(Frame {
                    container: Container::List { items, .. },
                    ..
                }) = self.frames.last_mut()
                {
                    items.push(ListItem {
                        checked,
                        blocks: frame.blocks,
                    });
                }
            }
            TagEnd::List(_) => {
                self.close_plain();
                let frame = self.frames.pop().expect("list frame");
                if let Container::List {
                    ordered,
                    start,
                    items,
                } = frame.container
                {
                    self.blocks().push(Block::List {
                        ordered,
                        start,
                        items,
                    });
                }
            }
            TagEnd::BlockQuote(_) => {
                self.close_plain();
                let frame = self.frames.pop().expect("quote frame");
                self.blocks().push(Block::Quote {
                    blocks: frame.blocks,
                });
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event, range: Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some((_, text, _)) = &mut self.code {
                    text.push_str(&t);
                } else {
                    push_text(self.inlines(), &t);
                }
            }
            Event::Code(t) => {
                let code = Inline::Code {
                    text: t.to_string(),
                    parts: match self.flavor {
                        Flavor::Docs if linkable_re().is_match(&t) => Some(autolink(&t)),
                        _ => None,
                    },
                };
                self.inlines().push(code);
            }
            Event::Html(t) | Event::InlineHtml(t) => self.html(&t),
            Event::SoftBreak => push_text(self.inlines(), " "),
            Event::HardBreak => self.inlines().push(Inline::Break),
            Event::Rule => {
                self.close_plain();
                self.blocks().push(Block::Hr);
            }
            Event::TaskListMarker(done) => {
                if let Some(Frame {
                    container: Container::Item { checked },
                    ..
                }) = self.frames.last_mut()
                {
                    *checked = Some(done);
                }
            }
            _ => {}
        }
    }
}

/// Whether a fenced code block's source ends with its closing fence.
fn fence_closed(block: &str) -> bool {
    let first = block.trim_start();
    let marker = first.chars().next().unwrap_or('`');
    if marker != '`' && marker != '~' {
        return true; // indented code
    }
    let len = first.chars().take_while(|c| *c == marker).count();
    let mut lines = block.trim_end_matches(['\n', '\r']).lines();
    lines.next();
    lines.next_back().is_some_and(|last| {
        let t = last.trim();
        t.chars().take_while(|c| *c == marker).count() >= len && t.chars().all(|c| c == marker)
    })
}

/// Markdown into blocks.
pub fn parse(src: &str, flavor: Flavor) -> Vec<Block> {
    let src = src.replace("\r\n", "\n");
    let mut b = Builder {
        src: &src,
        flavor,
        frames: vec![Frame {
            container: Container::Root,
            blocks: Vec::new(),
        }],
        spans: Vec::new(),
        code: None,
    };
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for (event, range) in Parser::new_ext(&src, opts).into_offset_iter() {
        b.event(event, range);
    }
    b.close_plain();
    while b.frames.len() > 1 {
        let frame = b.frames.pop().expect("frame");
        b.blocks().extend(frame.blocks);
    }
    b.frames.pop().map(|f| f.blocks).unwrap_or_default()
}

/// Inline Markdown (one paragraph) into inline content.
pub fn parse_inline(src: &str, flavor: Flavor) -> Vec<Inline> {
    let mut out = Vec::new();
    for block in parse(src, flavor) {
        if let Block::Paragraph { inline }
        | Block::Plain { inline }
        | Block::Heading { inline, .. } = block
        {
            if !out.is_empty() {
                out.push(Inline::Text { text: " ".into() });
            }
            out.extend(inline);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Inline {
        Inline::Text { text: t.into() }
    }

    #[test]
    fn parses_blocks() {
        let md = parse(
            "# Title\n\nSome **bold** and `code`.\n\n- [x] done\n- [ ] todo\n\n```php\n$a = 1;\n```\n1. one\n2. two",
            Flavor::Chat,
        );
        let kinds: Vec<&str> = md
            .iter()
            .map(|b| match b {
                Block::Heading { .. } => "heading",
                Block::Paragraph { .. } => "paragraph",
                Block::List { .. } => "list",
                Block::Code { .. } => "code",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["heading", "paragraph", "list", "code", "list"]);
        let Block::List { items, .. } = &md[2] else {
            panic!()
        };
        assert_eq!(
            items.iter().map(|i| i.checked).collect::<Vec<_>>(),
            [Some(true), Some(false)]
        );
        assert_eq!(
            md[3],
            Block::Code {
                lang: "php".into(),
                text: "$a = 1;".into(),
                open: false
            }
        );
        assert!(matches!(
            md[4],
            Block::List {
                ordered: true,
                start: 1,
                ..
            }
        ));
    }

    #[test]
    fn keeps_an_unterminated_fence_open_while_streaming() {
        assert_eq!(
            parse("```js\nlet a", Flavor::Chat),
            [Block::Code {
                lang: "js".into(),
                text: "let a".into(),
                open: true
            }]
        );
    }

    #[test]
    fn chat_keeps_html_as_text() {
        assert_eq!(
            parse_inline("a <b>x</b> `c` [l](https://x.dev) *e*", Flavor::Chat),
            [
                text("a <b>x</b> "),
                Inline::Code {
                    text: "c".into(),
                    parts: None
                },
                text(" "),
                Inline::Link {
                    href: "https://x.dev".into(),
                    children: vec![text("l")]
                },
                text(" "),
                Inline::Em {
                    children: vec![text("e")]
                },
            ]
        );
    }

    #[test]
    fn docs_never_keep_raw_html() {
        let md = parse(
            "Hello <script>alert(1)</script><img src=x onerror=\"alert(2)\"> <b>bold</b>",
            Flavor::Docs,
        );
        let Block::Paragraph { inline } = &md[0] else {
            panic!("{md:?}")
        };
        assert_eq!(Inline::text_of(inline), "Hello alert(1) bold");
        let md = parse("**strong** _em_ ~~old~~ `code`<br>next", Flavor::Docs);
        let Block::Paragraph { inline } = &md[0] else {
            panic!()
        };
        assert!(inline.contains(&Inline::Break));
        assert!(matches!(inline[4], Inline::Del { .. }));
    }

    #[test]
    fn docs_mark_urls_and_class_names() {
        let inline = parse_inline(
            "returns \\App\\Models\\Order and snake_case_name, see https://php.net/x.",
            Flavor::Docs,
        );
        assert_eq!(
            inline,
            [
                text("returns "),
                Inline::Fqcn {
                    text: "\\App\\Models\\Order".into()
                },
                text(" and snake_case_name, see "),
                Inline::Url {
                    text: "https://php.net/x".into()
                },
                text("."),
            ]
        );
        let code = &parse_inline("`\\App\\X<T>`", Flavor::Docs)[0];
        let Inline::Code {
            parts: Some(parts), ..
        } = code
        else {
            panic!("{code:?}")
        };
        assert_eq!(
            parts[0],
            Inline::Fqcn {
                text: "\\App\\X".into()
            }
        );
    }

    #[test]
    fn nests_lists_and_quotes() {
        let md = parse("- a\n  - b\n\n> quoted\n\n---", Flavor::Chat);
        let Block::List { items, .. } = &md[0] else {
            panic!()
        };
        assert!(matches!(items[0].blocks[1], Block::List { .. }));
        assert!(matches!(md[1], Block::Quote { .. }));
        assert_eq!(md[2], Block::Hr);
    }
}

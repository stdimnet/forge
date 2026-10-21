//! pulldown-cmark events into the [`Block`] tree.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
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
    /// The Markdown preview of a file: GitHub-flavored (tables, task lists, strikethrough,
    /// footnotes, autolinks, heading anchors, images). Raw HTML is sanitized into the AST:
    /// `<img>`, `<a href>`, `<br>`, `<b>`/`<strong>`, `<i>`/`<em>` and `<s>`/`<del>` become
    /// their Markdown counterparts, comments and every other tag are dropped.
    Preview,
}

enum Container {
    Root,
    Item {
        checked: Option<bool>,
    },
    Quote {
        /// GitHub alert (`> [!NOTE]`): `note`, `tip`, `important`, `warning`, `caution`.
        kind: Option<String>,
    },
    Footnote {
        label: String,
    },
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
    /// Preview: an image; the children are its alt text.
    Image {
        src: String,
        title: String,
    },
    /// Preview: a table cell.
    Cell,
    /// Preview: an inline formatting opened by a raw HTML tag (`<b>`, `<a href>`); the tag
    /// name closes it.
    Html(&'static str, HtmlSpan),
}

#[derive(Clone)]
enum HtmlSpan {
    Strong,
    Em,
    Del,
    Link(String),
}

#[derive(Default)]
struct TableState {
    align: Vec<String>,
    head: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
}

/// A footnote definition of the preview, numbered after parsing.
pub(crate) struct FootnoteDef {
    pub label: String,
    pub blocks: Vec<Block>,
    pub range: Range<usize>,
}

struct Builder<'a> {
    src: &'a str,
    flavor: Flavor,
    frames: Vec<Frame>,
    spans: Vec<(Span, Vec<Inline>)>,
    code: Option<(String, String, Range<usize>)>,
    table: Option<TableState>,
    /// Footnote numbers by normalized label, in the order of first reference.
    footnote_numbers: HashMap<String, u32>,
    footnotes: Vec<FootnoteDef>,
    footnote_start: Option<Range<usize>>,
    /// Heading anchors used so far (GitHub appends `-1`, `-2` to repeated ones).
    slugs: HashMap<String, u32>,
    /// Inside an HTML comment that spans several raw HTML events.
    in_comment: bool,
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

fn web_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:https?://|www\.)[^\s<>]*[^\s<>().,;:!?'\x22*_~]|[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
            .expect("web url regex")
    })
}

/// GitHub's extended autolinks: bare `https://`, `www.` and e-mail addresses in text.
fn web_autolink(text: &str, out: &mut Vec<Inline>) {
    let mut last = 0;
    for m in web_url_re().find_iter(text) {
        // Not inside a word (`foo@bar` in `x@y` is still linked by GitHub, URLs are not).
        let before = text[..m.start()].chars().next_back();
        if m.as_str().starts_with(['h', 'w']) && before.is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        push_text(out, &text[last..m.start()]);
        let t = m.as_str();
        let href = if t.starts_with("www.") {
            format!("http://{t}")
        } else if t.contains('@') && !t.contains("://") {
            format!("mailto:{t}")
        } else {
            t.to_string()
        };
        out.push(Inline::Link {
            href,
            children: vec![Inline::Text {
                text: t.to_string(),
            }],
        });
        last = m.end();
    }
    push_text(out, &text[last..]);
}

fn web_autolink_all(items: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::new();
    for i in items {
        match i {
            Inline::Text { text } => web_autolink(&text, &mut out),
            Inline::Strong { children } => out.push(Inline::Strong {
                children: web_autolink_all(children),
            }),
            Inline::Em { children } => out.push(Inline::Em {
                children: web_autolink_all(children),
            }),
            Inline::Del { children } => out.push(Inline::Del {
                children: web_autolink_all(children),
            }),
            other => out.push(other),
        }
    }
    out
}

/// Heading anchor like GitHub's: lower case, punctuation dropped, spaces to `-`.
pub(crate) fn slugify(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Decode the HTML character references of raw HTML text.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});").expect("entity regex")
    });
    let mut out = String::new();
    let mut last = 0;
    for c in re.captures_iter(text) {
        let m = c.get(0).expect("match");
        let name = &c[1];
        let decoded = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = name.strip_prefix('#') {
            dec.parse().ok().and_then(char::from_u32)
        } else {
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                "copy" => Some('©'),
                "reg" => Some('®'),
                "trade" => Some('™'),
                "hellip" => Some('…'),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "middot" => Some('·'),
                "bull" => Some('•'),
                "rarr" => Some('→'),
                "larr" => Some('←'),
                _ => None,
            }
        };
        out.push_str(&text[last..m.start()]);
        match decoded {
            Some(ch) => out.push(ch),
            None => out.push_str(m.as_str()),
        }
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// A raw HTML tag: its lower-cased name, whether it closes, and its attributes.
struct HtmlTag {
    name: String,
    closing: bool,
    attrs: Vec<(String, String)>,
}

impl HtmlTag {
    fn parse(tag: &str) -> Option<HtmlTag> {
        static NAME: OnceLock<Regex> = OnceLock::new();
        static ATTR: OnceLock<Regex> = OnceLock::new();
        let name_re = NAME.get_or_init(|| {
            Regex::new(r"^<\s*(/?)\s*([a-zA-Z][a-zA-Z0-9-]*)").expect("tag name regex")
        });
        let attr_re = ATTR.get_or_init(|| {
            Regex::new(
                r#"([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*(?:=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?"#,
            )
            .expect("attribute regex")
        });
        let c = name_re.captures(tag)?;
        let rest = &tag[c.get(0)?.end()..];
        let attrs = attr_re
            .captures_iter(rest)
            .map(|a| {
                let value = a
                    .get(2)
                    .or(a.get(3))
                    .or(a.get(4))
                    .map_or("", |m| m.as_str());
                (a[1].to_lowercase(), decode_entities(value))
            })
            .collect();
        Some(HtmlTag {
            name: c[2].to_lowercase(),
            closing: !c[1].is_empty(),
            attrs,
        })
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn html_span(name: &str) -> Option<(&'static str, HtmlSpan)> {
    Some(match name {
        "b" => ("b", HtmlSpan::Strong),
        "strong" => ("strong", HtmlSpan::Strong),
        "i" => ("i", HtmlSpan::Em),
        "em" => ("em", HtmlSpan::Em),
        "s" => ("s", HtmlSpan::Del),
        "del" => ("del", HtmlSpan::Del),
        "strike" => ("strike", HtmlSpan::Del),
        _ => return None,
    })
}

fn html_comment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<!--.*?-->").expect("comment regex"))
}

fn alignment(a: &Alignment) -> String {
    match a {
        Alignment::None => "",
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
    }
    .to_string()
}

/// Remove leading, trailing and repeated line breaks (left by dropped HTML block tags).
fn tidy_breaks(items: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for i in items {
        if i == Inline::Break {
            let last_is_break = matches!(out.last(), Some(Inline::Break));
            let blank_before = out
                .iter()
                .all(|x| matches!(x, Inline::Text { text } if text.trim().is_empty()));
            if last_is_break || blank_before {
                continue;
            }
        }
        out.push(i);
    }
    while matches!(out.last(), Some(Inline::Break))
        || matches!(out.last(), Some(Inline::Text { text }) if text.trim().is_empty())
    {
        out.pop();
    }
    // Whitespace around the breaks of an HTML block.
    let n = out.len();
    for k in 0..n {
        let prev_break = k > 0 && out[k - 1] == Inline::Break;
        let next_break = k + 1 < n && out[k + 1] == Inline::Break;
        if let Inline::Text { text } = &mut out[k] {
            if prev_break {
                *text = text.trim_start().to_string();
            }
            if next_break {
                *text = text.trim_end().to_string();
            }
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
            Flavor::Preview => tidy_breaks(web_autolink_all(items)),
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
            Flavor::Preview => self.preview_html(html),
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

    /// Raw HTML of the preview, sanitized into inline content.
    fn preview_html(&mut self, html: &str) {
        let mut html = html.to_string();
        // A comment that started in an earlier event.
        if self.in_comment {
            match html.find("-->") {
                Some(end) => {
                    html.drain(..end + 3);
                    self.in_comment = false;
                }
                None => return,
            }
        }
        let mut html = html_comment_re().replace_all(&html, "").into_owned();
        if let Some(open) = html.find("<!--") {
            html.truncate(open);
            self.in_comment = true;
        }
        let mut last = 0;
        for m in tag_re().find_iter(&html) {
            let before = decode_entities(&html[last..m.start()]);
            push_text(self.inlines(), &before);
            last = m.end();
            let Some(tag) = HtmlTag::parse(m.as_str()) else {
                continue;
            };
            self.html_tag(tag);
        }
        let rest = decode_entities(&html[last..]);
        push_text(self.inlines(), &rest);
    }

    fn html_tag(&mut self, tag: HtmlTag) {
        let name = tag.name.as_str();
        if tag.closing {
            match name {
                "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr" | "summary"
                | "details" | "pre" | "blockquote" | "table" => self.inlines().push(Inline::Break),
                _ => {
                    let kind = if name == "a" {
                        Some("a")
                    } else {
                        html_span(name).map(|(n, _)| n)
                    };
                    if let Some(kind) = kind {
                        if matches!(self.spans.last(), Some((Span::Html(n, _), _)) if *n == kind) {
                            self.close_html_span();
                        }
                    }
                }
            }
            return;
        }
        match name {
            "br" | "hr" => self.inlines().push(Inline::Break),
            "img" => {
                let Some(src) = tag.attr("src").filter(|s| !s.is_empty()) else {
                    return;
                };
                let image = Inline::Image {
                    src: src.to_string(),
                    alt: tag.attr("alt").unwrap_or_default().to_string(),
                    title: tag.attr("title").map(str::to_string),
                    width: tag.attr("width").map(str::to_string),
                    height: tag.attr("height").map(str::to_string),
                };
                self.inlines().push(image);
            }
            "a" => {
                let href = tag.attr("href").unwrap_or_default().to_string();
                let _ = self.inlines();
                self.spans
                    .push((Span::Html("a", HtmlSpan::Link(href)), Vec::new()));
            }
            _ => {
                if let Some((n, kind)) = html_span(name) {
                    let _ = self.inlines();
                    self.spans.push((Span::Html(n, kind), Vec::new()));
                }
            }
        }
    }

    /// Fold the innermost HTML span into its parent.
    fn close_html_span(&mut self) {
        let Some((Span::Html(_, kind), children)) = self.spans.pop() else {
            return;
        };
        let inline = match kind {
            HtmlSpan::Strong => Inline::Strong { children },
            HtmlSpan::Em => Inline::Em { children },
            HtmlSpan::Del => Inline::Del { children },
            HtmlSpan::Link(href) if href.is_empty() => {
                for c in children {
                    self.inlines().push(c);
                }
                return;
            }
            HtmlSpan::Link(href) => Inline::Link { href, children },
        };
        self.inlines().push(inline);
    }

    /// Close HTML spans left open at the end of a paragraph or cell.
    fn close_html_spans(&mut self) {
        while matches!(self.spans.last(), Some((Span::Html(..), _))) && self.spans.len() > 1 {
            self.close_html_span();
        }
        if matches!(self.spans.last(), Some((Span::Html(..), _))) {
            // Unbalanced at the top: keep the content as a paragraph.
            let (_, children) = self.spans.pop().expect("span");
            self.spans.push((Span::Paragraph, children));
        }
    }

    /// Close HTML spans opened inside an image's alt text.
    fn close_html_spans_above_image(&mut self) {
        while matches!(self.spans.last(), Some((Span::Html(..), _))) {
            self.close_html_span();
        }
    }

    /// The anchor of a heading, made unique like GitHub does.
    fn heading_id(&mut self, inline: &[Inline]) -> String {
        let base = slugify(&Inline::text_of(inline));
        let n = self.slugs.entry(base.clone()).or_insert(0);
        let id = if *n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        *n += 1;
        id
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
            Tag::BlockQuote(kind) => {
                self.close_plain();
                self.frames.push(Frame {
                    container: Container::Quote {
                        kind: kind.map(|k| format!("{k:?}").to_lowercase()),
                    },
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
            Tag::Image {
                dest_url, title, ..
            } if self.flavor == Flavor::Preview => {
                let _ = self.inlines();
                self.spans.push((
                    Span::Image {
                        src: dest_url.to_string(),
                        title: title.to_string(),
                    },
                    Vec::new(),
                ));
            }
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                let _ = self.inlines();
                self.spans
                    .push((Span::Link(dest_url.to_string()), Vec::new()));
            }
            Tag::Table(align) => {
                self.close_plain();
                self.table = Some(TableState {
                    align: align.iter().map(alignment).collect(),
                    ..TableState::default()
                });
            }
            Tag::TableCell => self.spans.push((Span::Cell, Vec::new())),
            Tag::FootnoteDefinition(label) => {
                self.close_plain();
                self.footnote_start = Some(range);
                self.frames.push(Frame {
                    container: Container::Footnote {
                        label: label.to_lowercase(),
                    },
                    blocks: Vec::new(),
                });
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
                self.close_html_spans();
                let Some((span, inline)) = self.spans.pop() else {
                    return;
                };
                let inline = self.finish_inline(inline);
                if inline.is_empty() && !matches!(span, Span::Heading(_)) {
                    return;
                }
                let block = match span {
                    Span::Heading(level) => {
                        let id = (self.flavor == Flavor::Preview).then(|| self.heading_id(&inline));
                        Block::Heading { level, inline, id }
                    }
                    _ => Block::Paragraph { inline },
                };
                self.blocks().push(block);
            }
            TagEnd::Image if self.flavor == Flavor::Preview => {
                self.close_html_spans_above_image();
                let Some((Span::Image { src, title }, children)) = self.spans.pop() else {
                    return;
                };
                let image = Inline::Image {
                    src,
                    alt: Inline::text_of(&children),
                    title: (!title.is_empty()).then_some(title),
                    width: None,
                    height: None,
                };
                self.inlines().push(image);
            }
            TagEnd::TableCell => {
                self.close_html_spans();
                let Some((_, inline)) = self.spans.pop() else {
                    return;
                };
                let inline = self.finish_inline(inline);
                if let Some(t) = &mut self.table {
                    t.row.push(inline);
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = &mut self.table {
                    t.head = std::mem::take(&mut t.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.row);
                    t.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.blocks().push(Block::Table {
                        align: t.align,
                        head: t.head,
                        rows: t.rows,
                    });
                }
            }
            TagEnd::FootnoteDefinition => {
                self.close_plain();
                let frame = self.frames.pop().expect("footnote frame");
                if let Container::Footnote { label } = frame.container {
                    let range = self.footnote_start.take().unwrap_or(0..0);
                    self.footnotes.push(FootnoteDef {
                        label,
                        blocks: frame.blocks,
                        range,
                    });
                }
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
                let kind = match frame.container {
                    Container::Quote { kind } => kind,
                    _ => None,
                };
                self.blocks().push(Block::Quote {
                    blocks: frame.blocks,
                    kind,
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
            Event::FootnoteReference(label) => {
                let key = label.to_lowercase();
                let next = self.footnote_numbers.len() as u32 + 1;
                let index = *self.footnote_numbers.entry(key.clone()).or_insert(next);
                self.inlines()
                    .push(Inline::FootnoteRef { label: key, index });
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

/// Blocks with the byte range of each top-level block, and the footnote definitions.
pub(crate) struct Parsed {
    pub blocks: Vec<Block>,
    pub ranges: Vec<Range<usize>>,
    pub footnotes: Vec<FootnoteDef>,
    pub footnote_numbers: HashMap<String, u32>,
}

/// Markdown (with `\n` line breaks) into blocks, remembering where each top-level block
/// starts and ends.
pub(crate) fn parse_ranges(src: &str, flavor: Flavor) -> Parsed {
    let mut b = Builder {
        src,
        flavor,
        frames: vec![Frame {
            container: Container::Root,
            blocks: Vec::new(),
        }],
        spans: Vec::new(),
        code: None,
        table: None,
        footnote_numbers: HashMap::new(),
        footnotes: Vec::new(),
        footnote_start: None,
        slugs: HashMap::new(),
        in_comment: false,
    };
    let mut opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    if flavor == Flavor::Preview {
        opts |= Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_GFM;
    }
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut top: Range<usize> = 0..0;
    for (event, range) in Parser::new_ext(src, opts).into_offset_iter() {
        let before = b.frames.first().map_or(0, |f| f.blocks.len());
        if depth == 0 {
            top = range.clone();
        }
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        b.event(event, range);
        if depth == 0 && b.frames.len() == 1 {
            let after = b.frames[0].blocks.len();
            ranges.extend((before..after).map(|_| top.clone()));
        }
    }
    b.close_plain();
    while b.frames.len() > 1 {
        let frame = b.frames.pop().expect("frame");
        b.blocks().extend(frame.blocks);
    }
    let blocks = b.frames.pop().map(|f| f.blocks).unwrap_or_default();
    ranges.resize(blocks.len(), src.len()..src.len());
    Parsed {
        blocks,
        ranges,
        footnotes: b.footnotes,
        footnote_numbers: b.footnote_numbers,
    }
}

/// Markdown into blocks.
pub fn parse(src: &str, flavor: Flavor) -> Vec<Block> {
    let src = src.replace("\r\n", "\n");
    parse_ranges(&src, flavor).blocks
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

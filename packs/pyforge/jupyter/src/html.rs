//! A small HTML tokenizer for notebook outputs: [`sanitize_html`] (an allowlist: no
//! scripts, event handlers, frames, forms or remote resources; the result is shown in a
//! sandboxed frame without scripts on top), [`sanitize_svg`] (scripts, event handlers,
//! `foreignObject` and external references removed) and [`tables`] (the cells of HTML
//! tables, for pandas DataFrames).

/// An attribute: name as written and the raw (still entity-encoded) value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Text(&'a str),
    Start {
        name: String,
        attrs: Vec<Attr>,
        self_closing: bool,
    },
    End {
        name: String,
    },
    /// Comments, doctypes and processing instructions.
    Other(&'a str),
}

/// Elements whose content is raw text up to their end tag.
const RAW_TEXT: [&str; 6] = ["script", "style", "textarea", "title", "xmp", "noscript"];

/// Split HTML (or SVG) into tokens. Malformed markup degrades to text.
pub fn tokenize(src: &str) -> Vec<Token<'_>> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        let kind = b.get(i + 1).copied().unwrap_or(0);
        let token_end = if rest.starts_with("<!--") {
            Some(rest.find("-->").map_or(src.len(), |e| i + e + 3))
        } else if kind == b'!' || kind == b'?' {
            // A doctype's internal subset (`[<!ENTITY …>]`) ends with `]>`.
            let close = rest.find('>').unwrap_or(rest.len());
            let end = match rest[..close].find('[') {
                Some(_) => rest.find("]>").map_or(rest.len(), |e| e + 2),
                None => (close + 1).min(rest.len()),
            };
            Some(i + end)
        } else if kind == b'/' || kind.is_ascii_alphabetic() {
            tag_end(b, i + 1)
        } else {
            None
        };
        let Some(end) = token_end else {
            i += 1;
            continue;
        };
        if text_start < i {
            out.push(Token::Text(&src[text_start..i]));
        }
        let raw = &src[i..end];
        if kind == b'!' || kind == b'?' {
            out.push(Token::Other(raw));
            i = end;
            text_start = end;
            continue;
        }
        let tok = parse_tag(raw);
        i = end;
        text_start = end;
        if let Token::Start {
            name, self_closing, ..
        } = &tok
        {
            let lower = name.to_ascii_lowercase();
            if !self_closing && RAW_TEXT.contains(&lower.as_str()) {
                let close = format!("</{lower}");
                let lower_rest = src[end..].to_ascii_lowercase();
                let stop = lower_rest.find(&close).map_or(src.len(), |p| end + p);
                out.push(tok);
                if stop > end {
                    out.push(Token::Text(&src[end..stop]));
                }
                i = stop;
                text_start = stop;
                continue;
            }
        }
        out.push(tok);
    }
    if text_start < b.len() {
        out.push(Token::Text(&src[text_start..]));
    }
    out
}

/// End (exclusive) of a tag starting after `<`, honoring quoted attribute values.
fn tag_end(b: &[u8], mut i: usize) -> Option<usize> {
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(i + 1);
        } else if c == b'<' {
            return None;
        }
        i += 1;
    }
    None
}

fn parse_tag(raw: &str) -> Token<'static> {
    let inner = raw.trim_start_matches('<').trim_end_matches('>');
    if let Some(name) = inner.strip_prefix('/') {
        return Token::End {
            name: name.split_whitespace().next().unwrap_or("").to_string(),
        };
    }
    let self_closing = inner.trim_end().ends_with('/');
    let inner = inner.trim_end().trim_end_matches('/');
    let name_len = inner
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(inner.len());
    let name = inner[..name_len].to_string();
    let mut attrs = Vec::new();
    let s = &inner.as_bytes()[name_len..];
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && (s[i].is_ascii_whitespace() || s[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < s.len() && !s[i].is_ascii_whitespace() && s[i] != b'=' && s[i] != b'/' {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let an = String::from_utf8_lossy(&s[start..i]).into_owned();
        while i < s.len() && s[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = None;
        if i < s.len() && s[i] == b'=' {
            i += 1;
            while i < s.len() && s[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < s.len() && (s[i] == b'"' || s[i] == b'\'') {
                let q = s[i];
                let vs = i + 1;
                i = vs;
                while i < s.len() && s[i] != q {
                    i += 1;
                }
                value = Some(String::from_utf8_lossy(&s[vs..i.min(s.len())]).into_owned());
                i += 1;
            } else {
                let vs = i;
                while i < s.len() && !s[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = Some(String::from_utf8_lossy(&s[vs..i]).into_owned());
            }
        }
        attrs.push(Attr { name: an, value });
    }
    Token::Start {
        name,
        attrs,
        self_closing,
    }
}

/// Decode the common character references (`&amp;`, `&#39;`, `&#x27;`, `&nbsp;`, …).
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace());
        let decoded = end
            .filter(|e| rest.as_bytes().get(e + 1) == Some(&b';'))
            .and_then(|e| {
                let name = &rest[1..=e];
                let ch = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    "times" => Some('×'),
                    "hellip" => Some('…'),
                    _ => name.strip_prefix('#').and_then(|n| {
                        let code = match n.strip_prefix(['x', 'X']) {
                            Some(h) => u32::from_str_radix(h, 16).ok(),
                            None => n.parse().ok(),
                        };
                        code.and_then(char::from_u32)
                    }),
                };
                ch.map(|c| (c, e + 2))
            });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape_attr(v: &str) -> String {
    v.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(v: &str) -> String {
    v.replace('<', "&lt;").replace('>', "&gt;")
}

/// A URL as the browser would read it: entities decoded, controls and spaces removed.
fn normalized_url(v: &str) -> String {
    decode_entities(v)
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn safe_css(v: &str) -> bool {
    let n = normalized_url(v);
    !(n.contains("expression(")
        || n.contains("url(")
        || n.contains("javascript:")
        || n.contains("@import"))
}

const HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "label",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "small",
    "span",
    "strike",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "wbr",
];

/// Elements removed together with their content.
const DROP_WITH_CONTENT: &[&str] = &[
    "script",
    "iframe",
    "frame",
    "frameset",
    "object",
    "embed",
    "applet",
    "noscript",
    "template",
    "title",
    "head",
    "foreignobject",
    "audio",
    "video",
    "canvas",
    "math",
    "svg",
    "select",
    "textarea",
    "button",
    "xmp",
];

const HTML_ATTRS: &[&str] = &[
    "align",
    "alt",
    "bgcolor",
    "border",
    "cellpadding",
    "cellspacing",
    "class",
    "color",
    "colspan",
    "dir",
    "face",
    "headers",
    "height",
    "href",
    "id",
    "lang",
    "open",
    "rowspan",
    "scope",
    "size",
    "span",
    "src",
    "start",
    "style",
    "summary",
    "title",
    "type",
    "valign",
    "width",
    "reversed",
    "datetime",
];

/// HTML for a sandboxed frame: allowlisted elements and attributes only, `href`s to
/// http(s)/mailto/anchors, images only as `data:image/…` URLs, CSS without `url()`.
pub fn sanitize_html(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut drop_depth: Vec<String> = Vec::new();
    let mut in_style = false;
    for tok in tokenize(src) {
        match tok {
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                let lower = name.to_ascii_lowercase();
                if !drop_depth.is_empty() || DROP_WITH_CONTENT.contains(&lower.as_str()) {
                    if !self_closing && !is_void(&lower) {
                        drop_depth.push(lower);
                    }
                    continue;
                }
                if !HTML_TAGS.contains(&lower.as_str()) {
                    continue;
                }
                out.push('<');
                out.push_str(&lower);
                for a in attrs {
                    let an = a.name.to_ascii_lowercase();
                    if !HTML_ATTRS.contains(&an.as_str()) {
                        continue;
                    }
                    let v = a.value.unwrap_or_default();
                    let ok = match an.as_str() {
                        "href" => {
                            let u = normalized_url(&v);
                            u.starts_with("http://")
                                || u.starts_with("https://")
                                || u.starts_with("mailto:")
                                || u.starts_with('#')
                        }
                        "src" => {
                            normalized_url(&v).starts_with("data:image/")
                                && !normalized_url(&v).starts_with("data:image/svg")
                        }
                        "style" => safe_css(&v),
                        _ => true,
                    };
                    if ok {
                        out.push_str(&format!(" {an}=\"{}\"", escape_attr(&v)));
                    }
                }
                if self_closing {
                    out.push_str("/>");
                } else {
                    out.push('>');
                }
                in_style = lower == "style" && !self_closing;
            }
            Token::End { name } => {
                let lower = name.to_ascii_lowercase();
                if let Some(top) = drop_depth.last() {
                    if *top == lower {
                        drop_depth.pop();
                    }
                    continue;
                }
                if lower == "style" {
                    in_style = false;
                }
                if HTML_TAGS.contains(&lower.as_str()) {
                    out.push_str(&format!("</{lower}>"));
                }
            }
            Token::Text(t) => {
                if !drop_depth.is_empty() {
                    continue;
                }
                if in_style {
                    if safe_css(t) {
                        out.push_str(&escape_text(t));
                    }
                } else {
                    out.push_str(&escape_text(t));
                }
            }
            Token::Other(_) => {}
        }
    }
    out
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

const SVG_DROP: &[&str] = &[
    "script",
    "foreignobject",
    "iframe",
    "object",
    "embed",
    "handler",
];

/// SVG without scripts, event handlers, `foreignObject` or external references (shown in
/// an `<img>`, which runs no scripts either).
pub fn sanitize_svg(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut drop_depth = 0usize;
    for tok in tokenize(src) {
        match tok {
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                let lower = name.to_ascii_lowercase();
                if drop_depth > 0 || SVG_DROP.contains(&lower.as_str()) {
                    if !self_closing {
                        drop_depth += 1;
                    }
                    continue;
                }
                out.push('<');
                out.push_str(&name);
                for a in attrs {
                    let an = a.name.to_ascii_lowercase();
                    if an.starts_with("on") {
                        continue;
                    }
                    let v = a.value.clone().unwrap_or_default();
                    if an == "href" || an.ends_with(":href") {
                        let u = normalized_url(&v);
                        if !(u.starts_with('#')
                            || u.starts_with("data:image/png")
                            || u.starts_with("data:image/jpeg"))
                        {
                            continue;
                        }
                    }
                    if an == "style" && !safe_css(&v) {
                        continue;
                    }
                    match a.value {
                        Some(v) => out.push_str(&format!(" {}=\"{}\"", a.name, escape_attr(&v))),
                        None => out.push_str(&format!(" {}", a.name)),
                    }
                }
                out.push_str(if self_closing { "/>" } else { ">" });
            }
            Token::End { name } => {
                if drop_depth > 0 {
                    drop_depth -= 1;
                    continue;
                }
                out.push_str(&format!("</{name}>"));
            }
            Token::Text(t) => {
                if drop_depth == 0 {
                    out.push_str(t);
                }
            }
            // `<?xml …?>` stays; doctypes (entity definitions) and stylesheets go.
            Token::Other(t) => {
                if drop_depth == 0 && t.starts_with("<?xml ") {
                    out.push_str(t);
                }
            }
        }
    }
    out
}

/// A table cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub text: String,
    pub header: bool,
    pub colspan: usize,
    pub rowspan: usize,
}

/// A table row and the section it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub head: bool,
    pub cells: Vec<TableCell>,
}

/// An HTML table: its `class` attribute and rows. Text after the table (pandas' "5 rows ×
/// 3 columns") is in `after`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    pub class: String,
    pub rows: Vec<TableRow>,
    pub after: String,
}

/// The top-level tables of an HTML fragment.
pub fn tables(src: &str) -> Vec<Table> {
    let mut out: Vec<Table> = Vec::new();
    let mut depth = 0usize;
    let mut head = false;
    let mut cell: Option<TableCell> = None;
    let mut skip = 0usize;
    for tok in tokenize(src) {
        match tok {
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                let lower = name.to_ascii_lowercase();
                let attr = |n: &str| {
                    attrs
                        .iter()
                        .find(|a| a.name.eq_ignore_ascii_case(n))
                        .and_then(|a| a.value.clone())
                };
                match lower.as_str() {
                    "style" | "script" if !self_closing => skip += 1,
                    "table" => {
                        depth += 1;
                        if depth == 1 {
                            out.push(Table {
                                class: attr("class").unwrap_or_default(),
                                ..Table::default()
                            });
                        }
                    }
                    "thead" if depth == 1 => head = true,
                    "tbody" | "tfoot" if depth == 1 => head = false,
                    "tr" if depth == 1 => {
                        if let Some(t) = out.last_mut() {
                            t.rows.push(TableRow {
                                head,
                                cells: Vec::new(),
                            });
                        }
                    }
                    "td" | "th" if depth == 1 => {
                        let num = |n: &str| {
                            attr(n)
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(1usize)
                                .max(1)
                        };
                        cell = Some(TableCell {
                            text: String::new(),
                            header: lower == "th",
                            colspan: num("colspan"),
                            rowspan: num("rowspan"),
                        });
                    }
                    "br" => {
                        if let Some(c) = cell.as_mut() {
                            c.text.push(' ');
                        }
                    }
                    _ => {}
                }
            }
            Token::End { name } => {
                let lower = name.to_ascii_lowercase();
                match lower.as_str() {
                    "style" | "script" => skip = skip.saturating_sub(1),
                    "table" => depth = depth.saturating_sub(1),
                    "td" | "th" if depth == 1 => {
                        if let (Some(mut c), Some(t)) = (cell.take(), out.last_mut()) {
                            c.text = collapse(&decode_entities(&c.text));
                            if t.rows.is_empty() {
                                t.rows.push(TableRow {
                                    head,
                                    cells: Vec::new(),
                                });
                            }
                            t.rows.last_mut().expect("a row").cells.push(c);
                        }
                    }
                    _ => {}
                }
            }
            Token::Text(t) => {
                if skip > 0 {
                    continue;
                }
                if let Some(c) = cell.as_mut() {
                    c.text.push_str(t);
                } else if depth == 0 {
                    if let Some(table) = out.last_mut() {
                        table.after.push_str(t);
                    }
                }
            }
            Token::Other(_) => {}
        }
    }
    for t in &mut out {
        t.after = collapse(&decode_entities(&t.after));
    }
    out
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_attributes_and_raw_text() {
        let toks =
            tokenize("<p class=\"a b\" data-x='1>2' hidden>x<script>if (a<b) {}</script></P><br/>");
        assert_eq!(toks.len(), 7, "{toks:?}");
        match &toks[0] {
            Token::Start { name, attrs, .. } => {
                assert_eq!(name, "p");
                assert_eq!(attrs[1].value.as_deref(), Some("1>2"));
                assert_eq!(
                    attrs[2],
                    Attr {
                        name: "hidden".into(),
                        value: None
                    }
                );
            }
            t => panic!("{t:?}"),
        }
        assert_eq!(toks[3], Token::Text("if (a<b) {}"));
        assert!(matches!(
            &toks[6],
            Token::Start {
                self_closing: true,
                ..
            }
        ));
    }

    #[test]
    fn sanitizes_html() {
        let dirty = concat!(
            "<div onclick=\"steal()\" style=\"color: red\"><script>alert(1)</script>",
            "<a href=\"javascript:alert(1)\">x</a><a href=\"https://pandas.pydata.org\" target=\"_top\">docs</a>",
            "<img src=\"https://tracker/x.png\"><img src=\"data:image/png;base64,AA\">",
            "<iframe src=\"x\"><p>inside</p></iframe><form><input value=1>text</form>",
            "<style>td { color: blue } .x { background: url(http://e/) }</style>",
            "<p style=\"background:url(x)\">p</p>a < b</div>"
        );
        assert_eq!(
            sanitize_html(dirty),
            concat!(
                "<div style=\"color: red\"><a>x</a><a href=\"https://pandas.pydata.org\">docs</a>",
                "<img/><img src=\"data:image/png;base64,AA\">text<style></style><p>p</p>a &lt; b</div>"
            )
            .replace("<img/>", "<img>")
        );
    }

    #[test]
    fn sanitizes_svg() {
        let svg = "<?xml version=\"1.0\"?><!DOCTYPE svg [<!ENTITY a \"b\">]><svg xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 4 4\"><linearGradient id=\"g\"/><use xlink:href=\"#g\"/><image href=\"http://x/y.png\"/><foreignObject><div>x</div></foreignObject><a href=\"javascript:x\"><text onmouseover=\"x()\">t</text></a></svg>";
        assert_eq!(
            sanitize_svg(svg),
            "<?xml version=\"1.0\"?><svg xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 4 4\"><linearGradient id=\"g\"/><use xlink:href=\"#g\"/><image/><a><text>t</text></a></svg>"
        );
    }

    #[test]
    fn reads_tables() {
        let t = tables("<style>x</style><table class=\"dataframe\"><thead><tr><th></th><th colspan=\"2\">a &amp; b</th></tr></thead><tbody><tr><th rowspan=2>0</th><td> 1 </td><td>x<br>y</td></tr></tbody></table><p>1 rows &times; 2 columns</p>");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].class, "dataframe");
        assert!(t[0].rows[0].head && !t[0].rows[1].head);
        assert_eq!(t[0].rows[0].cells[1].text, "a & b");
        assert_eq!(t[0].rows[0].cells[1].colspan, 2);
        assert_eq!(t[0].rows[1].cells[0].rowspan, 2);
        assert_eq!(t[0].rows[1].cells[2].text, "x y");
        assert_eq!(t[0].after, "1 rows × 2 columns");
        assert_eq!(decode_entities("&#39;&#x41;&bogus;&"), "'A&bogus;&");
    }
}

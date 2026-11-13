//! Markup output: resolves a parsed abbreviation (snippets, implicit tags, numbering, lorem,
//! repeats, wrapped text) and writes it as HTML or JSX with tab stops, formatted like
//! Emmet (block elements on their own lines, short runs of inline elements kept together).

use crate::abbr::{self, Attr, Element, Kind, Repeat, Tree};
use crate::lorem::{lorem, lorem_count};
use crate::snippets::{implicit_tag, is_inline, is_void, snippet_entry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Html,
    Jsx,
}

#[derive(Debug, Clone)]
pub struct MarkupOptions {
    pub flavor: Flavor,
    /// Attribute written for `.class` in JSX (`className` or `class`).
    pub jsx_class_attr: String,
    pub indent: String,
    /// Wrapping a URL or e-mail address in `a` fills its `href`.
    pub href_recognition: bool,
}

impl Default for MarkupOptions {
    fn default() -> Self {
        Self {
            flavor: Flavor::Html,
            jsx_class_attr: "className".into(),
            indent: "\t".into(),
            href_recognition: true,
        }
    }
}

/// A tab stop in an output text (UTF-16 offsets inside the text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    pub from: usize,
    pub to: usize,
    /// From 1; stops with the same index are edited together.
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    pub stops: Vec<Stop>,
}

const MAX_REPEAT: usize = 1000;
const MAX_OUTPUT_NODES: usize = 5000;
const MAX_STOPS: usize = 200;

/// Expand a markup abbreviation; `wrap` is the text Surround with Emmet puts inside.
pub fn expand(abbr: &str, opts: &MarkupOptions, wrap: Option<&[String]>) -> Result<Output, String> {
    let tree = abbr::parse(abbr)?;
    let mut r = Resolver {
        lines: wrap,
        seed: 0,
        active: Vec::new(),
        produced: 0,
        used_lines: false,
    };
    let root = tree.root().children.clone();
    let mut nodes = r.list(&tree, &root, "", &mut Vec::new())?;
    if let Some(lines) = wrap {
        if !r.used_lines && !lines.is_empty() {
            insert_content(&mut nodes, lines, opts.href_recognition);
        }
    }
    let mut w = Writer {
        opts,
        out: String::new(),
        stops: Vec::new(),
        auto: 0,
    };
    w.list(&nodes, 0);
    Ok(w.finish())
}

/// Whether the abbreviation is clearly meant as Emmet: it uses an operator or attribute
/// syntax, or names a known element or snippet.
pub fn looks_like_markup(tree: &Tree) -> bool {
    tree.nodes.len() > 2
        || tree.nodes.iter().any(|n| n.repeat.is_some())
        || tree.elements().any(|e| {
            !e.attrs.is_empty()
                || e.text.is_some()
                || crate::snippets::is_known_tag(&e.name)
                || lorem_count(&e.name).is_some()
        })
}

#[derive(Debug, Clone, Default)]
struct ONode {
    /// `None`: text.
    name: Option<String>,
    attrs: Vec<Attr>,
    text: Option<String>,
    /// Wrapped lines, written as a block.
    block: Vec<String>,
    children: Vec<ONode>,
    self_closing: bool,
}

impl ONode {
    fn text(t: String) -> Self {
        Self {
            text: Some(t),
            ..Self::default()
        }
    }

    fn is_inline(&self) -> bool {
        self.block.is_empty() && self.name.as_deref().is_none_or(is_inline)
    }

    fn is_inline_element(&self) -> bool {
        self.name.as_deref().is_some_and(is_inline)
    }
}

/// Repeat context for numbering: `$` is the index of the closest repeated element.
struct Num {
    index: usize,
    count: usize,
}

struct Resolver<'a> {
    lines: Option<&'a [String]>,
    seed: usize,
    /// Snippets being expanded (`a` → `a[href]` must not expand `a` again).
    active: Vec<&'static str>,
    produced: usize,
    used_lines: bool,
}

impl Resolver<'_> {
    fn list(
        &mut self,
        tree: &Tree,
        ids: &[usize],
        parent: &str,
        nums: &mut Vec<Num>,
    ) -> Result<Vec<ONode>, String> {
        let mut out = Vec::new();
        for &id in ids {
            let node = &tree.nodes[id];
            let implicit = node.repeat == Some(Repeat::Implicit);
            let count = match node.repeat {
                None => 1,
                Some(Repeat::Count(n)) => n.min(MAX_REPEAT),
                Some(Repeat::Implicit) => self.lines.map_or(1, |l| l.len().max(1)),
            };
            for i in 0..count {
                if node.repeat.is_some() {
                    nums.push(Num {
                        index: i + 1,
                        count,
                    });
                }
                let mut made = match &node.kind {
                    Kind::Group => self.list(tree, &node.children, parent, nums)?,
                    Kind::Element(e) => self.element(tree, e, &node.children, parent, nums)?,
                    Kind::Root => Vec::new(),
                };
                if implicit {
                    if let Some(line) = self.lines.and_then(|l| l.get(i)) {
                        insert_content(&mut made, std::slice::from_ref(line), false);
                        self.used_lines = true;
                    }
                }
                if node.repeat.is_some() {
                    nums.pop();
                }
                self.produced += made.len();
                if self.produced > MAX_OUTPUT_NODES {
                    return Err("abbreviation produces too much output".into());
                }
                out.extend(made);
            }
        }
        Ok(out)
    }

    fn element(
        &mut self,
        tree: &Tree,
        e: &Element,
        children: &[usize],
        parent: &str,
        nums: &mut Vec<Num>,
    ) -> Result<Vec<ONode>, String> {
        let mut el = numbered(e, nums.last());
        if let Some(n) = lorem_count(&el.name) {
            let text = lorem(n, self.seed);
            self.seed += 1;
            if el.attrs.is_empty() {
                return Ok(vec![ONode::text(text)]);
            }
            el.name.clear();
            el.text = Some(text);
        }
        if el.is_text() {
            return Ok(vec![ONode::text(el.text.unwrap_or_default())]);
        }
        if let Some((key, abbr)) = snippet_entry(&el.name)
            .filter(|(k, _)| !self.active.contains(k) && self.active.len() < 8)
        {
            let snippet_tree = abbr::parse(abbr)?;
            self.active.push(key);
            let ids = snippet_tree.root().children.clone();
            let mut nodes = self.list(&snippet_tree, &ids, parent, &mut Vec::new())?;
            self.active.pop();
            if let Some(first) = nodes.iter_mut().find(|n| n.name.is_some()) {
                for a in el.attrs {
                    if a.name == "class" {
                        if let Some(v) = a.value.as_deref().filter(|v| !v.is_empty()) {
                            add_class(&mut first.attrs, v);
                            continue;
                        }
                    }
                    set_attr(&mut first.attrs, a);
                }
                if el.text.is_some() {
                    first.text = el.text;
                }
                first.self_closing |= el.self_closing;
            }
            let deepest = deepest_last(&mut nodes).and_then(|n| n.name.clone());
            let kids = self.list(tree, children, deepest.as_deref().unwrap_or(parent), nums)?;
            match deepest_last(&mut nodes) {
                Some(target) => target.children.extend(kids),
                None => nodes.extend(kids),
            }
            return Ok(nodes);
        }
        if el.name.is_empty() {
            el.name = implicit_tag(parent).to_string();
        }
        let kids = self.list(tree, children, &el.name, nums)?;
        Ok(vec![ONode {
            name: Some(el.name),
            attrs: el.attrs,
            text: el.text,
            block: Vec::new(),
            children: kids,
            self_closing: el.self_closing,
        }])
    }
}

fn add_class(attrs: &mut Vec<Attr>, class: &str) {
    let mut el = Element {
        attrs: std::mem::take(attrs),
        ..Element::default()
    };
    el.add_class(class);
    *attrs = el.attrs;
}

fn set_attr(attrs: &mut Vec<Attr>, attr: Attr) {
    let mut el = Element {
        attrs: std::mem::take(attrs),
        ..Element::default()
    };
    el.set_attr(attr);
    *attrs = el.attrs;
}

/// The innermost last element: where children and wrapped text go.
fn deepest_last(nodes: &mut [ONode]) -> Option<&mut ONode> {
    let mut node = nodes.iter_mut().rev().find(|n| n.name.is_some())?;
    loop {
        let has_child = node.children.iter().any(|c| c.name.is_some());
        if !has_child {
            return Some(node);
        }
        node = node.children.iter_mut().rev().find(|n| n.name.is_some())?;
    }
}

/// Put wrapped text into the nodes: at `$#` placeholders, else into the deepest last element.
fn insert_content(nodes: &mut Vec<ONode>, lines: &[String], href_recognition: bool) {
    let joined = lines.join("\n");
    if replace_placeholders(nodes, &joined) {
        return;
    }
    let content = if lines.len() == 1 {
        ONode::text(lines[0].clone())
    } else {
        ONode {
            block: lines.to_vec(),
            ..ONode::default()
        }
    };
    match deepest_last(nodes) {
        Some(target) => {
            if href_recognition && lines.len() == 1 && target.name.as_deref() == Some("a") {
                if let Some(href) = url_of(lines[0].trim()) {
                    let empty = target
                        .attrs
                        .iter()
                        .find(|a| a.name == "href")
                        .is_none_or(|a| a.value.as_deref().unwrap_or("").is_empty());
                    if empty {
                        set_attr(
                            &mut target.attrs,
                            Attr {
                                name: "href".into(),
                                value: Some(href),
                                ..Attr::default()
                            },
                        );
                    }
                }
            }
            target.children.push(content);
        }
        None => nodes.push(content),
    }
}

fn replace_placeholders(nodes: &mut [ONode], content: &str) -> bool {
    let mut found = false;
    for n in nodes {
        if let Some(t) = &mut n.text {
            if t.contains("$#") {
                *t = t.replace("$#", content);
                found = true;
            }
        }
        for a in &mut n.attrs {
            if let Some(v) = &mut a.value {
                if v.contains("$#") {
                    *v = v.replace("$#", content);
                    found = true;
                }
            }
        }
        found |= replace_placeholders(&mut n.children, content);
    }
    found
}

/// `href` for wrapped text that is a URL or an e-mail address.
fn url_of(text: &str) -> Option<String> {
    if text.contains(char::is_whitespace) {
        return None;
    }
    if text.starts_with("http://") || text.starts_with("https://") || text.starts_with("//") {
        return Some(text.to_string());
    }
    if text.starts_with("www.") {
        return Some(format!("http://{text}"));
    }
    let (user, host) = text.split_once('@')?;
    (!user.is_empty() && host.contains('.') && !host.starts_with('.'))
        .then(|| format!("mailto:{text}"))
}

/// Apply `$` numbering of the closest repeat to an element's name, attributes and text.
fn numbered(e: &Element, num: Option<&Num>) -> Element {
    let Some(num) = num else {
        return e.clone();
    };
    let mut el = e.clone();
    el.name = number(&el.name, num);
    for a in &mut el.attrs {
        if let Some(v) = &mut a.value {
            *v = number(v, num);
        }
    }
    if let Some(t) = &mut el.text {
        *t = number(t, num);
    }
    el
}

/// Replace `$`, `$$`, `$@-`, `$@3`, `$@-3` (but not `${` or `$#`) with the repeat index.
fn number(s: &str, num: &Num) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' || matches!(chars.get(i + 1), Some('{' | '#')) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut width = 0;
        while chars.get(i) == Some(&'$') {
            width += 1;
            i += 1;
        }
        let mut reverse = false;
        let mut base = 1usize;
        if chars.get(i) == Some(&'@') {
            i += 1;
            if chars.get(i) == Some(&'-') {
                reverse = true;
                i += 1;
            }
            let start = i;
            while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            if i > start {
                base = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(1);
            }
        }
        let n = if reverse {
            num.count - num.index + base
        } else {
            num.index + base - 1
        };
        out.push_str(&format!("{n:0width$}"));
    }
    out
}

/// Should `items[i]` start on its own line (Emmet's formatting rules)?
fn should_format(items: &[ONode], i: usize) -> bool {
    let n = &items[i];
    if !n.is_inline() {
        return true;
    }
    if i == 0 {
        if items.iter().any(|x| !x.is_inline()) {
            return true;
        }
    } else if !items[i - 1].is_inline() {
        return true;
    }
    if n.is_inline_element() {
        let mut adjacent = 1;
        let mut j = i;
        while j > 0 && items[j - 1].is_inline_element() {
            adjacent += 1;
            j -= 1;
        }
        let mut j = i + 1;
        while j < items.len() && items[j].is_inline_element() {
            adjacent += 1;
            j += 1;
        }
        if adjacent >= 3 {
            return true;
        }
    }
    (0..n.children.len()).any(|j| should_format(&n.children, j))
}

enum StopKey {
    Marker(usize),
    Auto(usize),
}

struct Writer<'a> {
    opts: &'a MarkupOptions,
    out: String,
    /// Byte ranges of the stops with their key.
    stops: Vec<(usize, usize, StopKey)>,
    auto: usize,
}

impl Writer<'_> {
    fn newline(&mut self, depth: usize) {
        self.out.push('\n');
        for _ in 0..depth {
            self.out.push_str(&self.opts.indent);
        }
    }

    fn auto_stop(&mut self) {
        self.auto += 1;
        let at = self.out.len();
        self.stops.push((at, at, StopKey::Auto(self.auto)));
    }

    /// Text with `${n}` / `${n:default}` tab stops.
    fn text(&mut self, t: &str) {
        let mut rest = t;
        while let Some(i) = rest.find("${") {
            let after = &rest[i + 2..];
            let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
            let tail = &after[digits..];
            let parsed = if digits == 0 {
                None
            } else if let Some(stripped) = tail.strip_prefix('}') {
                Some(("", stripped))
            } else if let Some(def) = tail.strip_prefix(':') {
                def.find('}').map(|end| (&def[..end], &def[end + 1..]))
            } else {
                None
            };
            let Some((default, next)) = parsed else {
                self.out.push_str(&rest[..i + 2]);
                rest = after;
                continue;
            };
            self.out.push_str(&rest[..i]);
            let n: usize = after[..digits].parse().unwrap_or(0);
            let from = self.out.len();
            self.out.push_str(default);
            self.stops.push((from, self.out.len(), StopKey::Marker(n)));
            rest = next;
        }
        self.out.push_str(rest);
    }

    fn attr_name(&self, name: &str) -> String {
        match name {
            "class" if self.opts.flavor == Flavor::Jsx => self.opts.jsx_class_attr.clone(),
            "for" if self.opts.flavor == Flavor::Jsx => "htmlFor".into(),
            _ => name.to_string(),
        }
    }

    fn list(&mut self, nodes: &[ONode], depth: usize) {
        for i in 0..nodes.len() {
            let format = !self.out.is_empty() && should_format(nodes, i);
            self.node(&nodes[i], depth, format);
        }
    }

    fn node(&mut self, n: &ONode, depth: usize, format: bool) {
        if format {
            self.newline(depth);
        }
        let Some(name) = &n.name else {
            if n.block.is_empty() {
                self.text(n.text.as_deref().unwrap_or(""));
            } else {
                for (k, line) in n.block.iter().enumerate() {
                    if k > 0 {
                        if line.is_empty() {
                            self.out.push('\n');
                        } else {
                            self.newline(depth);
                        }
                    }
                    self.out.push_str(line);
                }
            }
            return;
        };
        self.out.push('<');
        self.out.push_str(name);
        for a in &n.attrs {
            self.out.push(' ');
            let attr = self.attr_name(&a.name);
            self.out.push_str(&attr);
            if a.boolean {
                continue;
            }
            if a.expression {
                self.out.push_str("={");
                self.text(a.value.as_deref().unwrap_or(""));
                self.out.push('}');
                continue;
            }
            self.out.push_str("=\"");
            match a.value.as_deref() {
                Some(v) if !v.is_empty() => self.text(v),
                _ => self.auto_stop(),
            }
            self.out.push('"');
        }
        let jsx = self.opts.flavor == Flavor::Jsx;
        if is_void(name) || n.self_closing {
            self.out
                .push_str(if jsx || !is_void(name) { " />" } else { ">" });
            return;
        }
        self.out.push('>');
        if let Some(t) = &n.text {
            self.text(t);
        }
        // `html` keeps head and body at its own level, as Emmet does.
        let inner = if name.eq_ignore_ascii_case("html") {
            depth
        } else {
            depth + 1
        };
        let formats: Vec<bool> = (0..n.children.len())
            .map(|j| should_format(&n.children, j))
            .collect();
        for (child, &f) in n.children.iter().zip(&formats) {
            self.node(child, inner, f);
        }
        if n.text.is_none() && n.children.is_empty() {
            self.auto_stop();
        }
        if formats.iter().any(|f| *f) {
            self.newline(depth);
        }
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    fn finish(self) -> Output {
        // Index stops by first appearance; markers with the same number share an index.
        let mut stops = Vec::new();
        let mut seen: Vec<(bool, usize, usize)> = Vec::new();
        for (from, to, key) in &self.stops {
            let (marker, k) = match key {
                StopKey::Marker(n) => (true, *n),
                StopKey::Auto(n) => (false, *n),
            };
            let index = match seen.iter().find(|(m, kk, _)| *m == marker && *kk == k) {
                Some((_, _, idx)) => *idx,
                None => {
                    let idx = seen.len() + 1;
                    seen.push((marker, k, idx));
                    idx
                }
            };
            if index > MAX_STOPS {
                continue;
            }
            stops.push(Stop {
                from: crate::text::utf16_of(&self.out, *from),
                to: crate::text::utf16_of(&self.out, *to),
                index,
            });
        }
        Output {
            text: self.out,
            stops,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(flavor: Flavor) -> MarkupOptions {
        MarkupOptions {
            flavor,
            indent: "  ".into(),
            ..MarkupOptions::default()
        }
    }

    fn html(abbr: &str) -> String {
        expand(abbr, &opts(Flavor::Html), None).unwrap().text
    }

    fn jsx(abbr: &str) -> String {
        expand(abbr, &opts(Flavor::Jsx), None).unwrap().text
    }

    #[test]
    fn single_elements_and_snippets() {
        assert_eq!(html("div"), "<div></div>");
        assert_eq!(html("a"), r#"<a href=""></a>"#);
        assert_eq!(html("img"), r#"<img src="" alt="">"#);
        assert_eq!(html("br"), "<br>");
        assert_eq!(html("input:c"), r#"<input type="checkbox" name="" id="">"#);
        assert_eq!(html("inp"), r#"<input type="text" name="" id="">"#);
        assert_eq!(
            html("link:css"),
            r#"<link rel="stylesheet" href="style.css">"#
        );
        assert_eq!(html("bq"), "<blockquote></blockquote>");
        assert_eq!(html("btn:s"), r#"<button type="submit"></button>"#);
        assert_eq!(html("c"), "<!--  -->");
        assert_eq!(html("custom-el"), "<custom-el></custom-el>");
    }

    #[test]
    fn ids_classes_and_attributes() {
        assert_eq!(
            html("div#page.wrap.dark[data-x=1 title='a b']"),
            r#"<div id="page" class="wrap dark" data-x="1" title="a b"></div>"#
        );
        assert_eq!(html("a.btn[href=/go]"), r#"<a href="/go" class="btn"></a>"#);
        assert_eq!(html("input[disabled.]"), r#"<input type="text" disabled>"#);
    }

    #[test]
    fn nesting_and_formatting() {
        assert_eq!(
            html("div>p+ul>li*2"),
            "<div>\n  <p></p>\n  <ul>\n    <li></li>\n    <li></li>\n  </ul>\n</div>"
        );
        assert_eq!(html("p>a*2"), r#"<p><a href=""></a><a href=""></a></p>"#);
        assert_eq!(
            html("p>span*3"),
            "<p>\n  <span></span>\n  <span></span>\n  <span></span>\n</p>"
        );
        assert_eq!(html("div+p"), "<div></div>\n<p></p>");
        assert_eq!(
            html("div>p^span"),
            "<div>\n  <p></p>\n</div>\n<span></span>"
        );
    }

    #[test]
    fn implicit_tags() {
        assert_eq!(html("ul>.item"), "<ul>\n  <li class=\"item\"></li>\n</ul>");
        assert_eq!(html(".box"), "<div class=\"box\"></div>");
        assert_eq!(html("em>.x"), "<em><span class=\"x\"></span></em>");
        assert_eq!(
            html("table>.row>.cell"),
            "<table>\n  <tr class=\"row\">\n    <td class=\"cell\"></td>\n  </tr>\n</table>"
        );
        assert_eq!(
            html("select>.o"),
            "<select name=\"\" id=\"\">\n  <option class=\"o\"></option>\n</select>"
        );
        assert_eq!(html("ul+"), "<ul>\n  <li></li>\n</ul>");
    }

    #[test]
    fn numbering() {
        assert_eq!(
            html("li.item$*3"),
            "<li class=\"item1\"></li>\n<li class=\"item2\"></li>\n<li class=\"item3\"></li>"
        );
        assert_eq!(html("h$*2"), "<h1></h1>\n<h2></h2>");
        assert_eq!(
            html("i.a$$*2"),
            "<i class=\"a01\"></i><i class=\"a02\"></i>"
        );
        assert_eq!(html("b{$@-}*3"), "<b>3</b>\n<b>2</b>\n<b>1</b>");
        assert_eq!(html("b{$@3}*2"), "<b>3</b><b>4</b>");
        assert_eq!(html("b{$@-5}*2"), "<b>6</b><b>5</b>");
        assert_eq!(
            html("ul>li*2>a{Item $}"),
            "<ul>\n  <li><a href=\"\">Item 1</a></li>\n  <li><a href=\"\">Item 2</a></li>\n</ul>"
        );
    }

    #[test]
    fn groups_and_text() {
        assert_eq!(
            html("(header>nav)+main+footer"),
            "<header>\n  <nav></nav>\n</header>\n<main></main>\n<footer></footer>"
        );
        assert_eq!(
            html("(dt+dd)*2"),
            "<dt></dt>\n<dd></dd>\n<dt></dt>\n<dd></dd>"
        );
        assert_eq!(html("p{Hello}"), "<p>Hello</p>");
        assert_eq!(html("{Click }+a{here}"), "Click <a href=\"\">here</a>");
    }

    #[test]
    fn lorem_text() {
        let out = html("p>lorem4");
        assert!(out.starts_with("<p>Lorem ipsum dolor sit"), "{out}");
        let out = html("lorem3.x");
        assert!(out.starts_with("<div class=\"x\">"), "{out}");
        let out = html("ul>lorem2.i*2");
        assert_eq!(out.matches("<li class=\"i\">").count(), 2, "{out}");
    }

    #[test]
    fn html5_document() {
        let out = html("!");
        assert!(
            out.starts_with(
                "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"UTF-8\">"
            ),
            "{out}"
        );
        assert!(out.contains("<title>Document</title>"));
        assert!(out.ends_with("<body></body>\n</html>"), "{out}");
    }

    #[test]
    fn jsx_attributes_and_self_closing() {
        assert_eq!(jsx("div.a.b"), r#"<div className="a b"></div>"#);
        assert_eq!(jsx("label[for=x]"), r#"<label htmlFor="x"></label>"#);
        assert_eq!(jsx("img"), r#"<img src="" alt="" />"#);
        assert_eq!(jsx("Foo/"), "<Foo />");
        assert_eq!(
            jsx("button[onClick={go}]"),
            "<button onClick={go}></button>"
        );
        let o = MarkupOptions {
            jsx_class_attr: "class".into(),
            ..opts(Flavor::Jsx)
        };
        assert_eq!(
            expand(".x", &o, None).unwrap().text,
            r#"<div class="x"></div>"#
        );
    }

    #[test]
    fn stops() {
        let out = expand("a+div", &opts(Flavor::Html), None).unwrap();
        assert_eq!(out.text, "<a href=\"\"></a>\n<div></div>");
        let at = |s: &Stop| (s.from, s.to, s.index);
        assert_eq!(
            out.stops.iter().map(at).collect::<Vec<_>>(),
            [(9, 9, 1), (11, 11, 2), (21, 21, 3)]
        );
        let out = expand("input", &opts(Flavor::Html), None).unwrap();
        assert_eq!(out.stops.iter().map(at).collect::<Vec<_>>(), [(13, 17, 1)]);
        let out = expand("p{${1:x} ${1:x}}", &opts(Flavor::Html), None).unwrap();
        assert_eq!(
            out.stops.iter().map(at).collect::<Vec<_>>(),
            [(3, 4, 1), (5, 6, 1)]
        );
    }

    #[test]
    fn wrapping() {
        let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let o = opts(Flavor::Html);
        assert_eq!(
            expand("div.box", &o, Some(&lines(&["Hello"])))
                .unwrap()
                .text,
            "<div class=\"box\">Hello</div>"
        );
        assert_eq!(
            expand("div>p", &o, Some(&lines(&["a", "b"]))).unwrap().text,
            "<div>\n  <p>\n    a\n    b\n  </p>\n</div>"
        );
        assert_eq!(
            expand("ul>li*", &o, Some(&lines(&["One", "Two"])))
                .unwrap()
                .text,
            "<ul>\n  <li>One</li>\n  <li>Two</li>\n</ul>"
        );
        assert_eq!(
            expand("ul>li.i$*>a", &o, Some(&lines(&["One", "Two"]))).unwrap().text,
            "<ul>\n  <li class=\"i1\"><a href=\"\">One</a></li>\n  <li class=\"i2\"><a href=\"\">Two</a></li>\n</ul>"
        );
        assert_eq!(
            expand("a", &o, Some(&lines(&["https://forge.dev"])))
                .unwrap()
                .text,
            "<a href=\"https://forge.dev\">https://forge.dev</a>"
        );
        assert_eq!(
            expand("a", &o, Some(&lines(&["me@example.com"])))
                .unwrap()
                .text,
            "<a href=\"mailto:me@example.com\">me@example.com</a>"
        );
        assert_eq!(
            expand("p[title=$#]{Say: $#}", &o, Some(&lines(&["hi"])))
                .unwrap()
                .text,
            "<p title=\"hi\">Say: hi</p>"
        );
    }

    #[test]
    fn limits() {
        assert!(expand("div*100000>p*100000", &opts(Flavor::Html), None).is_err());
        let out = expand("li*300", &opts(Flavor::Html), None).unwrap();
        assert!(out.stops.len() <= MAX_STOPS);
    }

    #[test]
    fn markup_likeness() {
        let t = |s: &str| looks_like_markup(&abbr::parse(s).unwrap());
        assert!(t("div") && t("ul>li") && t(".x") && t("Foo*2") && t("lorem"));
        assert!(!t("const") && !t("foo") && !t("Foo"));
    }
}

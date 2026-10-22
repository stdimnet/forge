//! HTML, XML, Blade and Twig: indentation by element (and directive/tag) nesting, the spaces
//! in attributes and empty tags, attribute wrapping, and the spaces inside Blade and Twig
//! delimiters. `<pre>`/`<textarea>` content is kept; `<script>`/`<style>` content moves as a
//! whole.

use super::Opts;

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const BLADE_OPEN: &[&str] = &[
    "if",
    "unless",
    "foreach",
    "forelse",
    "for",
    "while",
    "isset",
    "auth",
    "guest",
    "can",
    "cannot",
    "env",
    "production",
    "switch",
    "push",
    "prepend",
    "component",
    "slot",
    "once",
    "error",
    "verbatim",
    "fragment",
    "session",
    "pushOnce",
];
const BLADE_MID: &[&str] = &[
    "else", "elseif", "empty", "case", "default", "elsecan", "elseauth",
];
const TWIG_OPEN: &[&str] = &[
    "if",
    "for",
    "block",
    "macro",
    "embed",
    "apply",
    "autoescape",
    "with",
    "filter",
    "spaceless",
    "verbatim",
    "sandbox",
];
const TWIG_MID: &[&str] = &["else", "elseif"];
const BLADE_PAREN_DIRECTIVES: &[&str] = &[
    "if", "elseif", "unless", "foreach", "forelse", "for", "while", "isset", "empty", "switch",
    "case", "can", "cannot", "elsecan", "env", "auth", "guest",
];

#[derive(Default)]
struct LineScan {
    /// Closers (and middles like `@else`) before anything else on the line.
    leading: usize,
    opens: usize,
    closes: usize,
    /// The line ends inside a tag (attributes continue on the next line).
    open_tag: bool,
    /// A raw element (`script`, `style`, `pre`, `textarea`) opened and did not close.
    raw: Option<String>,
}

struct Markup<'a> {
    o: &'a Opts<'a>,
    lang: &'a str,
    no_indent: Vec<String>,
}

fn find_from(s: &str, at: usize, pat: &str) -> Option<usize> {
    s.get(at..)?.find(pat).map(|i| at + i)
}

impl Markup<'_> {
    fn html_like(&self) -> bool {
        self.lang != "xml"
    }

    fn counts(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        !(self.html_like() && (VOID.contains(&lower.as_str()) || self.no_indent.contains(&lower)))
    }

    /// The structure of one line (trimmed). `in_tag` says the line continues a tag.
    fn scan(&self, line: &str, in_tag: bool) -> LineScan {
        let mut s = LineScan::default();
        let mut i = 0;
        let mut seen_other = false;
        let b = line.as_bytes();
        if in_tag {
            match line.find('>') {
                Some(end) => {
                    i = end + 1;
                    seen_other = true;
                }
                None => {
                    s.open_tag = true;
                    return s;
                }
            }
        }
        while i < b.len() {
            let rest = &line[i..];
            if rest.starts_with("<!--") {
                i = find_from(line, i, "-->").map_or(b.len(), |e| e + 3);
                seen_other = true;
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                i = find_from(line, i, ">").map_or(b.len(), |e| e + 1);
                seen_other = true;
            } else if let Some(close) = rest.strip_prefix("</") {
                let name: String = close
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != '>')
                    .collect();
                if self.counts(&name) {
                    if seen_other {
                        s.closes += 1;
                    } else {
                        s.leading += 1;
                    }
                }
                i = find_from(line, i, ">").map_or(b.len(), |e| e + 1);
            } else if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_alphabetic()) {
                let name: String = rest[1..]
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
                    .collect();
                seen_other = true;
                match tag_end(line, i) {
                    Some(end) => {
                        let self_closing = line[..end].ends_with('/');
                        let lower = name.to_ascii_lowercase();
                        if self.html_like()
                            && ["script", "style", "pre", "textarea"].contains(&lower.as_str())
                        {
                            let close = format!("</{lower}");
                            match line[end..].to_ascii_lowercase().find(&close) {
                                Some(c) => i = end + c,
                                None => {
                                    s.raw = Some(lower);
                                    s.opens += 1;
                                    return s;
                                }
                            }
                            continue;
                        }
                        if !self_closing && self.counts(&name) {
                            s.opens += 1;
                        }
                        i = end + 1;
                    }
                    None => {
                        s.open_tag = true;
                        if self.counts(&name)
                            && !(self.html_like()
                                && VOID.contains(&name.to_ascii_lowercase().as_str()))
                        {
                            s.opens += 1;
                        }
                        return s;
                    }
                }
            } else if self.lang == "blade" && rest.starts_with("{{") {
                let close = if rest.starts_with("{{--") {
                    "--}}"
                } else {
                    "}}"
                };
                i = find_from(line, i + 2, close).map_or(b.len(), |e| e + close.len());
                seen_other = true;
            } else if self.lang == "blade" && rest.starts_with("{!!") {
                i = find_from(line, i + 3, "!!}").map_or(b.len(), |e| e + 3);
                seen_other = true;
            } else if self.lang == "blade"
                && rest.starts_with('@')
                && (i == 0 || !b[i - 1].is_ascii_alphanumeric())
            {
                let word: String = rest[1..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                let after = rest[1 + word.len()..].trim_start();
                let mut end = i + 1 + word.len();
                if after.starts_with('(') {
                    end = balanced(line, line.len() - after.len()).unwrap_or(b.len());
                }
                let args = &line[i + 1 + word.len()..end];
                if word.starts_with("end")
                    || word == "stop"
                    || word == "show"
                    || word == "append"
                    || word == "overwrite"
                {
                    if seen_other {
                        s.closes += 1;
                    } else {
                        s.leading += 1;
                    }
                } else if BLADE_MID.contains(&word.as_str())
                    && !(word == "empty" && args.contains('('))
                {
                    if !seen_other {
                        s.leading += 1;
                        s.opens += 1;
                    }
                } else if BLADE_OPEN.contains(&word.as_str())
                    || (word == "section" && !top_level_comma(args))
                    || (word == "php" && !args.contains('('))
                {
                    s.opens += 1;
                }
                if !word.is_empty() {
                    seen_other = true;
                }
                i = end.max(i + 1);
            } else if self.lang == "twig" && (rest.starts_with("{{") || rest.starts_with("{#")) {
                let close = if rest.starts_with("{#") { "#}" } else { "}}" };
                i = find_from(line, i + 2, close).map_or(b.len(), |e| e + 2);
                seen_other = true;
            } else if self.lang == "twig" && rest.starts_with("{%") {
                let end = find_from(line, i + 2, "%}").map_or(b.len(), |e| e + 2);
                let inner = line[i + 2..end.saturating_sub(2).max(i + 2)]
                    .trim_matches(|c| c == '-' || c == '~')
                    .trim();
                let word = inner.split_whitespace().next().unwrap_or("");
                if word.starts_with("end") {
                    if seen_other {
                        s.closes += 1;
                    } else {
                        s.leading += 1;
                    }
                } else if TWIG_MID.contains(&word) {
                    if !seen_other {
                        s.leading += 1;
                        s.opens += 1;
                    }
                } else if TWIG_OPEN.contains(&word) || (word == "set" && !inner.contains('=')) {
                    s.opens += 1;
                }
                seen_other = true;
                i = end;
            } else {
                seen_other = true;
                i += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        s
    }

    /// Normalize the tags of a line: spaces around `=` and in empty tags.
    fn fix_tags(&self, line: &str) -> String {
        let eq = self.o.bool("spaces.aroundAttributeEquals");
        let empty = self.o.bool("spaces.inEmptyTag");
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        while i < line.len() {
            let rest = &line[i..];
            if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_alphabetic()) {
                let end = tag_end(line, i).unwrap_or(line.len() - 1);
                out.push_str(&self.fix_tag(&line[i..=end.min(line.len() - 1)], eq, empty));
                i = end + 1;
            } else {
                let c = rest.chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
            }
        }
        out
    }

    fn fix_tag(&self, tag: &str, eq: bool, empty: bool) -> String {
        let mut out = String::with_capacity(tag.len() + 4);
        let mut quote: Option<char> = None;
        let chars: Vec<char> = tag.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = quote {
                out.push(c);
                if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    out.push(c);
                }
                '=' => {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push_str(if eq { " = " } else { "=" });
                    while chars.get(i + 1) == Some(&' ') {
                        i += 1;
                    }
                }
                '/' if chars.get(i + 1) == Some(&'>') => {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    if empty {
                        out.push(' ');
                    }
                    out.push('/');
                }
                _ => out.push(c),
            }
            i += 1;
        }
        out
    }

    /// "Chop down always": a tag with several attributes gets one attribute per line.
    fn chop(&self, line: &str, indent: &str, continuation: &str) -> Option<String> {
        let t = line.trim();
        if !t.starts_with('<') || t.starts_with("</") || t.starts_with("<!") || t.starts_with("<?")
        {
            return None;
        }
        let end = tag_end(t, 0)?;
        let inner = &t[1..end];
        let (inner, self_closing) = match inner.trim_end().strip_suffix('/') {
            Some(s) => (s, true),
            None => (inner, false),
        };
        let attrs = split_attributes(inner);
        if attrs.len() < 3 {
            return None;
        }
        let close = match (self_closing, self.o.bool("spaces.inEmptyTag")) {
            (true, true) => " />",
            (true, false) => "/>",
            _ => ">",
        };
        let mut out = format!("{indent}<{} {}", attrs[0], attrs[1]);
        for a in &attrs[2..] {
            out.push('\n');
            out.push_str(indent);
            out.push_str(continuation);
            out.push_str(a);
        }
        out.push_str(close);
        out.push_str(&t[end + 1..]);
        Some(out)
    }
}

/// End (index of `>`) of the tag starting at `start`, outside quotes.
fn tag_end(s: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s[start..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(start + i),
            _ => {}
        }
    }
    None
}

/// End (exclusive) of the parenthesized group starting at `open`.
fn balanced(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in s[open..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn top_level_comma(args: &str) -> bool {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for c in args.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth -= 1,
            (None, ',') if depth == 1 => return true,
            _ => {}
        }
    }
    false
}

fn split_attributes(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in inner.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() && !cur.ends_with('=') {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, '=') => {
                while cur.ends_with(' ') {
                    cur.pop();
                }
                cur.push('=');
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Spaces inside `open … close` delimiters (`{{ x }}`); Twig/Blade whitespace control
/// characters (`-`, `~`) stay next to the delimiter.
fn delimit(text: &str, open: &str, close: &str, space: bool, skip_prefix: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = find_from(text, at, open) {
        let body_start = i + open.len();
        if skip_prefix.is_some_and(|p| text[i..].starts_with(p)) {
            out.push_str(&text[at..body_start]);
            at = body_start;
            continue;
        }
        let Some(end) = find_from(text, body_start, close) else {
            break;
        };
        let inner = &text[body_start..end];
        out.push_str(&text[at..i]);
        let controls = usize::from(inner.starts_with(['-', '~']))
            + usize::from(inner.trim_end().ends_with(['-', '~']));
        if inner.contains('\n') || inner.trim().len() <= controls {
            out.push_str(&text[i..end + close.len()]);
        } else {
            let lead = if inner.starts_with(['-', '~']) {
                &inner[..1]
            } else {
                ""
            };
            let trimmed = inner.trim_end();
            let trail = if trimmed.ends_with(['-', '~']) {
                &trimmed[trimmed.len() - 1..]
            } else {
                ""
            };
            let body = inner[lead.len()..inner.len()].trim_end();
            let body = body[..body.len() - trail.len()].trim();
            let pad = if space { " " } else { "" };
            out.push_str(&format!("{open}{lead}{pad}{body}{pad}{trail}{close}"));
        }
        at = end + close.len();
    }
    out.push_str(&text[at..]);
    out
}

/// The Blade and Twig delimiter options (also run by Reformat Code).
pub fn delimiters(text: &str, o: &Opts<'_>) -> String {
    match o.lang {
        "blade" => {
            let space = o.bool("spaces.withinEchoBraces");
            let out = delimit(text, "{{", "}}", space, Some("{{--"));
            let out = delimit(&out, "{!!", "!!}", space, None);
            directive_parens(&out, o.bool("spaces.beforeDirectiveParentheses"))
        }
        "twig" => {
            let space = o.bool("spaces.withinDelimiters");
            let out = delimit(text, "{{", "}}", space, None);
            delimit(&out, "{%", "%}", space, None)
        }
        _ => text.to_string(),
    }
}

fn directive_parens(text: &str, space: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = find_from(text, at, "@") {
        out.push_str(&text[at..=i]);
        at = i + 1;
        if i > 0 && text.as_bytes()[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        let word: String = text[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !BLADE_PAREN_DIRECTIVES.contains(&word.as_str()) {
            continue;
        }
        let after = &text[at + word.len()..];
        let trimmed = after.trim_start_matches([' ', '\t']);
        if trimmed.starts_with('(') {
            out.push_str(&word);
            if space {
                out.push(' ');
            }
            at += word.len() + (after.len() - trimmed.len());
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Ranges whose blank lines must stay: `<pre>`, `<textarea>` and verbatim blocks.
pub fn protected(text: &str) -> Vec<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for (open, close) in [
        ("<pre", "</pre>"),
        ("<textarea", "</textarea>"),
        ("@verbatim", "@endverbatim"),
        ("{% verbatim", "endverbatim"),
    ] {
        let mut at = 0;
        while let Some(s) = find_from(&lower, at, open) {
            let e = find_from(&lower, s, close).map_or(text.len(), |e| e + close.len());
            out.push((s, e));
            at = e;
        }
    }
    out
}

/// Lay out a markup document.
pub fn format(src: &str, o: &Opts<'_>, _layout: bool) -> String {
    let m = Markup {
        o,
        lang: o.lang,
        no_indent: o
            .str("other.doNotIndentChildrenOf")
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
    };
    let ind = o.indent();
    let keep = o.int("blankLines.keepMax");
    let chop = o.is("wrapping.attributes", "force");
    let mut out = String::with_capacity(src.len());
    let mut depth = 0usize;
    let mut in_tag = false;
    let mut raw: Option<(String, usize, usize)> = None; // (element, depth, min width)
    let mut blank = 0usize;
    let lines: Vec<&str> = src.lines().collect();
    for (n, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some((name, d, min)) = &raw {
            let close = format!("</{name}");
            if trimmed.to_ascii_lowercase().starts_with(&close) {
                out.push_str(&ind.render(d * ind.size));
                out.push_str(trimmed);
                out.push('\n');
                depth = *d;
                raw = None;
            } else if name == "pre" || name == "textarea" || trimmed.is_empty() {
                out.push_str(
                    if trimmed.is_empty() && name != "pre" && name != "textarea" {
                        ""
                    } else {
                        line
                    },
                );
                out.push('\n');
            } else {
                let width = ind.width(&line[..line.len() - line.trim_start().len()]);
                out.push_str(&ind.render((d + 1) * ind.size + width.saturating_sub(*min)));
                out.push_str(trimmed);
                out.push('\n');
            }
            continue;
        }
        if trimmed.is_empty() {
            blank += 1;
            continue;
        }
        if n > 0 && !out.is_empty() {
            for _ in 0..blank.min(keep) {
                out.push('\n');
            }
        }
        blank = 0;
        let scan = m.scan(trimmed, in_tag);
        let level = if in_tag {
            depth
        } else {
            depth.saturating_sub(scan.leading)
        };
        let cols = level * ind.size + if in_tag { ind.continuation } else { 0 };
        let mut text = m.fix_tags(trimmed);
        if m.lang == "blade" || m.lang == "twig" {
            text = delimiters(&text, o);
        }
        let indent = ind.render(cols);
        match chop
            .then(|| m.chop(&text, &indent, &ind.render(ind.continuation)))
            .flatten()
        {
            Some(chopped) => out.push_str(&chopped),
            None => {
                out.push_str(&indent);
                out.push_str(&text);
            }
        }
        out.push('\n');
        depth = (depth + scan.opens)
            .saturating_sub(scan.closes + if in_tag { 0 } else { scan.leading });
        in_tag = scan.open_tag;
        if let Some(name) = scan.raw {
            // Indentation of the raw content, kept relative to its least indented line.
            let close = format!("</{name}");
            let min = lines[n + 1..]
                .iter()
                .take_while(|l| !l.trim().to_ascii_lowercase().starts_with(&close))
                .filter(|l| !l.trim().is_empty())
                .map(|l| ind.width(&l[..l.len() - l.trim_start().len()]))
                .min()
                .unwrap_or(0);
            raw = Some((name, depth.saturating_sub(1), min));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::model;
    use serde_json::json;

    fn fmt(lang: &str, src: &str, pairs: &[(&str, serde_json::Value)]) -> String {
        let mut v = model().defaults();
        for (k, val) in pairs {
            v.insert(format!("{lang}.{k}"), val.clone());
        }
        format(src, &Opts::new(lang, &v), true)
    }

    #[test]
    fn indents_html() {
        let src = "<html>\n<body>\n<ul class = \"a\">\n<li><br/></li>\n</ul>\n<script>\n  if (a) {\n    b();\n  }\n</script>\n<pre>\n  x\n</pre>\n</body>\n</html>\n";
        assert_eq!(
            fmt("html", src, &[]),
            "<html>\n<body>\n<ul class=\"a\">\n    <li><br/></li>\n</ul>\n<script>\n    if (a) {\n      b();\n    }\n</script>\n<pre>\n  x\n</pre>\n</body>\n</html>\n"
        );
        let out = fmt(
            "html",
            "<div>\n<img src=\"a\" alt=\"b\"/>\n</div>\n",
            &[
                ("spaces.inEmptyTag", json!(true)),
                ("wrapping.attributes", json!("force")),
            ],
        );
        assert_eq!(
            out,
            "<div>\n    <img src=\"a\"\n            alt=\"b\" />\n</div>\n"
        );
    }

    #[test]
    fn blade_and_twig_blocks() {
        let src = "@if($a)\n<p>{{$b}}</p>\n@else\n<p>x</p>\n@endif\n";
        assert_eq!(
            fmt(
                "blade",
                src,
                &[("spaces.beforeDirectiveParentheses", json!(false))]
            ),
            "@if($a)\n    <p>{{ $b }}</p>\n@else\n    <p>x</p>\n@endif\n"
        );
        assert_eq!(
            fmt("blade", src, &[("spaces.withinEchoBraces", json!(false))]),
            "@if ($a)\n    <p>{{$b}}</p>\n@else\n    <p>x</p>\n@endif\n"
        );
        let twig = "{% for a in b %}\n<li>{{a}}</li>\n{% else %}\n{{- 'x' -}}\n{% endfor %}\n";
        assert_eq!(
            fmt("twig", twig, &[]),
            "{% for a in b %}\n    <li>{{ a }}</li>\n{% else %}\n    {{- 'x' -}}\n{% endfor %}\n"
        );
    }

    #[test]
    fn delimiters_keep_comments_and_multiline() {
        let v = model().defaults();
        let o = Opts::new("blade", &v);
        assert_eq!(
            delimiters("{{-- c --}} {{$a}} {{\n$b}}", &o),
            "{{-- c --}} {{ $a }} {{\n$b}}"
        );
    }
}

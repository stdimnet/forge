//! A tolerant YAML parser that keeps byte ranges, for schema validation, completion and
//! hover. It covers what configuration files use: block mappings and sequences (also
//! `- key: value` compact items), flow collections, plain, quoted and block scalars,
//! comments, several documents, anchors, aliases and `<<` merge keys, tags. Plain scalars
//! resolve with the YAML 1.2 core schema (`yes` is a string, `~` is null).
//!
//! Unfinished input does not stop it: a line without a colon in a mapping becomes a key
//! without value, so completion knows where the user types.

use std::collections::HashMap;

use crate::tree::{Node, Property, Value};

struct Y<'a> {
    src: &'a str,
    b: &'a [u8],
    pos: usize,
    anchors: HashMap<String, Node>,
    depth: usize,
}

struct Key {
    text: String,
    start: usize,
    end: usize,
    colon: usize,
}

const MAX_DEPTH: usize = 256;

impl<'a> Y<'a> {
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(b'\n')
    }

    fn eof(&self) -> bool {
        self.pos >= self.b.len()
    }

    fn line_start(&self, pos: usize) -> usize {
        self.src[..pos.min(self.src.len())]
            .rfind('\n')
            .map_or(0, |i| i + 1)
    }

    fn line_end(&self, pos: usize) -> usize {
        self.src[pos.min(self.src.len())..]
            .find('\n')
            .map_or(self.src.len(), |i| pos + i)
    }

    fn col(&self, pos: usize) -> usize {
        pos - self.line_start(pos)
    }

    fn is_blank(c: u8) -> bool {
        c == b' ' || c == b'\t'
    }

    /// Whitespace, newline or end of input after `i`.
    fn sep_after(&self, i: usize) -> bool {
        matches!(self.at(i + 1), b' ' | b'\t' | b'\n' | b'\r')
    }

    fn skip_blanks(&mut self) {
        while self.pos < self.b.len() && Self::is_blank(self.b[self.pos]) {
            self.pos += 1;
        }
    }

    /// Only blanks and maybe a comment until the end of the line.
    fn at_eol(&self, mut i: usize) -> bool {
        while i < self.b.len() && Self::is_blank(self.b[i]) {
            i += 1;
        }
        matches!(self.at(i), b'\n' | b'\r' | b'#')
    }

    fn is_doc_marker(&self, i: usize) -> bool {
        self.col(i) == 0
            && (self.src[i..].starts_with("---") || self.src[i..].starts_with("..."))
            && matches!(self.at(i + 3), b' ' | b'\t' | b'\n' | b'\r')
    }

    /// Skip blanks, comments and line breaks to the next content.
    fn skip_to_content(&mut self) {
        loop {
            self.skip_blanks();
            match self.at(self.pos) {
                b'#' => self.pos = self.line_end(self.pos),
                b'\r' | b'\n' if !self.eof() => self.pos += 1,
                _ => {
                    if self.src[self.pos..].starts_with('\u{feff}') {
                        self.pos += 3;
                        continue;
                    }
                    return;
                }
            }
        }
    }

    /// From the end of a value: the column of the content on the next line, skipping
    /// whatever is left on this one. `None` at the end of input or of the document.
    fn next_line_content(&mut self) -> Option<usize> {
        let end = self.line_end(self.pos);
        self.pos = (end + 1).min(self.b.len());
        if end >= self.b.len() {
            self.pos = self.b.len();
            return None;
        }
        self.skip_to_content();
        if self.eof() || self.is_doc_marker(self.pos) {
            return None;
        }
        Some(self.col(self.pos))
    }

    // ---- Documents ------------------------------------------------------------------

    fn documents(&mut self) -> Vec<Node> {
        let mut docs = Vec::new();
        loop {
            self.skip_to_content();
            if self.eof() {
                break;
            }
            if self.col(self.pos) == 0 && self.at(self.pos) == b'%' {
                self.pos = self.line_end(self.pos);
                continue;
            }
            if self.is_doc_marker(self.pos) {
                let end_marker = self.src[self.pos..].starts_with("...");
                self.pos += 3;
                if end_marker {
                    continue;
                }
                self.skip_blanks();
                if self.at_eol(self.pos) {
                    let start = self.pos;
                    match self.block_node(-1, false) {
                        Some(n) => docs.push(n),
                        None => docs.push(Node::new(Value::Null, start, start)),
                    }
                    continue;
                }
            }
            let before = self.pos;
            match self.block_node(-1, false) {
                Some(n) => docs.push(n),
                None => {}
            }
            if self.pos == before {
                self.pos = self.line_end(self.pos);
            }
        }
        docs
    }

    // ---- Block context --------------------------------------------------------------

    /// The node on the following lines, indented deeper than `parent` (or a sequence at
    /// the same column when `seq_same` — `key:\n- item`).
    fn block_node(&mut self, parent: isize, seq_same: bool) -> Option<Node> {
        let save = self.pos;
        self.skip_to_content();
        if self.eof() || self.is_doc_marker(self.pos) {
            self.pos = save;
            return None;
        }
        let c = self.col(self.pos) as isize;
        let dash = self.at(self.pos) == b'-' && self.sep_after(self.pos);
        if c < parent || (c == parent && !(seq_same && dash)) {
            self.pos = save;
            return None;
        }
        Some(self.node_here(parent))
    }

    /// The node starting at `pos`; `parent` is the indentation of the enclosing block.
    fn node_here(&mut self, parent: isize) -> Node {
        if self.depth > MAX_DEPTH {
            let start = self.pos;
            self.pos = self.line_end(self.pos);
            return Node::new(Value::Error, start, self.pos);
        }
        self.depth += 1;
        let start = self.pos;
        let mut anchor = None;
        let mut tag: Option<String> = None;
        loop {
            match self.at(self.pos) {
                b'&' => {
                    self.pos += 1;
                    anchor = Some(self.word());
                }
                b'!' => tag = Some(self.word()),
                _ => break,
            }
            self.skip_blanks();
        }
        let mut node = if (anchor.is_some() || tag.is_some()) && self.at_eol(self.pos) {
            let at = self.pos;
            self.block_node(parent, false)
                .unwrap_or_else(|| Node::new(Value::Null, at, at))
        } else {
            self.content(parent)
        };
        if let Some(tag) = &tag {
            apply_tag(&mut node, tag, self.src);
        }
        if anchor.is_some() || tag.is_some() {
            node.start = node.start.min(start);
        }
        if let Some(name) = anchor {
            self.anchors.insert(name, node.clone());
        }
        self.depth -= 1;
        node
    }

    fn content(&mut self, parent: isize) -> Node {
        let start = self.pos;
        let c = self.at(start);
        match c {
            b'-' if self.sep_after(start) => self.block_seq(),
            b'[' | b'{' => {
                let n = self.flow_node();
                self.after_flow(n, parent)
            }
            b'|' | b'>' => self.block_scalar(parent),
            b'*' => self.alias(),
            _ => {
                if self.key_here(self.pos).is_some() {
                    return self.block_map();
                }
                if c == b'"' || c == b'\'' {
                    return self.quoted(parent);
                }
                self.plain_block(parent)
            }
        }
    }

    /// A flow collection used as a key (`[a, b]: x`) is not supported; just the node.
    fn after_flow(&mut self, n: Node, _parent: isize) -> Node {
        n
    }

    fn word(&mut self) -> String {
        let s = self.pos;
        while self.pos < self.b.len() && !matches!(self.b[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
        self.src[s..self.pos].to_string()
    }

    /// A mapping key at `i` (`key:`, `"key":`, `'key':`) on the current line.
    fn key_here(&self, i: usize) -> Option<Key> {
        let c = self.at(i);
        if c == b'"' || c == b'\'' {
            let (text, end) = self.quoted_line(i)?;
            let mut j = end;
            while Self::is_blank(self.at(j)) && j < self.b.len() {
                j += 1;
            }
            return (self.at(j) == b':' && self.sep_after(j)).then_some(Key {
                text,
                start: i,
                end,
                colon: j,
            });
        }
        if matches!(
            c,
            b'#' | b'['
                | b']'
                | b'{'
                | b'}'
                | b','
                | b'&'
                | b'*'
                | b'!'
                | b'|'
                | b'>'
                | b'%'
                | b'@'
                | b'`'
        ) || (matches!(c, b'-' | b'?' | b':') && self.sep_after(i))
        {
            return None;
        }
        let end = self.line_end(i);
        let mut j = i;
        while j < end {
            match self.b[j] {
                b':' if self.sep_after(j) => {
                    let text = self.src[i..j].trim_end();
                    if text.is_empty() {
                        return None;
                    }
                    return Some(Key {
                        text: text.to_string(),
                        start: i,
                        end: i + text.len(),
                        colon: j,
                    });
                }
                b'#' if j > i && Self::is_blank(self.b[j - 1]) => return None,
                _ => j += 1,
            }
        }
        None
    }

    /// A quoted scalar on one line: its value and the end of the closing quote.
    fn quoted_line(&self, i: usize) -> Option<(String, usize)> {
        let q = self.at(i);
        let end = self.line_end(i);
        let mut out = String::new();
        let mut j = i + 1;
        while j < end {
            let c = self.b[j];
            if c == q {
                if q == b'\'' && self.at(j + 1) == b'\'' {
                    out.push('\'');
                    j += 2;
                    continue;
                }
                return Some((out, j + 1));
            }
            if q == b'"' && c == b'\\' {
                j = unescape(self.src, j, &mut out);
                continue;
            }
            let ch = self.src[j..].chars().next()?;
            out.push(ch);
            j += ch.len_utf8();
        }
        None
    }

    fn block_map(&mut self) -> Node {
        let start = self.pos;
        let indent = self.col(start);
        let mut props: Vec<Property> = Vec::new();
        let mut end = start;
        loop {
            if let Some(k) = self.key_here(self.pos) {
                self.pos = k.colon + 1;
                self.skip_blanks();
                let value = if self.at_eol(self.pos) {
                    let after = k.colon + 1;
                    self.block_node(indent as isize, true)
                        .unwrap_or_else(|| Node::new(Value::Null, after, after))
                } else {
                    self.node_here(indent as isize)
                };
                end = end.max(value.end).max(k.colon + 1);
                props.push(Property {
                    key: k.text,
                    key_start: k.start,
                    key_end: k.end,
                    colon: Some(k.colon),
                    value: Some(value),
                });
            } else if self.at(self.pos) == b'-' && self.sep_after(self.pos) {
                // `- item` at the indentation of keys: not part of this mapping.
                break;
            } else {
                // A key being typed: `  na` without colon.
                let s = self.pos;
                let line_end = self.line_end(s);
                let text = self.src[s..line_end]
                    .split(" #")
                    .next()
                    .unwrap_or("")
                    .trim_end();
                let e = s + text.len();
                self.pos = e;
                if !text.is_empty() {
                    props.push(Property {
                        key: text.to_string(),
                        key_start: s,
                        key_end: e,
                        colon: None,
                        value: None,
                    });
                    end = end.max(e);
                }
            }
            let save = self.pos;
            match self.next_line_content() {
                Some(c) if c == indent => continue,
                Some(c) if c > indent => {
                    // Stray deeper content (invalid YAML): skip it.
                    while let Some(c) = self.next_line_content() {
                        if c <= indent {
                            break;
                        }
                    }
                    if self.eof() || self.is_doc_marker(self.pos) || self.col(self.pos) < indent {
                        self.pos = save;
                        break;
                    }
                }
                _ => {
                    self.pos = save;
                    break;
                }
            }
        }
        merge_keys(&mut props);
        Node::new(Value::Object(props), start, end)
    }

    fn block_seq(&mut self) -> Node {
        let start = self.pos;
        let indent = self.col(start);
        let mut items = Vec::new();
        let mut end = start + 1;
        loop {
            self.pos += 1;
            self.skip_blanks();
            let item = if self.at_eol(self.pos) {
                let at = self.pos;
                self.block_node(indent as isize, false)
                    .unwrap_or_else(|| Node::new(Value::Null, at, at))
            } else {
                self.node_here(indent as isize)
            };
            end = end.max(item.end);
            items.push(item);
            let save = self.pos;
            match self.next_line_content() {
                Some(c) if c == indent && self.at(self.pos) == b'-' && self.sep_after(self.pos) => {
                    continue
                }
                _ => {
                    self.pos = save;
                    break;
                }
            }
        }
        Node::new(Value::Array(items), start, end)
    }

    fn alias(&mut self) -> Node {
        let start = self.pos;
        self.pos += 1;
        let s = self.pos;
        while self.pos < self.b.len()
            && !matches!(
                self.b[self.pos],
                b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}'
            )
        {
            self.pos += 1;
        }
        let name = &self.src[s..self.pos];
        match self.anchors.get(name) {
            Some(n) => {
                let mut n = n.clone();
                n.relocate(start, self.pos);
                n
            }
            None => Node::new(Value::Error, start, self.pos),
        }
    }

    /// A plain scalar in block context, with continuation lines indented deeper than
    /// `parent`.
    fn plain_block(&mut self, parent: isize) -> Node {
        let start = self.pos;
        let mut text = self.plain_line(start);
        let mut end = start + text.len();
        self.pos = end;
        loop {
            let save = self.pos;
            let Some(c) = self.next_line_content() else {
                self.pos = save;
                break;
            };
            if (c as isize) <= parent
                || self.key_here(self.pos).is_some()
                || (self.at(self.pos) == b'-' && self.sep_after(self.pos))
            {
                self.pos = save;
                break;
            }
            let line = self.plain_line(self.pos);
            text.push(' ');
            text.push_str(&line);
            end = self.pos + line.len();
            self.pos = end;
        }
        self.pos = end;
        let value = resolve_plain(&text);
        Node::new(value, start, end)
    }

    /// The plain text from `i` to the end of the line, without a trailing comment.
    fn plain_line(&self, i: usize) -> String {
        let end = self.line_end(i);
        let line = &self.src[i..end];
        let mut cut = line.len();
        for (j, _) in line.match_indices('#') {
            if j > 0 && Self::is_blank(line.as_bytes()[j - 1]) {
                cut = j;
                break;
            }
        }
        line[..cut].trim_end().to_string()
    }

    /// A quoted scalar that may continue on deeper-indented lines.
    fn quoted(&mut self, parent: isize) -> Node {
        let start = self.pos;
        let q = self.at(start);
        let mut out = String::new();
        let mut j = start + 1;
        loop {
            if j >= self.b.len() {
                self.pos = self.line_end(start);
                return Node::new(Value::String(out), start, self.pos);
            }
            let c = self.b[j];
            if c == q {
                if q == b'\'' && self.at(j + 1) == b'\'' {
                    out.push('\'');
                    j += 2;
                    continue;
                }
                self.pos = j + 1;
                return Node::new(Value::String(out), start, j + 1);
            }
            if c == b'\n' {
                // Continue only on a deeper line that eventually closes the quote.
                let next = j + 1;
                let mut k = next;
                while k < self.b.len() && Self::is_blank(self.b[k]) {
                    k += 1;
                }
                if (k - next) as isize <= parent || k >= self.b.len() || self.key_here(k).is_some()
                {
                    self.pos = j;
                    return Node::new(Value::String(out), start, j);
                }
                if out.ends_with(' ') {
                    out.pop();
                }
                out.push(' ');
                j = k;
                continue;
            }
            if q == b'"' && c == b'\\' {
                j = unescape(self.src, j, &mut out);
                continue;
            }
            let ch = self.src[j..].chars().next().unwrap_or(' ');
            out.push(ch);
            j += ch.len_utf8();
        }
    }

    fn block_scalar(&mut self, parent: isize) -> Node {
        let start = self.pos;
        let folded = self.at(start) == b'>';
        let header_end = self.line_end(start);
        let header = &self.src[start + 1..header_end];
        let explicit = header
            .chars()
            .find(|c| c.is_ascii_digit())
            .and_then(|c| c.to_digit(10))
            .map(|d| d as usize);
        let strip = header.contains('-');
        let mut lines: Vec<&str> = Vec::new();
        let mut end = header_end;
        let mut i = header_end + 1;
        let mut content_indent: Option<usize> = explicit.map(|d| (parent.max(0) as usize) + d);
        while header_end < self.b.len() && i < self.b.len() {
            let le = self.line_end(i);
            let line = &self.src[i..le];
            let ind = line.len() - line.trim_start_matches(' ').len();
            if line.trim().is_empty() {
                lines.push("");
            } else {
                if (ind as isize) <= parent {
                    break;
                }
                let ci = *content_indent.get_or_insert(ind);
                if ind < ci {
                    break;
                }
                lines.push(&line[ci.min(line.len())..]);
                end = le;
            }
            if le >= self.b.len() {
                break;
            }
            i = le + 1;
        }
        while lines.last() == Some(&"") {
            lines.pop();
        }
        let mut text = if folded {
            let mut s = String::new();
            for (n, l) in lines.iter().enumerate() {
                if n > 0 {
                    s.push(if l.is_empty() || lines[n - 1].is_empty() {
                        '\n'
                    } else {
                        ' '
                    });
                }
                s.push_str(l);
            }
            s
        } else {
            lines.join("\n")
        };
        if !strip && !text.is_empty() {
            text.push('\n');
        }
        self.pos = end;
        Node::new(Value::String(text), start, end)
    }

    // ---- Flow context ---------------------------------------------------------------

    fn skip_flow_trivia(&mut self) {
        loop {
            while self.pos < self.b.len() && (self.b[self.pos] as char).is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.at(self.pos) == b'#' && !self.eof() {
                self.pos = self.line_end(self.pos);
                continue;
            }
            break;
        }
    }

    fn flow_node(&mut self) -> Node {
        self.skip_flow_trivia();
        let start = self.pos;
        if self.depth > MAX_DEPTH || self.eof() {
            return Node::new(Value::Null, start, start);
        }
        self.depth += 1;
        let mut tag = None;
        let mut anchor = None;
        loop {
            match self.at(self.pos) {
                b'&' => {
                    self.pos += 1;
                    anchor = Some(self.flow_word());
                }
                b'!' => tag = Some(self.flow_word()),
                _ => break,
            }
            self.skip_flow_trivia();
        }
        let mut node = match self.at(self.pos) {
            b'[' => self.flow_seq(),
            b'{' => self.flow_map(),
            b'*' => self.alias(),
            b'"' | b'\'' => {
                let s = self.pos;
                match self.quoted_line(s) {
                    Some((v, e)) => {
                        self.pos = e;
                        Node::new(Value::String(v), s, e)
                    }
                    None => self.quoted(-1),
                }
            }
            _ => {
                let s = self.pos;
                let text = self.flow_plain();
                Node::new(resolve_plain(&text), s, s + text.len())
            }
        };
        if let Some(t) = tag {
            apply_tag(&mut node, &t, self.src);
        }
        node.start = node.start.min(start);
        if let Some(a) = anchor {
            self.anchors.insert(a, node.clone());
        }
        self.depth -= 1;
        node
    }

    fn flow_word(&mut self) -> String {
        let s = self.pos;
        while self.pos < self.b.len()
            && !matches!(
                self.b[self.pos],
                b' ' | b'\t' | b'\n' | b'\r' | b',' | b'[' | b']' | b'{' | b'}'
            )
        {
            self.pos += 1;
        }
        self.src[s..self.pos].to_string()
    }

    /// A plain scalar in flow context (one line); the cursor ends after it.
    fn flow_plain(&mut self) -> String {
        let s = self.pos;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if matches!(c, b',' | b'[' | b']' | b'{' | b'}' | b'\n' | b'\r') {
                break;
            }
            if c == b':'
                && matches!(
                    self.at(self.pos + 1),
                    b' ' | b'\t' | b'\n' | b',' | b']' | b'}'
                )
            {
                break;
            }
            if c == b'#' && self.pos > s && Self::is_blank(self.b[self.pos - 1]) {
                break;
            }
            self.pos += 1;
        }
        let text = self.src[s..self.pos].trim_end().to_string();
        self.pos = s + text.len();
        text
    }

    fn flow_seq(&mut self) -> Node {
        let start = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_flow_trivia();
            match self.at(self.pos) {
                _ if self.eof() => return Node::new(Value::Array(items), start, self.b.len()),
                b']' => {
                    self.pos += 1;
                    return Node::new(Value::Array(items), start, self.pos);
                }
                b',' => self.pos += 1,
                b'}' => self.pos += 1,
                _ => {
                    let before = self.pos;
                    let item = self.flow_node();
                    self.skip_flow_trivia();
                    if self.at(self.pos) == b':' {
                        // `[a: 1]`: a single-pair mapping.
                        let colon = self.pos;
                        self.pos += 1;
                        self.skip_flow_trivia();
                        let value = if matches!(self.at(self.pos), b',' | b']') {
                            Node::new(Value::Null, colon + 1, colon + 1)
                        } else {
                            self.flow_node()
                        };
                        let end = value.end;
                        let key = scalar_text(&item);
                        items.push(Node::new(
                            Value::Object(vec![Property {
                                key,
                                key_start: item.start,
                                key_end: item.end,
                                colon: Some(colon),
                                value: Some(value),
                            }]),
                            item.start,
                            end,
                        ));
                    } else {
                        items.push(item);
                    }
                    if self.pos == before {
                        self.pos += 1;
                    }
                }
            }
        }
    }

    fn flow_map(&mut self) -> Node {
        let start = self.pos;
        self.pos += 1;
        let mut props = Vec::new();
        loop {
            self.skip_flow_trivia();
            match self.at(self.pos) {
                _ if self.eof() => {
                    merge_keys(&mut props);
                    return Node::new(Value::Object(props), start, self.b.len());
                }
                b'}' => {
                    self.pos += 1;
                    merge_keys(&mut props);
                    return Node::new(Value::Object(props), start, self.pos);
                }
                b',' | b']' => self.pos += 1,
                _ => {
                    let before = self.pos;
                    let key = self.flow_node();
                    self.skip_flow_trivia();
                    let mut prop = Property {
                        key: scalar_text(&key),
                        key_start: key.start,
                        key_end: key.end,
                        colon: None,
                        value: None,
                    };
                    if self.at(self.pos) == b':' {
                        prop.colon = Some(self.pos);
                        self.pos += 1;
                        self.skip_flow_trivia();
                        prop.value = Some(if matches!(self.at(self.pos), b',' | b'}') {
                            Node::new(Value::Null, self.pos, self.pos)
                        } else {
                            self.flow_node()
                        });
                    } else {
                        // `{a, b}`: keys with null values.
                        prop.value = Some(Node::new(Value::Null, key.end, key.end));
                    }
                    props.push(prop);
                    if self.pos == before {
                        self.pos += 1;
                    }
                }
            }
        }
    }
}

/// The text of a scalar used as a key.
fn scalar_text(n: &Node) -> String {
    match &n.value {
        Value::String(s) => s.clone(),
        Value::Number(x) => crate::tree::format_number(*x),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        _ => n.to_json(),
    }
}

/// Decode the escape at `j` (a backslash) of a double-quoted scalar; returns the next index.
fn unescape(src: &str, j: usize, out: &mut String) -> usize {
    let b = src.as_bytes();
    let Some(&e) = b.get(j + 1) else {
        return j + 1;
    };
    let hex = |n: usize| {
        src.get(j + 2..j + 2 + n)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
    };
    match e {
        b'n' => out.push('\n'),
        b't' | b'\t' => out.push('\t'),
        b'r' => out.push('\r'),
        b'0' => out.push('\0'),
        b'"' => out.push('"'),
        b'\\' => out.push('\\'),
        b'/' => out.push('/'),
        b' ' => out.push(' '),
        b'e' => out.push('\u{1b}'),
        b'N' => out.push('\u{85}'),
        b'_' => out.push('\u{a0}'),
        b'x' | b'u' | b'U' => {
            let n = match e {
                b'x' => 2,
                b'u' => 4,
                _ => 8,
            };
            if let Some(c) = hex(n) {
                out.push(c);
                return j + 2 + n;
            }
        }
        b'\n' => {}
        _ => {
            let ch = src[j + 1..].chars().next().unwrap_or('?');
            out.push(ch);
            return j + 1 + ch.len_utf8();
        }
    }
    j + 2
}

/// A plain scalar's value in the YAML 1.2 core schema.
pub fn resolve_plain(s: &str) -> Value {
    match s {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => {
            return Value::Number(f64::INFINITY)
        }
        "-.inf" | "-.Inf" | "-.INF" => return Value::Number(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => return Value::Number(f64::NAN),
        _ => {}
    }
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    if let Some(h) = s.strip_prefix("0x") {
        if let Ok(v) = u64::from_str_radix(h, 16) {
            return Value::Number(v as f64);
        }
    }
    if let Some(o) = s.strip_prefix("0o") {
        if let Ok(v) = u64::from_str_radix(o, 8) {
            return Value::Number(v as f64);
        }
    }
    let numeric = !digits.is_empty()
        && digits
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && digits.chars().any(|c| c.is_ascii_digit());
    if numeric {
        if let Ok(v) = s.parse::<f64>() {
            return Value::Number(v);
        }
    }
    Value::String(s.to_string())
}

fn apply_tag(node: &mut Node, tag: &str, src: &str) {
    match tag {
        "!!str" => {
            if !matches!(
                node.value,
                Value::Object(_) | Value::Array(_) | Value::String(_)
            ) {
                node.value = Value::String(src[node.start..node.end].trim().to_string());
            }
        }
        "!!int" | "!!float" | "!!bool" | "!!null" | "!!map" | "!!seq" | "!!binary"
        | "!!timestamp" | "!" => {}
        _ => node.custom_tag = true,
    }
}

/// Apply `<<: *anchor` merges: keys of the merged mappings that the mapping does not set
/// itself, reported at the `<<` key.
fn merge_keys(props: &mut Vec<Property>) {
    if !props.iter().any(|p| p.key == "<<" && p.colon.is_some()) {
        return;
    }
    let mut merged: Vec<Property> = Vec::new();
    let mut own: Vec<Property> = Vec::new();
    for p in props.drain(..) {
        if p.key != "<<" || p.colon.is_none() {
            own.push(p);
            continue;
        }
        let Some(v) = &p.value else { continue };
        let sources: Vec<&Node> = match &v.value {
            Value::Object(_) => vec![v],
            Value::Array(items) => items.iter().collect(),
            _ => Vec::new(),
        };
        for src in sources {
            for q in src.as_object().unwrap_or(&[]) {
                if merged.iter().any(|m| m.key == q.key) {
                    continue;
                }
                let mut q = q.clone();
                q.key_start = p.key_start;
                q.key_end = p.key_end;
                q.colon = p.colon;
                if let Some(v) = &mut q.value {
                    v.relocate(p.key_start, p.key_end);
                }
                merged.push(q);
            }
        }
    }
    merged.retain(|m| !own.iter().any(|o| o.key == m.key));
    own.extend(merged);
    *props = own;
}

/// Parse a YAML stream: one node per document.
pub fn parse(text: &str) -> Vec<Node> {
    let mut y = Y {
        src: text,
        b: text.as_bytes(),
        pos: 0,
        anchors: HashMap::new(),
        depth: 0,
    };
    y.documents()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(t: &str) -> Node {
        let mut docs = parse(t);
        assert_eq!(docs.len(), 1, "{docs:?}");
        docs.remove(0)
    }

    #[test]
    fn parses_block_mappings_and_sequences() {
        let t = "name: app\nversion: 1.5\nenabled: yes\nlist:\n  - a\n  - b: 1\n    c: [x, 2]\nempty:\nother: ~\n";
        let n = one(t);
        assert_eq!(n.get("name").unwrap().as_str(), Some("app"));
        assert_eq!(n.get("version").unwrap().as_f64(), Some(1.5));
        assert_eq!(n.get("enabled").unwrap().as_str(), Some("yes"));
        let list = n.get("list").unwrap().as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].get("b").unwrap().as_f64(), Some(1.0));
        let c = list[1].get("c").unwrap().as_array().unwrap();
        assert_eq!(c[0].as_str(), Some("x"));
        assert_eq!(c[1].type_name(), "integer");
        assert_eq!(n.get("empty").unwrap().value, Value::Null);
        assert_eq!(n.get("other").unwrap().value, Value::Null);
        let p = n.property("version").unwrap();
        assert_eq!(&t[p.key_start..p.key_end], "version");
        let v = p.value.as_ref().unwrap();
        assert_eq!(&t[v.start..v.end], "1.5");
    }

    #[test]
    fn sequences_at_key_indent_and_nested() {
        let n = one(
            "on:\n  push:\n    branches:\n    - main\n    - 'release/*'\njobs:\n  - - a\n    - b\n",
        );
        let branches = n
            .get("on")
            .unwrap()
            .get("push")
            .unwrap()
            .get("branches")
            .unwrap();
        let b = branches.as_array().unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].as_str(), Some("release/*"));
        let jobs = n.get("jobs").unwrap().as_array().unwrap();
        assert_eq!(jobs[0].as_array().unwrap().len(), 2);
    }

    #[test]
    fn scalars_quotes_comments_and_block_scalars() {
        let t = "a: \"x\\ty\" # c\nb: 'it''s'\nc: |\n  line 1\n  line 2\nd: >-\n  folded\n  text\ne: plain # comment\nf: \"multi\n  line\"\n";
        let n = one(t);
        assert_eq!(n.get("a").unwrap().as_str(), Some("x\ty"));
        assert_eq!(n.get("b").unwrap().as_str(), Some("it's"));
        assert_eq!(n.get("c").unwrap().as_str(), Some("line 1\nline 2\n"));
        assert_eq!(n.get("d").unwrap().as_str(), Some("folded text"));
        assert_eq!(n.get("e").unwrap().as_str(), Some("plain"));
        assert_eq!(n.get("f").unwrap().as_str(), Some("multi line"));
        let c = n.get("c").unwrap();
        assert_eq!(&t[c.start..c.end], "|\n  line 1\n  line 2");
    }

    #[test]
    fn anchors_aliases_and_merge_keys() {
        let t =
            ".base: &base\n  image: php\n  tags: [a]\njob:\n  <<: *base\n  tags: [b]\nref: *base\n";
        let n = one(t);
        let job = n.get("job").unwrap();
        assert_eq!(job.get("image").unwrap().as_str(), Some("php"));
        assert_eq!(
            job.get("tags").unwrap().as_array().unwrap()[0].as_str(),
            Some("b")
        );
        assert!(job.get("<<").is_none());
        let image = job.property("image").unwrap();
        assert_eq!(&t[image.key_start..image.key_end], "<<");
        assert_eq!(
            n.get("ref").unwrap().get("image").unwrap().as_str(),
            Some("php")
        );
    }

    #[test]
    fn flow_collections_and_tags() {
        let n = one("a: {x: 1, 'y': [true, null], z}\nb: !reference [.a, b]\nc: !!str 123\n");
        let a = n.get("a").unwrap();
        assert_eq!(a.get("x").unwrap().as_f64(), Some(1.0));
        assert_eq!(a.get("y").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(a.get("z").unwrap().value, Value::Null);
        assert!(n.get("b").unwrap().custom_tag);
        assert_eq!(n.get("c").unwrap().as_str(), Some("123"));
    }

    #[test]
    fn several_documents() {
        let docs = parse("---\na: 1\n---\nb: 2\n...\n");
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[1].get("b").unwrap().as_f64(), Some(2.0));
        assert!(parse("# only a comment\n").is_empty());
    }

    #[test]
    fn keeps_keys_being_typed() {
        let t = "services:\n  web:\n    image: nginx\n    po\n";
        let n = one(t);
        let web = n.get("services").unwrap().get("web").unwrap();
        let props = web.as_object().unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].key, "po");
        assert!(props[1].colon.is_none());
    }

    #[test]
    fn compact_mappings_in_sequences() {
        let t = "steps:\n  - uses: actions/checkout@v4\n    with:\n      fetch-depth: 0\n  - run: echo hi\n";
        let n = one(t);
        let steps = n.get("steps").unwrap().as_array().unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0]
                .get("with")
                .unwrap()
                .get("fetch-depth")
                .unwrap()
                .as_f64(),
            Some(0.0)
        );
        assert_eq!(steps[1].get("run").unwrap().as_str(), Some("echo hi"));
    }

    #[test]
    fn core_schema_scalars() {
        assert_eq!(resolve_plain("12"), Value::Number(12.0));
        assert_eq!(resolve_plain("-1.5e3"), Value::Number(-1500.0));
        assert_eq!(resolve_plain("0x1f"), Value::Number(31.0));
        assert_eq!(resolve_plain("1.2.3"), Value::String("1.2.3".into()));
        assert_eq!(resolve_plain("3.10"), Value::Number(3.1));
        assert_eq!(resolve_plain("on"), Value::String("on".into()));
        assert_eq!(resolve_plain("e5"), Value::String("e5".into()));
    }
}

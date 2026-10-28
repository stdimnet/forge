//! Hotwire: Stimulus controllers (`app/javascript/controllers/**/*_controller.js`) with
//! their targets, values, classes, outlets and methods, the `data-controller`,
//! `data-action` and `data-<id>-target` attributes of views (HTML and the `data: { … }`
//! hashes of tag helpers), and Turbo frame ids (`turbo_frame_tag`, `<turbo-frame>`,
//! `turbo_stream.replace`, `data-turbo-frame`).

use serde::{Deserialize, Serialize};

use crate::app::Named;
use crate::source::{tokens, LineIndex, Tok};
use crate::Location;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StimulusController {
    /// `hello`, `users--list-item`.
    pub identifier: String,
    pub path: String,
    pub location: Location,
    pub targets: Vec<Named>,
    pub values: Vec<Named>,
    pub classes: Vec<Named>,
    pub outlets: Vec<Named>,
    /// Methods (action handlers).
    pub methods: Vec<Named>,
}

const ROOTS: &[&str] = &["app/javascript/controllers/", "app/frontend/controllers/", "app/assets/javascripts/controllers/"];

/// The identifier of a controller file: `app/javascript/controllers/users/list_item_controller.js`
/// → `users--list-item`.
pub fn identifier(path: &str) -> Option<String> {
    let rest = ROOTS.iter().find_map(|r| path.strip_prefix(r))?;
    let stem = ["_controller.js", "-controller.js", "_controller.ts", "-controller.ts"]
        .iter()
        .find_map(|s| rest.strip_suffix(s))?;
    Some(stem.replace('/', "--").replace('_', "-"))
}

fn quoted_items(text: &str, from: usize, to: usize) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = from;
    while i < to {
        let c = b[i];
        if c == b'"' || c == b'\'' || c == b'`' {
            if let Some(len) = text[i + 1..to].find(c as char) {
                out.push((text[i + 1..i + 1 + len].to_string(), i + 1));
                i += len + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn matching(text: &str, open_at: usize) -> usize {
    let b = text.as_bytes();
    let (open, close) = (b[open_at], if b[open_at] == b'[' { b']' } else { b'}' });
    let mut depth = 0;
    for (i, &c) in b.iter().enumerate().skip(open_at) {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    b.len()
}

/// Keys at the top level of an object literal `{ url: String, count: { type: Number } }`.
fn object_keys(text: &str, from: usize, to: usize) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut depth = 0;
    let mut i = from + 1;
    let mut expect_key = true;
    while i < to {
        let c = b[i];
        match c {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth -= 1,
            b',' if depth == 0 => expect_key = true,
            _ if depth == 0 && expect_key && (c.is_ascii_alphabetic() || c == b'_' || c == b'"' || c == b'\'') => {
                let quoted = c == b'"' || c == b'\'';
                let s = if quoted { i + 1 } else { i };
                let mut e = s;
                while e < to && (b[e].is_ascii_alphanumeric() || b[e] == b'_' || b[e] == b'-') {
                    e += 1;
                }
                out.push((text[s..e].to_string(), s));
                expect_key = false;
                i = e;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

const NOT_METHODS: &[&str] = &["if", "for", "while", "switch", "catch", "function", "return", "constructor", "super"];

/// Read a Stimulus controller.
pub fn controller(path: &str, text: &str) -> Option<StimulusController> {
    let identifier = identifier(path)?;
    let idx = LineIndex::new(text);
    let loc = |byte: usize| {
        let (line, character) = idx.position(byte);
        Location {
            path: path.to_string(),
            line,
            character,
        }
    };
    let class_at = text.find("class ").unwrap_or(0);
    let mut c = StimulusController {
        identifier,
        path: path.to_string(),
        location: loc(class_at),
        ..StimulusController::default()
    };
    for (kind, list) in [
        ("targets", &mut c.targets),
        ("values", &mut c.values),
        ("classes", &mut c.classes),
        ("outlets", &mut c.outlets),
    ] {
        let needle = format!("static {kind}");
        let Some(at) = text.find(&needle) else { continue };
        let rest = &text[at + needle.len()..];
        let Some(open) = rest.find(['[', '{']) else { continue };
        let open = at + needle.len() + open;
        let close = matching(text, open);
        let items = if text.as_bytes()[open] == b'[' {
            quoted_items(text, open, close)
        } else {
            object_keys(text, open, close)
        };
        *list = items
            .into_iter()
            .map(|(name, at)| Named { name, location: loc(at) })
            .collect();
    }
    // Methods: definitions one level inside the class body.
    let b = text.as_bytes();
    let body = text[class_at..].find('{').map(|i| class_at + i);
    if let Some(body) = body {
        let mut depth = 0;
        let mut line_start = true;
        let mut i = body;
        while i < b.len() {
            let ch = b[i];
            match ch {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                b'\n' => line_start = true,
                b'"' | b'\'' | b'`' => {
                    if let Some(len) = text[i + 1..].find(ch as char) {
                        i += len + 1;
                    }
                }
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                _ if depth == 1 && line_start && !ch.is_ascii_whitespace() => {
                    line_start = false;
                    let rest = &text[i..text[i..].find('\n').map_or(b.len(), |n| i + n)];
                    let r = rest.strip_prefix("async ").unwrap_or(rest).trim_start();
                    let name: String = r
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '#')
                        .collect();
                    let after = r[name.len()..].trim_start();
                    let is_method = after.starts_with('(')
                        || (after.starts_with('=') && (after.contains("=>") || after.contains("function")));
                    if !name.is_empty() && is_method && !NOT_METHODS.contains(&name.as_str()) && !name.starts_with('#') {
                        let at = i + (rest.len() - r.len());
                        c.methods.push(Named {
                            name,
                            location: loc(at),
                        });
                    }
                    continue;
                }
                _ if !ch.is_ascii_whitespace() => line_start = false,
                _ => {}
            }
            i += 1;
        }
    }
    Some(c)
}

/// What a Stimulus attribute names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StimulusRefKind {
    Controller,
    /// `click->hello#greet`: the method part (`identifier` is the controller).
    Method,
    /// `data-hello-target="name"`.
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StimulusRef {
    pub kind: StimulusRefKind,
    pub identifier: String,
    /// The name (method or target; the identifier for controllers).
    pub name: String,
    /// Byte range of the name in the file.
    pub start: usize,
    pub end: usize,
}

/// One attribute value: the attribute (`data-controller`, `controller`, `hello_target`),
/// the value and its byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrValue {
    pub attr: String,
    pub value: String,
    pub start: usize,
}

/// `data-*` attribute values of a line (HTML `data-x="…"` and Ruby `data: { x: "…" }`).
pub fn data_attributes(line: &str, base: usize) -> Vec<AttrValue> {
    let mut out = Vec::new();
    // HTML attributes.
    let mut search = 0;
    while let Some(i) = line[search..].find("data-") {
        let at = search + i;
        search = at + 5;
        if at > 0 && line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let name_end = at + line[at..].find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_')).unwrap_or(line.len() - at);
        let attr = &line[at + 5..name_end];
        let rest = &line[name_end..];
        let Some(eq) = rest.strip_prefix('=') else { continue };
        let q = eq.chars().next().unwrap_or(' ');
        if q != '"' && q != '\'' {
            continue;
        }
        let vstart = name_end + 2;
        let Some(len) = line[vstart..].find(q) else { continue };
        out.push(AttrValue {
            attr: attr.to_string(),
            value: line[vstart..vstart + len].to_string(),
            start: base + vstart,
        });
    }
    // `data: { controller: "x", hello_target: "y" }` hashes of tag helpers.
    let toks = tokens(line);
    for (i, t) in toks.iter().enumerate() {
        if !matches!(&t.tok, Tok::Key(k) if k == "data") || !toks.get(i + 1).is_some_and(|x| x.is_punct('{')) {
            continue;
        }
        let mut depth = 0;
        for (j, x) in toks.iter().enumerate().skip(i + 1) {
            match &x.tok {
                Tok::Punct('{') => depth += 1,
                Tok::Punct('}') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Tok::Key(k) if depth == 1 => {
                    if let Some(Token2 { value, start }) = toks.get(j + 1).and_then(|v| match &v.tok {
                        Tok::Str { value, interp: false } => Some(Token2 { value: value.clone(), start: v.start + 1 }),
                        Tok::Sym(value) => Some(Token2 { value: value.clone(), start: v.start + 1 }),
                        _ => None,
                    }) {
                        out.push(AttrValue {
                            attr: k.replace('_', "-"),
                            value,
                            start: base + start,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

struct Token2 {
    value: String,
    start: usize,
}

/// Words of an attribute value with their byte offsets.
fn words(v: &AttrValue) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    for w in v.value.split(' ') {
        if !w.is_empty() {
            out.push((w.to_string(), v.start + i));
        }
        i += w.len() + 1;
    }
    out
}

/// Stimulus references of a view.
pub fn stimulus_refs(text: &str) -> Vec<StimulusRef> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let base = offset;
        offset += line.len() + 1;
        if !line.contains("data") {
            continue;
        }
        for a in data_attributes(line, base) {
            if a.attr == "controller" {
                for (w, s) in words(&a) {
                    out.push(StimulusRef {
                        kind: StimulusRefKind::Controller,
                        identifier: w.clone(),
                        name: w.clone(),
                        start: s,
                        end: s + w.len(),
                    });
                }
            } else if a.attr == "action" {
                for (w, s) in words(&a) {
                    let (desc, ds) = match w.find("->") {
                        Some(i) => (&w[i + 2..], s + i + 2),
                        None => (w.as_str(), s),
                    };
                    let Some((id, method)) = desc.split_once('#') else { continue };
                    let method = method.split(':').next().unwrap_or(method);
                    out.push(StimulusRef {
                        kind: StimulusRefKind::Controller,
                        identifier: id.to_string(),
                        name: id.to_string(),
                        start: ds,
                        end: ds + id.len(),
                    });
                    let ms = ds + id.len() + 1;
                    out.push(StimulusRef {
                        kind: StimulusRefKind::Method,
                        identifier: id.to_string(),
                        name: method.to_string(),
                        start: ms,
                        end: ms + method.len(),
                    });
                }
            } else if let Some(id) = a.attr.strip_suffix("-target") {
                for (w, s) in words(&a) {
                    out.push(StimulusRef {
                        kind: StimulusRefKind::Target,
                        identifier: id.to_string(),
                        name: w.clone(),
                        start: s,
                        end: s + w.len(),
                    });
                }
            }
        }
    }
    out
}

/// A Turbo frame id in a view or a controller's `turbo_stream` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRef {
    pub id: String,
    pub start: usize,
    pub end: usize,
    /// `turbo_frame_tag "x"` / `<turbo-frame id="x">` (else a target).
    pub definition: bool,
}

const STREAM_ACTIONS: &[&str] = &["append", "prepend", "replace", "update", "remove", "before", "after", "refresh"];

/// Turbo frame ids of a file.
pub fn frame_refs(text: &str) -> Vec<FrameRef> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let base = offset;
        offset += line.len() + 1;
        if !line.contains("turbo") {
            continue;
        }
        // <turbo-frame id="x" target="y">
        if let Some(i) = line.find("<turbo-frame") {
            let tag_end = line[i..].find('>').map_or(line.len(), |e| i + e);
            for (attr, definition) in [(" id=", true), (" target=", false)] {
                if let Some(a) = line[i..tag_end].find(attr) {
                    let vs = i + a + attr.len() + 1;
                    if let Some(len) = line.get(vs..).and_then(|r| r.find(['"', '\''])) {
                        out.push(FrameRef {
                            id: line[vs..vs + len].to_string(),
                            start: base + vs,
                            end: base + vs + len,
                            definition,
                        });
                    }
                }
            }
        }
        for a in data_attributes(line, base) {
            if a.attr == "turbo-frame" {
                out.push(FrameRef {
                    id: a.value.clone(),
                    start: a.start,
                    end: a.start + a.value.len(),
                    definition: false,
                });
            }
        }
        let toks = tokens(line);
        for (i, t) in toks.iter().enumerate() {
            let Some(name) = t.ident() else { continue };
            let definition = name == "turbo_frame_tag";
            let stream = STREAM_ACTIONS.contains(&name)
                && i >= 2
                && toks[i - 1].is_punct('.')
                && toks[i - 2].is_ident("turbo_stream");
            if !definition && !stream {
                continue;
            }
            let mut j = i + 1;
            if toks.get(j).is_some_and(|x| x.is_punct('(')) {
                j += 1;
            }
            if let Some(arg) = toks.get(j) {
                let (s, e, id) = match &arg.tok {
                    Tok::Str { value, interp: false } => (arg.start + 1, arg.end - 1, value.clone()),
                    Tok::Sym(value) => (arg.start + 1, arg.end, value.clone()),
                    _ => continue,
                };
                out.push(FrameRef {
                    id,
                    start: base + s,
                    end: base + e,
                    definition,
                });
            }
            if definition {
                // turbo_frame_tag "x", target: "_top"
                for (k, x) in toks.iter().enumerate().skip(j) {
                    if matches!(&x.tok, Tok::Key(key) if key == "target") {
                        if let Some(v) = toks.get(k + 1) {
                            if let Tok::Str { value, interp: false } = &v.tok {
                                out.push(FrameRef {
                                    id: value.clone(),
                                    start: base + v.start + 1,
                                    end: base + v.end - 1,
                                    definition: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out.retain(|f| !f.id.starts_with('_') && !f.id.is_empty());
    out
}

/// Turbo helpers for completion in ERB: (label, detail).
pub const TURBO_HELPERS: &[(&str, &str)] = &[
    ("turbo_frame_tag", "Turbo frame"),
    ("turbo_stream_from", "Turbo Streams over Action Cable"),
    ("turbo_include_tags", "Turbo script tags"),
    ("turbo_refreshes_with", "Page refresh method and scroll"),
    ("turbo_exempts_page_from_cache", "Turbo cache control"),
    ("turbo_page_requires_reload", "Turbo cache control"),
];

pub const TURBO_STREAM_ACTIONS: &[(&str, &str)] = &[
    ("append", "Append to the target"),
    ("prepend", "Prepend to the target"),
    ("replace", "Replace the target"),
    ("update", "Update the target's content"),
    ("remove", "Remove the target"),
    ("before", "Insert before the target"),
    ("after", "Insert after the target"),
    ("refresh", "Refresh the page"),
    ("action", "A custom stream action"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controllers_with_targets_values_and_methods() {
        let js = r#"import { Controller } from "@hotwired/stimulus"

// Connects to data-controller="clipboard"
export default class extends Controller {
  static targets = ["source", "button"]
  static values = { successDuration: { type: Number, default: 2000 }, label: String }
  static classes = [ "copied" ]

  connect() {
    if (this.hasButtonTarget) { this.originalText = this.buttonTarget.textContent }
  }

  async copy(event) {
    event.preventDefault()
  }

  reset = () => {
    this.buttonTarget.textContent = this.labelValue
  }

  #hidden() {}
}
"#;
        let c = controller("app/javascript/controllers/clipboard_controller.js", js).unwrap();
        assert_eq!(c.identifier, "clipboard");
        let names = |l: &[Named]| l.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&c.targets), ["source", "button"]);
        assert_eq!(names(&c.values), ["successDuration", "label"]);
        assert_eq!(names(&c.classes), ["copied"]);
        assert_eq!(names(&c.methods), ["connect", "copy", "reset"]);
        assert_eq!(c.targets[0].location.line, 4);
        assert_eq!(c.methods[1].location.line, 12);
        assert_eq!(identifier("app/javascript/controllers/users/list_item_controller.js").as_deref(), Some("users--list-item"));
        assert!(identifier("app/javascript/application.js").is_none());
    }

    #[test]
    fn attribute_references() {
        let html = "<div data-controller=\"clipboard toggle\" data-clipboard-success-duration-value=\"1000\">\n  <button data-action=\"click->clipboard#copy toggle#flip\" data-clipboard-target=\"button\">Copy</button>\n  <%= tag.div data: { controller: \"hello\", hello_target: \"name\", action: \"hello#greet\" } %>\n</div>\n";
        let refs = stimulus_refs(html);
        let got: Vec<String> = refs
            .iter()
            .map(|r| format!("{:?} {} {} {}", r.kind, r.identifier, r.name, &html[r.start..r.end]))
            .collect();
        assert_eq!(
            got,
            [
                "Controller clipboard clipboard clipboard",
                "Controller toggle toggle toggle",
                "Controller clipboard clipboard clipboard",
                "Method clipboard copy copy",
                "Controller toggle toggle toggle",
                "Method toggle flip flip",
                "Target clipboard button button",
                "Controller hello hello hello",
                "Target hello name name",
                "Controller hello hello hello",
                "Method hello greet greet",
            ]
        );
    }

    #[test]
    fn turbo_frames() {
        let erb = "<%= turbo_frame_tag \"new_article\", target: \"_top\" do %>\n<turbo-frame id=\"comments\">\n<%= link_to \"New\", new_article_path, data: { turbo_frame: \"new_article\" } %>\n";
        let ctl = "render turbo_stream: turbo_stream.replace(\"comments\", partial: \"x\")\n";
        let f = frame_refs(erb);
        let got: Vec<(String, bool)> = f.iter().map(|r| (r.id.clone(), r.definition)).collect();
        assert_eq!(
            got,
            [("new_article".into(), true), ("comments".into(), true), ("new_article".into(), false)]
        );
        assert_eq!(&erb[f[2].start..f[2].end], "new_article");
        let f = frame_refs(ctl);
        assert_eq!((f[0].id.as_str(), f[0].definition), ("comments", false));
    }
}

//! WebForge Angular plugin (`forge.angular`): `@Component`, `@Directive` and `@Pipe`
//! classes with selectors, inputs and outputs (decorators and signal functions), their
//! `templateUrl`/`template` and `styleUrls`/`styleUrl`. Navigation like WebStorm's:
//! element and attribute selectors, `[input]`/`(output)` bindings and `| pipe`s in
//! templates go to their declarations; `templateUrl` and `styleUrls` strings open the
//! files; line markers link a component class and its template and styles.

use std::collections::HashMap;

use webforge_lang_web::js::{fields, ident, matching, skip_trivia, string_at};
use webforge_lang_web::markup::{self, scan_tags, Tag, TagHit};
use webforge_lang_web::text::{ident_at, is_ident_char, join_path, parent_dir, utf16_to_byte};
use webforge_lang_web::{
    Component, ComponentKind, Framework, FrameworkIndex, GutterKind, GutterMark, Location, Member,
    Sources,
};

/// A component, directive or pipe with what navigation needs beyond [`Component`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub component: Component,
    /// Byte range of an inline `template: \`…\`` in the class's file.
    pub inline_template: Option<(usize, usize)>,
    /// `templateUrl` / `styleUrls` string contents (byte ranges) and the files they name.
    pub file_refs: Vec<((usize, usize), String)>,
    /// Byte range of the selector string.
    pub selector_range: Option<(usize, usize)>,
}

/// The Angular declarations of one TypeScript file.
pub fn scan_file(path: &str, content: &str) -> Vec<Declaration> {
    if !path.ends_with(".ts") || path.ends_with(".d.ts") || path.ends_with(".spec.ts") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (decorator, kind) in [
        ("@Component", ComponentKind::Component),
        ("@Directive", ComponentKind::Directive),
        ("@Pipe", ComponentKind::Pipe),
    ] {
        let mut from = 0;
        while let Some(found) = content[from..].find(decorator) {
            let at = from + found;
            from = at + decorator.len();
            let open_paren = skip_trivia(content, from);
            if !content[open_paren..].starts_with('(') {
                continue;
            }
            let Some(close_paren) = matching(content, open_paren) else {
                continue;
            };
            let open = skip_trivia(content, open_paren + 1);
            let Some(close) = content[open..]
                .starts_with('{')
                .then(|| matching(content, open))
                .flatten()
            else {
                continue;
            };
            if let Some(d) = declaration(path, content, kind, open, close, close_paren) {
                out.push(d);
            }
        }
    }
    out.sort_by_key(|d| d.component.location.line);
    out
}

fn declaration(
    path: &str,
    content: &str,
    kind: ComponentKind,
    open: usize,
    close: usize,
    after: usize,
) -> Option<Declaration> {
    // `export class UserCardComponent implements OnInit {`
    let class_kw = after + find_word(&content[after..], "class")?;
    let name_at = skip_trivia(content, class_kw + 5);
    let (ns, ne) = ident(content, name_at)?;
    let body_open = ne + content[ne..].find('{')?;
    let body_close = matching(content, body_open).unwrap_or(content.len());

    let options = fields(content, open + 1, close);
    let string_value = |key: &str| {
        options
            .iter()
            .find(|f| f.key == key)
            .and_then(|f| f.value)
            .and_then(|(vs, _)| string_at(content, vs))
    };
    let dir = parent_dir(path);
    let selector_range = string_value(if kind == ComponentKind::Pipe {
        "name"
    } else {
        "selector"
    });
    let mut file_refs = Vec::new();
    let mut template = None;
    let mut inline_template = None;
    if let Some((a, b)) = string_value("templateUrl") {
        let file = join_path(dir, &content[a..b]);
        template = Some(Location::file(&file));
        file_refs.push(((a, b), file));
    } else if let Some((a, b)) = string_value("template") {
        template = Some(Location::at(path, content, a));
        inline_template = Some((a, b));
    }
    let mut styles = Vec::new();
    for f in options
        .iter()
        .filter(|f| f.key == "styleUrls" || f.key == "styleUrl")
    {
        let Some((vs, ve)) = f.value else { continue };
        let mut i = vs;
        while i < ve {
            match string_at(content, i) {
                Some((a, b)) => {
                    let file = join_path(dir, &content[a..b]);
                    styles.push(Location::file(&file));
                    file_refs.push(((a, b), file));
                    i = b + 1;
                }
                None => i += 1,
            }
        }
    }
    let (mut props, mut events) = class_members(path, content, body_open, body_close);
    // `inputs: ['user', 'size: compact']` in the decorator.
    for (key, list) in [("inputs", &mut props), ("outputs", &mut events)] {
        let Some((vs, ve)) = options.iter().find(|f| f.key == key).and_then(|f| f.value) else {
            continue;
        };
        let mut i = vs;
        while i < ve {
            match string_at(content, i) {
                Some((a, b)) => {
                    let text = &content[a..b];
                    let (name, alias) = match text.split_once(':') {
                        Some((n, al)) => (n.trim(), Some(al.trim().to_string())),
                        None => (text.trim(), None),
                    };
                    list.push(Member {
                        name: name.to_string(),
                        alias,
                        location: Location::at(path, content, a),
                        type_text: None,
                        required: false,
                    });
                    i = b + 1;
                }
                None => i += 1,
            }
        }
    }
    Some(Declaration {
        component: Component {
            name: content[ns..ne].to_string(),
            framework: Framework::Angular,
            kind,
            selector: selector_range.map(|(a, b)| content[a..b].trim().to_string()),
            location: Location::at(path, content, ns),
            props,
            events,
            template,
            styles,
            default_export: false,
            usages: 0,
        },
        inline_template,
        file_refs,
        selector_range,
    })
}

/// Offset of `word` as a whole word in `s`.
fn find_word(s: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find(word) {
        let at = from + i;
        let before_ok = !s[..at].chars().last().is_some_and(is_ident_char);
        let after_ok = !s[at + word.len()..]
            .chars()
            .next()
            .is_some_and(is_ident_char);
        if before_ok && after_ok {
            return Some(at);
        }
        from = at + word.len();
    }
    None
}

/// Inputs and outputs in a class body: `@Input() user!: User;`, `@Input('alias') x`,
/// `@Input({ required: true }) y`, `@Output() saved = new EventEmitter<User>()`,
/// `size = input<'sm' | 'md'>('md')`, `user = input.required<User>()`, `value = model(0)`,
/// `saved = output<User>()`.
fn class_members(
    path: &str,
    content: &str,
    open: usize,
    close: usize,
) -> (Vec<Member>, Vec<Member>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (decorator, is_input) in [("@Input", true), ("@Output", false)] {
        let mut from = open;
        while let Some(found) = content[from..close].find(decorator) {
            let at = from + found;
            from = at + decorator.len();
            let mut j = skip_trivia(content, from);
            let mut alias = None;
            let mut required = false;
            if content[j..].starts_with('(') {
                let Some(c) = matching(content, j) else {
                    continue;
                };
                let arg = skip_trivia(content, j + 1);
                if let Some((a, b)) = string_at(content, arg) {
                    alias = Some(content[a..b].to_string());
                } else if content[arg..].starts_with('{') {
                    let opts = &content[arg..c];
                    required = opts.replace(' ', "").contains("required:true");
                    if let Some(k) = opts.find("alias") {
                        let v = skip_trivia(content, arg + k + 5);
                        let v = skip_trivia(content, v + 1);
                        alias = string_at(content, v).map(|(a, b)| content[a..b].to_string());
                    }
                }
                j = skip_trivia(content, c + 1);
            }
            // Modifiers and accessors before the name.
            let mut name = None;
            while let Some((a, b)) = ident(content, j) {
                let w = &content[a..b];
                if matches!(
                    w,
                    "public"
                        | "private"
                        | "protected"
                        | "readonly"
                        | "set"
                        | "get"
                        | "override"
                        | "declare"
                ) {
                    j = skip_trivia(content, b);
                    continue;
                }
                name = Some((a, b));
                break;
            }
            let Some((a, b)) = name else { continue };
            let after = skip_trivia(content, b);
            let after = if content[after..].starts_with(['!', '?']) {
                after + 1
            } else {
                after
            };
            let type_text = content[after..]
                .strip_prefix(':')
                .map(|t| {
                    t[..t.find([';', '=', '\n']).unwrap_or(t.len())]
                        .trim()
                        .to_string()
                })
                .filter(|t| !t.is_empty());
            let m = Member {
                name: content[a..b].to_string(),
                alias,
                location: Location::at(path, content, a),
                type_text,
                required,
            };
            if is_input {
                inputs.push(m)
            } else {
                outputs.push(m)
            }
        }
    }
    // Signal-based inputs, models and outputs.
    for (func, is_input) in [
        ("input", true),
        ("model", true),
        ("output", false),
        ("outputFromObservable", false),
    ] {
        let mut from = open;
        while let Some(found) = content[from..close].find(func) {
            let at = from + found;
            from = at + func.len();
            if content[..at].chars().last().is_some_and(is_ident_char) {
                continue;
            }
            let mut j = from;
            let required = content[j..].starts_with(".required");
            if required {
                j += 9;
            }
            let mut type_text = None;
            j = skip_trivia(content, j);
            if content[j..].starts_with('<') {
                let Some(c) = matching(content, j) else {
                    continue;
                };
                type_text = Some(content[j + 1..c].trim().to_string());
                j = skip_trivia(content, c + 1);
            }
            if !content[j..].starts_with('(') {
                continue;
            }
            // `name = input(…)`: the property before `=`.
            let before = content[..at].trim_end();
            let Some(before) = before.strip_suffix('=') else {
                continue;
            };
            let before = before.trim_end();
            let start = before
                .char_indices()
                .rev()
                .take_while(|(_, c)| is_ident_char(*c))
                .last()
                .map(|(i, _)| i);
            let Some(start) = start else { continue };
            let alias = content[j..matching(content, j).unwrap_or(j)]
                .find("alias")
                .and_then(|k| {
                    let v = skip_trivia(content, j + k + 5);
                    let v = skip_trivia(content, v + 1);
                    string_at(content, v).map(|(a, b)| content[a..b].to_string())
                });
            let m = Member {
                name: before[start..].to_string(),
                alias,
                location: Location::at(path, content, start),
                type_text,
                required,
            };
            if is_input {
                inputs.push(m);
            } else {
                outputs.push(m);
            }
        }
    }
    inputs.sort_by_key(|m| (m.location.line, m.location.column));
    outputs.sort_by_key(|m| (m.location.line, m.location.column));
    (inputs, outputs)
}

/// One alternative of a selector: `button[appRipple]` -> element `button`, attribute `appRipple`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectorPart {
    element: Option<String>,
    attrs: Vec<String>,
}

fn selector_parts(selector: &str) -> Vec<SelectorPart> {
    selector
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with(':'))
        .map(|s| {
            let s = s.split(":not(").next().unwrap_or(s);
            let element_end = s.find(['[', '.']).unwrap_or(s.len());
            let element = (element_end > 0).then(|| s[..element_end].to_string());
            let attrs = s[element_end..]
                .split('[')
                .filter_map(|a| a.split(']').next())
                .map(|a| a.split('=').next().unwrap_or(a).trim().to_string())
                .filter(|a| !a.is_empty() && !a.starts_with('.'))
                .collect();
            SelectorPart { element, attrs }
        })
        .collect()
}

/// `[user]`, `(saved)`, `[(value)]`, `*ngIf`, `bind-x`, `on-y` -> the bound name.
fn binding_name(attr: &str) -> Option<&str> {
    if attr.starts_with('#')
        || attr.starts_with("[attr.")
        || attr.starts_with("[class.")
        || attr.starts_with("[style.")
    {
        return None;
    }
    let name = attr
        .trim_start_matches("[(")
        .trim_end_matches(")]")
        .trim_start_matches(['[', '(', '*'])
        .trim_end_matches([']', ')']);
    let name = name
        .strip_prefix("bind-")
        .or(name.strip_prefix("on-"))
        .unwrap_or(name);
    let name = name.split('.').next().unwrap_or(name);
    (!name.is_empty()).then_some(name)
}

fn is_output_binding(attr: &str) -> bool {
    attr.starts_with('(') || attr.starts_with("on-")
}

fn matches_tag(selector: &str, tag: &Tag) -> bool {
    let attrs: Vec<&str> = tag
        .attrs
        .iter()
        .filter_map(|a| binding_name(&a.name))
        .collect();
    selector_parts(selector).iter().any(|p| {
        p.element
            .as_ref()
            .is_none_or(|e| e.eq_ignore_ascii_case(&tag.name))
            && (p.element.is_some() || !p.attrs.is_empty())
            && p.attrs.iter().all(|a| attrs.contains(&a.as_str()))
    })
}

/// The Angular index of a project.
pub struct AngularIndex<'a> {
    sources: &'a Sources,
    declarations: Vec<Declaration>,
    /// Template or stylesheet path -> declarations that use it.
    owners: HashMap<String, Vec<usize>>,
}

impl<'a> AngularIndex<'a> {
    pub fn new(sources: &'a Sources) -> Self {
        let mut declarations = Vec::new();
        for (path, content) in sources.files() {
            declarations.extend(scan_file(path, content));
        }
        let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, d) in declarations.iter().enumerate() {
            for (_, file) in &d.file_refs {
                owners.entry(file.clone()).or_default().push(i);
            }
        }
        let mut index = AngularIndex {
            sources,
            declarations,
            owners,
        };
        let mut counts = vec![0u32; index.declarations.len()];
        for (_, text, start, end) in index.templates() {
            for (i, _) in index.references(text, start, end) {
                counts[i] += 1;
            }
        }
        for (d, n) in index.declarations.iter_mut().zip(counts) {
            d.component.usages = n;
        }
        index
    }

    /// `(path, content, start, end)` of every template: HTML files and inline templates.
    fn templates(&self) -> Vec<(&'a str, &'a str, usize, usize)> {
        let mut out: Vec<(&str, &str, usize, usize)> = self
            .sources
            .files()
            .filter(|(p, _)| p.ends_with(".html") && self.owners.contains_key(*p))
            .map(|(p, c)| (p, c, 0, c.len()))
            .collect();
        for d in &self.declarations {
            let path = d.component.location.path.as_str();
            if let (Some((a, b)), Some((path, content))) = (
                d.inline_template,
                self.sources.files().find(|(p, _)| *p == path),
            ) {
                out.push((path, content, a, b));
            }
        }
        out
    }

    /// Declarations used in a template range, with the byte offset of the reference.
    fn references(&self, text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for tag in scan_tags(text, start, end, false)
            .iter()
            .filter(|t| !t.closing)
        {
            for (i, d) in self.declarations.iter().enumerate() {
                let Some(sel) = &d.component.selector else {
                    continue;
                };
                if d.component.kind != ComponentKind::Pipe && matches_tag(sel, tag) {
                    out.push((i, tag.name_start));
                }
            }
        }
        for (name, at) in pipes(text, start, end) {
            if let Some(i) = self.pipe(name) {
                out.push((i, at));
            }
        }
        out
    }

    fn pipe(&self, name: &str) -> Option<usize> {
        self.declarations.iter().position(|d| {
            d.component.kind == ComponentKind::Pipe && d.component.selector.as_deref() == Some(name)
        })
    }

    /// Template range around `byte` in the file at `path`: the whole file for HTML, the
    /// inline template for TypeScript.
    fn template_range(&self, path: &str, content: &str, byte: usize) -> Option<(usize, usize)> {
        if path.ends_with(".html") {
            return Some((0, content.len()));
        }
        scan_file(path, content)
            .into_iter()
            .filter_map(|d| d.inline_template)
            .find(|(a, b)| byte >= *a && byte <= *b)
    }

    /// Usages of a declaration in all templates.
    pub fn usages(&self, index: usize) -> Vec<Location> {
        let mut out: Vec<Location> = Vec::new();
        for (path, text, a, b) in self.templates() {
            for (i, at) in self.references(text, a, b) {
                if i == index {
                    out.push(Location::at(path, text, at));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn goto_in_template(
        &self,
        content: &str,
        start: usize,
        end: usize,
        byte: usize,
    ) -> Vec<Location> {
        let tags = scan_tags(content, start, end, false);
        match markup::hit(&tags, byte) {
            Some(TagHit::Name(tag)) => self
                .declarations
                .iter()
                .filter(|d| d.component.kind == ComponentKind::Component)
                .filter(|d| {
                    d.component
                        .selector
                        .as_deref()
                        .is_some_and(|s| matches_tag(s, tag))
                })
                .map(|d| d.component.location.clone())
                .collect(),
            Some(TagHit::Attr(tag, attr)) => {
                let Some(name) = binding_name(&attr.name) else {
                    return Vec::new();
                };
                let output = is_output_binding(&attr.name);
                let mut out = Vec::new();
                for d in &self.declarations {
                    let Some(sel) = &d.component.selector else {
                        continue;
                    };
                    if d.component.kind == ComponentKind::Pipe || !matches_tag(sel, tag) {
                        continue;
                    }
                    let list = if output {
                        &d.component.events
                    } else {
                        &d.component.props
                    };
                    if let Some(m) = list.iter().find(|m| m.public_name() == name) {
                        out.push(m.location.clone());
                    } else if d.component.kind == ComponentKind::Directive
                        && selector_parts(sel)
                            .iter()
                            .any(|p| p.attrs.iter().any(|a| a == name))
                    {
                        out.push(d.component.location.clone());
                    }
                }
                out
            }
            None => {
                let Some((s, e)) = ident_at(content, byte) else {
                    return Vec::new();
                };
                let is_pipe = pipes(content, start, end).iter().any(|(_, at)| *at == s);
                if !is_pipe {
                    return Vec::new();
                }
                self.pipe(&content[s..e])
                    .map(|i| vec![self.declarations[i].component.location.clone()])
                    .unwrap_or_default()
            }
        }
    }
}

/// `| name` pipe references in `{{ … }}` interpolations and attribute values: `(name, byte)`.
fn pipes(text: &str, start: usize, end: usize) -> Vec<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        if bytes[i] == b'|' && bytes.get(i + 1) != Some(&b'|') && (i == 0 || bytes[i - 1] != b'|') {
            let j = skip_trivia(text, i + 1);
            if let Some((a, b)) = ident(text, j).filter(|(_, b)| *b <= end) {
                out.push((&text[a..b], a));
            }
        }
        i += 1;
    }
    out
}

impl FrameworkIndex for AngularIndex<'_> {
    fn components(&self) -> Vec<Component> {
        self.declarations
            .iter()
            .map(|d| d.component.clone())
            .collect()
    }

    fn goto(&self, path: &str, content: &str, offset: usize) -> Vec<Location> {
        let byte = utf16_to_byte(content, offset);
        if let Some((a, b)) = self.template_range(path, content, byte) {
            return self.goto_in_template(content, a, b, byte);
        }
        if !path.ends_with(".ts") {
            return Vec::new();
        }
        for d in scan_file(path, content) {
            // templateUrl / styleUrls strings open the files.
            if let Some((_, file)) = d
                .file_refs
                .iter()
                .find(|((a, b), _)| byte >= *a && byte <= *b)
            {
                return vec![Location::file(file)];
            }
            // The selector or the class name: usages in templates.
            let on_selector = d
                .selector_range
                .is_some_and(|(a, b)| byte >= a && byte <= b);
            let on_name = ident_at(content, byte)
                .is_some_and(|(s, _)| Location::at(path, content, s) == d.component.location);
            if on_selector || on_name {
                let Some(i) = self.declarations.iter().position(|x| {
                    x.component.location.path == path && x.component.name == d.component.name
                }) else {
                    return Vec::new();
                };
                return self.usages(i);
            }
        }
        Vec::new()
    }

    fn gutter(&self, path: &str, content: &str) -> Vec<GutterMark> {
        let mut out = Vec::new();
        if path.ends_with(".ts") {
            for d in scan_file(path, content) {
                let line = d.component.location.line;
                if let Some(t) = d
                    .component
                    .template
                    .as_ref()
                    .filter(|_| d.inline_template.is_none())
                {
                    out.push(GutterMark {
                        line,
                        kind: GutterKind::Template,
                        title: format!(
                            "Go to template {}",
                            t.path.rsplit('/').next().unwrap_or(&t.path)
                        ),
                        targets: vec![t.clone()],
                    });
                }
                if !d.component.styles.is_empty() {
                    out.push(GutterMark {
                        line,
                        kind: GutterKind::Styles,
                        title: if d.component.styles.len() == 1 {
                            "Go to styles".into()
                        } else {
                            format!("Go to styles ({})", d.component.styles.len())
                        },
                        targets: d.component.styles.clone(),
                    });
                }
            }
        } else if let Some(owners) = self.owners.get(path) {
            let targets: Vec<Location> = owners
                .iter()
                .map(|&i| self.declarations[i].component.location.clone())
                .collect();
            let names: Vec<&str> = owners
                .iter()
                .map(|&i| self.declarations[i].component.name.as_str())
                .collect();
            out.push(GutterMark {
                line: 0,
                kind: GutterKind::Component,
                title: format!("Go to component {}", names.join(", ")),
                targets,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests;

//! WebForge React plugin (`forge.react`): function and class components with their props,
//! JSX tag resolution through imports (relative paths, path aliases, barrel re-exports)
//! and usages. Go to Declaration on `<UserCard` opens the component, on `title=` the prop,
//! and on a component's own name lists the JSX tags that use it, like WebStorm.

use std::collections::HashMap;

use webforge_lang_web::imports::{self, Import, Imported};
use webforge_lang_web::js::{self, fields, ident, matching, skip_trivia};
use webforge_lang_web::markup::{self, scan_tags, TagHit};
use webforge_lang_web::text::{ident_at, utf16_to_byte};
use webforge_lang_web::{
    Component, ComponentKind, Framework, FrameworkIndex, GutterMark, Location, Member, Sources,
};

/// Files that may define React components.
pub fn is_script(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(ext, "jsx" | "tsx" | "js" | "ts" | "mjs" | "mts") && !path.ends_with(".d.ts")
}

fn is_jsx_file(path: &str, content: &str) -> bool {
    path.ends_with(".jsx")
        || path.ends_with(".tsx")
        || content.contains("createElement(")
        // A closing or self-closing tag; type arguments never look like that.
        || scan_tags(content, 0, content.len(), true)
            .iter()
            .any(|t| t.closing || content[..t.end].ends_with("/>"))
}

fn is_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().any(|c| c.is_ascii_lowercase())
}

/// The React components defined in one file.
pub fn scan_file(path: &str, content: &str) -> Vec<Component> {
    if !is_script(path) || !is_jsx_file(path, content) {
        return Vec::new();
    }
    let mut out: Vec<Component> = Vec::new();
    for keyword in ["function", "const", "let", "var", "class"] {
        let mut from = 0;
        while let Some(found) = content[from..].find(keyword) {
            let at = from + found;
            from = at + keyword.len();
            let boundary_before = content[..at]
                .chars()
                .last()
                .is_none_or(|c| !webforge_lang_web::text::is_ident_char(c) && c != '.');
            let after = content[from..].chars().next();
            if !boundary_before || !after.is_some_and(char::is_whitespace) {
                continue;
            }
            let name_at = skip_trivia(content, from);
            let Some((ns, ne)) = ident(content, name_at) else {
                continue;
            };
            let name = &content[ns..ne];
            if !is_component_name(name) || out.iter().any(|c| c.name == name) {
                continue;
            }
            let found = match keyword {
                "function" => function_component(content, ne),
                "class" => class_component(content, ne),
                _ => variable_component(content, ne),
            };
            let Some((kind, props)) = found else {
                continue;
            };
            out.push(Component {
                name: name.to_string(),
                framework: Framework::React,
                kind,
                selector: None,
                location: Location::at(path, content, ns),
                props: props.into_members(path, content),
                events: Vec::new(),
                template: None,
                styles: Vec::new(),
                default_export: is_default_export(content, name),
                usages: 0,
            });
        }
    }
    out.sort_by_key(|c| (c.location.line, c.location.column));
    out
}

/// Where the props of a component come from.
#[derive(Default)]
struct Props {
    /// `({ a, b = 1 })`: destructured names.
    destructured: Vec<js::Field>,
    /// Members of the props type (inline literal, interface or type alias).
    typed: Vec<js::Field>,
}

impl Props {
    fn into_members(self, path: &str, content: &str) -> Vec<Member> {
        let mut out: Vec<Member> = self
            .typed
            .iter()
            .map(|f| Member {
                name: f.key.clone(),
                alias: None,
                location: Location::at(path, content, f.key_start),
                type_text: f.value.map(|(a, b)| content[a..b].to_string()),
                required: !f.optional,
            })
            .collect();
        for f in &self.destructured {
            if !out.iter().any(|m| m.name == f.key) {
                out.push(Member {
                    name: f.key.clone(),
                    alias: None,
                    location: Location::at(path, content, f.key_start),
                    type_text: None,
                    required: !f.has_default,
                });
            }
        }
        out
    }
}

/// `function Name<T>(params) {` after the name.
fn function_component(content: &str, after_name: usize) -> Option<(ComponentKind, Props)> {
    let mut i = skip_trivia(content, after_name);
    if content[i..].starts_with('<') {
        i = skip_trivia(content, matching(content, i)? + 1);
    }
    if !content[i..].starts_with('(') {
        return None;
    }
    Some((ComponentKind::Function, params(content, i, None)))
}

/// `class Name extends React.Component<Props> {`.
fn class_component(content: &str, after_name: usize) -> Option<(ComponentKind, Props)> {
    let head_end = content[after_name..].find('{').map(|e| after_name + e)?;
    let head = &content[after_name..head_end];
    let ext = head.find("extends")?;
    let base = head[ext + 7..].trim_start();
    let base_name: String = base
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
        .collect();
    if !(base_name.ends_with("Component") || base_name.ends_with("PureComponent")) {
        return None;
    }
    let mut props = Props::default();
    let generic = after_name + ext + 7 + (head[ext + 7..].len() - base.len()) + base_name.len();
    if content[generic..].starts_with('<') {
        props.typed = type_fields(content, generic + 1, None);
    }
    Some((ComponentKind::Class, props))
}

/// `const Name = (props) => …`, `= function (…)`, `= memo((…) => …)`, `= forwardRef(…)`,
/// optionally typed `const Name: FC<Props> = …`.
fn variable_component(content: &str, after_name: usize) -> Option<(ComponentKind, Props)> {
    let mut i = skip_trivia(content, after_name);
    let mut annotated: Option<usize> = None;
    if content[i..].starts_with(':') {
        // `: React.FC<Props>`: remember where the generic argument starts.
        let eq = content[i..].find('=').map(|e| i + e)?;
        let ann = &content[i + 1..eq];
        if let Some(lt) = ann.find('<') {
            let name = ann[..lt].trim();
            if name.ends_with("FC") || name.ends_with("FunctionComponent") || name.ends_with("ComponentType") {
                annotated = Some(i + 1 + lt + 1);
            }
        }
        i = eq;
    }
    if !content[i..].starts_with('=') || content[i..].starts_with("==") {
        return None;
    }
    i = skip_trivia(content, i + 1);
    // Unwrap `memo(`, `React.memo(`, `forwardRef(`, `React.forwardRef<Ref, Props>(`.
    let mut wrapper_props: Option<usize> = None;
    for _ in 0..2 {
        let rest = &content[i..];
        let wrapper = ["React.memo", "React.forwardRef", "memo", "forwardRef", "observer"]
            .into_iter()
            .find(|w| rest.starts_with(w) && rest[w.len()..].starts_with(['(', '<']));
        let Some(w) = wrapper else { break };
        i += w.len();
        if content[i..].starts_with('<') {
            let close = matching(content, i)?;
            // forwardRef<Ref, Props>: the props type is the last argument.
            let args = &content[i + 1..close];
            let last = args.rfind(',').map_or(0, |c| c + 1);
            wrapper_props = Some(i + 1 + last);
            i = close + 1;
        }
        i = skip_trivia(content, i + 1);
    }
    let rest = &content[i..];
    let params_at = if rest.starts_with("async") {
        skip_trivia(content, i + 5)
    } else {
        i
    };
    let rest = &content[params_at..];
    let open = if rest.starts_with("function") {
        let mut j = skip_trivia(content, params_at + 8);
        if let Some((_, e)) = ident(content, j) {
            j = skip_trivia(content, e);
        }
        content[j..].starts_with('(').then_some(j)?
    } else if rest.starts_with('(') {
        let close = matching(content, params_at)?;
        let after = skip_trivia(content, close + 1);
        // `(…) =>` or `(…): JSX.Element =>`.
        let arrow = &content[after..content.len().min(after + 200)];
        if !(arrow.starts_with("=>") || (arrow.starts_with(':') && arrow.contains("=>"))) {
            return None;
        }
        params_at
    } else if rest.starts_with('<') {
        // Generic arrow function `<T,>(props: Props<T>) =>`.
        let close = matching(content, params_at)?;
        let j = skip_trivia(content, close + 1);
        content[j..].starts_with('(').then_some(j)?
    } else if let Some((_, e)) = ident(content, params_at) {
        // `props => …`
        let j = skip_trivia(content, e);
        if !content[j..].starts_with("=>") {
            return None;
        }
        let mut props = Props::default();
        if let Some(at) = annotated.or(wrapper_props) {
            props.typed = type_fields(content, at, None);
        }
        return Some((ComponentKind::Function, props));
    } else {
        return None;
    };
    Some((ComponentKind::Function, params(content, open, annotated.or(wrapper_props))))
}

/// Props from the first parameter of the list starting at `open` (`(`).
fn params(content: &str, open: usize, type_at: Option<usize>) -> Props {
    let mut props = Props::default();
    let Some(close) = matching(content, open) else {
        return props;
    };
    let i = skip_trivia(content, open + 1);
    let mut after_pattern = i;
    if content[i..close].starts_with('{') {
        if let Some(c) = matching(content, i) {
            props.destructured = fields(content, i + 1, c);
            after_pattern = c + 1;
        }
    } else if let Some((_, e)) = ident(content, i) {
        after_pattern = e;
    }
    let j = skip_trivia(content, after_pattern);
    if content[j..close].starts_with(':') {
        props.typed = type_fields(content, skip_trivia(content, j + 1), Some(close));
    } else if let Some(at) = type_at {
        props.typed = type_fields(content, at, None);
    }
    props
}

/// Members of the type written at `at`: an inline literal `{ … }` or the name of an
/// interface or type alias declared in the same file (`Readonly<…>` is unwrapped).
fn type_fields(content: &str, at: usize, limit: Option<usize>) -> Vec<js::Field> {
    let at = skip_trivia(content, at);
    let limit = limit.unwrap_or(content.len());
    if content[at..].starts_with('{') {
        return matching(content, at)
            .filter(|c| *c <= limit)
            .map(|c| fields(content, at + 1, c))
            .unwrap_or_default();
    }
    let Some((s, e)) = ident(content, at) else {
        return Vec::new();
    };
    let name = &content[s..e];
    if matches!(name, "Readonly" | "PropsWithChildren") && content[e..].starts_with('<') {
        return type_fields(content, e + 1, None);
    }
    declared_type_fields(content, name, 0)
}

/// Members of `interface Name { … }` or `type Name = { … }` (with `extends` / `&` parts of
/// the same file merged in).
fn declared_type_fields(content: &str, name: &str, depth: u32) -> Vec<js::Field> {
    if depth > 4 {
        return Vec::new();
    }
    for keyword in ["interface", "type"] {
        let pattern = format!("{keyword} {name}");
        let mut from = 0;
        while let Some(found) = content[from..].find(&pattern) {
            let at = from + found;
            from = at + pattern.len();
            if content[from..].starts_with(|c: char| webforge_lang_web::text::is_ident_char(c)) {
                continue;
            }
            let Some(open) = content[from..].find('{').map(|o| from + o) else {
                continue;
            };
            let head = &content[from..open];
            let declares = head.trim().is_empty()
                || head.trim_start().starts_with(['=', '<'])
                || head.trim_start().starts_with("extends");
            if !declares || head.contains(';') {
                continue;
            }
            let Some(close) = matching(content, open) else {
                continue;
            };
            let mut out = fields(content, open + 1, close);
            // `interface A extends B` / `type A = B & { … }`
            for base in head
                .split(|c: char| !webforge_lang_web::text::is_ident_char(c))
                .filter(|w| is_component_name(w) && *w != name)
            {
                out.extend(declared_type_fields(content, base, depth + 1));
            }
            return out;
        }
    }
    Vec::new()
}

fn is_default_export(content: &str, name: &str) -> bool {
    [
        format!("export default function {name}"),
        format!("export default class {name}"),
        format!("export default {name};"),
        format!("export default {name}\n"),
        format!("export default memo({name})"),
        format!("export default React.memo({name})"),
        format!("{name} as default"),
    ]
    .iter()
    .any(|p| content.contains(p.as_str()))
        || content.trim_end().ends_with(&format!("export default {name}"))
}

/// The React component index of a project.
pub struct ReactIndex<'a> {
    sources: &'a Sources,
    components: Vec<Component>,
    by_path: HashMap<String, Vec<usize>>,
}

impl<'a> ReactIndex<'a> {
    pub fn new(sources: &'a Sources) -> Self {
        let mut components = Vec::new();
        let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
        for (path, content) in sources.files() {
            for c in scan_file(path, content) {
                by_path.entry(path.to_string()).or_default().push(components.len());
                components.push(c);
            }
        }
        let mut index = ReactIndex {
            sources,
            components,
            by_path,
        };
        let mut counts: HashMap<(String, String), u32> = HashMap::new();
        for (path, content) in sources.files().filter(|(p, _)| is_script(p)) {
            for loc in index.jsx_references(path, content) {
                *counts.entry(loc).or_default() += 1;
            }
        }
        for c in &mut index.components {
            c.usages = counts
                .get(&(c.location.path.clone(), c.name.clone()))
                .copied()
                .unwrap_or(0);
        }
        index
    }

    /// `(definition path, component name)` of every component tag in a file.
    fn jsx_references(&self, path: &str, content: &str) -> Vec<(String, String)> {
        let local = scan_file(path, content);
        let imports = imports::imports(content);
        scan_tags(content, 0, content.len(), true)
            .iter()
            .filter(|t| !t.closing)
            .filter_map(|t| self.resolve(path, &local, &imports, &t.name))
            .map(|c| (c.location.path.clone(), c.name.clone()))
            .collect()
    }

    /// The component a JSX tag name refers to in `path`.
    fn resolve<'s>(&'s self, path: &str, local: &'s [Component], imports: &[Import], tag: &str) -> Option<&'s Component> {
        let head = tag.split('.').next().unwrap_or(tag);
        if !is_component_name(head) && !tag.contains('.') {
            return None;
        }
        if let Some(c) = local.iter().find(|c| c.name == head) {
            // Prefer the indexed copy so callers compare like with like.
            return self.find(path, &c.name).or(Some(c));
        }
        if let Some(import) = imports.iter().find(|i| i.local == head) {
            let module = self.sources.resolve_module(path, &import.source)?;
            return match (&import.imported, tag.split_once('.')) {
                // `import * as ui from './ui'` + `<ui.Button>`
                (Imported::Namespace, Some((_, member))) => self.in_module(&module, &Imported::Named(member.to_string()), 0),
                (imported, _) => self.in_module(&module, imported, 0),
            };
        }
        // Not imported: a global or auto-imported component with that name.
        self.components.iter().find(|c| c.name == head)
    }

    fn find(&self, path: &str, name: &str) -> Option<&Component> {
        self.by_path
            .get(path)?
            .iter()
            .map(|&i| &self.components[i])
            .find(|c| c.name == name)
    }

    /// The component a module exports under `imported`, following re-exports.
    fn in_module(&self, module: &str, imported: &Imported, depth: u32) -> Option<&Component> {
        if depth > 4 {
            return None;
        }
        let own: Vec<&Component> = self
            .by_path
            .get(module)
            .map(|v| v.iter().map(|&i| &self.components[i]).collect())
            .unwrap_or_default();
        match imported {
            Imported::Default => {
                if let Some(c) = own.iter().find(|c| c.default_export).or(own.first().filter(|_| own.len() == 1)) {
                    return Some(c);
                }
            }
            Imported::Named(name) => {
                if let Some(c) = own.iter().find(|c| &c.name == name) {
                    return Some(c);
                }
            }
            Imported::Namespace => return None,
        }
        let wanted = match imported {
            Imported::Default => "default",
            Imported::Named(n) => n.as_str(),
            Imported::Namespace => return None,
        };
        let content = self.sources.get(module)?;
        for re in imports::re_exports(content) {
            let target = if re.exported == wanted {
                if re.imported == "default" {
                    Imported::Default
                } else {
                    Imported::Named(re.imported.clone())
                }
            } else if re.exported == "*" {
                imported.clone()
            } else {
                continue;
            };
            if let Some(next) = self.sources.resolve_module(module, &re.source) {
                if let Some(c) = self.in_module(&next, &target, depth + 1) {
                    return Some(c);
                }
            }
        }
        None
    }

    /// JSX tags in the whole project that use `component` (the current file's unsaved text
    /// replaces its saved one).
    pub fn usages(&self, component: &Component, current: Option<(&str, &str)>) -> Vec<Location> {
        let mut out = Vec::new();
        let files = self.sources.files().filter(|(p, _)| Some(*p) != current.map(|c| c.0));
        for (path, content) in files.chain(current) {
            if !is_script(path) {
                continue;
            }
            let local = scan_file(path, content);
            let imports = imports::imports(content);
            for tag in scan_tags(content, 0, content.len(), true) {
                if tag.closing {
                    continue;
                }
                if let Some(c) = self.resolve(path, &local, &imports, &tag.name) {
                    if c.name == component.name && c.location.path == component.location.path {
                        out.push(Location::at(path, content, tag.name_start));
                    }
                }
            }
        }
        out.sort();
        out
    }
}

impl FrameworkIndex for ReactIndex<'_> {
    fn components(&self) -> Vec<Component> {
        self.components.clone()
    }

    fn goto(&self, path: &str, content: &str, offset: usize) -> Vec<Location> {
        if !is_script(path) {
            return Vec::new();
        }
        let byte = utf16_to_byte(content, offset);
        let local = scan_file(path, content);
        let imports = imports::imports(content);
        let tags = scan_tags(content, 0, content.len(), true);
        match markup::hit(&tags, byte) {
            Some(TagHit::Name(tag)) => self
                .resolve(path, &local, &imports, &tag.name)
                .map(|c| vec![c.location.clone()])
                .unwrap_or_default(),
            Some(TagHit::Attr(tag, attr)) => self
                .resolve(path, &local, &imports, &tag.name)
                .and_then(|c| c.props.iter().find(|p| p.name == attr.name))
                .map(|p| vec![p.location.clone()])
                .unwrap_or_default(),
            None => {
                // On a component's own name: its usages.
                let Some((s, e)) = ident_at(content, byte) else {
                    return Vec::new();
                };
                let here = Location::at(path, content, s);
                local
                    .iter()
                    .find(|c| c.location == here && c.name == content[s..e])
                    .map(|c| self.usages(c, Some((path, content))))
                    .unwrap_or_default()
            }
        }
    }

    fn gutter(&self, _path: &str, _content: &str) -> Vec<GutterMark> {
        // WebStorm shows no line markers for React components.
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

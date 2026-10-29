//! WebForge Vue plugin (`forge.vue`): single-file components (`<script setup>` and the
//! Options API) with their props and emits, and template tag resolution: `<UserCard>` and
//! `<user-card>` through the script's imports, barrel re-exports, or project-wide (global
//! registration, Nuxt auto-imports). Go to Declaration on a tag opens the component, on
//! `:title` / `@select` the prop or emit.

pub mod extract;

use std::collections::HashMap;

use webforge_lang_web::imports::{self, Import, Imported};
use webforge_lang_web::js::{fields, ident, matching, skip_trivia, string_at};
use webforge_lang_web::markup::{self, scan_tags, TagHit};
use webforge_lang_web::text::{camel_case, file_stem, kebab_case, pascal_case, utf16_to_byte};
use webforge_lang_web::{
    Component, ComponentKind, Framework, FrameworkIndex, GutterMark, Location, Member, Sources,
};

/// A top-level block of a single-file component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Offset of `<`.
    pub tag_start: usize,
    /// Content range.
    pub start: usize,
    pub end: usize,
    pub setup: bool,
    pub lang: Option<String>,
}

/// The blocks of a `.vue` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sfc {
    pub template: Option<Block>,
    pub scripts: Vec<Block>,
    pub styles: Vec<Block>,
}

impl Sfc {
    pub fn parse(content: &str) -> Sfc {
        let mut sfc = Sfc::default();
        let mut i = 0;
        while let Some(found) = content[i..].find('<') {
            let at = i + found;
            i = at + 1;
            let rest = &content[at + 1..];
            let kind = ["template", "script", "style"].into_iter().find(|k| {
                rest.starts_with(k) && rest[k.len()..].starts_with([' ', '>', '\n', '\t'])
            });
            let Some(kind) = kind else {
                if rest.starts_with("!--") {
                    i = content[at..]
                        .find("-->")
                        .map_or(content.len(), |e| at + e + 3);
                }
                continue;
            };
            let Some(open_end) = content[at..].find('>').map(|e| at + e + 1) else {
                break;
            };
            let head = &content[at..open_end];
            let close_tag = format!("</{kind}>");
            // The template's closing tag is the last one (templates nest); others the first.
            let close = if kind == "template" {
                content.rfind(&close_tag).filter(|c| *c >= open_end)
            } else {
                content[open_end..].find(&close_tag).map(|e| open_end + e)
            }
            .unwrap_or(content.len());
            let lang = head.find("lang=").and_then(|l| {
                let v = &head[l + 5..];
                let q = v.chars().next()?;
                let v = &v[1..];
                v.find(q).map(|e| v[..e].to_string())
            });
            let block = Block {
                tag_start: at,
                start: open_end,
                end: close,
                setup: kind == "script"
                    && head
                        .split_whitespace()
                        .any(|w| w == "setup" || w == "setup>"),
                lang,
            };
            match kind {
                "template" if sfc.template.is_none() => sfc.template = Some(block),
                "script" => sfc.scripts.push(block),
                "style" => sfc.styles.push(block),
                _ => {}
            }
            i = (close + close_tag.len()).min(content.len());
        }
        sfc
    }

    /// The `<script setup>` block, else the plain `<script>`.
    pub fn main_script(&self) -> Option<&Block> {
        self.scripts
            .iter()
            .find(|b| b.setup)
            .or(self.scripts.first())
    }
}

/// The component a `.vue` file defines, or those a script file defines with
/// `defineComponent`.
pub fn scan_file(path: &str, content: &str) -> Vec<Component> {
    if path.ends_with(".vue") {
        return vec![single_file(path, content)];
    }
    let ext = path.rsplit('.').next().unwrap_or("");
    if !matches!(ext, "js" | "ts" | "mjs" | "mts") || !content.contains("defineComponent(") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = content[from..].find("defineComponent(") {
        let at = from + found;
        from = at + 16;
        let open = skip_trivia(content, from);
        if !content[open..].starts_with('{') {
            continue;
        }
        let Some(close) = matching(content, open) else {
            continue;
        };
        // `const Name = defineComponent(` or `export default defineComponent({ name: … })`.
        let before = content[..at].trim_end().trim_end_matches('=').trim_end();
        let var = before
            .rsplit(|c: char| !webforge_lang_web::text::is_ident_char(c))
            .next()
            .filter(|n| n.starts_with(|c: char| c.is_ascii_uppercase()));
        let option_name = string_option(content, open, close, "name");
        let Some((name, name_at)) = option_name
            .map(|(s, e)| (pascal_case(&content[s..e]), s))
            .or_else(|| var.map(|v| (v.to_string(), before.len() - v.len())))
        else {
            continue;
        };
        let mut component = new_component(path, content, name, name_at, ComponentKind::Script);
        component.props = options_members(path, content, open, close, "props");
        component.events = options_members(path, content, open, close, "emits");
        component.default_export = content[..at].trim_end().ends_with("export default");
        out.push(component);
    }
    out
}

fn new_component(
    path: &str,
    content: &str,
    name: String,
    at: usize,
    kind: ComponentKind,
) -> Component {
    Component {
        selector: Some(kebab_case(&name)),
        name,
        framework: Framework::Vue,
        kind,
        location: Location::at(path, content, at),
        props: Vec::new(),
        events: Vec::new(),
        template: None,
        styles: Vec::new(),
        default_export: true,
        usages: 0,
    }
}

fn single_file(path: &str, content: &str) -> Component {
    let sfc = Sfc::parse(content);
    let script = sfc.main_script();
    let at = script.map_or(0, |b| b.tag_start);
    let mut c = new_component(
        path,
        content,
        pascal_case(file_stem(path)),
        at,
        ComponentKind::SingleFile,
    );
    c.template = sfc
        .template
        .as_ref()
        .map(|t| Location::at(path, content, t.tag_start));
    c.styles = sfc
        .styles
        .iter()
        .map(|s| Location::at(path, content, s.tag_start))
        .collect();
    for block in &sfc.scripts {
        let (s, e) = (block.start, block.end);
        if block.setup {
            if let Some((open, close)) = macro_call(content, s, e, "defineOptions") {
                if let Some((ns, ne)) = string_option(content, open, close, "name") {
                    c.name = pascal_case(&content[ns..ne]);
                    c.selector = Some(kebab_case(&c.name));
                }
            }
            c.props
                .extend(macro_members(path, content, s, e, "defineProps"));
            c.events
                .extend(macro_members(path, content, s, e, "defineEmits"));
            // `defineModel('title')` / `defineModel()` add props too.
            let mut from = s;
            while let Some(found) = content[from..e].find("defineModel") {
                let at = from + found;
                from = at + 11;
                let mut j = skip_trivia(content, from);
                if content[j..].starts_with('<') {
                    j = matching(content, j).map_or(j, |c| skip_trivia(content, c + 1));
                }
                if !content[j..].starts_with('(') {
                    continue;
                }
                let arg = skip_trivia(content, j + 1);
                let (name, name_at) = match string_at(content, arg) {
                    Some((a, b)) => (content[a..b].to_string(), a),
                    None => ("modelValue".to_string(), at),
                };
                c.props
                    .push(member(path, content, name, name_at, None, false));
            }
        } else if let Some((open, close)) = options_object(content, s, e) {
            if let Some((ns, ne)) = string_option(content, open, close, "name") {
                c.name = pascal_case(&content[ns..ne]);
                c.selector = Some(kebab_case(&c.name));
            }
            c.props
                .extend(options_members(path, content, open, close, "props"));
            c.events
                .extend(options_members(path, content, open, close, "emits"));
        }
    }
    c
}

fn member(
    path: &str,
    content: &str,
    name: String,
    at: usize,
    type_text: Option<String>,
    required: bool,
) -> Member {
    Member {
        name,
        alias: None,
        location: Location::at(path, content, at),
        type_text,
        required,
    }
}

/// `export default {` or `export default defineComponent({`: the object's braces.
fn options_object(content: &str, s: usize, e: usize) -> Option<(usize, usize)> {
    let at = s + content[s..e].find("export default")?;
    let mut j = skip_trivia(content, at + 14);
    if content[j..].starts_with("defineComponent") {
        j = skip_trivia(content, j + 15);
        if !content[j..].starts_with('(') {
            return None;
        }
        j = skip_trivia(content, j + 1);
    }
    if !content[j..].starts_with('{') {
        return None;
    }
    Some((j, matching(content, j)?))
}

/// The string value of `key: '…'` directly inside the object `open..close`.
fn string_option(content: &str, open: usize, close: usize, key: &str) -> Option<(usize, usize)> {
    let f = fields(content, open + 1, close)
        .into_iter()
        .find(|f| f.key == key)?;
    string_at(content, f.value?.0)
}

/// Members of an options-API `props` / `emits` option: an array of names or an object.
fn options_members(path: &str, content: &str, open: usize, close: usize, key: &str) -> Vec<Member> {
    let Some((vs, ve)) = fields(content, open + 1, close)
        .into_iter()
        .find(|f| f.key == key)
        .and_then(|f| f.value)
    else {
        return Vec::new();
    };
    runtime_members(path, content, vs, ve)
}

/// `['a', 'b']` or `{ a: String, b: { type: Number, required: true } }` at `vs..ve`.
fn runtime_members(path: &str, content: &str, vs: usize, ve: usize) -> Vec<Member> {
    match content.as_bytes().get(vs) {
        Some(b'[') => {
            let Some(close) = matching(content, vs).filter(|c| *c <= ve) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            let mut i = vs + 1;
            while i < close {
                if let Some((a, b)) = string_at(content, i) {
                    out.push(member(
                        path,
                        content,
                        content[a..b].to_string(),
                        a,
                        None,
                        false,
                    ));
                    i = b + 1;
                } else {
                    i += 1;
                }
            }
            out
        }
        Some(b'{') => {
            let Some(close) = matching(content, vs).filter(|c| *c <= ve) else {
                return Vec::new();
            };
            fields(content, vs + 1, close)
                .into_iter()
                .map(|f| {
                    let value = f.value.map(|(a, b)| &content[a..b]).unwrap_or("");
                    let (type_text, required) = if value.starts_with('{') {
                        let inner = &value[1..value.len().saturating_sub(1)];
                        let ty = inner
                            .split([',', '\n'])
                            .find_map(|p| p.trim().strip_prefix("type:"))
                            .map(|t| t.trim().to_string());
                        let req = inner.replace(' ', "").contains("required:true");
                        (ty, req)
                    } else {
                        ((!value.is_empty()).then(|| value.to_string()), false)
                    };
                    member(path, content, f.key, f.key_start, type_text, required)
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The argument list of `name(…)` / `name<…>(…)` in `s..e`: `(open, close)` of the type
/// argument when there is one, else of the call parentheses.
fn macro_call(content: &str, s: usize, e: usize, name: &str) -> Option<(usize, usize)> {
    let mut from = s;
    while let Some(found) = content[from..e].find(name) {
        let at = from + found;
        from = at + name.len();
        let j = skip_trivia(content, from);
        match content.as_bytes().get(j) {
            Some(b'<') => {
                let close = matching(content, j)?;
                return Some((j, close));
            }
            Some(b'(') => {
                let close = matching(content, j)?;
                return Some((j, close));
            }
            _ => {}
        }
    }
    None
}

/// Props of `defineProps` / emits of `defineEmits`, typed (`<{ … }>`, `<Props>`) or at
/// runtime (`({ … })`, `([…])`).
fn macro_members(path: &str, content: &str, s: usize, e: usize, name: &str) -> Vec<Member> {
    let Some((open, close)) = macro_call(content, s, e, name) else {
        return Vec::new();
    };
    if content.as_bytes()[open] == b'(' {
        let arg = skip_trivia(content, open + 1);
        return runtime_members(path, content, arg, close);
    }
    let ty = skip_trivia(content, open + 1);
    let (body_open, body_close) = if content[ty..].starts_with('{') {
        match matching(content, ty) {
            Some(c) => (ty, c),
            None => return Vec::new(),
        }
    } else {
        // `defineProps<Props>()`: an interface or type alias in the same script.
        let Some((a, b)) = ident(content, ty) else {
            return Vec::new();
        };
        match declared_body(content, s, e, &content[a..b]) {
            Some(r) => r,
            None => return Vec::new(),
        }
    };
    let mut out: Vec<Member> = fields(content, body_open + 1, body_close)
        .into_iter()
        .map(|f| {
            let ty = f.value.map(|(a, b)| content[a..b].to_string());
            member(path, content, f.key, f.key_start, ty, !f.optional)
        })
        .collect();
    if name == "defineEmits" {
        // Call signatures: `(e: 'select', id: number): void`.
        let mut i = body_open + 1;
        while let Some(found) = content[i..body_close].find('(') {
            let at = i + found;
            let Some(c) = matching(content, at) else {
                break;
            };
            let sig = &content[at..c];
            if let Some(colon) = sig.find(':') {
                let lit = skip_trivia(content, at + colon + 1);
                if let Some((a, b)) = string_at(content, lit) {
                    out.push(member(
                        path,
                        content,
                        content[a..b].to_string(),
                        a,
                        None,
                        false,
                    ));
                }
            }
            i = c + 1;
        }
    }
    out
}

/// Braces of `interface Name {` or `type Name = {` inside `s..e`.
fn declared_body(content: &str, s: usize, e: usize, name: &str) -> Option<(usize, usize)> {
    for pattern in [format!("interface {name}"), format!("type {name}")] {
        if let Some(found) = content[s..e].find(&pattern) {
            let after = s + found + pattern.len();
            let open = after + content[after..e].find('{')?;
            if content[after..open].contains(';') {
                continue;
            }
            return Some((open, matching(content, open)?));
        }
    }
    None
}

/// Names under which Nuxt auto-imports a component: `components/base/Button.vue` ->
/// `BaseButton`.
fn nuxt_name(path: &str) -> Option<String> {
    let rest = path.split_once("components/")?.1;
    let (dirs, file) = rest.rsplit_once('/').unwrap_or(("", rest));
    let stem = file_stem(file);
    let prefix: String = dirs
        .split('/')
        .filter(|d| !d.is_empty())
        .map(pascal_case)
        .collect();
    let stem = pascal_case(stem);
    Some(if stem.starts_with(&prefix) {
        stem
    } else {
        format!("{prefix}{stem}")
    })
}

/// HTML elements and Vue's built-in components are never project components.
fn is_builtin(tag: &str) -> bool {
    (!tag.contains('-') && tag.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        || matches!(
            tag,
            "Transition"
                | "TransitionGroup"
                | "KeepAlive"
                | "Teleport"
                | "Suspense"
                | "Component"
                | "Slot"
                | "keep-alive"
                | "transition-group"
        )
}

/// The Vue component index of a project.
pub struct VueIndex<'a> {
    sources: &'a Sources,
    components: Vec<Component>,
    by_path: HashMap<String, Vec<usize>>,
}

impl<'a> VueIndex<'a> {
    pub fn new(sources: &'a Sources) -> Self {
        let mut components = Vec::new();
        let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
        for (path, content) in sources.files() {
            for c in scan_file(path, content) {
                by_path
                    .entry(path.to_string())
                    .or_default()
                    .push(components.len());
                components.push(c);
            }
        }
        let mut index = VueIndex {
            sources,
            components,
            by_path,
        };
        let mut counts: HashMap<String, u32> = HashMap::new();
        for (path, content) in sources.files().filter(|(p, _)| p.ends_with(".vue")) {
            for c in index.template_references(path, content) {
                *counts.entry(c).or_default() += 1;
            }
        }
        for c in &mut index.components {
            c.usages = counts.get(&key(c)).copied().unwrap_or(0);
        }
        index
    }

    fn template_references(&self, path: &str, content: &str) -> Vec<String> {
        let imports = script_imports(content);
        template_tags(content)
            .iter()
            .filter(|t| !t.closing)
            .filter_map(|t| self.resolve(path, &imports, &t.name))
            .map(key)
            .collect()
    }

    /// The component a template tag refers to in the `.vue` file at `path`.
    fn resolve(&self, path: &str, imports: &[Import], tag: &str) -> Option<&Component> {
        if is_builtin(tag) {
            return None;
        }
        let name = pascal_case(tag);
        if let Some(import) = imports.iter().find(|i| i.local == name || i.local == tag) {
            let module = self.sources.resolve_module(path, &import.source)?;
            return self.in_module(&module, &import.imported, 0);
        }
        self.components.iter().find(|c| c.name == name).or_else(|| {
            self.components
                .iter()
                .find(|c| nuxt_name(&c.location.path).as_deref() == Some(name.as_str()))
        })
    }

    fn in_module(&self, module: &str, imported: &Imported, depth: u32) -> Option<&Component> {
        if depth > 4 {
            return None;
        }
        let own: Vec<&Component> = self
            .by_path
            .get(module)
            .map(|v| v.iter().map(|&i| &self.components[i]).collect())
            .unwrap_or_default();
        let found = match imported {
            Imported::Default => own.iter().find(|c| c.default_export).copied(),
            Imported::Named(n) => own.iter().find(|c| &c.name == n).copied(),
            Imported::Namespace => None,
        };
        if found.is_some() {
            return found;
        }
        let wanted = match imported {
            Imported::Default => "default",
            Imported::Named(n) => n.as_str(),
            Imported::Namespace => return None,
        };
        let content = self.sources.get(module)?;
        imports::re_exports(content).into_iter().find_map(|re| {
            let target = if re.exported == wanted {
                if re.imported == "default" {
                    Imported::Default
                } else {
                    Imported::Named(re.imported.clone())
                }
            } else if re.exported == "*" {
                imported.clone()
            } else {
                return None;
            };
            let next = self.sources.resolve_module(module, &re.source)?;
            self.in_module(&next, &target, depth + 1)
        })
    }

    /// Template tags in the project that use `component`.
    pub fn usages(&self, component: &Component) -> Vec<Location> {
        let mut out = Vec::new();
        for (path, content) in self.sources.files().filter(|(p, _)| p.ends_with(".vue")) {
            let imports = script_imports(content);
            for tag in template_tags(content).iter().filter(|t| !t.closing) {
                if self
                    .resolve(path, &imports, &tag.name)
                    .is_some_and(|c| key(c) == key(component))
                {
                    out.push(Location::at(path, content, tag.name_start));
                }
            }
        }
        out
    }
}

fn key(c: &Component) -> String {
    format!("{}#{}", c.location.path, c.name)
}

fn script_imports(content: &str) -> Vec<Import> {
    Sfc::parse(content)
        .scripts
        .iter()
        .flat_map(|b| imports::imports(&content[b.start..b.end]))
        .collect()
}

fn template_tags(content: &str) -> Vec<markup::Tag> {
    match Sfc::parse(content).template {
        Some(t) => scan_tags(content, t.start, t.end, false),
        None => Vec::new(),
    }
}

/// `:title`, `v-bind:title`, `v-model:title` -> prop `title`; `@select`, `v-on:select` ->
/// emit `select`. Modifiers (`.prevent`) are dropped.
fn attribute_target(attr: &str) -> Option<(bool, String)> {
    let (is_event, name) = if let Some(n) = attr.strip_prefix('@').or(attr.strip_prefix("v-on:")) {
        (true, n)
    } else if let Some(n) = attr
        .strip_prefix(':')
        .or(attr.strip_prefix("v-bind:"))
        .or(attr.strip_prefix("v-model:"))
    {
        (false, n)
    } else if attr == "v-model" {
        (false, "modelValue")
    } else if attr.starts_with("v-") || attr.starts_with('#') {
        return None;
    } else {
        (false, attr)
    };
    let name = name.split('.').next().unwrap_or(name);
    (!name.is_empty()).then(|| (is_event, name.to_string()))
}

impl FrameworkIndex for VueIndex<'_> {
    fn components(&self) -> Vec<Component> {
        self.components.clone()
    }

    fn goto(&self, path: &str, content: &str, offset: usize) -> Vec<Location> {
        if !path.ends_with(".vue") {
            return Vec::new();
        }
        let byte = utf16_to_byte(content, offset);
        let imports = script_imports(content);
        let tags = template_tags(content);
        match markup::hit(&tags, byte) {
            Some(TagHit::Name(tag)) => self
                .resolve(path, &imports, &tag.name)
                .map(|c| vec![c.location.clone()])
                .unwrap_or_default(),
            Some(TagHit::Attr(tag, attr)) => {
                let Some(c) = self.resolve(path, &imports, &tag.name) else {
                    return Vec::new();
                };
                let Some((is_event, name)) = attribute_target(&attr.name) else {
                    return Vec::new();
                };
                let list = if is_event { &c.events } else { &c.props };
                let camel = camel_case(&name);
                list.iter()
                    .find(|m| m.name == name || m.name == camel || kebab_case(&m.name) == name)
                    .map(|m| vec![m.location.clone()])
                    .unwrap_or_default()
            }
            None => {
                // On the `<template>` / `<script>` tag of a component: its usages.
                let sfc = Sfc::parse(content);
                let on_block = sfc
                    .template
                    .iter()
                    .chain(sfc.main_script())
                    .any(|b| byte > b.tag_start && byte <= b.tag_start + 9);
                if !on_block {
                    return Vec::new();
                }
                self.by_path
                    .get(path)
                    .and_then(|v| v.first())
                    .map(|&i| self.usages(&self.components[i]))
                    .unwrap_or_default()
            }
        }
    }

    fn gutter(&self, _path: &str, _content: &str) -> Vec<GutterMark> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

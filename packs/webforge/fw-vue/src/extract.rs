//! Extract Component for Vue single-file components: the selected elements of the
//! `<template>` become a new `.vue` component with a prop for every name they take from the
//! parent (`{{ total }}`, `:user="user"`, `@click="save"`, the list of a `v-for` outside the
//! selection); names bound inside (`v-for` aliases, slot props) are not props. Components and
//! helpers the parent's `<script setup>` imports are imported by the new component instead.
//! The parent uses `<NewComponent :total="total" />` and imports (and with the Options API
//! registers) it.

use webforge_lang_web::extract::{
    after_imports, check_component_name, default_import, import_line, import_style, indent_unit,
    line_indent, rebase_spec, reindent, relative_spec, trimmed, ExtractComponentInfo,
    ExtractedComponent, ImportStyle, Replacement,
};
use webforge_lang_web::imports::{self, Import};
use webforge_lang_web::scope::{scan_code, Names};
use webforge_lang_web::text::{camel_case, pascal_case};

use crate::Sfc;

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

const STYLE: ImportStyle = ImportStyle {
    quote: '\'',
    semicolon: false,
};

/// Every identifier of a pattern (`(item, index)`, `{ row }`) is bound.
fn bind_pattern(text: &str, names: &mut Names) {
    let mut i = 0;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap_or(' ');
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let e = text[i..]
                .char_indices()
                .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '$'))
                .map_or(text.len(), |(k, _)| i + k);
            names.bound.insert(text[i..e].to_string());
            i = e;
            continue;
        }
        i += ch.len_utf8();
    }
}

/// The value range of a quoted attribute value starting at `i` (at the quote).
fn quoted(text: &str, i: usize, end: usize) -> Option<(usize, usize)> {
    let q = *text.as_bytes().get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let close = text[i + 1..end].find(q as char)? + i + 1;
    Some((i + 1, close))
}

/// Scan the template element at `lt`: returns the offset after it.
fn element(text: &str, lt: usize, end: usize, names: &mut Names) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = lt + 1;
    let name_start = i;
    while i < end
        && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_' | b':' | b'.'))
    {
        i += 1;
    }
    let name = &text[name_start..i];
    if name.is_empty() {
        return None;
    }
    if name.contains('-') || name.starts_with(|c: char| c.is_ascii_uppercase()) {
        let component = pascal_case(name);
        if !names.components.contains(&component) {
            names.components.push(component);
        }
    }
    loop {
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            return None;
        }
        if text[i..].starts_with("/>") {
            return Some(i + 2);
        }
        if bytes[i] == b'>' {
            i += 1;
            break;
        }
        let attr_start = i;
        while i < end
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>')
            && !text[i..].starts_with("/>")
        {
            i += 1;
        }
        let attr = &text[attr_start..i];
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        let Some((vs, ve)) = quoted(text, i, end) else {
            while i < end && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                i += 1;
            }
            continue;
        };
        i = ve + 1;
        let value = &text[vs..ve];
        if attr == "v-for" {
            let split = [" in ", " of "]
                .iter()
                .filter_map(|kw| value.find(kw).map(|k| (k, kw.len())))
                .min();
            if let Some((k, len)) = split {
                bind_pattern(&value[..k], names);
                scan_code(text, vs + k + len, ve, false, names);
            }
        } else if attr.starts_with('#') || attr.starts_with("v-slot") || attr == "slot-scope" {
            bind_pattern(value, names);
        } else if attr.starts_with(':') || attr.starts_with('@') || attr.starts_with("v-") {
            scan_code(text, vs, ve, false, names);
        }
    }
    if VOID.contains(&name.to_ascii_lowercase().as_str()) {
        return Some(i);
    }
    loop {
        if i >= end {
            return None;
        }
        if text[i..].starts_with("<!--") {
            i = text[i..end].find("-->").map(|e| i + e + 3)?;
        } else if text[i..].starts_with("</") {
            let close = text[i..end].find('>')? + i;
            if !text[i + 2..close].trim().eq_ignore_ascii_case(name) {
                return None;
            }
            return Some(close + 1);
        } else if bytes[i] == b'<' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic()) {
            i = element(text, i, end, names)?;
        } else if text[i..].starts_with("{{") {
            let close = text[i + 2..end].find("}}")? + i + 2;
            scan_code(text, i + 2, close, false, names);
            i = close + 2;
        } else {
            i += text[i..].chars().next()?.len_utf8();
        }
    }
}

/// The selection as whole template elements: how many, and what they use.
struct Analysis {
    start: usize,
    end: usize,
    roots: usize,
    props: Vec<String>,
    /// The parent's imports the new component takes over.
    imports: Vec<Import>,
    sfc: Sfc,
}

fn analyse(content: &str, start: usize, end: usize) -> Result<Analysis, String> {
    let sfc = Sfc::parse(content);
    let template = sfc.template.clone().ok_or("The file has no <template>")?;
    let (start, end) = trimmed(content, start.min(end), start.max(end));
    if start < template.start || end > template.end || start >= end {
        return Err("Select elements of the <template> to extract".into());
    }
    let mut names = Names::default();
    let mut roots = 0;
    let mut i = start;
    while i < end {
        let b = content.as_bytes()[i];
        if b.is_ascii_whitespace() {
            i += 1;
        } else if content[i..].starts_with("<!--") {
            i = content[i..end].find("-->").map_or(end, |e| i + e + 3);
        } else if b == b'<' {
            i = element(content, i, end, &mut names)
                .ok_or("The selection should be whole elements of the template")?;
            roots += 1;
        } else {
            return Err("The selection should be whole elements of the template".into());
        }
    }
    if roots == 0 {
        return Err("Select elements of the <template> to extract".into());
    }
    let script = sfc.main_script().filter(|s| s.setup).cloned();
    let parent_imports: Vec<Import> = script
        .as_ref()
        .map(|s| imports::imports(&content[s.start..s.end]))
        .unwrap_or_default();
    let mut imports_used: Vec<Import> = Vec::new();
    let mut props = Vec::new();
    for name in names.free() {
        if name.starts_with('$') {
            continue;
        }
        if let Some(i) = parent_imports.iter().find(|i| i.local == name) {
            imports_used.push(i.clone());
        } else if !names.components.contains(&name) {
            props.push(name);
        }
    }
    for c in &names.components {
        if let Some(i) = parent_imports.iter().find(|i| i.local == *c) {
            if !imports_used.iter().any(|u| u.local == i.local) {
                imports_used.push(i.clone());
            }
        }
    }
    Ok(Analysis {
        start,
        end,
        roots,
        props,
        imports: imports_used,
        sfc,
    })
}

/// Extract Component's data for the selection `start..end` of a `.vue` file.
pub fn prepare(content: &str, start: usize, end: usize) -> Result<ExtractComponentInfo, String> {
    let a = analyse(content, start, end)?;
    let _ = a.roots;
    Ok(ExtractComponentInfo {
        framework: "vue".into(),
        name: "NewComponent".into(),
        props: a.props,
        imports: a.imports.iter().map(|i| i.local.clone()).collect(),
        extension: "vue".into(),
        start: a.start,
        end: a.end,
    })
}

/// Extract the selection of `path` into the component `name` at `new_path`.
pub fn extract(
    path: &str,
    content: &str,
    start: usize,
    end: usize,
    name: &str,
    new_path: &str,
) -> Result<ExtractedComponent, String> {
    check_component_name(name)?;
    let a = analyse(content, start, end)?;
    let script = a.sfc.main_script().cloned();
    if let Some(s) = &script {
        let taken = imports::imports(&content[s.start..s.end])
            .into_iter()
            .any(|i| i.local == name);
        if taken {
            return Err(format!("{name} is already defined in this file"));
        }
    }
    let style = script
        .as_ref()
        .map_or(STYLE, |s| import_style(&content[s.start..s.end], STYLE));
    let unit = indent_unit(content);
    // The new component.
    let markup = reindent(
        &content[a.start..a.end],
        &line_indent(content, a.start),
        &unit,
    );
    let mut text = format!("<template>\n{markup}\n</template>\n");
    let lines: Vec<String> = a
        .imports
        .iter()
        .map(|i| import_line(i, &rebase_spec(&i.source, path, new_path), style))
        .collect();
    let props_list = a
        .props
        .iter()
        .map(|p| format!("{q}{p}{q}", q = style.quote))
        .collect::<Vec<_>>()
        .join(", ");
    let semi = if style.semicolon { ";" } else { "" };
    let lang = script
        .as_ref()
        .and_then(|s| s.lang.clone())
        .map(|l| format!(" lang=\"{l}\""))
        .unwrap_or_default();
    let setup = script.as_ref().is_none_or(|s| s.setup);
    if setup && (!lines.is_empty() || !a.props.is_empty()) {
        let mut body = lines.join("\n");
        if !a.props.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str(&format!("defineProps([{props_list}]){semi}"));
        }
        text.push_str(&format!("\n<script setup{lang}>\n{body}\n</script>\n"));
    } else if !setup {
        let mut body = String::new();
        for l in &lines {
            body.push_str(l);
            body.push('\n');
        }
        if !lines.is_empty() {
            body.push('\n');
        }
        body.push_str("export default {\n");
        body.push_str(&format!("{unit}name: {q}{name}{q},\n", q = style.quote));
        let components: Vec<&str> = a
            .imports
            .iter()
            .filter(|i| i.local.starts_with(|c: char| c.is_ascii_uppercase()))
            .map(|i| i.local.as_str())
            .collect();
        if !components.is_empty() {
            body.push_str(&format!(
                "{unit}components: {{ {} }},\n",
                components.join(", ")
            ));
        }
        if !a.props.is_empty() {
            body.push_str(&format!("{unit}props: [{props_list}],\n"));
        }
        body.push_str(&format!("}}{semi}"));
        text.push_str(&format!("\n<script{lang}>\n{body}\n</script>\n"));
    }

    // The parent: the tag, the import (and the registration).
    let attrs: String = a
        .props
        .iter()
        .map(|p| format!(" :{}=\"{p}\"", camel_case(p)))
        .collect();
    let mut edits = vec![Replacement {
        start: a.start,
        end: a.end,
        text: format!("<{name}{attrs} />"),
    }];
    let spec = relative_spec(path, new_path);
    let import = default_import(name, &spec, style);
    match &script {
        Some(s) => {
            let at = after_imports(content, s.start, s.end);
            let edit = match at {
                Some(at) => Replacement {
                    start: at,
                    end: at,
                    text: format!("\n{import}"),
                },
                None => {
                    let at = if content[s.start..].starts_with('\n') {
                        s.start + 1
                    } else {
                        s.start
                    };
                    Replacement {
                        start: at,
                        end: at,
                        text: format!("{import}\n{}", if s.setup { "\n" } else { "" }),
                    }
                }
            };
            edits.push(edit);
            if !s.setup {
                edits.extend(register(content, s.start, s.end, name, &unit));
            }
        }
        None => {
            let at = a.sfc.template.as_ref().map_or(0, |t| t.tag_start);
            edits.push(Replacement {
                start: at,
                end: at,
                text: format!("<script setup>\n{import}\n</script>\n\n"),
            });
        }
    }
    Ok(ExtractedComponent {
        path: new_path.to_string(),
        text,
        edits,
    })
}

/// Options API: `components: { Name }` in the exported object.
fn register(
    content: &str,
    start: usize,
    end: usize,
    name: &str,
    unit: &str,
) -> Option<Replacement> {
    let code = &content[start..end];
    if let Some(k) = code.find("components:") {
        let open = code[k..].find('{')? + k + start;
        return Some(Replacement {
            start: open + 1,
            end: open + 1,
            text: format!(" {name},"),
        });
    }
    let k = code
        .find("export default {")
        .map(|k| k + "export default {".len())
        .or_else(|| {
            code.find("defineComponent({")
                .map(|k| k + "defineComponent({".len())
        })?;
    let at = start + k;
    Some(Replacement {
        start: at,
        end: at,
        text: format!("\n{unit}components: {{ {name} }},"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edits: &[Replacement]) -> String {
        let mut sorted: Vec<&Replacement> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut out = text.to_string();
        for e in sorted {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    const PARENT: &str = "<template>\n  <section>\n    <h1>{{ title }}</h1>\n    <ul>\n      <li v-for=\"item in items\" :key=\"item.id\">\n        <PriceTag :value=\"item.price * rate\" @click=\"select(item)\" />\n        <span v-if=\"$slots.extra\">{{ formatDate(item.at) }}</span>\n      </li>\n    </ul>\n  </section>\n</template>\n\n<script setup lang=\"ts\">\nimport PriceTag from './PriceTag.vue'\nimport { formatDate } from '../util/date'\n\nconst props = defineProps<{ title: string; items: Item[]; rate: number }>()\n</script>\n";

    #[test]
    fn extracts_template_elements_with_props() {
        let start = PARENT.find("<ul>").unwrap();
        let end = PARENT.find("</ul>").unwrap() + 5;
        let info = prepare(PARENT, start, end).unwrap();
        assert_eq!(info.props, ["items", "rate", "select"]);
        assert_eq!(info.imports, ["formatDate", "PriceTag"]);
        let out = extract(
            "src/components/Cart.vue",
            PARENT,
            start,
            end,
            "CartItems",
            "src/components/parts/CartItems.vue",
        )
        .unwrap();
        assert_eq!(
            out.text,
            "<template>\n  <ul>\n    <li v-for=\"item in items\" :key=\"item.id\">\n      <PriceTag :value=\"item.price * rate\" @click=\"select(item)\" />\n      <span v-if=\"$slots.extra\">{{ formatDate(item.at) }}</span>\n    </li>\n  </ul>\n</template>\n\n<script setup lang=\"ts\">\nimport { formatDate } from '../../util/date'\nimport PriceTag from '../PriceTag.vue'\n\ndefineProps(['items', 'rate', 'select'])\n</script>\n"
        );
        let parent = apply(PARENT, &out.edits);
        assert!(parent.contains("    <h1>{{ title }}</h1>\n    <CartItems :items=\"items\" :rate=\"rate\" :select=\"select\" />\n  </section>"));
        assert!(parent.contains("import { formatDate } from '../util/date'\nimport CartItems from './parts/CartItems.vue'\n"));
    }

    #[test]
    fn options_api_and_files_without_script() {
        let src = "<template>\n  <div>\n    <p class=\"x\">{{ count }}</p>\n  </div>\n</template>\n\n<script>\nexport default {\n  data() { return { count: 1 } },\n}\n</script>\n";
        let start = src.find("<p").unwrap();
        let end = src.find("</p>").unwrap() + 4;
        let out = extract("src/App.vue", src, start, end, "Counter", "src/Counter.vue").unwrap();
        assert!(
            out.text.contains(
                "<script>\nexport default {\n  name: 'Counter',\n  props: ['count'],\n}\n</script>"
            ),
            "{}",
            out.text
        );
        let parent = apply(src, &out.edits);
        assert!(parent.contains("<script>\nimport Counter from './Counter.vue'\nexport default {\n  components: { Counter },\n  data()"), "{parent}");
        let bare = "<template>\n  <b>hi</b>\n</template>\n";
        let out = extract("App.vue", bare, 13, 22, "Hi", "Hi.vue").unwrap();
        assert_eq!(out.text, "<template>\n  <b>hi</b>\n</template>\n");
        assert!(apply(bare, &out.edits).starts_with(
            "<script setup>\nimport Hi from './Hi.vue'\n</script>\n\n<template>\n  <Hi />"
        ));
    }

    #[test]
    fn refuses_partial_selections() {
        let start = PARENT.find("<h1>").unwrap();
        assert!(prepare(PARENT, start, start + 6).is_err());
        assert!(prepare(PARENT, PARENT.find("<script").unwrap(), PARENT.len()).is_err());
        let end = PARENT.find("</h1>").unwrap() + 5;
        assert!(extract("a.vue", PARENT, start, end, "lower", "b.vue").is_err());
        assert!(extract("a.vue", PARENT, start, end, "PriceTag", "b.vue").is_err());
    }
}

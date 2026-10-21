//! Expansion: by abbreviation (Tab), from the Insert Live Template list (Ctrl+J) or completion,
//! and around a selection (Surround With, Ctrl+Alt+T / Ctrl+Alt+J).
//!
//! The result is the text to insert with its tab stops: one [`Field`] per occurrence of a
//! variable (occurrences of the same variable share an `index` and are edited together) and
//! the final caret position (`$END$`, or the end of the text).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::context::{analyze, applies, CaretInfo};
use crate::expr::{self, EvalEnv};
use crate::sets::{active_templates, Template, TemplateSettings, Variable};
use crate::surround::{surrounders, Surrounder};
use crate::template::{self, Segment, END, SELECTION};
use crate::text::{
    byte_of, dedent, line_end, line_indent, line_start, normalize_indent, utf16_len, utf16_of,
};

/// What expressions may need from outside: clock, clipboard, user and the file's indent unit.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Env {
    #[serde(default)]
    pub now_ms: f64,
    #[serde(default)]
    pub tz_offset_minutes: i32,
    #[serde(default)]
    pub clipboard: String,
    #[serde(default)]
    pub user: String,
    #[serde(default = "default_indent")]
    pub indent_unit: String,
}

fn default_indent() -> String {
    "    ".into()
}

impl Default for Env {
    fn default() -> Self {
        Env {
            now_ms: 0.0,
            tz_offset_minutes: 0,
            clipboard: String::new(),
            user: String::new(),
            indent_unit: default_indent(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandRequest {
    pub text: String,
    /// Caret (UTF-16).
    pub offset: usize,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub settings: TemplateSettings,
    /// Expand this template (Insert Live Template, completion); otherwise the abbreviation
    /// before the caret.
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub env: Env,
}

/// A tab stop (UTF-16 offsets into [`Expansion::text`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub from: usize,
    pub to: usize,
    /// Order of the stop, from 1; every occurrence of a variable has the same index.
    pub index: usize,
}

/// Replace `from..to` of the document (UTF-16) with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub fields: Vec<Field>,
    /// Final caret position in `text`.
    pub end: usize,
    pub template_id: String,
}

/// Rendered text with byte positions.
struct Rendered {
    text: String,
    fields: Vec<(usize, usize, usize)>,
    end: Option<usize>,
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Variables in stop order: declared ones (that occur in the text) first, then the rest.
fn ordered_variables(text: &str, declared: &[Variable]) -> Vec<Variable> {
    let names = template::variable_names(text);
    let mut out: Vec<Variable> = declared
        .iter()
        .filter(|v| names.contains(&v.name))
        .cloned()
        .collect();
    for name in names {
        if !out.iter().any(|v| v.name == name) {
            out.push(Variable {
                name,
                expression: String::new(),
                default_value: String::new(),
                always_stop_at: true,
            });
        }
    }
    out
}

fn evaluate(text: &str, env: &EvalEnv, vars: &HashMap<String, String>) -> String {
    expr::parse(text).map_or_else(String::new, |e| expr::eval(&e, env, vars))
}

/// Re-indent a (dedented) multi-line insertion for a position whose line has `indent`.
fn reindent(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.is_empty() {
                l.clone()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(
    t: &Template,
    caret: &CaretInfo,
    env: &Env,
    path: &str,
    selection: Option<&str>,
) -> Rendered {
    let eval_env = EvalEnv {
        caret: Some(caret),
        file_name: file_name(path),
        now_ms: env.now_ms,
        tz_offset_minutes: env.tz_offset_minutes,
        clipboard: &env.clipboard,
        user: &env.user,
    };
    let variables = ordered_variables(&t.text, &t.variables);
    let mut values: HashMap<String, String> = HashMap::new();
    let mut stops: HashMap<String, usize> = HashMap::new();
    for v in &variables {
        let mut value = evaluate(&v.expression, &eval_env, &values);
        if value.is_empty() && !v.default_value.is_empty() {
            value = match expr::parse(&v.default_value) {
                Some(e) => expr::eval(&e, &eval_env, &values),
                None => v.default_value.clone(),
            };
        }
        if v.always_stop_at || value.is_empty() {
            stops.insert(v.name.clone(), stops.len() + 1);
        }
        values.insert(v.name.clone(), value);
    }
    let selection_lines = selection.map(|s| dedent(&s.split('\n').collect::<Vec<_>>()));
    let mut out = Rendered {
        text: String::new(),
        fields: Vec::new(),
        end: None,
    };
    for seg in template::parse(&normalize_indent(&t.text, &env.indent_unit)) {
        match seg {
            Segment::Text(s) => out.text.push_str(&s),
            Segment::Var(name) if name == END => out.end = Some(out.text.len()),
            Segment::Var(name) if name == SELECTION => {
                if let Some(lines) = &selection_lines {
                    let indent = line_indent(&out.text, out.text.len()).to_string();
                    out.text.push_str(&reindent(lines, &indent));
                }
            }
            Segment::Var(name) => {
                let value = values.get(&name).cloned().unwrap_or_default();
                let from = out.text.len();
                out.text.push_str(&value);
                if let Some(&index) = stops.get(&name) {
                    out.fields.push((from, out.text.len(), index));
                }
            }
        }
    }
    out
}

fn finish(r: Rendered, from: usize, to: usize, template_id: &str) -> Expansion {
    let text = r.text;
    Expansion {
        from,
        to,
        fields: r
            .fields
            .iter()
            .map(|&(a, b, index)| Field {
                from: utf16_of(&text, a),
                to: utf16_of(&text, b),
                index,
            })
            .collect(),
        end: r
            .end
            .map_or_else(|| utf16_len(&text), |e| utf16_of(&text, e)),
        template_id: template_id.to_string(),
        text,
    }
}

/// Whether `before[start..]` can be an abbreviation (not part of a longer word, a variable
/// or a member access).
fn at_word_start(before: &str, start: usize) -> bool {
    let head = &before[..start];
    let Some(prev) = head.chars().next_back() else {
        return true;
    };
    !(prev.is_alphanumeric()
        || prev == '_'
        || prev == '$'
        || prev == '.'
        || head.ends_with("->")
        || head.ends_with("::"))
}

/// The abbreviation-like word before the caret (byte start).
fn word_start(before: &str) -> usize {
    before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(before.len(), |(i, _)| i)
}

/// Expand the abbreviation before the caret or the requested template. `None` when nothing
/// applies (the key then keeps its usual meaning).
pub fn expand(req: &ExpandRequest) -> Option<Expansion> {
    let text = req.text.as_str();
    let caret = byte_of(text, req.offset);
    let before = &text[..caret];
    let templates = active_templates(&req.settings);
    let lang = req.language_id.as_deref();
    match &req.template_id {
        Some(id) => {
            let t = templates.iter().find(|t| &t.id == id)?;
            let ws = word_start(before);
            let prefix = &before[ws..];
            let start = if !prefix.is_empty() && t.abbreviation.starts_with(prefix) {
                ws
            } else {
                caret
            };
            let info = analyze(text, start, lang, &req.path);
            let r = render(t, &info, &req.env, &req.path, None);
            Some(finish(r, utf16_of(text, start), req.offset, &t.id))
        }
        None => {
            let mut candidates: Vec<&Template> = templates
                .iter()
                .filter(|t| !t.abbreviation.is_empty() && before.ends_with(&t.abbreviation))
                .filter(|t| at_word_start(before, caret - t.abbreviation.len()))
                .collect();
            candidates.sort_by_key(|t| std::cmp::Reverse(t.abbreviation.len()));
            candidates.into_iter().find_map(|t| {
                let start = caret - t.abbreviation.len();
                let info = analyze(text, start, lang, &req.path);
                applies(&t.contexts, &info.contexts).then(|| {
                    let r = render(t, &info, &req.env, &req.path, None);
                    finish(r, utf16_of(text, start), req.offset, &t.id)
                })
            })
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicableRequest {
    pub text: String,
    pub offset: usize,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub settings: TemplateSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicableItem {
    pub id: String,
    pub abbreviation: String,
    pub description: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicableResult {
    /// The word before the caret, which the list is filtered by.
    pub prefix: String,
    pub items: Vec<ApplicableItem>,
}

/// Templates applicable at the caret (Insert Live Template, completion).
pub fn applicable(req: &ApplicableRequest) -> ApplicableResult {
    let text = req.text.as_str();
    let caret = byte_of(text, req.offset);
    let ws = word_start(&text[..caret]);
    let start = if at_word_start(&text[..caret], ws) {
        ws
    } else {
        caret
    };
    let info = analyze(text, start, req.language_id.as_deref(), &req.path);
    let mut items: Vec<ApplicableItem> = active_templates(&req.settings)
        .into_iter()
        .filter(|t| applies(&t.contexts, &info.contexts))
        .map(|t| ApplicableItem {
            id: t.id,
            abbreviation: t.abbreviation,
            description: t.description,
            group: t.group,
        })
        .collect();
    items.sort_by_key(|a| a.abbreviation.to_lowercase());
    ApplicableResult {
        prefix: text[start..caret].to_string(),
        items,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurroundRequest {
    pub text: String,
    /// Selection (UTF-16); an empty selection surrounds the caret line.
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub settings: TemplateSettings,
    #[serde(default)]
    pub env: Env,
    /// Item to apply (`surround_apply`).
    #[serde(default)]
    pub id: Option<String>,
    /// Only live templates (Surround with Live Template, Ctrl+Alt+J).
    #[serde(default)]
    pub templates_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurroundItem {
    pub id: String,
    pub label: String,
    /// Key that picks the item (`1`…`9`, `0`, `A`…).
    pub mnemonic: String,
    /// `surrounder` or `template`.
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

const MNEMONICS: &str = "1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Byte range of the lines the selection touches, without their indentation and trailing
/// whitespace (statement surrounders work on whole lines).
fn line_range(text: &str, from: usize, to: usize) -> (usize, usize) {
    let mut to = to;
    if to > from && text[..to].ends_with('\n') {
        to -= 1;
    }
    let start = line_start(text, from);
    let start = start + (text[start..].len() - text[start..].trim_start_matches([' ', '\t']).len());
    let end = line_end(text, to);
    let end = start.max(end - (text[..end].len() - text[..end].trim_end().len()).min(end - start));
    (start, end)
}

struct SurroundTarget {
    /// Range replaced by statement surrounders (whole lines) and the others (the selection).
    lines: (usize, usize),
    exact: (usize, usize),
    info: CaretInfo,
}

fn target(req: &SurroundRequest) -> SurroundTarget {
    let text = req.text.as_str();
    let (a, b) = (
        byte_of(text, req.from.min(req.to)),
        byte_of(text, req.from.max(req.to)),
    );
    let lines = line_range(text, a, b);
    let exact = if a == b { lines } else { (a, b) };
    let info = analyze(text, lines.0, req.language_id.as_deref(), &req.path);
    SurroundTarget { lines, exact, info }
}

fn selection_templates(req: &SurroundRequest, info: &CaretInfo) -> Vec<Template> {
    active_templates(&req.settings)
        .into_iter()
        .filter(|t| template::uses_selection(&t.text) && applies(&t.contexts, &info.contexts))
        .collect()
}

fn available(req: &SurroundRequest, info: &CaretInfo) -> (Vec<Surrounder>, Vec<Template>) {
    let s = if req.templates_only {
        Vec::new()
    } else {
        surrounders(&info.contexts)
    };
    let mut t = selection_templates(req, info);
    t.sort_by_key(|a| a.abbreviation.to_lowercase());
    (s, t)
}

/// The Surround With popup: code surrounders, then live templates with `$SELECTION$`.
pub fn surround_items(req: &SurroundRequest) -> Vec<SurroundItem> {
    let t = target(req);
    let (surrounders, templates) = available(req, &t.info);
    let mut mnemonics = MNEMONICS.chars();
    let mut items: Vec<SurroundItem> = surrounders
        .iter()
        .map(|s| SurroundItem {
            id: s.id.to_string(),
            label: s.label.to_string(),
            mnemonic: mnemonics.next().map(String::from).unwrap_or_default(),
            kind: "surrounder".into(),
            description: String::new(),
        })
        .collect();
    items.extend(templates.into_iter().map(|t| SurroundItem {
        mnemonic: mnemonics.next().map(String::from).unwrap_or_default(),
        label: t.abbreviation,
        id: t.id,
        kind: "template".into(),
        description: t.description,
    }));
    items
}

/// Surround the selection with the item `req.id`.
pub fn surround_apply(req: &SurroundRequest) -> Option<Expansion> {
    let id = req.id.as_deref()?;
    let text = req.text.as_str();
    let target = target(req);
    let (surrounders, templates) = available(req, &target.info);
    let (template, range) = if let Some(s) = surrounders.iter().find(|s| s.id == id) {
        let t = Template {
            id: s.id.to_string(),
            group: String::new(),
            abbreviation: String::new(),
            description: String::new(),
            text: s.text.to_string(),
            variables: s.variables(),
            contexts: Vec::new(),
            applicable: String::new(),
            enabled: true,
            builtin: None,
            modified: false,
        };
        (
            t,
            if s.expression {
                target.exact
            } else {
                target.lines
            },
        )
    } else {
        (templates.into_iter().find(|t| t.id == id)?, target.exact)
    };
    let (a, b) = range;
    // Whole lines keep the first line's indentation so the selection dedents correctly.
    let selection = format!("{}{}", line_indent(text, a), &text[a..b]);
    let selection = if a == line_start(text, a) + line_indent(text, a).len() {
        selection
    } else {
        text[a..b].to_string()
    };
    let r = render(
        &template,
        &target.info,
        &req.env,
        &req.path,
        Some(&selection),
    );
    Some(finish(r, utf16_of(text, a), utf16_of(text, b), id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req(src: &str, lang: &str) -> ExpandRequest {
        let offset = src.find('|').unwrap();
        ExpandRequest {
            text: src.replacen('|', "", 1),
            offset,
            path: format!("src/Demo.{lang}"),
            language_id: Some(lang.into()),
            ..ExpandRequest::default()
        }
    }

    /// Apply an expansion and mark fields as `[n:value]` and the end as `|`.
    fn show(r: &ExpandRequest, e: &Expansion) -> String {
        let mut text: Vec<u16> = e.text.encode_utf16().collect();
        let mut marks: Vec<(usize, String)> = vec![(e.end, "|".into())];
        for f in &e.fields {
            marks.push((f.to, "]".into()));
            marks.push((f.from, format!("[{}:", f.index)));
        }
        // Inserted back to front; at one position "[n:" ends up before "]" before "|".
        let rank = |m: &str| match m.as_bytes()[0] {
            b'|' => 0,
            b']' => 1,
            _ => 2,
        };
        marks.sort_by(|a, b| b.0.cmp(&a.0).then(rank(&a.1).cmp(&rank(&b.1))));
        for (pos, m) in marks {
            text.splice(pos..pos, m.encode_utf16());
        }
        apply(&r.text, e.from, e.to, &String::from_utf16(&text).unwrap())
    }

    /// Insert like CodeMirror's snippets: lines after the first get the start line's indentation.
    fn apply(doc: &str, from: usize, to: usize, text: &str) -> String {
        let (a, b) = (byte_of(doc, from), byte_of(doc, to));
        let indent = line_indent(doc, a);
        let text = text.replace('\n', &format!("\n{indent}"));
        format!("{}{}{}", &doc[..a], text, &doc[b..])
    }

    #[test]
    fn expands_php_foreach_with_suggested_names() {
        let r = req(
            "<?php\nfunction f() {\n    $orders = [];\n    fore|\n}",
            "php",
        );
        let e = expand(&r).unwrap();
        assert_eq!(e.template_id, "PHP/fore");
        assert_eq!(
            show(&r, &e),
            "<?php\nfunction f() {\n    $orders = [];\n    foreach ([1:$orders] as $[2:order]) {\n        |\n    }\n}"
        );
    }

    #[test]
    fn honours_contexts_and_word_boundaries() {
        assert!(expand(&req("<?php class A {\n    fore|\n}", "php")).is_none());
        assert_eq!(
            expand(&req("<?php class A {\n    pubf|\n}", "php"))
                .unwrap()
                .template_id,
            "PHP/pubf"
        );
        assert!(expand(&req("<?php function f() { pubf| }", "php")).is_none());
        assert!(expand(&req("<?php $x->fore|", "php")).is_none());
        assert!(expand(&req("<?php $prefore|", "php")).is_none());
        assert!(expand(&req("<?php // fore|", "php")).is_none());
        assert!(expand(&req("<p>fore|</p>", "php")).is_none());
        assert!(expand(&req("unknown|", "php")).is_none());
        assert_eq!(
            expand(&req("<?php $f = __c|", "php")).unwrap().template_id,
            "PHP/__c"
        );
        assert_eq!(
            expand(&req("const a = 1;\nlog|", "typescript"))
                .unwrap()
                .template_id,
            "JavaScript/log"
        );
        assert_eq!(
            expand(&req("forp|", "lua")).unwrap().template_id,
            "Lua/forp"
        );
    }

    #[test]
    fn framework_sets_need_the_framework() {
        let r = req("<?php\nrget|", "php");
        assert!(expand(&r).is_none());
        let mut r = r;
        r.settings.frameworks = vec!["laravel".into()];
        let e = expand(&r).unwrap();
        assert_eq!(
            show(&r, &e),
            "<?php\nRoute::get('/[1:]', [[2:]::class, '[3:]'])->name('[4:]');|"
        );
        let b = ExpandRequest {
            settings: r.settings.clone(),
            ..req("<div>\n    bif|\n</div>", "blade")
        };
        assert_eq!(
            show(&b, &expand(&b).unwrap()),
            "<div>\n    @if ([1:])\n        |\n    @endif\n</div>"
        );
    }

    #[test]
    fn evaluates_skip_if_defined_variables_and_indent_units() {
        let mut r = req("local x = 1\nreq|", "lua");
        r.env.indent_unit = "\t".into();
        let e = expand(&r).unwrap();
        // NAME depends on MODULE, which is still empty, so it stays a stop.
        assert_eq!(show(&r, &e), "local x = 1\nlocal [2:] = require(\"[1:]\")|");
        let mut r = req("fun|", "lua");
        r.env.indent_unit = "\t".into();
        assert_eq!(
            show(&r, &expand(&r).unwrap()),
            "function [1:]([2:])\n\t|\nend"
        );
    }

    #[test]
    fn expands_user_templates_with_expressions() {
        let entry: BTreeMap<String, String> = [
            ("abbreviation", "hdr"),
            (
                "text",
                "// $FILE$ by $USER$ in $CLASS$ ($$$NAME$)$END$ $DATE$",
            ),
            ("contexts", "PHP"),
            (
                "variables",
                r#"[{"name":"NAME","expression":"camelCase(\"some-name\")"},
                    {"name":"FILE","expression":"fileName()","alwaysStopAt":false},
                    {"name":"USER","expression":"user()","alwaysStopAt":false},
                    {"name":"CLASS","expression":"className()","alwaysStopAt":false},
                    {"name":"DATE","expression":"date(\"yyyy\")","alwaysStopAt":false}]"#,
            ),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let mut r = req("<?php class Cart {\n    hdr|\n}", "php");
        r.settings.custom = vec![entry];
        r.env.user = "marlon".into();
        r.env.now_ms = 1_792_674_309_000.0;
        let e = expand(&r).unwrap();
        assert_eq!(e.template_id, "user/hdr");
        assert_eq!(
            show(&r, &e),
            "<?php class Cart {\n    // Demo.php by marlon in Cart ($[1:someName])| 2026\n}"
        );
    }

    #[test]
    fn expands_a_chosen_template_replacing_the_typed_prefix() {
        let mut r = req("<?php\nfo|", "php");
        r.template_id = Some("PHP/forek".into());
        let e = expand(&r).unwrap();
        assert_eq!(e.from, 6);
        assert!(
            e.text.starts_with("foreach ($array as $key => $value) {"),
            "{}",
            e.text
        );
        let mut r = req("<?php\nx|", "php");
        r.template_id = Some("PHP/eco".into());
        assert_eq!(expand(&r).unwrap().from, 7);
    }

    #[test]
    fn lists_applicable_templates() {
        let a = |src: &str, lang: &str| {
            let r = req(src, lang);
            applicable(&ApplicableRequest {
                text: r.text,
                offset: r.offset,
                path: r.path,
                language_id: r.language_id,
                settings: TemplateSettings::default(),
            })
        };
        let r = a("<?php\nfo|", "php");
        assert_eq!(r.prefix, "fo");
        assert!(r.items.iter().any(|i| i.abbreviation == "fore"));
        assert!(!r.items.iter().any(|i| i.abbreviation == "pubf"));
        let r = a("<?php class A { | }", "php");
        assert_eq!(r.prefix, "");
        assert!(r.items.iter().any(|i| i.abbreviation == "pubf"));
        assert!(a("x|", "markdown").items.is_empty());
    }

    fn sreq(src: &str, lang: &str) -> SurroundRequest {
        let from = src.find('[').unwrap();
        let to = src.find(']').unwrap() - 1;
        SurroundRequest {
            text: src.replacen('[', "", 1).replacen(']', "", 1),
            from,
            to,
            path: format!("a.{lang}"),
            language_id: Some(lang.into()),
            ..SurroundRequest::default()
        }
    }

    #[test]
    fn surrounds_lines_with_statements() {
        let mut r = sreq(
            "<?php\nfunction f() {\n    [a();\n    if (x) {\n        b();\n    }]\n}",
            "php",
        );
        let items = surround_items(&r);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(&labels[..3], ["if", "if / else", "while"]);
        assert_eq!(items[0].mnemonic, "1");
        r.id = Some("php.if".into());
        let e = surround_apply(&r).unwrap();
        let out = apply(&r.text, e.from, e.to, &e.text);
        assert_eq!(out, "<?php\nfunction f() {\n    if () {\n        a();\n        if (x) {\n            b();\n        }\n    }\n}");
        assert_eq!(e.end, 4);
    }

    #[test]
    fn surrounds_the_caret_line_and_expressions() {
        let mut r = sreq("<?php\n    $a = [1]bc;", "php");
        r.to = r.from;
        r.id = Some("php.try-catch".into());
        let e = surround_apply(&r).unwrap();
        assert_eq!(&r.text[..e.from], "<?php\n    ");
        assert_eq!(
            e.text,
            "try {\n    $a = 1bc;\n} catch (\\Exception $e) {\n    \n}"
        );
        let mut r = sreq("<?php\n$a = [1 + 2];", "php");
        r.id = Some("php.parentheses".into());
        let e = surround_apply(&r).unwrap();
        assert_eq!(e.text, "(1 + 2)");
        assert_eq!((e.from, e.to), (11, 16));
    }

    #[test]
    fn offers_selection_templates_and_tags() {
        let r = sreq("<div>[text]</div>", "html");
        let items = surround_items(&r);
        assert_eq!(items[0].label, "<tag></tag>");
        let mut r = r;
        r.id = Some(items[0].id.clone());
        let e = surround_apply(&r).unwrap();
        assert_eq!(e.text, "<div>text</div>");
        assert_eq!(e.fields.iter().filter(|f| f.index == 1).count(), 2);
        let mut lt = sreq("<?php\n[foo();]", "php");
        lt.templates_only = true;
        lt.settings.custom = vec![[
            ("abbreviation", "tc"),
            ("text", "try { $SELECTION$ } finally {}"),
            ("contexts", "PHP_STATEMENT"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()];
        let items = surround_items(&lt);
        assert_eq!(items.len(), 1);
        assert_eq!(
            (items[0].label.as_str(), items[0].kind.as_str()),
            ("tc", "template")
        );
        lt.id = Some("user/tc".into());
        assert_eq!(
            surround_apply(&lt).unwrap().text,
            "try { foo(); } finally {}"
        );
    }
}

//! Editor features on the hook index: completion of hook names inside `add_action('…')`
//! and friends, Go to Declaration between registrations and invocations (and from a
//! callback to its function), gutter marks and Find Usages of a hook.
//!
//! Offsets are bytes into the current (possibly unsaved) buffer; [`crate::api`] converts
//! from and to UTF-16.

use std::collections::{BTreeMap, HashSet};

use forge_codeintel::usages::{UsageFile, UsageGroup, UsageKind, UsageRow, UsageTree};
use serde::{Deserialize, Serialize};

use crate::core::{core_hook, CORE_HOOKS};
use crate::hooks::{self, hook_function, HookKind, HookRef, HookRole};
use crate::index::HookIndex;
use crate::lexer::{self, TokKind};
use crate::Location;

/// The hook-name string the caret is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookContext {
    pub function: String,
    pub kind: HookKind,
    pub role: HookRole,
    /// Byte range of the string content up to the caret (`from`) and to its end (`to`).
    pub from: usize,
    pub to: usize,
    pub prefix: String,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The first-argument string of a hook function around `offset`. Text based, so an
/// unclosed string or call works while typing.
pub fn hook_context(src: &str, offset: usize) -> Option<HookContext> {
    let offset = offset.min(src.len());
    let line_start = src[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let before = &src[line_start..offset];
    let q = before.rfind(['\'', '"'])?;
    let quote = before[q..].chars().next()?;
    let prefix = &before[q + 1..];
    if prefix.contains(quote) {
        return None;
    }
    let head = src[..line_start + q].trim_end();
    let head = head.strip_suffix('(')?.trim_end();
    let name_start = head
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word(c) || c == '\\')
        .last()
        .map(|(i, _)| i)?;
    let function = &head[name_start..];
    let (kind, role) = hook_function(function)?;
    // Not a method call or a declaration.
    let pre = head[..name_start].trim_end();
    if pre.ends_with("->") || pre.ends_with("::") || pre.ends_with("function") {
        return None;
    }
    let to = offset
        + src[offset..]
            .find(|c: char| c == quote || c == '\n')
            .unwrap_or(src.len() - offset);
    Some(HookContext {
        function: function.trim_start_matches('\\').to_ascii_lowercase(),
        kind,
        role,
        from: line_start + q + 1,
        to,
        prefix: prefix.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: HookKind,
    /// `2 callbacks · fired in post.php`.
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    /// From Forge's list of WordPress core hooks (not found in the project).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub core: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// Start of the text the completion replaces (bytes here, UTF-16 over IPC).
    pub from: usize,
    pub to: usize,
    pub items: Vec<CompletionItem>,
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Hook names for the string at `offset`, the kind of the called function first: for a
/// registration the hooks that fire, for an invocation every known name.
pub fn complete(index: &HookIndex, path: &str, src: &str, offset: usize) -> Option<CompletionList> {
    let ctx = hook_context(src, offset)?;
    let current = hooks::scan(path, src);
    let mut items: Vec<(u8, CompletionItem)> = Vec::new();
    let mut seen = HashSet::new();
    for h in hooks_with(index, path, &current) {
        if h.dynamic || h.name.is_empty() {
            continue;
        }
        // The reference being typed is not a suggestion of its own. Hooks that are only
        // registered rank after the ones that fire.
        if current.iter().any(|r| {
            r.start == ctx.from && r.name == h.name && h.registrations + h.invocations <= 1
        }) {
            continue;
        }
        let rank = u8::from(h.kind != ctx.kind) * 2
            + u8::from(ctx.role != HookRole::Invocation && h.invocations == 0);
        let mut parts = Vec::new();
        if h.registrations > 0 {
            parts.push(plural(h.registrations, "callback", "callbacks"));
        }
        if let Some(loc) = &h.fired_in {
            parts.push(format!("fired in {}", file_name(loc)));
        }
        seen.insert((h.name.clone(), h.kind));
        items.push((
            rank,
            CompletionItem {
                label: h.name.clone(),
                kind: h.kind,
                detail: if parts.is_empty() {
                    h.kind.label().to_string()
                } else {
                    parts.join(" · ")
                },
                info: core_hook(&h.name).map(|(_, s)| s.to_string()),
                core: false,
            },
        ));
    }
    for &(name, kind, summary) in CORE_HOOKS {
        if seen.contains(&(name.to_string(), kind)) {
            continue;
        }
        items.push((
            u8::from(kind != ctx.kind) * 2 + 1,
            CompletionItem {
                label: name.into(),
                kind,
                detail: "WordPress core".into(),
                info: Some(summary.into()),
                core: true,
            },
        ));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    Some(CompletionList {
        from: ctx.from,
        to: ctx.to,
        items: items.into_iter().map(|(_, i)| i).collect(),
    })
}

struct HookCount {
    name: String,
    kind: HookKind,
    dynamic: bool,
    registrations: usize,
    invocations: usize,
    fired_in: Option<String>,
}

fn hooks_with(index: &HookIndex, path: &str, current: &[HookRef]) -> Vec<HookCount> {
    let mut by: BTreeMap<(String, HookKind), HookCount> = BTreeMap::new();
    for r in index.refs_with(path, current) {
        let e = by
            .entry((r.name.clone(), r.kind))
            .or_insert_with(|| HookCount {
                name: r.name.clone(),
                kind: r.kind,
                dynamic: r.dynamic,
                registrations: 0,
                invocations: 0,
                fired_in: None,
            });
        match r.role {
            HookRole::Registration => e.registrations += 1,
            HookRole::Invocation => {
                e.invocations += 1;
                e.fired_in.get_or_insert_with(|| r.path.clone());
            }
            _ => {}
        }
    }
    by.into_values().collect()
}

fn location(r: &HookRef) -> Location {
    Location::new(&r.path, r.line, r.column)
}

/// The reference whose hook name contains `offset`.
fn ref_at(current: &[HookRef], offset: usize) -> Option<&HookRef> {
    current
        .iter()
        .find(|r| r.start <= offset && offset <= r.end)
}

/// Go to Declaration: from a hook name to the other side (registrations ↔ invocations),
/// from a callback name to its function, and from a WordPress function to the stubs.
pub fn goto(index: &HookIndex, path: &str, src: &str, offset: usize) -> Vec<Location> {
    let current = hooks::scan(path, src);
    if let Some(r) = ref_at(&current, offset) {
        return index
            .related(r, path, &current)
            .into_iter()
            .map(location)
            .collect();
    }
    if let Some(name) = current.iter().find_map(|r| {
        r.callback_range
            .filter(|(s, e)| *s <= offset && offset <= *e)
            .and(r.callback_name.as_deref())
    }) {
        return index
            .function_decls(name, path)
            .into_iter()
            .map(|f| Location::new(&f.path, f.line, f.column))
            .collect();
    }
    // A call of a WordPress function the project does not declare: the stubs.
    let tokens = lexer::tokenize(src);
    let Some(i) = tokens
        .iter()
        .position(|t| t.kind == TokKind::Ident && t.start <= offset && offset <= t.end)
    else {
        return Vec::new();
    };
    let t = tokens[i];
    let is_call = tokens.get(i + 1).is_some_and(|n| n.is("("))
        && !i.checked_sub(1).is_some_and(|p| {
            tokens[p].is("->") || tokens[p].is("::") || tokens[p].is_ident("function")
        });
    if !is_call {
        return Vec::new();
    }
    let name = t.text.trim_start_matches('\\');
    if index
        .functions
        .iter()
        .any(|f| f.class.is_none() && f.name.eq_ignore_ascii_case(name))
    {
        return Vec::new();
    }
    index
        .stub_functions
        .iter()
        .filter(|f| f.name.eq_ignore_ascii_case(name))
        .map(|f| Location::new(&f.path, f.line, f.column))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GutterKind {
    /// `add_action` / `add_filter`: go to where the hook fires.
    Registration,
    /// `do_action` / `apply_filters`: go to the callbacks.
    Invocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    /// 0-based line.
    pub line: u32,
    pub kind: GutterKind,
    pub hook: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Location>,
}

/// Gutter marks of one file: each registration links to the invocations of its hook, each
/// invocation to the registered callbacks.
pub fn marks(index: &HookIndex, path: &str, src: &str) -> Vec<GutterMark> {
    let current = hooks::scan(path, src);
    let mut out = Vec::new();
    for r in &current {
        let related = index.related(r, path, &current);
        let targets: Vec<Location> = related.iter().map(|o| location(o)).collect();
        let n = targets.len();
        let (kind, title) = match r.role {
            HookRole::Registration => {
                let title = if n > 0 {
                    format!(
                        "{} '{}' is fired in {}",
                        capitalize(r.kind.label()),
                        r.name,
                        plural(n, "place", "places")
                    )
                } else if let Some((_, summary)) = core_hook(&r.name) {
                    format!("WordPress core {} '{}': {summary}", r.kind.label(), r.name)
                } else {
                    format!(
                        "{} '{}' is not fired in the project",
                        capitalize(r.kind.label()),
                        r.name
                    )
                };
                (GutterKind::Registration, title)
            }
            HookRole::Invocation => {
                let title = if n > 0 {
                    format!("{} for '{}'", plural(n, "callback", "callbacks"), r.name)
                } else {
                    format!("No callbacks registered for '{}'", r.name)
                };
                (GutterKind::Invocation, title)
            }
            _ => continue,
        };
        out.push(GutterMark {
            line: r.line,
            kind,
            hook: r.name.clone(),
            title,
            targets,
        });
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Find Usages of the hook at `offset`: every registration, invocation, removal and check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookUsages {
    pub hook: String,
    pub kind: HookKind,
    pub refs: Vec<HookRef>,
}

pub fn usages(index: &HookIndex, path: &str, src: &str, offset: usize) -> Option<HookUsages> {
    let current = hooks::scan(path, src);
    let r = ref_at(&current, offset)?;
    Some(HookUsages {
        hook: r.name.clone(),
        kind: r.kind,
        refs: index
            .usages(r, path, &current)
            .into_iter()
            .cloned()
            .collect(),
    })
}

/// The usage kind (Find tool window group) of a reference.
pub fn usage_kind(role: HookRole) -> UsageKind {
    match role {
        HookRole::Registration => UsageKind::HookRegistration,
        HookRole::Invocation => UsageKind::HookInvocation,
        HookRole::Removal | HookRole::Check => UsageKind::FunctionCall,
    }
}

fn utf16(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn row(r: &HookRef) -> UsageRow {
    let at = (r.column.saturating_sub(r.indent) as usize).min(r.text.len());
    let at = (0..=at)
        .rev()
        .find(|&i| r.text.is_char_boundary(i))
        .unwrap_or(0);
    let mut end = (at + (r.end - r.start)).min(r.text.len());
    while !r.text.is_char_boundary(end) {
        end -= 1;
    }
    let start16 = utf16(&r.text[..at]);
    let end16 = utf16(&r.text[..end]);
    let indent16 = r.indent; // indentation is ASCII whitespace
    UsageRow {
        line: r.line,
        column: indent16 + start16,
        end_line: r.line,
        end_column: indent16 + end16,
        text: r.text.clone(),
        ranges: vec![[start16, end16]],
        kind: usage_kind(r.role),
    }
}

/// The usage tree the Find tool window renders, grouped by usage kind (or only by file).
pub fn usage_tree(refs: &[HookRef], by_type: bool) -> UsageTree {
    let mut groups: BTreeMap<Option<UsageKind>, BTreeMap<String, Vec<UsageRow>>> = BTreeMap::new();
    for r in refs {
        let key = by_type.then(|| usage_kind(r.role));
        groups
            .entry(key)
            .or_default()
            .entry(r.path.clone())
            .or_default()
            .push(row(r));
    }
    let mut files = HashSet::new();
    let groups: Vec<UsageGroup> = groups
        .into_iter()
        .map(|(kind, by_file)| {
            let count = by_file.values().map(Vec::len).sum();
            UsageGroup {
                kind,
                label: kind.map(|k| k.label()).unwrap_or("Usages").to_string(),
                count,
                files: by_file
                    .into_iter()
                    .map(|(path, usages)| {
                        files.insert(path.clone());
                        UsageFile { path, usages }
                    })
                    .collect(),
            }
        })
        .collect();
    UsageTree {
        total: refs.len(),
        file_count: files.len(),
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::tests::demo;

    const PLUGIN: &str = "wp-content/plugins/acme/acme.php";

    #[test]
    fn finds_the_hook_context_while_typing() {
        let src = "<?php\nadd_action( 'sa";
        let ctx = hook_context(src, src.len()).unwrap();
        assert_eq!(
            (ctx.function.as_str(), ctx.kind, ctx.role),
            ("add_action", HookKind::Action, HookRole::Registration)
        );
        assert_eq!(ctx.prefix, "sa");
        assert_eq!(&src[ctx.from..ctx.to], "sa");
        let multi = "<?php\n\\apply_filters(\n\t\"the_";
        assert_eq!(
            hook_context(multi, multi.len()).unwrap().kind,
            HookKind::Filter
        );
        assert!(hook_context("<?php\n$x->add_action('a", 22).is_none());
        assert!(hook_context("<?php\nfoo('a", 12).is_none());
        assert!(
            hook_context("<?php\nadd_action('init', 'cb", 30).is_none(),
            "second argument"
        );
    }

    #[test]
    fn completes_fired_hooks_first() {
        let index = demo();
        let src = "<?php\nadd_action( '";
        let list = complete(&index, PLUGIN, src, src.len()).unwrap();
        assert_eq!(list.from, src.len());
        let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels[0], "save_post", "fired in the project");
        assert!(
            !labels.contains(&"save_post_{$post->post_type}"),
            "patterns are not names"
        );
        let core = list.items.iter().find(|i| i.label == "init").unwrap();
        assert!(core.core);
        assert_eq!(
            list.items[0].detail, "fired in post.php",
            "the buffer replaces acme.php"
        );
        // Filters rank after actions in add_action.
        let first_filter = labels.iter().position(|l| *l == "the_content").unwrap();
        let last_action = labels.iter().rposition(|l| *l == "wp_head").unwrap();
        assert!(first_filter > last_action);
    }

    #[test]
    fn navigates_between_registration_and_invocation() {
        let index = demo();
        let src = "<?php\nadd_action( 'save_post', 'acme_save' );\nadd_action( 'save_post_book', 'acme_save_book', 5 );\nfunction acme_save() {}\nfunction acme_save_book() {}\n";
        let at = src.find("save_post'").unwrap() + 2;
        assert_eq!(
            goto(&index, PLUGIN, src, at),
            vec![Location::new("wp-includes/post.php", 2, 13)]
        );
        let cb = src.find("acme_save_book'").unwrap() + 1;
        assert_eq!(
            goto(&index, PLUGIN, src, cb),
            vec![Location::new(PLUGIN, 4, 9)]
        );
        let fired = "<?php\nfunction wp_insert_post() {\n\tdo_action( 'save_post', $id, $post );\n\tdo_action( \"save_post_{$post->post_type}\", $id );\n}\n";
        let at = fired.find("post_type}").unwrap();
        let targets = goto(&index, "wp-includes/post.php", fired, at);
        assert_eq!(targets, vec![Location::new(PLUGIN, 2, 13)]);
    }

    #[test]
    fn goes_to_stub_functions() {
        let mut index = demo();
        index.add_stubs(
            "vendor/php-stubs/wordpress-stubs/wordpress-stubs.php",
            "<?php\nnamespace {\n    function add_action($hook_name, $callback, $priority = 10, $accepted_args = 1) {}\n    class WP_Query { public function get_posts() {} }\n}\n",
        );
        assert_eq!(index.stub_functions.len(), 1);
        let src = "<?php\nadd_action( 'init', 'x' );";
        let at = src.find("add_action").unwrap() + 3;
        assert_eq!(
            goto(&index, PLUGIN, src, at),
            vec![Location::new(
                "vendor/php-stubs/wordpress-stubs/wordpress-stubs.php",
                2,
                13
            )]
        );
    }

    #[test]
    fn computes_gutter_marks() {
        let index = demo();
        let src = "<?php\nadd_action( 'save_post', 'acme_save' );\nadd_action( 'init', 'boot' );\nadd_filter( 'acme_unused', 'x' );\ndo_action( 'acme_ready' );\n";
        let marks = marks(&index, PLUGIN, src);
        let titles: Vec<&str> = marks.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles[0], "Action 'save_post' is fired in 1 place");
        assert!(titles[1].starts_with("WordPress core action 'init': "));
        assert_eq!(
            titles[2],
            "Filter 'acme_unused' is not fired in the project"
        );
        assert_eq!(titles[3], "No callbacks registered for 'acme_ready'");
        assert_eq!(marks[0].targets.len(), 1);
        assert_eq!(marks[3].kind, GutterKind::Invocation);
        assert_eq!(marks[0].line, 1);
    }

    #[test]
    fn finds_usages_of_a_hook() {
        let index = demo();
        let src = "<?php\nadd_action( 'save_post', 'acme_save' );\nremove_action( 'save_post', 'acme_save' );\n";
        let u = usages(&index, PLUGIN, src, src.find("save_post").unwrap()).unwrap();
        assert_eq!(u.hook, "save_post");
        assert_eq!(u.refs.len(), 3, "invocation, registration, removal");
        let tree = usage_tree(&u.refs, true);
        assert_eq!(tree.total, 3);
        assert_eq!(tree.file_count, 2);
        let labels: Vec<&str> = tree.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Hook registration", "Hook invocation", "Function call"]
        );
        let reg = &tree.groups[0].files[0].usages[0];
        assert_eq!((reg.line, reg.column, reg.end_column), (1, 13, 22));
        assert_eq!(reg.ranges, vec![[13, 22]]);
        let flat = usage_tree(&u.refs, false);
        assert_eq!(flat.groups.len(), 1);
        assert_eq!(flat.groups[0].files.len(), 2);
    }
}

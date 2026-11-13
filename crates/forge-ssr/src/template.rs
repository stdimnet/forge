//! Turning a search template into a syntax tree: every `$name$` becomes an identifier or a
//! PHP variable the grammar accepts at that place, and the tree's top nodes are the pattern.

#[cfg(feature = "engine")]
use std::collections::HashMap;

use regex::Regex;
use std::sync::OnceLock;
#[cfg(feature = "engine")]
use tree_sitter::{Node, Parser, Tree};

#[cfg(feature = "engine")]
use crate::Grammar;

pub(crate) fn variable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)\$").expect("variable regex"))
}

/// The `$name$` variables of a template, in order of first appearance.
pub fn variables(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for caps in variable_re().captures_iter(template) {
        let name = caps[1].to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

#[cfg(feature = "engine")]
/// The identifier standing in for variable `i`.
pub(crate) fn ident(i: usize) -> String {
    format!("__ssr_{i}__")
}

#[cfg(feature = "engine")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// PHP: identifiers where names go (after `->`, `new`, …), variables elsewhere.
    Context,
    Identifiers,
    Variables,
}

#[cfg(feature = "engine")]
/// Words and operators after which PHP expects a name, not an expression.
const NAME_AFTER: &[&str] = &[
    "->",
    "?->",
    "::",
    "new",
    "function",
    "fn",
    "class",
    "interface",
    "trait",
    "enum",
    "extends",
    "implements",
    "instanceof",
    "insteadof",
    "use",
    "namespace",
    "const",
    "goto",
    "|",
    "?",
    ":",
    "catch(",
    "catch (",
];

#[cfg(feature = "engine")]
fn php_name_expected(before: &str, after: &str) -> bool {
    let b = before.trim_end();
    let word_end = |w: &str| {
        b.ends_with(w)
            && (w.chars().all(|c| !c.is_alphanumeric())
                || !b[..b.len() - w.len()]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_'))
    };
    // A type before a parameter or catch variable: `$Type$ $e`.
    let a = after.trim_start();
    NAME_AFTER.iter().any(|w| word_end(w)) || (a.starts_with('$') && !after.starts_with('$'))
}

#[cfg(feature = "engine")]
/// The template with placeholders and, per variable, whether it only stands for PHP
/// variables (`$$name$`).
fn substitute(
    template: &str,
    grammar: Grammar,
    mode: Mode,
    names: &[String],
) -> (String, Vec<bool>) {
    let mut out = String::with_capacity(template.len() + 32);
    let mut var_only = vec![false; names.len()];
    let mut at = 0;
    for caps in variable_re().captures_iter(template) {
        let whole = caps.get(0).expect("match");
        let i = names
            .iter()
            .position(|n| *n == caps[1])
            .expect("known variable");
        let before = &template[..whole.start()];
        out.push_str(&template[at..whole.start()]);
        let name = ident(i);
        if grammar == Grammar::Php {
            let after = &template[whole.end()..];
            if before.ends_with('$') {
                var_only[i] = true;
                out.push_str(&name);
            } else {
                let as_name = match mode {
                    Mode::Identifiers => true,
                    Mode::Variables => false,
                    Mode::Context => php_name_expected(before, after),
                };
                if !as_name {
                    out.push('$');
                }
                out.push_str(&name);
            }
        } else {
            out.push_str(&name);
        }
        at = whole.end();
    }
    out.push_str(&template[at..]);
    (out, var_only)
}

#[cfg(feature = "engine")]
/// A parsed template.
pub(crate) struct Pattern {
    pub source: String,
    pub tree: Tree,
    /// Byte ranges and kinds of the top pattern nodes in `source` (several for a statement
    /// sequence).
    roots: Vec<(usize, usize, &'static str)>,
    /// Placeholder texts (`__ssr_0__`, `$__ssr_0__`) to variable indexes.
    pub placeholders: HashMap<String, usize>,
    pub longest_placeholder: usize,
    pub var_only: Vec<bool>,
}

#[cfg(feature = "engine")]
impl Pattern {
    /// The top pattern nodes.
    pub fn roots(&self) -> Vec<Node<'_>> {
        let mut out = Vec::new();
        for &(start, end, kind) in &self.roots {
            let mut node = self
                .tree
                .root_node()
                .descendant_for_byte_range(start, end)
                .expect("root node");
            // Nodes with the same range nest (an expression in a statement without `;`).
            while node.kind() != kind {
                match node.parent() {
                    Some(p) if p.start_byte() == start && p.end_byte() == end => node = p,
                    _ => break,
                }
            }
            out.push(node);
        }
        out
    }
}

#[cfg(feature = "engine")]
pub(crate) fn parser(grammar: Grammar) -> Parser {
    let mut p = Parser::new();
    let lang: tree_sitter::Language = match grammar {
        Grammar::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Grammar::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Grammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Grammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
    };
    p.set_language(&lang).expect("grammar version");
    p
}

#[cfg(feature = "engine")]
/// Kinds that never are part of a pattern's top level.
fn skipped_top(kind: &str) -> bool {
    matches!(
        kind,
        "php_tag" | "text" | "text_interpolation" | "comment" | "php_end_tag"
    )
}

#[cfg(feature = "engine")]
fn top_nodes<'t>(tree: &'t Tree, wrapped: bool) -> Option<Vec<Node<'t>>> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    let mut top: Vec<Node<'t>> = root
        .named_children(&mut cursor)
        .filter(|n| !skipped_top(n.kind()))
        .collect();
    if wrapped {
        // `class __ssr_wrap__ { … }`: the class body's members.
        let class = *top.first()?;
        let body = (0..class.named_child_count())
            .filter_map(|i| class.named_child(i))
            .find(|n| matches!(n.kind(), "declaration_list" | "class_body"))?;
        let mut c = body.walk();
        top = body
            .named_children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect();
    }
    if top.len() == 1 && top[0].kind() == "expression_statement" && top[0].named_child_count() == 1
    {
        top = vec![top[0].named_child(0)?];
    }
    (!top.is_empty()).then_some(top)
}

#[cfg(feature = "engine")]
/// Parse `template` for `grammar`; the error explains a template that is no code fragment.
pub(crate) fn compile(
    template: &str,
    grammar: Grammar,
    names: &[String],
) -> Result<Pattern, String> {
    if template.trim().is_empty() {
        return Err("The search template is empty".into());
    }
    let modes: &[Mode] = if grammar == Grammar::Php {
        &[Mode::Context, Mode::Identifiers, Mode::Variables]
    } else {
        &[Mode::Identifiers]
    };
    let (prefix, wrap_prefix) = if grammar == Grammar::Php {
        ("<?php\n", "<?php\nclass __ssr_wrap__ {\n")
    } else {
        ("", "class __ssr_wrap__ {\n")
    };
    let mut p = parser(grammar);
    for &mode in modes {
        let (code, var_only) = substitute(template, grammar, mode, names);
        let candidates = [
            (format!("{prefix}{code}"), false),
            (format!("{prefix}{code};"), false),
            (format!("{wrap_prefix}{code}\n}}"), true),
        ];
        for (source, wrapped) in candidates {
            let Some(tree) = p.parse(&source, None) else {
                continue;
            };
            if tree.root_node().has_error() {
                continue;
            }
            let Some(top) = top_nodes(&tree, wrapped) else {
                continue;
            };
            let roots = top
                .iter()
                .map(|n| (n.start_byte(), n.end_byte(), n.kind()))
                .collect();
            let mut placeholders = HashMap::new();
            for i in 0..names.len() {
                placeholders.insert(ident(i), i);
                placeholders.insert(format!("${}", ident(i)), i);
            }
            let longest_placeholder = placeholders.keys().map(String::len).max().unwrap_or(0);
            return Ok(Pattern {
                source,
                tree,
                roots,
                placeholders,
                longest_placeholder,
                var_only,
            });
        }
    }
    Err(format!(
        "The search template is not a valid {} code fragment",
        grammar.language_label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_in_order() {
        assert_eq!(
            variables("$Instance$->$Method$($Args$, $Instance$)"),
            ["Instance", "Method", "Args"]
        );
        assert!(variables("$this->save()").is_empty());
    }

    #[cfg(feature = "engine")]
    #[test]
    fn php_placeholders_fit_their_place() {
        let names = variables("$obj$->$method$($args$)");
        let (code, _) = substitute(
            "$obj$->$method$($args$)",
            Grammar::Php,
            Mode::Context,
            &names,
        );
        assert_eq!(code, "$__ssr_0__->__ssr_1__($__ssr_2__)");
        let names = variables("try { } catch ($E$ $e$) { }");
        let (code, _) = substitute(
            "try { } catch ($E$ $e$) { }",
            Grammar::Php,
            Mode::Context,
            &names,
        );
        assert_eq!(code, "try { } catch (__ssr_0__ $__ssr_1__) { }");
        let names = variables("$$v$ = new $C$()");
        let (code, only) = substitute("$$v$ = new $C$()", Grammar::Php, Mode::Context, &names);
        assert_eq!(code, "$__ssr_0__ = new __ssr_1__()");
        assert_eq!(only, [true, false]);
    }

    #[cfg(feature = "engine")]
    #[test]
    fn templates_parse_as_expressions_statements_or_members() {
        let compile_ok = |t: &str, g: Grammar| {
            let names = variables(t);
            let p = compile(t, g, &names).unwrap_or_else(|e| panic!("{t}: {e}"));
            p.roots()
                .iter()
                .map(|n| n.kind().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            compile_ok("$a$->$m$($p$)", Grammar::Php),
            ["member_call_expression"]
        );
        assert_eq!(
            compile_ok("new $C$($p$)", Grammar::Php),
            ["object_creation_expression"]
        );
        assert_eq!(
            compile_ok("if ($c$) { $s$; }", Grammar::Php),
            ["if_statement"]
        );
        assert_eq!(
            compile_ok("public function $m$($p$) { $s$; }", Grammar::Php),
            ["method_declaration"]
        );
        assert_eq!(
            compile_ok("$a$ = 1; $b$ = 2;", Grammar::Php),
            ["expression_statement", "expression_statement"]
        );
        assert_eq!(
            compile_ok("$a$.$m$($p$)", Grammar::JavaScript),
            ["call_expression"]
        );
        assert_eq!(
            compile_ok("let $x$: $T$ = $v$", Grammar::TypeScript),
            ["lexical_declaration"]
        );
        let err = compile("if (", Grammar::Php, &[]).err().unwrap();
        assert_eq!(err, "The search template is not a valid PHP code fragment");
        assert!(compile(" ", Grammar::Php, &[]).is_err());
    }
}

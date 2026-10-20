//! DQL inside PHP code: `createQuery()` arguments and QueryBuilder fragments.
//!
//! A [`Site`] is one DQL text assembled from the string literals of a call argument
//! (concatenations, heredocs, `Entity::class`), with a mapping back to source offsets.

use std::sync::OnceLock;

use phpforge_lang_php::{parser, ClassLike, FileIndex, SymbolKind};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::dql::{self, lexer, Mode, PathTarget};
use crate::{line_col, Location, Snapshot};

/// One literal piece of the DQL text and where it is in the PHP source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub dql_start: usize,
    pub src_start: usize,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub dql: String,
    pub mode: Mode,
    /// Aliases declared by the QueryBuilder chain (alias, entity class).
    pub preset: Vec<(String, String)>,
    pub segments: Vec<Segment>,
    /// Method the argument belongs to (`createQuery`, `andWhere`, ...).
    pub method: String,
}

impl Site {
    pub fn to_dql(&self, src: usize) -> Option<usize> {
        self.segments
            .iter()
            .find(|s| s.src_start <= src && src <= s.src_start + s.len)
            .map(|s| s.dql_start + (src - s.src_start))
    }

    pub fn to_src(&self, dql: usize) -> Option<usize> {
        self.segments
            .iter()
            .find(|s| s.dql_start <= dql && dql <= s.dql_start + s.len)
            .map(|s| s.src_start + (dql - s.dql_start))
    }
}

const QB_EXPR: &[&str] = &[
    "select",
    "addSelect",
    "where",
    "andWhere",
    "orWhere",
    "having",
    "andHaving",
    "orHaving",
    "orderBy",
    "addOrderBy",
    "groupBy",
    "addGroupBy",
    "set",
    "distinct",
];
const QB_JOIN: &[&str] = &["join", "innerJoin", "leftJoin"];
const QB_ENTITY: &[&str] = &["from", "update", "delete"];
const EXPR_METHODS: &[&str] = &[
    "eq",
    "neq",
    "lt",
    "lte",
    "gt",
    "gte",
    "isNull",
    "isNotNull",
    "in",
    "notIn",
    "like",
    "notLike",
    "between",
    "andX",
    "orX",
    "not",
    "count",
    "countDistinct",
    "max",
    "min",
    "avg",
    "sum",
    "abs",
    "lower",
    "upper",
    "length",
    "concat",
    "trim",
    "isMemberOf",
    "isInstanceOf",
    "exists",
    "all",
    "some",
    "any",
    "asc",
    "desc",
    "literal",
    "diff",
    "prod",
    "quot",
    "sqrt",
    "mod",
];

fn mode_for(method: &str, arg: usize, object: &str) -> Option<Mode> {
    if method == "createQuery" && arg == 0 {
        return Some(Mode::Full);
    }
    if QB_EXPR.contains(&method) {
        let ok = match method {
            "orderBy" | "addOrderBy" | "set" => arg == 0,
            _ => true,
        };
        return ok.then_some(Mode::Expression);
    }
    if QB_JOIN.contains(&method) {
        return match arg {
            0 => Some(Mode::Join),
            3 => Some(Mode::Expression),
            _ => None,
        };
    }
    if QB_ENTITY.contains(&method) && arg == 0 {
        return Some(Mode::Entity);
    }
    if EXPR_METHODS.contains(&method) && object.contains("expr()") {
        return Some(Mode::Expression);
    }
    None
}

struct Builder<'a> {
    src: &'a str,
    index: &'a FileIndex,
    dql: String,
    segments: Vec<Segment>,
    has_literal: bool,
}

impl Builder<'_> {
    fn literal(&mut self, start: usize, end: usize) {
        let end = end.max(start);
        self.segments.push(Segment {
            dql_start: self.dql.len(),
            src_start: start,
            len: end - start,
        });
        self.dql.push_str(&self.src[start..end]);
        self.has_literal = true;
    }

    fn flatten(&mut self, node: Node) {
        match node.kind() {
            "string" | "encapsed_string" => {
                let text = &self.src[node.byte_range()];
                let open = text.find(['\'', '"']).map(|i| i + 1).unwrap_or(0);
                let start = node.start_byte() + open;
                let end = if text.len() > open && (text.ends_with('\'') || text.ends_with('"')) {
                    node.end_byte() - 1
                } else {
                    node.end_byte()
                };
                self.literal(start, end);
            }
            "heredoc" | "nowdoc" => {
                let mut c = node.walk();
                let body = node
                    .named_children(&mut c)
                    .find(|n| n.kind() == "heredoc_body" || n.kind() == "nowdoc_body");
                if let Some(b) = body {
                    self.literal(b.start_byte(), b.end_byte());
                }
            }
            "binary_expression" => {
                let mut c = node.walk();
                let kids: Vec<Node> = node.named_children(&mut c).collect();
                for k in kids {
                    self.flatten(k);
                }
            }
            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    self.flatten(inner);
                }
            }
            "class_constant_access_expression" => {
                let text = &self.src[node.byte_range()];
                match text.strip_suffix("::class") {
                    Some(name) => {
                        let fqn = self.index.resolve_class(name.trim(), node.start_byte());
                        self.dql.push_str(&fqn);
                    }
                    None => self.dql.push_str(" ? "),
                }
            }
            _ => self.dql.push_str(" ? "),
        }
    }
}

/// The call around an argument node: (method name, object text, argument index).
fn call_of<'t>(src: &str, argument: Node<'t>) -> Option<(String, String, usize, Node<'t>)> {
    let args = argument.parent().filter(|p| p.kind() == "arguments")?;
    let mut c = args.walk();
    let index = args
        .named_children(&mut c)
        .filter(|n| n.kind() == "argument")
        .position(|n| n.id() == argument.id())?;
    let call = args.parent()?;
    let name = match call.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            call.child_by_field_name("name")
                .map(|n| src[n.byte_range()].to_string())?
        }
        "function_call_expression" => call
            .child_by_field_name("function")
            .map(|n| src[n.byte_range()].to_string())?,
        _ => return None,
    };
    let object = call
        .child_by_field_name("object")
        .or_else(|| call.child_by_field_name("scope"))
        .map(|n| src[n.byte_range()].to_string())
        .unwrap_or_default();
    Some((name, object, index, call))
}

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).expect("valid regex"))
}

/// Aliases the QueryBuilder chain in the enclosing method declares.
fn builder_aliases(
    snap: &Snapshot,
    index: &FileIndex,
    src: &str,
    offset: usize,
) -> Vec<(String, String)> {
    static CREATE: OnceLock<Regex> = OnceLock::new();
    static REPO_CREATE: OnceLock<Regex> = OnceLock::new();
    static FROM: OnceLock<Regex> = OnceLock::new();
    static ENTITY_JOIN: OnceLock<Regex> = OnceLock::new();
    static PATH_JOIN: OnceLock<Regex> = OnceLock::new();
    let class: Option<&ClassLike> = index.class_at(offset);
    let body = class
        .and_then(|c| {
            c.members
                .iter()
                .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
        })
        .map(|m| &src[m.range.start..m.range.end.min(src.len())])
        .unwrap_or(src);
    let resolve = |name: &str| index.resolve_class(name, offset);
    let mut out: Vec<(String, String)> = Vec::new();
    for c in re(
        &REPO_CREATE,
        r#"getRepository\(\s*([\w\\]+)::class\s*\)\s*->\s*createQueryBuilder\(\s*['"](\w+)['"]"#,
    )
    .captures_iter(body)
    {
        out.push((c[2].to_string(), resolve(&c[1])));
    }
    if let Some(entity) = class.and_then(|c| snap.entity_of_repository(&c.fqn)) {
        for c in re(
            &CREATE,
            r#"\$this\s*->\s*createQueryBuilder\(\s*['"](\w+)['"]"#,
        )
        .captures_iter(body)
        {
            out.push((c[1].to_string(), entity.class.clone()));
        }
    }
    for c in re(&FROM, r#"->\s*(?:from|update|delete)\(\s*(?:([\w\\]+)::class|['"]([\w\\:]+)['"])\s*,\s*['"](\w+)['"]"#).captures_iter(body) {
        let entity = c.get(1).map(|m| resolve(m.as_str())).or_else(|| c.get(2).map(|m| m.as_str().replace("\\\\", "\\")));
        if let Some(e) = entity {
            out.push((c[3].to_string(), e));
        }
    }
    for c in re(
        &ENTITY_JOIN,
        r#"->\s*(?:join|innerJoin|leftJoin)\(\s*([\w\\]+)::class\s*,\s*['"](\w+)['"]"#,
    )
    .captures_iter(body)
    {
        out.push((c[2].to_string(), resolve(&c[1])));
    }
    // Path joins depend on earlier aliases; resolve until nothing changes.
    let joins: Vec<(String, String, String)> = re(
        &PATH_JOIN,
        r#"->\s*(?:join|innerJoin|leftJoin)\(\s*['"](\w+)\.(\w+)['"]\s*,\s*['"](\w+)['"]"#,
    )
    .captures_iter(body)
    .map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string()))
    .collect();
    for _ in 0..3 {
        for (alias, assoc, new_alias) in &joins {
            if out.iter().any(|(a, _)| a == new_alias) {
                continue;
            }
            let target = out
                .iter()
                .find(|(a, _)| a == alias)
                .and_then(|(_, e)| snap.entity(e))
                .and_then(|e| e.association(assoc))
                .map(|a| a.target.clone());
            if let Some(t) = target {
                out.push((new_alias.clone(), t));
            }
        }
    }
    out
}

fn site_for_argument(
    snap: &Snapshot,
    index: &FileIndex,
    src: &str,
    argument: Node,
) -> Option<Site> {
    let (method, object, arg, _) = call_of(src, argument)?;
    let mode = mode_for(&method, arg, &object)?;
    let value = argument.named_child(argument.named_child_count().saturating_sub(1))?;
    let mut b = Builder {
        src,
        index,
        dql: String::new(),
        segments: Vec::new(),
        has_literal: false,
    };
    b.flatten(value);
    if !b.has_literal {
        return None;
    }
    let preset = if mode == Mode::Full {
        Vec::new()
    } else {
        builder_aliases(snap, index, src, argument.start_byte())
    };
    Some(Site {
        dql: b.dql,
        mode,
        preset,
        segments: b.segments,
        method,
    })
}

/// The DQL site whose string literal contains `offset`.
pub fn site_at(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<Site> {
    let mut p = parser::new_parser();
    let tree = parser::parse_tree(&mut p, src)?;
    let index = parser::index_source(path, src);
    let mut node = tree.root_node().descendant_for_byte_range(offset, offset)?;
    // Up to the argument, remembering whether we passed a string literal.
    let mut in_string = false;
    loop {
        if matches!(
            node.kind(),
            "string"
                | "encapsed_string"
                | "heredoc"
                | "nowdoc"
                | "string_content"
                | "nowdoc_body"
                | "heredoc_body"
        ) {
            in_string = true;
        }
        if node.kind() == "argument" {
            break;
        }
        node = node.parent()?;
    }
    if !in_string {
        return None;
    }
    let site = site_for_argument(snap, &index, src, node)?;
    site.to_dql(offset)?;
    Some(site)
}

/// Every DQL site in a file.
pub fn sites(snap: &Snapshot, path: &str, src: &str) -> Vec<Site> {
    let mut p = parser::new_parser();
    let Some(tree) = parser::parse_tree(&mut p, src) else {
        return Vec::new();
    };
    let index = parser::index_source(path, src);
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "argument" {
            if let Some(site) = site_for_argument(snap, &index, src, n) {
                out.push(site);
            }
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c));
    }
    out.sort_by_key(|s| s.segments.first().map(|x| x.src_start).unwrap_or(0));
    out
}

/// Completion in PHP source; `from` is a source byte offset.
pub fn complete(
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Option<(usize, dql::Completion)> {
    let site = site_at(snap, path, src, offset)?;
    let cursor = site.to_dql(offset)?;
    let c = dql::complete(snap, &site.dql, cursor, site.mode, &site.preset);
    let from = site.to_src(c.from).unwrap_or(offset);
    Some((from, c))
}

/// Declarations of the entity, field or association under the cursor.
pub fn goto(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Vec<Location> {
    let Some(site) = site_at(snap, path, src, offset) else {
        return Vec::new();
    };
    let Some(cursor) = site.to_dql(offset) else {
        return Vec::new();
    };
    let query = dql::parse(snap, &site.dql, &site.preset);
    let Some(tok) = lexer::tokenize(&site.dql)
        .into_iter()
        .find(|t| t.kind == lexer::TokenKind::Identifier && t.start <= cursor && cursor <= t.end)
    else {
        return Vec::new();
    };
    let entity_ref = query.entity_refs.contains(&(tok.start, tok.end))
        || (site.mode == Mode::Entity && !tok.text.contains('.'));
    if entity_ref {
        return snap
            .entity(&tok.text)
            .map(|e| e.location())
            .into_iter()
            .collect();
    }
    // Path prefix up to the part under the cursor.
    let rel = cursor - tok.start;
    let end = tok.text[rel..]
        .find('.')
        .map(|i| rel + i)
        .unwrap_or(tok.text.len());
    let path_expr = &tok.text[..end];
    let parts: Vec<&str> = path_expr.split('.').collect();
    if parts.len() == 1 {
        return query
            .alias(parts[0])
            .and_then(|a| a.entity.as_deref())
            .and_then(|e| snap.entity(e))
            .map(|e| e.location())
            .into_iter()
            .collect();
    }
    let parent = parts[..parts.len() - 1].join(".");
    let last = parts[parts.len() - 1];
    match dql::resolve_path(snap, &query, &parent).0 {
        PathTarget::Entity(e) => {
            let line = e
                .field(last)
                .map(|f| f.line)
                .or_else(|| e.association(last).map(|a| a.line));
            line.map(|l| Location::new(&e.path, l, 0))
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A diagnostic in source byte offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiagnostic {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// DQL problems in a PHP file.
pub fn diagnostics(snap: &Snapshot, path: &str, src: &str) -> Vec<SourceDiagnostic> {
    let mut out = Vec::new();
    for site in sites(snap, path, src) {
        if site.mode != Mode::Full && site.preset.is_empty() {
            continue;
        }
        for d in dql::diagnostics(snap, &site.dql, &site.preset) {
            if let (Some(start), Some(end)) = (site.to_src(d.start), site.to_src(d.end)) {
                out.push(SourceDiagnostic {
                    start,
                    end,
                    message: d.message,
                });
            }
        }
    }
    out
}

/// Gutter marks for entities (go to repository) and repositories (go to entity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    pub line: u32,
    /// `entity` or `repository`.
    pub kind: String,
    pub title: String,
    pub targets: Vec<Location>,
}

pub fn gutter(snap: &Snapshot, path: &str, src: &str) -> Vec<GutterMark> {
    let index = parser::index_source(path, src);
    let mut out = Vec::new();
    for c in &index.classes {
        if let Some(e) = snap.entity(&c.fqn).filter(|e| e.class == c.fqn) {
            if let Some(repo) = e.repository_class.as_deref() {
                let target = snap
                    .repositories
                    .iter()
                    .find(|r| r.class == repo)
                    .map(|r| Location::new(&r.path, r.line, 0));
                out.push(GutterMark {
                    line: c.name_range.line,
                    kind: "entity".into(),
                    title: format!(
                        "Doctrine entity{}. Go to repository {}",
                        e.table
                            .as_deref()
                            .map(|t| format!(" (table {t})"))
                            .unwrap_or_default(),
                        repo.rsplit('\\').next().unwrap_or(repo)
                    ),
                    targets: target.into_iter().collect(),
                });
            }
        } else if let Some(e) = snap.entity_of_repository(&c.fqn) {
            out.push(GutterMark {
                line: c.name_range.line,
                kind: "repository".into(),
                title: format!("Repository of {}. Go to entity", e.name),
                targets: vec![e.location()],
            });
        }
    }
    let _ = line_col;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    fn labels(c: &dql::Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    const REPO: &str = "src/Repository/OrderRepository.php";

    #[test]
    fn create_query_with_concatenation() {
        let snap = fixture::snapshot();
        let (src, pos) = at(r#"<?php
namespace App\Repository;
use App\Entity\Order;
class X {
    function f($em) {
        return $em->createQuery('SELECT o FROM ' . Order::class . ' o WHERE o.sta¦')->getResult();
    }
}
"#);
        let site = site_at(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(site.mode, Mode::Full);
        assert_eq!(site.dql, "SELECT o FROM App\\Entity\\Order o WHERE o.sta");
        let (from, c) = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(labels(&c), vec!["status"]);
        assert_eq!(&src[from..pos], "sta");
    }

    #[test]
    fn multi_line_query_in_repository() {
        let snap = fixture::snapshot();
        let src = fixture::ORDER_REPOSITORY;
        let pos = src.find("o.customer c").unwrap() + 2;
        let (_, c) = complete(&snap, REPO, src, pos + 3).unwrap();
        assert_eq!(labels(&c), vec!["customer"]);
        let loc = &goto(&snap, REPO, src, pos + 1)[0];
        assert_eq!(loc.path, "src/Entity/Order.php");
        assert_eq!(loc.line, 30);
        let entity_pos = src.find("App\\Entity\\Order o").unwrap() + 4;
        assert_eq!(goto(&snap, REPO, src, entity_pos)[0].line, 12);
    }

    #[test]
    fn query_builder_fragments() {
        let snap = fixture::snapshot();
        let src = fixture::ORDER_REPOSITORY;
        let pos = src.find("o.status = :status").unwrap() + 2;
        let site = site_at(&snap, REPO, src, pos).unwrap();
        assert_eq!(site.mode, Mode::Expression);
        assert_eq!(
            site.preset,
            vec![
                ("o".to_string(), "App\\Entity\\Order".to_string()),
                ("c".to_string(), "App\\Entity\\Customer".to_string())
            ]
        );
        let (_, c) = complete(&snap, REPO, src, pos + 2).unwrap();
        assert_eq!(labels(&c), vec!["status"]);

        let join = src.find("'o.customer'").unwrap() + 3;
        let site = site_at(&snap, REPO, src, join).unwrap();
        assert_eq!(site.mode, Mode::Join);
        let (_, c) = complete(&snap, REPO, src, join).unwrap();
        assert_eq!(labels(&c), vec!["customer", "items"]);

        // The direction argument of orderBy is not DQL.
        let desc = src.find("'DESC'").unwrap() + 2;
        assert!(site_at(&snap, REPO, src, desc).is_none());
    }

    #[test]
    fn expr_builder_and_heredoc() {
        let snap = fixture::snapshot();
        let (src, pos) = at(r#"<?php
use App\Entity\Order;
function f($em) {
    $qb = $em->createQueryBuilder()->select('o')->from(Order::class, 'o');
    $qb->where($qb->expr()->eq('o.cust¦', ':c'));
    $q = $em->createQuery(<<<'DQL'
        SELECT o FROM App\Entity\Order o WHERE o.nope = 1
        DQL);
}
"#);
        let (_, c) = complete(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(labels(&c), vec!["customer"]);
        let all = sites(&snap, "src/X.php", &src);
        assert!(all
            .iter()
            .any(|s| s.mode == Mode::Full && s.dql.contains("o.nope")));
        let d = diagnostics(&snap, "src/X.php", &src);
        assert!(d.iter().any(|x| &src[x.start..x.end] == "nope"));
    }

    #[test]
    fn ignores_other_strings() {
        let snap = fixture::snapshot();
        let (src, pos) = at("<?php $this->render('order/sh¦ow.html.twig');");
        assert!(site_at(&snap, "src/X.php", &src, pos).is_none());
    }

    #[test]
    fn gutter_links_entities_and_repositories() {
        let snap = fixture::snapshot();
        let m = gutter(&snap, "src/Entity/Order.php", fixture::ORDER);
        assert_eq!(m[0].kind, "entity");
        assert_eq!(m[0].targets[0].path, "src/Repository/OrderRepository.php");
        let m = gutter(&snap, REPO, fixture::ORDER_REPOSITORY);
        assert_eq!(m[0].kind, "repository");
        assert_eq!(m[0].targets[0].path, "src/Entity/Order.php");
    }
}

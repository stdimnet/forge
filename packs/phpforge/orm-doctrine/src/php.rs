//! DQL inside PHP code: `createQuery()` arguments and QueryBuilder fragments.
//!
//! A [`Site`] is one DQL text assembled from the string literals of a call argument
//! (concatenations, heredocs, `Entity::class`), with a mapping back to source offsets.
//! Sites are found with the tree-sitter PHP parser ([`TreeSitter`]); completion,
//! navigation, diagnostics and highlighting over sites live in `phpforge_dql::site`.

use phpforge_lang_php::{parser, ClassLike, FileIndex, SymbolKind};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

pub use phpforge_dql::site::{
    mode_for, Segment, Site, SiteFinder, SourceDiagnostic, SourceHighlight, SourceSpan,
};

use crate::dql::{self, Mode};
use crate::{line_col, Location, Snapshot};

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

/// Aliases the QueryBuilder chain in the enclosing method declares.
fn builder_aliases(
    snap: &Snapshot,
    index: &FileIndex,
    src: &str,
    offset: usize,
) -> Vec<(String, String)> {
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
    let this_entity = class.and_then(|c| snap.entity_of_repository(&c.fqn));
    phpforge_dql::site::builder_aliases(snap, body, &resolve, this_entity)
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
fn tree_site_at(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<Site> {
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
fn tree_sites(snap: &Snapshot, path: &str, src: &str) -> Vec<Site> {
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

/// Finds sites with the tree-sitter PHP parser.
pub struct TreeSitter;

impl SiteFinder for TreeSitter {
    fn sites(&self, snap: &Snapshot, path: &str, src: &str) -> Vec<Site> {
        tree_sites(snap, path, src)
    }

    fn site_at(&self, snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<Site> {
        tree_site_at(snap, path, src, offset)
    }
}

/// The DQL site whose string literal contains `offset`.
pub fn site_at(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<Site> {
    TreeSitter.site_at(snap, path, src, offset)
}

/// Every DQL site in a file.
pub fn sites(snap: &Snapshot, path: &str, src: &str) -> Vec<Site> {
    TreeSitter.sites(snap, path, src)
}

/// Completion in PHP source; `from` is a source byte offset.
pub fn complete(
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Option<(usize, dql::Completion)> {
    phpforge_dql::site::complete(&TreeSitter, snap, path, src, offset)
}

/// Declarations of the entity, field or association under the cursor.
pub fn goto(snap: &Snapshot, path: &str, src: &str, offset: usize) -> Vec<Location> {
    phpforge_dql::site::goto(&TreeSitter, snap, path, src, offset)
}

/// DQL problems in a PHP file.
pub fn diagnostics(snap: &Snapshot, path: &str, src: &str) -> Vec<SourceDiagnostic> {
    phpforge_dql::site::diagnostics(&TreeSitter, snap, path, src)
}

/// DQL injection highlighting (string pieces and semantic spans).
pub fn highlight(snap: &Snapshot, path: &str, src: &str) -> SourceHighlight {
    phpforge_dql::site::highlight(&TreeSitter, snap, path, src)
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
    fn highlights_dql_in_source() {
        let snap = fixture::snapshot();
        let src = fixture::ORDER_REPOSITORY;
        let h = highlight(&snap, REPO, src);
        let (a, b) = (src.find("andWhere").unwrap(), src.find("orderBy").unwrap());
        let kinds: Vec<(&str, dql::SpanKind)> = h
            .spans
            .iter()
            .filter(|s| s.from >= a && s.to <= b)
            .map(|s| (&src[s.from..s.to], s.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("o", dql::SpanKind::Alias),
                ("status", dql::SpanKind::Field),
                ("=", dql::SpanKind::Operator),
                (":status", dql::SpanKind::Parameter)
            ]
        );
        assert!(h.injected.iter().all(|(a, b)| a < b));
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

//! DQL inside PHP code: a [`Site`] is one DQL text assembled from the string literals of a
//! call argument (`createQuery('…')`, QueryBuilder fragments like `->andWhere('o.status = :s')`)
//! with a mapping back to source offsets.
//!
//! Finding sites needs a PHP parser; [`SiteFinder`] abstracts it so the functions here work
//! with the tree-sitter finder of `phpforge-orm-doctrine` and the lexical finder in
//! [`crate::lexical`] (browser mode). All offsets are byte offsets unless the function is in
//! [`crate::editor`].

use std::sync::OnceLock;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::dql::{self, lexer, Mode, PathTarget, SpanKind};
use crate::model::{Entity, Location, Snapshot};

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

/// Finds the DQL sites of a PHP file.
pub trait SiteFinder {
    fn sites(&self, snap: &Snapshot, path: &str, src: &str) -> Vec<Site>;

    /// The site whose string literal contains `offset`.
    fn site_at(&self, snap: &Snapshot, path: &str, src: &str, offset: usize) -> Option<Site> {
        self.sites(snap, path, src)
            .into_iter()
            .find(|s| s.to_dql(offset).is_some())
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

/// What argument `arg` of `object->method(…)` holds, if it is DQL.
pub fn mode_for(method: &str, arg: usize, object: &str) -> Option<Mode> {
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

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).expect("valid regex"))
}

/// Aliases a QueryBuilder chain in `body` declares: `createQueryBuilder('o')` (of the
/// repository's entity `this_entity`), `->from(Order::class, 'o')`, joins. `resolve` turns a
/// class name as written into its FQN.
pub fn builder_aliases(
    snap: &Snapshot,
    body: &str,
    resolve: &dyn Fn(&str) -> String,
    this_entity: Option<&Entity>,
) -> Vec<(String, String)> {
    static CREATE: OnceLock<Regex> = OnceLock::new();
    static REPO_CREATE: OnceLock<Regex> = OnceLock::new();
    static FROM: OnceLock<Regex> = OnceLock::new();
    static ENTITY_JOIN: OnceLock<Regex> = OnceLock::new();
    static PATH_JOIN: OnceLock<Regex> = OnceLock::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for c in re(
        &REPO_CREATE,
        r#"getRepository\(\s*([\w\\]+)::class\s*\)\s*->\s*createQueryBuilder\(\s*['"](\w+)['"]"#,
    )
    .captures_iter(body)
    {
        out.push((c[2].to_string(), resolve(&c[1])));
    }
    if let Some(entity) = this_entity {
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
        let entity = c
            .get(1)
            .map(|m| resolve(m.as_str()))
            .or_else(|| c.get(2).map(|m| m.as_str().replace("\\\\", "\\")));
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

/// Completion in PHP source; `from` is a source offset.
pub fn complete(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Option<(usize, dql::Completion)> {
    let site = finder.site_at(snap, path, src, offset)?;
    let cursor = site.to_dql(offset)?;
    let c = dql::complete(snap, &site.dql, cursor, site.mode, &site.preset);
    let from = site.to_src(c.from).unwrap_or(offset);
    Some((from, c))
}

/// Declarations of the entity, field or association under the cursor.
pub fn goto(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Vec<Location> {
    let Some(site) = finder.site_at(snap, path, src, offset) else {
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

/// A diagnostic in source offsets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiagnostic {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// DQL problems in a PHP file.
pub fn diagnostics(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
) -> Vec<SourceDiagnostic> {
    let mut out = Vec::new();
    for site in finder.sites(snap, path, src) {
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

/// A highlighted range in source offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub from: usize,
    pub to: usize,
    pub kind: SpanKind,
}

/// DQL injection highlighting of a PHP file: the string pieces holding DQL and the semantic
/// spans inside them, sorted by position.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHighlight {
    /// `[from, to]` of every non-empty DQL string piece.
    pub injected: Vec<(usize, usize)>,
    pub spans: Vec<SourceSpan>,
}

pub fn highlight(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
) -> SourceHighlight {
    let mut out = SourceHighlight::default();
    for site in finder.sites(snap, path, src) {
        for s in &site.segments {
            if s.len > 0 {
                out.injected.push((s.src_start, s.src_start + s.len));
            }
        }
        for sp in dql::highlight(snap, &site.dql, &site.preset) {
            if let (Some(from), Some(to)) = (site.to_src(sp.start), site.to_src(sp.end)) {
                if to > from {
                    out.spans.push(SourceSpan {
                        from,
                        to,
                        kind: sp.kind,
                    });
                }
            }
        }
    }
    out.injected.sort();
    out.spans.sort_by_key(|s| (s.from, s.to));
    out
}

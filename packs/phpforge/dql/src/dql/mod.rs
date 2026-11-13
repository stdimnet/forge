//! DQL: parsing (identification variables and their entities), semantic highlighting,
//! completion and diagnostics against the project's entity mappings.
//!
//! The same functions serve full queries (`createQuery('SELECT …')`) and QueryBuilder
//! fragments (`->andWhere('o.status = :s')`); for fragments a [`Mode`] tells what the
//! fragment is and `preset` aliases come from the builder chain.

pub mod lexer;

use serde::{Deserialize, Serialize};

use crate::model::{AssociationKind, Entity, Snapshot};
use lexer::{significant, tokenize, Token, TokenKind, FUNCTIONS};

/// What a DQL text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// A complete statement.
    Full,
    /// A select/where/order-by expression fragment.
    Expression,
    /// An entity name (`->from('App\Entity\Order', 'o')`).
    Entity,
    /// A join path (`->join('o.customer', 'c')`).
    Join,
}

/// An identification variable (`o` in `FROM Order o`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AliasDecl {
    pub alias: String,
    /// Resolved entity class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// Join path it was declared through (`o.items`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// Byte range of the declaration in the DQL (empty for preset aliases).
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatementKind {
    Select,
    Update,
    Delete,
    Fragment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub kind: StatementKind,
    pub aliases: Vec<AliasDecl>,
    /// Entity name tokens (FROM/JOIN/UPDATE/DELETE/INSTANCE OF) with their byte ranges.
    pub entity_refs: Vec<(usize, usize)>,
}

impl Query {
    pub fn alias(&self, name: &str) -> Option<&AliasDecl> {
        self.aliases.iter().rev().find(|a| a.alias == name)
    }
}

/// What a path expression resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum PathTarget<'a> {
    /// An entity or embeddable (alias, association or embedded field).
    Entity(&'a Entity),
    /// A scalar field.
    Field(&'a Entity, &'a crate::model::Field),
    Unknown,
}

/// Resolve `o.customer.email` through aliases, associations and embedded fields.
pub fn resolve_path<'a>(snap: &'a Snapshot, query: &Query, path: &str) -> (PathTarget<'a>, usize) {
    let mut parts = path.split('.');
    let Some(first) = parts.next() else {
        return (PathTarget::Unknown, 0);
    };
    let Some(mut entity) = query
        .alias(first)
        .and_then(|a| a.entity.as_deref())
        .and_then(|e| snap.entity(e))
    else {
        return (PathTarget::Unknown, 0);
    };
    let mut resolved = 1;
    for part in parts {
        if let Some(a) = entity.association(part) {
            match snap.entity(&a.target) {
                Some(t) => entity = t,
                None => return (PathTarget::Unknown, resolved),
            }
        } else if let Some(f) = entity.field(part) {
            if f.embedded {
                match snap.entity(&f.type_name) {
                    Some(t) => entity = t,
                    None => return (PathTarget::Unknown, resolved),
                }
            } else {
                return (PathTarget::Field(entity, f), resolved + 1);
            }
        } else {
            return (PathTarget::Unknown, resolved);
        }
        resolved += 1;
    }
    (PathTarget::Entity(entity), resolved)
}

fn is_alias_token(t: Option<&&Token>) -> bool {
    t.is_some_and(|t| {
        t.kind == TokenKind::Identifier && !t.text.contains('.') && !t.text.contains('\\')
    })
}

/// Identification variables of a statement (or fragment), plus `preset` ones.
pub fn parse(snap: &Snapshot, dql: &str, preset: &[(String, String)]) -> Query {
    let tokens = tokenize(dql);
    let sig = significant(&tokens);
    let kind = match sig.first().map(|t| t.upper()) {
        Some(k) if k == "SELECT" => StatementKind::Select,
        Some(k) if k == "UPDATE" => StatementKind::Update,
        Some(k) if k == "DELETE" => StatementKind::Delete,
        _ => StatementKind::Fragment,
    };
    let mut aliases: Vec<AliasDecl> = preset
        .iter()
        .map(|(a, e)| AliasDecl {
            alias: a.clone(),
            entity: snap
                .entity(e)
                .map(|x| x.class.clone())
                .or_else(|| Some(e.clone())),
            via: None,
            start: 0,
            end: 0,
        })
        .collect();
    let mut entity_refs = Vec::new();
    let mut i = 0;
    while i < sig.len() {
        let t = sig[i];
        let up = if t.kind == TokenKind::Keyword {
            t.upper()
        } else {
            String::new()
        };
        let declares = match up.as_str() {
            "FROM" | "UPDATE" => true,
            "DELETE" => !sig.get(i + 1).is_some_and(|n| n.is_keyword("FROM")),
            "JOIN" => true,
            _ => false,
        };
        if up == "OF" && i > 0 && sig[i - 1].is_keyword("INSTANCE") {
            if let Some(n) = sig.get(i + 1).filter(|n| n.kind == TokenKind::Identifier) {
                entity_refs.push((n.start, n.end));
            }
        }
        if !declares {
            i += 1;
            continue;
        }
        // One or more `Entity [AS] alias` / `path [AS] alias` declarations.
        let mut j = i + 1;
        while let Some(target) = sig.get(j).filter(|n| n.kind == TokenKind::Identifier) {
            let mut k = j + 1;
            if sig.get(k).is_some_and(|n| n.is_keyword("AS")) {
                k += 1;
            }
            let is_path = up == "JOIN" && target.text.contains('.') && !target.text.contains('\\');
            if !is_path {
                entity_refs.push((target.start, target.end));
            }
            if is_alias_token(sig.get(k)) {
                let alias_tok = sig[k];
                let partial = Query {
                    kind,
                    aliases: aliases.clone(),
                    entity_refs: Vec::new(),
                };
                let entity = if is_path {
                    match resolve_path(snap, &partial, &target.text).0 {
                        PathTarget::Entity(e) => Some(e.class.clone()),
                        _ => None,
                    }
                } else {
                    snap.entity(&target.text).map(|e| e.class.clone())
                };
                aliases.push(AliasDecl {
                    alias: alias_tok.text.clone(),
                    entity,
                    via: is_path.then(|| target.text.clone()),
                    start: alias_tok.start,
                    end: alias_tok.end,
                });
                k += 1;
            }
            // `INDEX BY o.id`
            if sig.get(k).is_some_and(|n| n.is_keyword("INDEX")) {
                k += 3;
            }
            if up == "FROM" && sig.get(k).is_some_and(|n| n.kind == TokenKind::Comma) {
                j = k + 1;
                continue;
            }
            j = k;
            break;
        }
        i = j.max(i + 1);
    }
    Query {
        kind,
        aliases,
        entity_refs,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpanKind {
    Keyword,
    Function,
    Entity,
    Alias,
    Field,
    Association,
    Parameter,
    String,
    Number,
    Operator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

/// Semantic token spans for highlighting a DQL string.
pub fn highlight(snap: &Snapshot, dql: &str, preset: &[(String, String)]) -> Vec<Span> {
    let query = parse(snap, dql, preset);
    let mut out = Vec::new();
    for t in tokenize(dql) {
        let simple = match t.kind {
            TokenKind::Keyword => Some(SpanKind::Keyword),
            TokenKind::Function => Some(SpanKind::Function),
            TokenKind::String => Some(SpanKind::String),
            TokenKind::Number => Some(SpanKind::Number),
            TokenKind::Parameter => Some(SpanKind::Parameter),
            TokenKind::Operator => Some(SpanKind::Operator),
            _ => None,
        };
        if let Some(kind) = simple {
            out.push(Span {
                start: t.start,
                end: t.end,
                kind,
            });
            continue;
        }
        if t.kind != TokenKind::Identifier {
            continue;
        }
        if query.entity_refs.contains(&(t.start, t.end)) {
            out.push(Span {
                start: t.start,
                end: t.end,
                kind: SpanKind::Entity,
            });
            continue;
        }
        // alias[.part[.part]]
        let mut pos = t.start;
        for (n, part) in t.text.split('.').enumerate() {
            let end = pos + part.len();
            let kind = if n == 0 {
                query.alias(part).map(|_| SpanKind::Alias)
            } else {
                let prefix = &t.text[..end - t.start];
                let parent = &t.text[..pos - t.start - 1];
                match resolve_path(snap, &query, parent).0 {
                    PathTarget::Entity(e) => {
                        let last = prefix.rsplit('.').next().unwrap_or("");
                        if e.association(last).is_some() {
                            Some(SpanKind::Association)
                        } else {
                            Some(SpanKind::Field)
                        }
                    }
                    _ => Some(SpanKind::Field),
                }
            };
            if let Some(kind) = kind {
                if end > pos {
                    out.push(Span {
                        start: pos,
                        end,
                        kind,
                    });
                }
            }
            pos = end + 1;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Keyword,
    Function,
    Entity,
    Alias,
    Field,
    Association,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// Start of the replaced text (byte offset in the DQL).
    pub from: usize,
    pub items: Vec<Item>,
}

fn short(fqn: &str) -> &str {
    fqn.rsplit('\\').next().unwrap_or(fqn)
}

fn member_items(entity: &Entity) -> Vec<Item> {
    let mut out = Vec::new();
    for f in &entity.fields {
        out.push(Item {
            label: f.name.clone(),
            kind: if f.embedded {
                ItemKind::Embedded
            } else {
                ItemKind::Field
            },
            detail: if f.embedded {
                short(&f.type_name).to_string()
            } else {
                f.type_name.clone()
            },
            apply: None,
        });
    }
    for a in &entity.associations {
        let target = short(&a.target);
        out.push(Item {
            label: a.name.clone(),
            kind: ItemKind::Association,
            detail: if a.kind.is_collection() {
                format!("Collection<{target}>")
            } else {
                target.to_string()
            },
            apply: None,
        });
    }
    out
}

fn entity_items(snap: &Snapshot) -> Vec<Item> {
    snap.entities
        .iter()
        .filter(|e| !e.embeddable && !e.mapped_superclass)
        .map(|e| Item {
            label: e.name.clone(),
            kind: ItemKind::Entity,
            detail: e.class.clone(),
            apply: Some(e.class.clone()),
        })
        .collect()
}

fn alias_items(query: &Query) -> Vec<Item> {
    let mut seen = std::collections::HashSet::new();
    query
        .aliases
        .iter()
        .rev()
        .filter(|a| seen.insert(a.alias.clone()))
        .map(|a| Item {
            label: a.alias.clone(),
            kind: ItemKind::Alias,
            detail: a.entity.as_deref().map(short).unwrap_or("").to_string(),
            apply: None,
        })
        .collect()
}

fn keyword_items(words: &[&str]) -> Vec<Item> {
    words
        .iter()
        .map(|k| Item {
            label: (*k).to_string(),
            kind: ItemKind::Keyword,
            detail: String::new(),
            apply: None,
        })
        .collect()
}

fn function_items() -> Vec<Item> {
    FUNCTIONS
        .iter()
        .map(|(f, sig)| Item {
            label: (*f).to_string(),
            kind: ItemKind::Function,
            detail: (*sig).to_string(),
            apply: None,
        })
        .collect()
}

const CLAUSE_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "JOIN",
    "LEFT JOIN",
    "INNER JOIN",
    "WITH",
    "AND",
    "OR",
    "NOT",
    "IN",
    "IS NULL",
    "IS NOT NULL",
    "LIKE",
    "BETWEEN",
    "MEMBER OF",
    "INSTANCE OF",
    "EXISTS",
    "ORDER BY",
    "GROUP BY",
    "HAVING",
    "ASC",
    "DESC",
    "AS",
    "DISTINCT",
    "NEW",
    "PARTIAL",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "INDEX BY",
    "HIDDEN",
    "UPDATE",
    "SET",
    "DELETE",
    "TRUE",
    "FALSE",
    "NULL",
];

/// Suggested identification variable for an entity (`Order` -> `o`).
fn alias_suggestions(entity: &str) -> Vec<String> {
    let name = short(entity);
    let first = name
        .chars()
        .next()
        .map(|c| c.to_ascii_lowercase().to_string())
        .unwrap_or_default();
    let mut lower = name.to_string();
    if let Some(r) = lower.get_mut(0..1) {
        r.make_ascii_lowercase();
    }
    let mut out = vec![first];
    if lower.len() > 1 {
        out.push(lower);
    }
    out
}

/// Completion at `cursor` (byte offset into `dql`).
pub fn complete(
    snap: &Snapshot,
    dql: &str,
    cursor: usize,
    mode: Mode,
    preset: &[(String, String)],
) -> Completion {
    let cursor = cursor.min(dql.len());
    let query = parse(snap, dql, preset);
    let tokens = tokenize(&dql[..cursor]);
    // Inside a string literal or parameter: nothing to complete.
    if let Some(last) = tokens.last() {
        let unterminated = last.kind == TokenKind::String
            && (last.text.len() < 2 || !last.text.ends_with(&last.text[..1]));
        if (unterminated || last.kind == TokenKind::Parameter) && last.end == cursor {
            return Completion {
                from: cursor,
                items: Vec::new(),
            };
        }
    }
    let word_tok = tokens.last().filter(|t| {
        t.end == cursor
            && matches!(
                t.kind,
                TokenKind::Identifier | TokenKind::Keyword | TokenKind::Function
            )
    });
    let word = word_tok.map(|t| t.text.as_str()).unwrap_or("");
    let word_start = word_tok.map(|t| t.start).unwrap_or(cursor);
    let prev: Vec<&Token> = significant(&tokens)
        .into_iter()
        .filter(|t| t.end <= word_start)
        .collect();
    let prev_kw = |n: usize| -> String {
        prev.iter()
            .rev()
            .nth(n)
            .map(|t| {
                if t.kind == TokenKind::Keyword {
                    t.upper()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default()
    };
    let virtual_prev = match mode {
        Mode::Entity => "FROM",
        Mode::Join => "JOIN",
        _ => "",
    };
    let p0 = if prev.is_empty() {
        virtual_prev.to_string()
    } else {
        prev_kw(0)
    };

    // `alias.part.` — members of the resolved entity.
    if let Some(dot) = word.rfind('.') {
        let head = &word[..dot];
        let mut items = match resolve_path(snap, &query, head).0 {
            PathTarget::Entity(e) => member_items(e),
            _ => Vec::new(),
        };
        if p0 == "JOIN" {
            items.retain(|i| i.kind == ItemKind::Association);
        }
        return finish(word_start + dot + 1, &word[dot + 1..], items);
    }

    let mut items: Vec<Item> = Vec::new();
    match p0.as_str() {
        "FROM" | "UPDATE" => items = entity_items(snap),
        "DELETE" => {
            items = entity_items(snap);
            items.extend(keyword_items(&["FROM"]));
        }
        "JOIN" => {
            // Association paths of the known aliases, then entities (arbitrary joins).
            for a in &query.aliases {
                if let Some(e) = a.entity.as_deref().and_then(|e| snap.entity(e)) {
                    for assoc in &e.associations {
                        items.push(Item {
                            label: format!("{}.{}", a.alias, assoc.name),
                            kind: ItemKind::Association,
                            detail: short(&assoc.target).to_string(),
                            apply: None,
                        });
                    }
                }
            }
            items.extend(entity_items(snap));
        }
        "OF" if prev_kw(1) == "INSTANCE" => items = entity_items(snap),
        "ORDER" | "GROUP" | "INDEX" => items = keyword_items(&["BY"]),
        "LEFT" | "INNER" | "OUTER" => items = keyword_items(&["JOIN"]),
        "IS" => items = keyword_items(&["NULL", "NOT NULL", "EMPTY", "NOT EMPTY"]),
        "MEMBER" => items = keyword_items(&["OF"]),
        _ => {
            // Right after an entity in FROM/JOIN: suggest an identification variable.
            let last = prev.last();
            let declared_entity = last
                .filter(|t| t.kind == TokenKind::Identifier)
                .and_then(|t| {
                    let before = prev
                        .iter()
                        .rev()
                        .nth(1)
                        .map(|b| b.upper())
                        .unwrap_or_default();
                    let decl =
                        matches!(before.as_str(), "FROM" | "JOIN" | "UPDATE" | "DELETE" | ",")
                            || (prev.len() == 1 && matches!(mode, Mode::Entity | Mode::Join));
                    if !decl {
                        return None;
                    }
                    if t.text.contains('.') {
                        match resolve_path(snap, &query, &t.text).0 {
                            PathTarget::Entity(e) => Some(e.class.clone()),
                            _ => None,
                        }
                    } else {
                        snap.entity(&t.text).map(|e| e.class.clone())
                    }
                });
            if let Some(entity) = declared_entity {
                for s in alias_suggestions(&entity) {
                    items.push(Item {
                        label: s,
                        kind: ItemKind::Alias,
                        detail: short(&entity).to_string(),
                        apply: None,
                    });
                }
                items.extend(keyword_items(&["AS", "INDEX BY", "WITH"]));
            } else {
                items = alias_items(&query);
                items.extend(function_items());
                items.extend(keyword_items(CLAUSE_KEYWORDS));
            }
        }
    }
    finish(word_start, word, items)
}

fn finish(from: usize, prefix: &str, items: Vec<Item>) -> Completion {
    let p = prefix.to_ascii_lowercase();
    let mut items: Vec<Item> = items
        .into_iter()
        .filter(|i| {
            p.is_empty()
                || i.label.to_ascii_lowercase().starts_with(&p)
                || (i.kind == ItemKind::Entity && i.detail.to_ascii_lowercase().starts_with(&p))
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert((i.label.clone(), i.kind)));
    Completion { from, items }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// Unknown entities, aliases and fields.
pub fn diagnostics(snap: &Snapshot, dql: &str, preset: &[(String, String)]) -> Vec<Diagnostic> {
    if snap.entities.is_empty() {
        return Vec::new();
    }
    let query = parse(snap, dql, preset);
    let mut out = Vec::new();
    for t in tokenize(dql) {
        if t.kind != TokenKind::Identifier {
            continue;
        }
        if query.entity_refs.contains(&(t.start, t.end)) {
            if snap.entity(&t.text).is_none() {
                out.push(Diagnostic {
                    start: t.start,
                    end: t.end,
                    message: format!("Unknown entity '{}'", t.text),
                });
            }
            continue;
        }
        if !t.text.contains('.') {
            continue;
        }
        let first = t.text.split('.').next().unwrap_or("");
        let Some(decl) = query.alias(first) else {
            out.push(Diagnostic {
                start: t.start,
                end: t.start + first.len(),
                message: format!("Undefined identification variable '{first}'"),
            });
            continue;
        };
        if decl
            .entity
            .as_deref()
            .and_then(|e| snap.entity(e))
            .is_none()
        {
            continue;
        }
        let (target, resolved) = resolve_path(snap, &query, &t.text);
        let parts: Vec<&str> = t.text.split('.').collect();
        if matches!(target, PathTarget::Unknown) || resolved < parts.len() {
            let bad = parts.get(resolved).copied().unwrap_or("");
            if bad.is_empty() {
                // `o.` while typing.
                continue;
            }
            let start = t.start + parts[..resolved].iter().map(|p| p.len() + 1).sum::<usize>();
            let owner = match resolve_path(snap, &query, &parts[..resolved].join(".")).0 {
                PathTarget::Entity(e) => e.name.clone(),
                _ => String::new(),
            };
            out.push(Diagnostic {
                start,
                end: start + bad.len(),
                message: format!("Field '{bad}' not found in {owner}"),
            });
        }
    }
    out
}

/// Kind label for association kinds in the UI.
pub fn association_label(kind: AssociationKind) -> &'static str {
    match kind {
        AssociationKind::ManyToOne => "ManyToOne",
        AssociationKind::OneToMany => "OneToMany",
        AssociationKind::ManyToMany => "ManyToMany",
        AssociationKind::OneToOne => "OneToOne",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    const Q: &str = "SELECT o, c FROM App\\Entity\\Order o JOIN o.customer c LEFT JOIN o.items i WHERE o.status = :s";

    #[test]
    fn parses_aliases_through_joins() {
        let snap = fixture::snapshot();
        let q = parse(&snap, Q, &[]);
        assert_eq!(q.kind, StatementKind::Select);
        let names: Vec<(&str, Option<&str>)> = q
            .aliases
            .iter()
            .map(|a| (a.alias.as_str(), a.entity.as_deref()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("o", Some("App\\Entity\\Order")),
                ("c", Some("App\\Entity\\Customer")),
                ("i", Some("App\\Entity\\OrderItem")),
            ]
        );
        assert_eq!(q.aliases[1].via.as_deref(), Some("o.customer"));
        assert_eq!(q.entity_refs.len(), 1);
    }

    #[test]
    fn multiple_from_and_update() {
        let snap = fixture::snapshot();
        let q = parse(
            &snap,
            "SELECT o, p FROM Order o, App\\Entity\\Product AS p",
            &[],
        );
        assert_eq!(q.aliases.len(), 2);
        assert_eq!(q.aliases[1].entity.as_deref(), Some("App\\Entity\\Product"));
        let q = parse(
            &snap,
            "UPDATE App\\Entity\\Order o SET o.status = 'paid'",
            &[],
        );
        assert_eq!(q.kind, StatementKind::Update);
        assert_eq!(q.aliases[0].alias, "o");
        let q = parse(&snap, "DELETE App\\Entity\\Order o WHERE o.id = 1", &[]);
        assert_eq!(q.aliases[0].entity.as_deref(), Some("App\\Entity\\Order"));
    }

    #[test]
    fn completes_fields_and_associations() {
        let snap = fixture::snapshot();
        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o WHERE o.¦");
        let c = complete(&snap, &dql, pos, Mode::Full, &[]);
        assert_eq!(c.from, pos);
        let l = labels(&c);
        assert!(l.contains(&"status") && l.contains(&"customer") && l.contains(&"items"));
        let items = c.items.iter().find(|i| i.label == "items").unwrap();
        assert_eq!(items.detail, "Collection<OrderItem>");

        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o WHERE o.customer.em¦");
        let c = complete(&snap, &dql, pos, Mode::Full, &[]);
        assert_eq!(labels(&c), vec!["email"]);

        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o WHERE o.shippingAddress.c¦");
        assert_eq!(
            labels(&complete(&snap, &dql, pos, Mode::Full, &[])),
            vec!["city"]
        );
    }

    #[test]
    fn completes_entities_aliases_joins() {
        let snap = fixture::snapshot();
        let (dql, pos) = at("SELECT o FROM Or¦");
        let c = complete(&snap, &dql, pos, Mode::Full, &[]);
        assert_eq!(labels(&c), vec!["Order", "OrderItem"]);
        assert_eq!(c.items[0].apply.as_deref(), Some("App\\Entity\\Order"));
        assert!(
            !labels(&complete(&snap, "SELECT a FROM ", 14, Mode::Full, &[])).contains(&"Address")
        );

        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order ¦");
        assert_eq!(
            labels(&complete(&snap, &dql, pos, Mode::Full, &[]))[..2],
            ["o", "order"]
        );

        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o JOIN ¦");
        let c = complete(&snap, &dql, pos, Mode::Full, &[]);
        assert_eq!(labels(&c)[..2], ["o.customer", "o.items"]);

        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o JOIN o.cu¦");
        assert_eq!(
            labels(&complete(&snap, &dql, pos, Mode::Full, &[])),
            vec!["customer"]
        );
    }

    #[test]
    fn completes_keywords_functions_and_aliases() {
        let snap = fixture::snapshot();
        let (dql, pos) = at("SELECT o FROM App\\Entity\\Order o ORDER ¦");
        assert_eq!(
            labels(&complete(&snap, &dql, pos, Mode::Full, &[])),
            vec!["BY"]
        );

        let (dql, pos) = at("SELECT CO¦ FROM App\\Entity\\Order o");
        let l = labels(&complete(&snap, &dql, pos, Mode::Full, &[]))
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert!(l.contains(&"COUNT".to_string()) && l.contains(&"CONCAT".to_string()));

        let (dql, pos) = at("SELECT ¦ FROM App\\Entity\\Order o");
        let c = complete(&snap, &dql, pos, Mode::Full, &[]);
        assert_eq!(c.items[0].label, "o");
        assert_eq!(c.items[0].kind, ItemKind::Alias);

        let (dql, pos) = at("WHERE o.name = 'x¦'");
        assert!(complete(&snap, &dql, pos, Mode::Full, &[]).items.is_empty());
    }

    #[test]
    fn fragments_use_preset_aliases() {
        let snap = fixture::snapshot();
        let preset = vec![("o".to_string(), "App\\Entity\\Order".to_string())];
        let c = complete(&snap, "o.st", 4, Mode::Expression, &preset);
        assert_eq!(labels(&c), vec!["status"]);
        let c = complete(&snap, "o.", 2, Mode::Join, &preset);
        assert_eq!(labels(&c), vec!["customer", "items"]);
        let c = complete(&snap, "App\\Entity\\Cu", 13, Mode::Entity, &[]);
        assert_eq!(labels(&c), vec!["Customer"]);
    }

    #[test]
    fn highlights_semantically() {
        let snap = fixture::snapshot();
        let spans = highlight(&snap, Q, &[]);
        let text = |s: &Span| &Q[s.start..s.end];
        let find = |k: SpanKind| {
            spans
                .iter()
                .filter(|s| s.kind == k)
                .map(text)
                .collect::<Vec<_>>()
        };
        assert_eq!(find(SpanKind::Entity), vec!["App\\Entity\\Order"]);
        assert!(find(SpanKind::Keyword).contains(&"LEFT"));
        assert_eq!(find(SpanKind::Association), vec!["customer", "items"]);
        assert_eq!(find(SpanKind::Field), vec!["status"]);
        assert_eq!(find(SpanKind::Parameter), vec![":s"]);
        assert!(find(SpanKind::Alias).iter().filter(|a| **a == "o").count() >= 3);
    }

    #[test]
    fn reports_unknown_names() {
        let snap = fixture::snapshot();
        let d = diagnostics(
            &snap,
            "SELECT x FROM App\\Entity\\Nope x WHERE y.id = 1",
            &[],
        );
        assert_eq!(d[0].message, "Unknown entity 'App\\Entity\\Nope'");
        assert_eq!(d[1].message, "Undefined identification variable 'y'");
        let dql = "SELECT o FROM App\\Entity\\Order o WHERE o.customer.nme = 1";
        let d = diagnostics(&snap, dql, &[]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "Field 'nme' not found in Customer");
        assert_eq!(&dql[d[0].start..d[0].end], "nme");
        assert!(diagnostics(&snap, Q, &[]).is_empty());
        // A path still being typed is not a problem yet.
        assert!(diagnostics(&snap, "SELECT o FROM App\\Entity\\Order o WHERE o.", &[]).is_empty());
    }
}

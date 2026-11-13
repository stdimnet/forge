//! Finding the schemas that apply at a place in the document: `$ref`s, `allOf`, the
//! `anyOf`/`oneOf` branches that fit what is written, `if`/`then`/`else`, then
//! `properties` / `patternProperties` / `additionalProperties` and `items` step by step.

use crate::schema::{Registry, Sch, SchemaDoc};
use crate::tree::{Node, Seg, Value};
use crate::validate::{Kind, Validator};

/// A schema reached through a property, with where that property is declared.
#[derive(Debug, Clone, Copy)]
pub struct At<'r> {
    pub sch: Sch<'r>,
    /// The key of the declaration in the schema file (`"name": {…}` in `properties`).
    pub decl: Option<(&'r SchemaDoc, usize, usize)>,
}

const MAX_DEPTH: usize = 24;

/// `s` and the schemas it pulls in for `node` (when known).
pub fn expand<'r>(reg: &'r Registry, s: Sch<'r>, node: Option<&Node>) -> Vec<Sch<'r>> {
    let mut out = Vec::new();
    expand_into(reg, s, node, &mut out, 0);
    out
}

fn push<'r>(out: &mut Vec<Sch<'r>>, s: Sch<'r>) -> bool {
    if out.iter().any(|o| o.same(&s)) {
        return false;
    }
    out.push(s);
    true
}

/// A branch fits `node` when the value has the branch's type and values
/// (missing or extra properties are fine while typing).
pub fn compatible<'r>(reg: &'r Registry, node: &Node, s: Sch<'r>) -> bool {
    let mut v = Validator::new(reg);
    v.validate(node, s, None)
        .problems
        .iter()
        .all(|p| !matches!(p.kind, Kind::Type(_) | Kind::Enum(_)))
}

fn expand_into<'r>(
    reg: &'r Registry,
    s: Sch<'r>,
    node: Option<&Node>,
    out: &mut Vec<Sch<'r>>,
    depth: usize,
) {
    if depth > MAX_DEPTH || !push(out, s) || s.node.as_object().is_none() {
        return;
    }
    for kw in ["$ref", "$dynamicRef", "$recursiveRef"] {
        if let Some(t) = s.str(kw).and_then(|r| reg.resolve(s, r)) {
            expand_into(reg, t, node, out, depth + 1);
        }
    }
    for b in s.get("allOf").and_then(Node::as_array).unwrap_or(&[]) {
        expand_into(reg, reg.enter(s, b), node, out, depth + 1);
    }
    for kw in ["anyOf", "oneOf"] {
        let branches: Vec<Sch<'r>> = s
            .get(kw)
            .and_then(Node::as_array)
            .unwrap_or(&[])
            .iter()
            .map(|b| reg.enter(s, b))
            .collect();
        let fitting: Vec<Sch<'r>> = match node {
            Some(n) => branches
                .iter()
                .copied()
                .filter(|b| compatible(reg, n, *b))
                .collect(),
            None => Vec::new(),
        };
        for b in if fitting.is_empty() {
            branches
        } else {
            fitting
        } {
            expand_into(reg, b, node, out, depth + 1);
        }
    }
    if let (Some(cond), Some(n)) = (s.get("if"), node) {
        let mut v = Validator::new(reg);
        let branch = if v.matches(n, reg.enter(s, cond)) {
            s.get("then")
        } else {
            s.get("else")
        };
        if let Some(b) = branch {
            expand_into(reg, reg.enter(s, b), node, out, depth + 1);
        }
    }
    if let Some(n) = node {
        for d in s
            .get("dependentSchemas")
            .and_then(Node::as_object)
            .unwrap_or(&[])
        {
            if let (Some(v), Some(_)) = (&d.value, n.get(&d.key)) {
                expand_into(reg, reg.enter(s, v), node, out, depth + 1);
            }
        }
    }
}

fn decl<'r>(s: Sch<'r>, container: &str, key: &str) -> Option<(&'r SchemaDoc, usize, usize)> {
    let p = s.get(container)?.property(key)?;
    Some((s.doc, p.key_start, p.key_end))
}

/// The schemas of property `key` in objects described by `schemas` (already expanded).
pub fn property<'r>(reg: &'r Registry, schemas: &[Sch<'r>], key: &str) -> Vec<At<'r>> {
    let mut out: Vec<At<'r>> = Vec::new();
    let mut add = |at: At<'r>| {
        if !out.iter().any(|o| o.sch.same(&at.sch)) {
            out.push(at);
        }
    };
    for s in schemas {
        let mut found = false;
        if let Some(ps) = s.get("properties").and_then(|p| p.get(key)) {
            found = true;
            add(At {
                sch: reg.enter(*s, ps),
                decl: decl(*s, "properties", key),
            });
        }
        for pat in s
            .get("patternProperties")
            .and_then(Node::as_object)
            .unwrap_or(&[])
        {
            let Some(ps) = &pat.value else { continue };
            let mut v = Validator::new(reg);
            if v.regex(&pat.key).is_some_and(|re| re.is_match(key)) {
                found = true;
                add(At {
                    sch: reg.enter(*s, ps),
                    decl: Some((s.doc, pat.key_start, pat.key_end)),
                });
            }
        }
        if !found {
            if let Some(a) = s
                .get("additionalProperties")
                .filter(|a| a.as_object().is_some())
            {
                add(At {
                    sch: reg.enter(*s, a),
                    decl: s
                        .node
                        .property("additionalProperties")
                        .map(|p| (s.doc, p.key_start, p.key_end)),
                });
            }
        }
    }
    out
}

/// The schemas of item `index` of arrays described by `schemas` (already expanded).
pub fn item<'r>(reg: &'r Registry, schemas: &[Sch<'r>], index: usize) -> Vec<At<'r>> {
    let mut out = Vec::new();
    for s in schemas {
        let prefix = s
            .get("prefixItems")
            .and_then(Node::as_array)
            .or_else(|| s.get("items").and_then(Node::as_array));
        let sub = match prefix.and_then(|p| p.get(index)) {
            Some(p) => Some(p),
            None if s.get("prefixItems").is_some() || prefix.is_none() => {
                s.get("items").filter(|i| i.as_array().is_none())
            }
            None => s.get("additionalItems"),
        };
        if let Some(sub) = sub.filter(|x| x.as_object().is_some()) {
            out.push(At {
                sch: reg.enter(*s, sub),
                decl: None,
            });
        }
    }
    out
}

fn child<'a>(node: Option<&'a Node>, seg: &Seg) -> Option<&'a Node> {
    let n = node?;
    match seg {
        Seg::Key(k) => n.get(k),
        Seg::Index(i) => n.as_array()?.get(*i),
    }
}

/// The schemas at `path` in the document (not expanded), with the document's node there.
pub fn at_path<'r, 'd>(
    reg: &'r Registry,
    root: Sch<'r>,
    doc: Option<&'d Node>,
    path: &[Seg],
) -> (Vec<At<'r>>, Option<&'d Node>) {
    let mut cur = vec![At {
        sch: root,
        decl: None,
    }];
    let mut node = doc;
    for seg in path {
        let mut expanded = Vec::new();
        for at in &cur {
            for s in expand(reg, at.sch, node) {
                push(&mut expanded, s);
            }
        }
        cur = match seg {
            Seg::Key(k) => property(reg, &expanded, k),
            Seg::Index(i) => item(reg, &expanded, *i),
        };
        node = child(node, seg);
        if cur.is_empty() {
            break;
        }
    }
    (cur, node)
}

/// All schemas at `path`, expanded for the node written there.
pub fn expanded_at<'r>(
    reg: &'r Registry,
    root: Sch<'r>,
    doc: Option<&Node>,
    path: &[Seg],
) -> Vec<Sch<'r>> {
    let (ats, node) = at_path(reg, root, doc, path);
    let mut out = Vec::new();
    for at in ats {
        for s in expand(reg, at.sch, node) {
            push(&mut out, s);
        }
    }
    out
}

/// What a schema accepts, for completion details and hover titles: `string`,
/// `integer | string`, `object`, …
pub fn type_label(schemas: &[Sch<'_>]) -> Option<String> {
    let mut types: Vec<String> = Vec::new();
    for s in schemas {
        let mut own: Vec<String> = s.types().iter().map(|t| t.to_string()).collect();
        if own.is_empty() {
            if s.get("properties").is_some() || s.get("additionalProperties").is_some() {
                own.push("object".into());
            } else if s.get("items").is_some() || s.get("prefixItems").is_some() {
                own.push("array".into());
            } else if let Some(values) = s.get("enum").and_then(Node::as_array) {
                own.extend(
                    values
                        .iter()
                        .map(|v| v.type_name().replace("integer", "number")),
                );
            } else if let Some(c) = s.get("const") {
                own.push(c.type_name().replace("integer", "number"));
            }
        }
        for t in own {
            if !types.contains(&t) {
                types.push(t);
            }
        }
    }
    if types.contains(&"number".to_string()) {
        types.retain(|t| t != "integer");
    }
    (!types.is_empty()).then(|| types.join(" | "))
}

/// The first description among the schemas.
pub fn description<'r>(schemas: &[Sch<'r>]) -> Option<&'r str> {
    schemas.iter().find_map(|s| s.description())
}

/// What the cursor is on, for hover and Go to Declaration.
#[derive(Debug)]
pub enum Hit<'d> {
    /// A property key: the path to its object, the key text and its range.
    Key(Vec<Seg>, &'d str, (usize, usize)),
    /// A scalar value: its path.
    Value(Vec<Seg>, &'d Node),
}

pub fn locate(n: &Node, offset: usize) -> Option<Hit<'_>> {
    let mut path = Vec::new();
    locate_in(n, offset, &mut path)
}

fn locate_in<'d>(n: &'d Node, offset: usize, path: &mut Vec<Seg>) -> Option<Hit<'d>> {
    match &n.value {
        Value::Object(props) => {
            for p in props {
                if p.key_start <= offset && offset <= p.key_end && p.key_end > p.key_start {
                    return Some(Hit::Key(
                        path.clone(),
                        p.key.as_str(),
                        (p.key_start, p.key_end),
                    ));
                }
                if let Some(v) = &p.value {
                    if v.start <= offset && offset <= v.end && v.end > v.start {
                        path.push(Seg::Key(p.key.clone()));
                        let hit = locate_in(v, offset, path);
                        path.pop();
                        if hit.is_some() {
                            return hit;
                        }
                    }
                }
            }
            None
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                if item.start <= offset && offset <= item.end && item.end > item.start {
                    path.push(Seg::Index(i));
                    let hit = locate_in(item, offset, path);
                    path.pop();
                    if hit.is_some() {
                        return hit;
                    }
                }
            }
            None
        }
        _ => Some(Hit::Value(path.clone(), n)),
    }
}

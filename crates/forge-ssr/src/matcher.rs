//! Matching a template's tree against a file's tree.
//!
//! Two nodes match when their kinds are equal and their significant children (named nodes,
//! keywords and operators; comments and punctuation are left out) match in order; leaves
//! compare their text. A template node whose text is a placeholder matches any node, or a
//! run of sibling nodes within the variable's count.

use tree_sitter::{Node, Tree};

use crate::template::Pattern;
use crate::{types, Compiled};

/// A match before it is positioned: byte range and each variable's byte range.
pub(crate) struct Found {
    pub start: usize,
    pub end: usize,
    pub bindings: Vec<Option<(usize, usize)>>,
}

/// Punctuation that does not distinguish trees of the same kind.
fn is_punctuation(kind: &str) -> bool {
    matches!(kind, "," | ";" | "(" | ")" | "{" | "}" | "[" | "]")
}

fn significant<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() != "comment" && !(!c.is_named() && is_punctuation(c.kind())))
        .collect()
}

/// Parts of a declaration a template may leave out: a template without a return type,
/// attributes or modifiers matches declarations with any.
const OPTIONAL_FIELDS: &[&str] = &["return_type", "attributes"];
const OPTIONAL_KINDS: &[&str] = &[
    "static_modifier",
    "final_modifier",
    "abstract_modifier",
    "readonly_modifier",
    "attribute_list",
    "decorator",
    "accessibility_modifier",
    "async",
];

/// The significant children of a target node, without the optional parts the template's
/// node `p` does not have.
fn target_children<'t>(p: Node, pc: &[Node], t: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = t.walk();
    let mut fields: Vec<(Node<'t>, Option<&'static str>)> = Vec::new();
    for (i, c) in t.children(&mut cursor).enumerate() {
        if c.kind() == "comment" || !c.is_named() && is_punctuation(c.kind()) {
            continue;
        }
        fields.push((c, t.field_name_for_child(i as u32)));
    }
    let mut pcursor = p.walk();
    let p_fields: Vec<&str> = p
        .children(&mut pcursor)
        .enumerate()
        .filter_map(|(i, _)| p.field_name_for_child(i as u32))
        .collect();
    let dropped = |n: &Node, field: Option<&str>| {
        field.is_some_and(|f| OPTIONAL_FIELDS.contains(&f) && !p_fields.contains(&f))
            || OPTIONAL_KINDS.contains(&n.kind()) && !pc.iter().any(|x| x.kind() == n.kind())
    };
    let mut out = Vec::new();
    for (i, (c, field)) in fields.iter().enumerate() {
        if dropped(c, *field) {
            continue;
        }
        // The `:` before a dropped return type.
        if c.kind() == ":" && fields.get(i + 1).is_some_and(|(n, f)| dropped(n, *f)) {
            continue;
        }
        out.push(*c);
    }
    out
}

/// Whitespace collapsed, for comparing two occurrences of a variable.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone)]
struct Bound {
    start: usize,
    end: usize,
    norm: String,
}

struct Matcher<'a> {
    c: &'a Compiled,
    p: &'a Pattern,
    text: &'a str,
}

impl<'a> Matcher<'a> {
    fn ptext(&self, n: Node) -> &'a str {
        &self.p.source[n.start_byte()..n.end_byte()]
    }

    fn ttext(&self, n: Node) -> &'a str {
        &self.text[n.start_byte()..n.end_byte()]
    }

    fn placeholder(&self, n: Node) -> Option<usize> {
        if n.end_byte() - n.start_byte() > self.p.longest_placeholder + 2 {
            return None;
        }
        let t = self.ptext(n).trim();
        let t = t.strip_suffix(';').unwrap_or(t).trim_end();
        self.p.placeholders.get(t).copied()
    }

    fn text_ok(&self, var: usize, n: Node) -> bool {
        let spec = &self.c.vars[var];
        let Some(re) = &spec.text else { return true };
        let t = self.ttext(n);
        // `$user` also matches `user`: PHP variables are named without the `$`.
        let hit = re.is_match(t) || t.strip_prefix('$').is_some_and(|s| re.is_match(s));
        hit != spec.invert_text
    }

    fn type_ok(&self, var: usize, n: Node) -> bool {
        let spec = &self.c.vars[var];
        let Some(re) = &spec.type_re else { return true };
        let hit = types::infer(n, self.text).is_some_and(|t| {
            re.is_match(&t)
                || t.rsplit('\\')
                    .next()
                    .is_some_and(|short| re.is_match(short))
        });
        hit != spec.invert_type
    }

    fn bind(&self, var: usize, nodes: &[Node], b: &mut [Option<Bound>]) -> bool {
        if self.p.var_only.get(var).copied().unwrap_or(false)
            && nodes.iter().any(|n| n.kind() != "variable_name")
        {
            return false;
        }
        if !nodes
            .iter()
            .all(|n| self.text_ok(var, *n) && self.type_ok(var, *n))
        {
            return false;
        }
        let (start, end) = match (nodes.first(), nodes.last()) {
            (Some(f), Some(l)) => (f.start_byte(), l.end_byte()),
            _ => (0, 0),
        };
        let norm = normalize(&self.text[start..end]);
        match &b[var] {
            Some(prev) => {
                if self.c.match_case {
                    prev.norm == norm
                } else {
                    prev.norm.eq_ignore_ascii_case(&norm)
                }
            }
            None => {
                b[var] = Some(Bound { start, end, norm });
                true
            }
        }
    }

    fn node(&self, p: Node, t: Node, b: &mut Vec<Option<Bound>>) -> bool {
        if let Some(var) = self.placeholder(p) {
            let spec = &self.c.vars[var];
            if spec.min > 1 || spec.max == Some(0) {
                return false;
            }
            return self.bind(var, &[t], b);
        }
        if p.kind() != t.kind() {
            return false;
        }
        if p.child_count() == 0 || t.child_count() == 0 {
            if p.child_count() != t.child_count() {
                return false;
            }
            let (a, z) = (self.ptext(p), self.ttext(t));
            return if self.c.match_case {
                a == z
            } else {
                a.eq_ignore_ascii_case(z)
            };
        }
        let pc = significant(p);
        let tc = target_children(p, &pc, t);
        self.seq(&pc, &tc, b)
    }

    fn seq(&self, p: &[Node], t: &[Node], b: &mut Vec<Option<Bound>>) -> bool {
        let Some(&first) = p.first() else {
            return t.is_empty();
        };
        if let Some(var) = self.placeholder(first) {
            let spec = &self.c.vars[var];
            let min = spec.min as usize;
            let max = spec.max.map_or(t.len(), |m| (m as usize).min(t.len()));
            // Greedy: the most nodes first, like an argument list taken as a whole.
            for n in (min..=max).rev() {
                let saved = b.clone();
                if self.bind(var, &t[..n], b) && self.seq(&p[1..], &t[n..], b) {
                    return true;
                }
                *b = saved;
            }
            return false;
        }
        let Some(&head) = t.first() else {
            return false;
        };
        let saved = b.clone();
        if self.node(first, head, b) && self.seq(&p[1..], &t[1..], b) {
            return true;
        }
        *b = saved;
        false
    }
}

fn result(b: Vec<Option<Bound>>, start: usize, end: usize) -> Found {
    Found {
        start,
        end,
        bindings: b.into_iter().map(|x| x.map(|x| (x.start, x.end))).collect(),
    }
}

/// Every match of `pattern` in `tree` (document order, nested matches included).
pub(crate) fn find(c: &Compiled, pattern: &Pattern, tree: &Tree, text: &str) -> Vec<Found> {
    let m = Matcher {
        c,
        p: pattern,
        text,
    };
    let roots = pattern.roots();
    let mut out: Vec<Found> = Vec::new();
    let mut stack = vec![tree.root_node()];
    let single = roots.len() == 1;
    let root_kind = roots[0].kind();
    let root_is_var = m.placeholder(roots[0]).is_some();
    // Depth-first in document order.
    let mut order = Vec::new();
    while let Some(n) = stack.pop() {
        order.push(n);
        let mut cursor = n.walk();
        let children: Vec<Node> = n.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    for n in order {
        if n.is_error() || n.kind() == "comment" {
            continue;
        }
        if single {
            if !root_is_var && n.kind() != root_kind {
                continue;
            }
            if root_is_var && (n.kind() == "program" || n.parent().is_none()) {
                continue;
            }
            let mut b = vec![None; c.vars.len()];
            if m.node(roots[0], n, &mut b)
                && !out
                    .iter()
                    .any(|f| f.start == n.start_byte() && f.end == n.end_byte())
            {
                out.push(result(b, n.start_byte(), n.end_byte()));
            }
        } else {
            let mut cursor = n.walk();
            let kids: Vec<Node> = n
                .named_children(&mut cursor)
                .filter(|k| k.kind() != "comment")
                .collect();
            let mut i = 0;
            while i < kids.len() {
                let mut found = None;
                for j in (i + 1..=kids.len()).rev() {
                    let mut b = vec![None; c.vars.len()];
                    if m.seq(&roots, &kids[i..j], &mut b) {
                        found = Some((j, b));
                        break;
                    }
                }
                match found {
                    Some((j, b)) => {
                        out.push(result(b, kids[i].start_byte(), kids[j - 1].end_byte()));
                        i = j;
                    }
                    None => i += 1,
                }
            }
        }
    }
    out
}

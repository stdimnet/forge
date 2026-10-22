//! Validation with PHPStorm's messages ("Missing required property 'name'", "Incompatible
//! types. Required: string. Actual: integer.", "Property 'x' is not allowed", "Value should
//! be one of: …"). Problems carry byte ranges: the key of a property, the value of a
//! scalar, the opening bracket (or first key) of a collection.

use std::collections::HashMap;

use serde::Serialize;

use crate::schema::{Draft, Registry, Sch};
use crate::tree::{format_number, Node, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// PHPStorm's "JSON schema compliance" inspection reports warnings.
    Warning,
    /// Deprecated properties.
    Weak,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Type(Vec<String>),
    Enum(Vec<String>),
    Missing,
    NotAllowed,
    Deprecated,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub start: usize,
    pub end: usize,
    pub message: String,
    pub severity: Severity,
    pub kind: Kind,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub problems: Vec<Problem>,
    /// Properties evaluated by this schema and its applicators (`unevaluatedProperties`).
    props: Vec<String>,
    /// Items evaluated from the start (`unevaluatedItems`); `usize::MAX` for all.
    items: usize,
    /// The node itself has the wrong type (not one of its children).
    type_mismatch: bool,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.problems
            .iter()
            .all(|p| p.severity != Severity::Warning)
    }

    fn merge(&mut self, other: Outcome) {
        self.problems.extend(other.problems);
        self.annotations(other.props, other.items);
        self.type_mismatch |= other.type_mismatch;
    }

    fn annotations(&mut self, props: Vec<String>, items: usize) {
        for p in props {
            if !self.props.contains(&p) {
                self.props.push(p);
            }
        }
        self.items = self.items.max(items);
    }

    fn warn(&mut self, (start, end): (usize, usize), message: String, kind: Kind) {
        self.problems.push(Problem {
            start,
            end,
            message,
            severity: Severity::Warning,
            kind,
        });
    }
}

const MAX_DEPTH: usize = 64;

pub struct Validator<'r> {
    reg: &'r Registry,
    depth: usize,
    /// (value, schema) pairs being validated: a `$ref` cycle that does not descend into the
    /// value stops here instead of recursing (exponentially, through `anyOf`).
    active: Vec<(*const Node, *const Node)>,
    regexes: HashMap<String, Option<regex_lite::Regex>>,
}

/// Where problems about a whole node are shown.
pub fn anchor_of(n: &Node, key: Option<(usize, usize)>) -> (usize, usize) {
    if let Some(k) = key {
        return k;
    }
    match &n.value {
        Value::Object(props) => match props.first() {
            // A YAML block mapping starts with its first key.
            Some(p) if p.key_start == n.start => (p.key_start, p.key_end),
            _ => (n.start, (n.start + 1).min(n.end.max(n.start + 1))),
        },
        Value::Array(_) => (n.start, n.start + 1),
        _ => (n.start, n.end),
    }
}

/// The range a value problem (type, enum) is shown at.
fn value_range(n: &Node, key: Option<(usize, usize)>) -> (usize, usize) {
    match n.value {
        Value::Object(_) | Value::Array(_) => anchor_of(n, key),
        _ if n.end > n.start => (n.start, n.end),
        _ => anchor_of(n, key),
    }
}

pub fn type_matches(n: &Node, t: &str) -> bool {
    match t {
        "integer" => matches!(n.value, Value::Number(x) if x.fract() == 0.0 && x.is_finite()),
        "number" => matches!(n.value, Value::Number(_)),
        "any" => true,
        _ => n.type_name() == t || (t == "number" && n.type_name() == "integer"),
    }
}

fn list(values: &[String]) -> String {
    let mut out: Vec<&str> = values.iter().map(String::as_str).take(30).collect();
    if values.len() > 30 {
        out.push("…");
    }
    out.join(", ")
}

fn quoted_list(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("'{n}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn type_message(required: &[String], actual: &str) -> String {
    let mut req: Vec<String> = Vec::new();
    for r in required {
        if !req.contains(r) {
            req.push(r.clone());
        }
    }
    if req.len() == 1 {
        format!(
            "Incompatible types. Required: {}. Actual: {actual}.",
            req[0]
        )
    } else {
        format!(
            "Incompatible types. Required one of: {}. Actual: {actual}.",
            req.join(", ")
        )
    }
}

fn enum_message(values: &[String]) -> String {
    format!("Value should be one of: {}", list(values))
}

impl<'r> Validator<'r> {
    pub fn new(reg: &'r Registry) -> Validator<'r> {
        Validator {
            reg,
            depth: 0,
            active: Vec::new(),
            regexes: HashMap::new(),
        }
    }

    pub fn regex(&mut self, pattern: &str) -> Option<&regex_lite::Regex> {
        self.regexes
            .entry(pattern.to_string())
            .or_insert_with(|| regex_lite::Regex::new(&translate_pattern(pattern)).ok())
            .as_ref()
    }

    /// Whether `n` is valid against `s` (deprecations do not count).
    pub fn matches(&mut self, n: &Node, s: Sch<'r>) -> bool {
        self.validate(n, s, None).ok()
    }

    /// Validate `n` against `s`; `key` is the range of the property key `n` is the value of.
    pub fn validate(&mut self, n: &Node, s: Sch<'r>, key: Option<(usize, usize)>) -> Outcome {
        let mut out = Outcome::default();
        if self.depth > MAX_DEPTH || n.custom_tag || n.value == Value::Error {
            out.items = usize::MAX;
            return out;
        }
        if let Some(b) = s.boolean() {
            if !b {
                out.warn(
                    anchor_of(n, key),
                    "Value is not allowed".into(),
                    Kind::NotAllowed,
                );
            }
            return out;
        }
        if s.node.as_object().is_none() {
            return out;
        }
        let pair = (n as *const Node, s.node as *const Node);
        if self.active.contains(&pair) {
            return out;
        }
        self.active.push(pair);
        self.depth += 1;
        self.run(n, s, key, &mut out);
        self.depth -= 1;
        self.active.pop();
        out
    }

    fn run(&mut self, n: &Node, s: Sch<'r>, key: Option<(usize, usize)>, out: &mut Outcome) {
        let reg = self.reg;
        for kw in ["$ref", "$dynamicRef", "$recursiveRef"] {
            if let Some(r) = s.str(kw) {
                if let Some(target) = reg.resolve(s, r) {
                    if !target.same(&s) {
                        let o = self.validate(n, target, key);
                        out.merge(o);
                    }
                }
            }
        }
        let types = s.types();
        if !types.is_empty() && !types.iter().any(|t| type_matches(n, t)) {
            let req: Vec<String> = types.iter().map(|t| t.to_string()).collect();
            out.warn(
                value_range(n, key),
                type_message(&req, n.type_name()),
                Kind::Type(req),
            );
            out.type_mismatch = true;
            return;
        }
        if let Some(values) = s.get("enum").and_then(Node::as_array) {
            if !values.iter().any(|v| v.same_value(n)) {
                let vals: Vec<String> = values.iter().map(Node::to_json).collect();
                out.warn(value_range(n, key), enum_message(&vals), Kind::Enum(vals));
            }
        }
        if let Some(c) = s.get("const") {
            if !c.same_value(n) {
                let vals = vec![c.to_json()];
                out.warn(value_range(n, key), enum_message(&vals), Kind::Enum(vals));
            }
        }
        match &n.value {
            Value::Number(x) => self.number(*x, n, s, key, out),
            Value::String(text) => self.string(text, n, s, key, out),
            Value::Array(_) => self.array(n, s, key, out),
            Value::Object(_) => self.object(n, s, key, out),
            _ => {}
        }
        self.applicators(n, s, key, out);
        if let Value::Object(_) = n.value {
            self.unevaluated_props(n, s, out);
        }
        if let Value::Array(items) = &n.value {
            if let Some(u) = s.get("unevaluatedItems") {
                let u = reg.enter(s, u);
                for item in items.iter().skip(out.items.min(items.len())) {
                    if u.boolean() == Some(false) {
                        out.warn(
                            anchor_of(item, None),
                            "Additional items are not allowed".into(),
                            Kind::NotAllowed,
                        );
                    } else {
                        let o = self.validate(item, u, None);
                        out.problems.extend(o.problems);
                    }
                }
                out.items = usize::MAX;
            }
        }
    }

    fn number(
        &mut self,
        x: f64,
        n: &Node,
        s: Sch<'r>,
        key: Option<(usize, usize)>,
        out: &mut Outcome,
    ) {
        let r = value_range(n, key);
        let draft4 = self.reg.draft_of(s) == Draft::D4;
        let excl_bool = |kw: &str| s.get(kw).and_then(Node::as_bool).unwrap_or(false);
        if let Some(min) = s.num("minimum") {
            if draft4 && excl_bool("exclusiveMinimum") {
                if x <= min {
                    out.warn(
                        r,
                        format!("Less than an exclusive minimum {}", format_number(min)),
                        Kind::Other,
                    );
                }
            } else if x < min {
                out.warn(
                    r,
                    format!("Less than a minimum {}", format_number(min)),
                    Kind::Other,
                );
            }
        }
        if let Some(max) = s.num("maximum") {
            if draft4 && excl_bool("exclusiveMaximum") {
                if x >= max {
                    out.warn(
                        r,
                        format!("Greater than an exclusive maximum {}", format_number(max)),
                        Kind::Other,
                    );
                }
            } else if x > max {
                out.warn(
                    r,
                    format!("Greater than a maximum {}", format_number(max)),
                    Kind::Other,
                );
            }
        }
        if let Some(min) = s.num("exclusiveMinimum") {
            if x <= min {
                out.warn(
                    r,
                    format!("Less than an exclusive minimum {}", format_number(min)),
                    Kind::Other,
                );
            }
        }
        if let Some(max) = s.num("exclusiveMaximum") {
            if x >= max {
                out.warn(
                    r,
                    format!("Greater than an exclusive maximum {}", format_number(max)),
                    Kind::Other,
                );
            }
        }
        if let Some(m) = s.num("multipleOf").filter(|m| *m > 0.0) {
            let q = x / m;
            if (q - q.round()).abs() > 1e-9 * q.abs().max(1.0) {
                out.warn(
                    r,
                    format!("Is not multiple of {}", format_number(m)),
                    Kind::Other,
                );
            }
        }
    }

    fn string(
        &mut self,
        text: &str,
        n: &Node,
        s: Sch<'r>,
        key: Option<(usize, usize)>,
        out: &mut Outcome,
    ) {
        let r = value_range(n, key);
        let len = text.chars().count() as f64;
        if let Some(min) = s.num("minLength") {
            if len < min {
                out.warn(
                    r,
                    format!("String is shorter than {}", format_number(min)),
                    Kind::Other,
                );
            }
        }
        if let Some(max) = s.num("maxLength") {
            if len > max {
                out.warn(
                    r,
                    format!("String is longer than {}", format_number(max)),
                    Kind::Other,
                );
            }
        }
        if let Some(p) = s.str("pattern") {
            if self.regex(p).is_some_and(|re| !re.is_match(text)) {
                let msg = s
                    .str("patternErrorMessage")
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("String violates the pattern: '{p}'"));
                out.warn(r, msg, Kind::Other);
            }
        }
        if let Some(f) = s.str("format") {
            if !crate::format::check(f, text) {
                out.warn(r, format!("String violates the format '{f}'"), Kind::Other);
            }
        }
    }

    fn array(&mut self, n: &Node, s: Sch<'r>, key: Option<(usize, usize)>, out: &mut Outcome) {
        let reg = self.reg;
        let items = n.as_array().unwrap_or(&[]);
        let prefix: Option<&[Node]> = s
            .get("prefixItems")
            .and_then(Node::as_array)
            .or_else(|| s.get("items").and_then(Node::as_array));
        let rest: Option<&Node> = if s.get("prefixItems").is_some() || prefix.is_none() {
            s.get("items").filter(|i| i.as_array().is_none())
        } else {
            s.get("additionalItems")
        };
        let mut evaluated = 0;
        for (i, item) in items.iter().enumerate() {
            let sub = match prefix.and_then(|p| p.get(i)) {
                Some(p) => Some(p),
                None => rest,
            };
            let Some(sub) = sub else { continue };
            evaluated = i + 1;
            let sub = reg.enter(s, sub);
            if sub.boolean() == Some(false) {
                out.warn(
                    anchor_of(item, None),
                    "Additional items are not allowed".into(),
                    Kind::NotAllowed,
                );
                continue;
            }
            let o = self.validate(item, sub, None);
            out.problems.extend(o.problems);
        }
        if rest.is_some() {
            evaluated = usize::MAX;
        }
        out.items = out.items.max(evaluated);
        let r = anchor_of(n, key);
        let count = items.len() as f64;
        if let Some(min) = s.num("minItems") {
            if count < min {
                out.warn(
                    r,
                    format!("Array is shorter than {}", format_number(min)),
                    Kind::Other,
                );
            }
        }
        if let Some(max) = s.num("maxItems") {
            if count > max {
                out.warn(
                    r,
                    format!("Array is longer than {}", format_number(max)),
                    Kind::Other,
                );
            }
        }
        if s.get("uniqueItems").and_then(Node::as_bool) == Some(true) {
            for (i, item) in items.iter().enumerate() {
                if items[..i].iter().any(|prev| prev.same_value(item)) {
                    out.warn(
                        value_range(item, None),
                        "Item is not unique".into(),
                        Kind::Other,
                    );
                }
            }
        }
        if let Some(c) = s.get("contains") {
            let c = reg.enter(s, c);
            let hits = items.iter().filter(|i| self.matches(i, c)).count() as f64;
            let min = s.num("minContains").unwrap_or(1.0);
            if hits < min {
                out.warn(
                    r,
                    "Array does not contain an item that matches the 'contains' schema".into(),
                    Kind::Other,
                );
            }
            if let Some(max) = s.num("maxContains") {
                if hits > max {
                    out.warn(
                        r,
                        format!(
                            "Array contains more than {} items that match the 'contains' schema",
                            format_number(max)
                        ),
                        Kind::Other,
                    );
                }
            }
        }
    }

    fn object(&mut self, n: &Node, s: Sch<'r>, key: Option<(usize, usize)>, out: &mut Outcome) {
        let reg = self.reg;
        let props = n.as_object().unwrap_or(&[]);
        let has = |k: &str| props.iter().any(|p| p.key == k);
        let anchor = anchor_of(n, key);
        if let Some(req) = s.get("required").and_then(Node::as_array) {
            let missing: Vec<&str> = req
                .iter()
                .filter_map(Node::as_str)
                .filter(|r| !has(r))
                .collect();
            if !missing.is_empty() {
                let what = if missing.len() == 1 {
                    "property"
                } else {
                    "properties"
                };
                out.warn(
                    anchor,
                    format!("Missing required {what} {}", quoted_list(&missing)),
                    Kind::Missing,
                );
            }
        }
        let properties = s.get("properties");
        let patterns = s.get("patternProperties").and_then(Node::as_object);
        let additional = s.get("additionalProperties");
        for p in props {
            let Some(v) = &p.value else { continue };
            let k = (p.key_start, p.key_end);
            let mut matched = false;
            if let Some(ps) = properties.and_then(|ps| ps.get(&p.key)) {
                matched = true;
                let sub = reg.enter(s, ps);
                if sub.boolean() == Some(false) {
                    out.warn(
                        k,
                        format!("Property '{}' is not allowed", p.key),
                        Kind::NotAllowed,
                    );
                } else {
                    let o = self.validate(v, sub, Some(k));
                    out.problems.extend(o.problems);
                    self.deprecation(&p.key, k, sub, out);
                }
            }
            for pat in patterns.unwrap_or(&[]) {
                let Some(ps) = &pat.value else { continue };
                if self.regex(&pat.key).is_some_and(|re| re.is_match(&p.key)) {
                    matched = true;
                    let sub = reg.enter(s, ps);
                    if sub.boolean() == Some(false) {
                        out.warn(
                            k,
                            format!("Property '{}' is not allowed", p.key),
                            Kind::NotAllowed,
                        );
                    } else {
                        let o = self.validate(v, sub, Some(k));
                        out.problems.extend(o.problems);
                    }
                }
            }
            if !matched {
                if let Some(a) = additional {
                    matched = true;
                    let sub = reg.enter(s, a);
                    if sub.boolean() == Some(false) {
                        if p.key != "$schema" {
                            out.warn(
                                k,
                                format!("Property '{}' is not allowed", p.key),
                                Kind::NotAllowed,
                            );
                        }
                    } else {
                        let o = self.validate(v, sub, Some(k));
                        out.problems.extend(o.problems);
                    }
                }
            }
            if matched && !out.props.contains(&p.key) {
                out.props.push(p.key.clone());
            }
        }
        if let Some(names) = s.get("propertyNames") {
            let names = reg.enter(s, names);
            for p in props {
                let key_node = Node::new(Value::String(p.key.clone()), p.key_start, p.key_end);
                let o = self.validate(&key_node, names, None);
                if !o.ok() {
                    out.warn(
                        (p.key_start, p.key_end),
                        format!("Property '{}' is not allowed", p.key),
                        Kind::NotAllowed,
                    );
                }
            }
        }
        let count = props.len() as f64;
        if let Some(min) = s.num("minProperties") {
            if count < min {
                out.warn(
                    anchor,
                    format!("Number of properties is less than {}", format_number(min)),
                    Kind::Other,
                );
            }
        }
        if let Some(max) = s.num("maxProperties") {
            if count > max {
                out.warn(
                    anchor,
                    format!(
                        "Number of properties is greater than {}",
                        format_number(max)
                    ),
                    Kind::Other,
                );
            }
        }
        let mut dependent: Vec<(&str, &Node)> = Vec::new();
        for kw in ["dependencies", "dependentRequired", "dependentSchemas"] {
            for d in s.get(kw).and_then(Node::as_object).unwrap_or(&[]) {
                if let Some(v) = &d.value {
                    dependent.push((d.key.as_str(), v));
                }
            }
        }
        for (trigger, dep) in dependent {
            let Some(tp) = props.iter().find(|p| p.key == trigger) else {
                continue;
            };
            match &dep.value {
                Value::Array(names) => {
                    let missing: Vec<&str> = names
                        .iter()
                        .filter_map(Node::as_str)
                        .filter(|r| !has(r))
                        .collect();
                    if !missing.is_empty() {
                        out.warn(
                            (tp.key_start, tp.key_end),
                            format!(
                                "Dependency is violated: {} must be specified, since '{trigger}' is specified",
                                if missing.len() == 1 {
                                    format!("property {}", quoted_list(&missing))
                                } else {
                                    format!("properties {}", quoted_list(&missing))
                                }
                            ),
                            Kind::Missing,
                        );
                    }
                }
                _ => {
                    let o = self.validate(n, reg.enter(s, dep), key);
                    out.merge(o);
                }
            }
        }
    }

    fn deprecation(&mut self, name: &str, k: (usize, usize), sub: Sch<'r>, out: &mut Outcome) {
        let target = match sub.str("$ref").and_then(|r| self.reg.resolve(sub, r)) {
            Some(t)
                if sub.get("deprecated").is_none() && sub.get("deprecationMessage").is_none() =>
            {
                t
            }
            _ => sub,
        };
        let message = target.str("deprecationMessage");
        if message.is_some() || target.get("deprecated").and_then(Node::as_bool) == Some(true) {
            out.problems.push(Problem {
                start: k.0,
                end: k.1,
                message: match message {
                    Some(m) => format!("Property '{name}' is deprecated: {m}"),
                    None => format!("Property '{name}' is deprecated"),
                },
                severity: Severity::Weak,
                kind: Kind::Deprecated,
            });
        }
    }

    fn applicators(
        &mut self,
        n: &Node,
        s: Sch<'r>,
        key: Option<(usize, usize)>,
        out: &mut Outcome,
    ) {
        let reg = self.reg;
        for b in s.get("allOf").and_then(Node::as_array).unwrap_or(&[]) {
            let o = self.validate(n, reg.enter(s, b), key);
            out.merge(o);
        }
        for (kw, one) in [("anyOf", false), ("oneOf", true)] {
            let Some(branches) = s.get(kw).and_then(Node::as_array) else {
                continue;
            };
            if branches.is_empty() {
                continue;
            }
            let results: Vec<Outcome> = branches
                .iter()
                .map(|b| self.validate(n, reg.enter(s, b), key))
                .collect();
            let passing = results.iter().filter(|o| o.ok()).count();
            if passing == 0 {
                self.report_best(n, key, results, out);
            } else if one && passing > 1 {
                out.warn(
                    anchor_of(n, key),
                    "Validates to more than one variant".into(),
                    Kind::Other,
                );
            } else {
                for o in results.into_iter().filter(Outcome::ok) {
                    out.problems.extend(
                        o.problems
                            .into_iter()
                            .filter(|p| p.severity == Severity::Weak),
                    );
                    out.annotations(o.props, o.items);
                }
            }
        }
        if let Some(not) = s.get("not") {
            if self.matches(n, reg.enter(s, not)) {
                out.warn(
                    anchor_of(n, key),
                    "Validates against 'not' schema".into(),
                    Kind::Other,
                );
            }
        }
        if let Some(cond) = s.get("if") {
            let cond = self.validate(n, reg.enter(s, cond), key);
            let branch = if cond.ok() {
                out.annotations(cond.props, cond.items);
                s.get("then")
            } else {
                s.get("else")
            };
            if let Some(b) = branch {
                let o = self.validate(n, reg.enter(s, b), key);
                out.merge(o);
            }
        }
    }

    /// No branch of `anyOf`/`oneOf` matched: one merged message when every branch failed
    /// only on the type or the value, else the problems of the closest branch.
    fn report_best(
        &mut self,
        n: &Node,
        key: Option<(usize, usize)>,
        results: Vec<Outcome>,
        out: &mut Outcome,
    ) {
        let only_self = |o: &Outcome| {
            o.problems
                .iter()
                .filter(|p| p.severity == Severity::Warning)
                .count()
                == 1
                && o.problems.iter().any(|p| {
                    p.severity == Severity::Warning
                        && matches!(p.kind, Kind::Type(_) | Kind::Enum(_))
                        && (p.start, p.end) == value_range(n, key)
                })
        };
        if results.iter().all(only_self) {
            let mut types: Vec<String> = Vec::new();
            let mut values: Vec<String> = Vec::new();
            for o in &results {
                for p in &o.problems {
                    match &p.kind {
                        Kind::Type(t) => types.extend(t.iter().cloned()),
                        Kind::Enum(v) => values.extend(v.iter().cloned()),
                        _ => {}
                    }
                }
            }
            let r = value_range(n, key);
            if values.is_empty() {
                out.warn(r, type_message(&types, n.type_name()), Kind::Type(types));
                out.type_mismatch = true;
            } else if types.is_empty() {
                values.dedup();
                out.warn(r, enum_message(&values), Kind::Enum(values));
            } else {
                // Mixed: the branches with the right type are closer.
                out.warn(r, enum_message(&values), Kind::Enum(values));
            }
            return;
        }
        let best = results
            .into_iter()
            .min_by_key(|o| {
                (
                    o.type_mismatch,
                    o.problems
                        .iter()
                        .filter(|p| p.severity == Severity::Warning)
                        .count(),
                )
            })
            .expect("branches");
        out.problems.extend(best.problems);
        out.type_mismatch |= best.type_mismatch;
    }

    fn unevaluated_props(&mut self, n: &Node, s: Sch<'r>, out: &mut Outcome) {
        let Some(u) = s.get("unevaluatedProperties") else {
            return;
        };
        let u = self.reg.enter(s, u);
        for p in n.as_object().unwrap_or(&[]) {
            if out.props.contains(&p.key) || p.key == "$schema" {
                continue;
            }
            let Some(v) = &p.value else { continue };
            let k = (p.key_start, p.key_end);
            if u.boolean() == Some(false) {
                out.warn(
                    k,
                    format!("Property '{}' is not allowed", p.key),
                    Kind::NotAllowed,
                );
            } else {
                let o = self.validate(v, u, Some(k));
                out.problems.extend(o.problems);
            }
        }
        let all: Vec<String> = n
            .as_object()
            .unwrap_or(&[])
            .iter()
            .map(|p| p.key.clone())
            .collect();
        out.annotations(all, 0);
    }
}

/// ECMAScript regex syntax that `regex-lite` spells differently: `\/` is a plain `/`, and
/// a `-` right after a range in a class (`[a-z0-9-_]`) is a literal.
pub fn translate_pattern(p: &str) -> String {
    let mut out = String::with_capacity(p.len() + 8);
    let chars: Vec<char> = p.chars().collect();
    let mut i = 0;
    let mut in_class = false;
    // Inside a class: atoms since the class opened, and whether the last atom ended a range.
    let mut atoms = 0;
    let mut after_range = false;
    let mut range_open = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let n = chars.get(i + 1).copied();
            match n {
                Some('/') => out.push('/'),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push_str("\\\\"),
            }
            i += 2;
            if in_class {
                atoms += 1;
                after_range = range_open;
                range_open = false;
            }
            continue;
        }
        if in_class {
            match c {
                ']' if atoms > 0 => {
                    in_class = false;
                    out.push(']');
                }
                '-' if atoms == 0
                    || after_range
                    || chars.get(i + 1) == Some(&']')
                    || range_open =>
                {
                    out.push_str("\\-");
                    atoms += 1;
                    after_range = false;
                }
                '-' => {
                    out.push('-');
                    range_open = true;
                }
                '[' => {
                    out.push_str("\\[");
                    atoms += 1;
                    after_range = range_open;
                    range_open = false;
                }
                _ => {
                    if c == '^' && atoms == 0 && out.ends_with('[') {
                        out.push('^');
                    } else {
                        out.push(c);
                        atoms += 1;
                        after_range = range_open;
                        range_open = false;
                    }
                }
            }
        } else {
            if c == '[' {
                in_class = true;
                atoms = 0;
                after_range = false;
                range_open = false;
            }
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Validate a document against a schema: the problems, ordered by position.
pub fn validate(reg: &Registry, root: Sch<'_>, doc: &Node) -> Vec<Problem> {
    let mut v = Validator::new(reg);
    let mut problems = v.validate(doc, root, None).problems;
    problems.sort_by_key(|p| (p.start, p.end));
    problems.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.message == b.message);
    problems
}

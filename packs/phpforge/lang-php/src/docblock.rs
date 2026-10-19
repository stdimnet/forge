//! PHPDoc parser: summary/description, the standard tags (`@param`, `@return`, `@throws`,
//! `@var`, `@property[-read|-write]`, `@method`, `@deprecated`, `@see`, `@template`,
//! `@extends`/`@implements`), static-analysis prefixes (`@phpstan-`/`@psalm-`) and
//! Doctrine/Symfony style annotations (`@ORM\Column(type="string")`, `@Route("/x")`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocBlock {
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<DocParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<DocType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub throws: Vec<DocType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub var: Option<DocParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<DocProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<DocMethod>,
    /// `Some("")` for a bare `@deprecated`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<DocTemplate>,
    /// Generic parents: `@extends Collection<int, User>`, `@implements`, `@use`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<DocGeneric>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    /// Every other tag, verbatim (`@author`, `@since`, `@internal`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<DocTag>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocParam {
    #[serde(rename = "type")]
    pub type_name: String,
    /// Name without `$`; empty when the tag omits it (`@var int`).
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub variadic: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocType {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyAccess {
    #[default]
    ReadWrite,
    Read,
    Write,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocProperty {
    pub access: PropertyAccess,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocMethod {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_static: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub name: String,
    /// Raw parameter list between the parentheses.
    pub params: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocTemplate {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound: Option<String>,
    /// `covariant` / `contravariant` for `@template-covariant` etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variance: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocGeneric {
    /// `extends`, `implements` or `use`.
    pub tag: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocTag {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
}

/// Doctrine/Symfony annotation, e.g. `@ORM\Column(type="string", length=255)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub name: String,
    pub args: Vec<AnnotationArg>,
}

impl Annotation {
    pub fn render(&self) -> String {
        if self.args.is_empty() {
            return format!("@{}", self.name);
        }
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| match &a.name {
                Some(n) => format!("{n}={}", a.value),
                None => a.value.clone(),
            })
            .collect();
        format!("@{}({})", self.name, args.join(", "))
    }

    pub fn arg(&self, name: &str) -> Option<&AnnotationArg> {
        self.args.iter().find(|a| a.name.as_deref() == Some(name))
    }
}

/// One argument of an annotation or attribute. `value` is the source text as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationArg {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub value: String,
}

impl AnnotationArg {
    /// Value with surrounding quotes removed (`"string"` -> `string`).
    pub fn unquoted(&self) -> &str {
        let v = self.value.trim();
        for q in ['"', '\''] {
            if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
                return &v[1..v.len() - 1];
            }
        }
        v
    }
}

impl DocBlock {
    pub fn param(&self, name: &str) -> Option<&DocParam> {
        let name = name.trim_start_matches('$');
        self.params.iter().find(|p| p.name == name)
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }
}

/// Is this comment text a docblock (`/** ... */`)?
pub fn is_docblock(comment: &str) -> bool {
    comment.starts_with("/**") && !comment.starts_with("/***") && comment.ends_with("*/")
}

/// Strip comment delimiters and leading asterisks.
fn clean_lines(raw: &str) -> Vec<String> {
    let body = raw.trim().trim_start_matches("/**").trim_end_matches("*/");
    body.lines()
        .map(|l| {
            let t = l.trim_start();
            let t = t.strip_prefix('*').unwrap_or(t);
            let t = t.strip_prefix(' ').unwrap_or(t);
            t.trim_end().to_string()
        })
        .collect()
}

/// Parse a raw `/** ... */` comment.
pub fn parse(raw: &str) -> DocBlock {
    let lines = clean_lines(raw);
    let mut text: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
    for line in lines {
        let starts_tag = line.trim_start().starts_with('@');
        if depth > 0 {
            if let Some(last) = tags.last_mut() {
                last.push('\n');
                last.push_str(&line);
            }
        } else if starts_tag {
            tags.push(line.trim_start().to_string());
        } else if let Some(last) = tags.last_mut() {
            if !line.trim().is_empty() {
                last.push('\n');
                last.push_str(line.trim());
            }
        } else {
            text.push(line);
        }
        if let Some(last) = tags.last() {
            depth = paren_depth(last);
        }
    }

    let mut doc = DocBlock::default();
    split_summary(&text, &mut doc);
    for tag in tags {
        apply_tag(&tag, &mut doc);
    }
    doc
}

fn paren_depth(s: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' => quote = Some(c),
                '(' | '{' => depth += 1,
                ')' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth.max(0)
}

fn split_summary(text: &[String], doc: &mut DocBlock) {
    let trimmed: Vec<&str> = {
        let mut v: Vec<&str> = text.iter().map(|s| s.as_str()).collect();
        while v.first().is_some_and(|l| l.trim().is_empty()) {
            v.remove(0);
        }
        while v.last().is_some_and(|l| l.trim().is_empty()) {
            v.pop();
        }
        v
    };
    let mut summary: Vec<&str> = Vec::new();
    let mut idx = 0;
    while idx < trimmed.len() {
        let line = trimmed[idx];
        idx += 1;
        if line.trim().is_empty() {
            break;
        }
        summary.push(line.trim());
        if line.trim_end().ends_with('.') {
            break;
        }
    }
    doc.summary = summary.join(" ");
    doc.description = trimmed[idx..].join("\n").trim().to_string();
}

fn tag_name(tag: &str) -> (&str, &str) {
    let t = tag.trim_start_matches('@');
    let end = t
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(t.len());
    (&t[..end], &t[end..])
}

fn apply_tag(tag: &str, doc: &mut DocBlock) {
    let (name, rest) = tag_name(tag);
    if name.is_empty() {
        return;
    }
    let is_annotation =
        name.contains('\\') || name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    if is_annotation {
        doc.annotations.push(parse_annotation(name, rest));
        return;
    }
    let body = rest.trim();
    let lname = name.to_ascii_lowercase();
    let (prefixed, base) = match lname
        .strip_prefix("phpstan-")
        .or_else(|| lname.strip_prefix("psalm-"))
    {
        Some(b) => (true, b.to_string()),
        None => (false, lname.clone()),
    };
    match base.as_str() {
        "param" => {
            let p = parse_param_like(body);
            if let Some(existing) = doc.params.iter_mut().find(|e| e.name == p.name) {
                if prefixed {
                    existing.type_name = p.type_name;
                }
            } else {
                doc.params.push(p);
            }
        }
        "return" | "returns" => {
            let (ty, desc) = split_type(body);
            if prefixed || doc.returns.is_none() {
                doc.returns = Some(DocType {
                    type_name: ty,
                    description: desc,
                });
            }
        }
        "throws" | "throw" => {
            let (ty, desc) = split_type(body);
            doc.throws.push(DocType {
                type_name: ty,
                description: desc,
            });
        }
        "var" => {
            if prefixed || doc.var.is_none() {
                doc.var = Some(parse_param_like(body));
            }
        }
        "property" | "property-read" | "property-write" => {
            let p = parse_param_like(body);
            doc.properties.push(DocProperty {
                access: match base.as_str() {
                    "property-read" => PropertyAccess::Read,
                    "property-write" => PropertyAccess::Write,
                    _ => PropertyAccess::ReadWrite,
                },
                type_name: p.type_name,
                name: p.name,
                description: p.description,
            });
        }
        "method" => {
            if let Some(m) = parse_method(body) {
                doc.methods.push(m);
            }
        }
        "deprecated" => doc.deprecated = Some(body.to_string()),
        "see" | "link" => doc.see.push(body.to_string()),
        "template" | "template-covariant" | "template-contravariant" => {
            let mut words = body.split_whitespace();
            if let Some(n) = words.next() {
                let mut bound = None;
                if let Some(kw) = words.next() {
                    if kw == "of" || kw == "as" {
                        let rest: Vec<&str> = words.collect();
                        let (ty, _) = split_type(&rest.join(" "));
                        if !ty.is_empty() {
                            bound = Some(ty);
                        }
                    }
                }
                doc.templates.push(DocTemplate {
                    name: n.to_string(),
                    bound,
                    variance: base.strip_prefix("template-").map(str::to_string),
                });
            }
        }
        "extends"
        | "implements"
        | "use"
        | "template-extends"
        | "template-implements"
        | "template-use" => {
            let (ty, _) = split_type(body);
            doc.generics.push(DocGeneric {
                tag: base.trim_start_matches("template-").to_string(),
                type_name: ty,
            });
        }
        _ => doc.tags.push(DocTag {
            name: name.to_string(),
            body: body.to_string(),
        }),
    }
}

/// Split a leading type expression from the rest: `Collection<int, User> the users` ->
/// (`Collection<int, User>`, `the users`). Handles generics, shapes, unions with spaces
/// around `|` and callable signatures.
pub fn split_type(s: &str) -> (String, String) {
    let s = s.trim();
    if s.starts_with('$') {
        return (String::new(), s.to_string());
    }
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut end = s.len();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => {
                let prev = s[..pos].trim_end().chars().last();
                let next = s[pos..].trim_start().chars().next();
                let joins = matches!(prev, Some('|' | '&' | ':' | ','))
                    || matches!(next, Some('|' | '&'))
                    || (prev == Some(')') && next == Some(':'));
                if !joins {
                    end = pos;
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (s[..end].to_string(), s[end..].trim().to_string())
}

/// `type $name description` (type and name each optional).
fn parse_param_like(body: &str) -> DocParam {
    let (ty, rest) = split_type(body);
    let rest = rest.trim();
    let (variadic, rest) = match rest.strip_prefix("...") {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let rest = rest.strip_prefix('&').unwrap_or(rest);
    if let Some(r) = rest.strip_prefix('$') {
        let end = r
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(r.len());
        DocParam {
            type_name: ty,
            name: r[..end].to_string(),
            description: r[end..].trim().to_string(),
            variadic,
        }
    } else {
        DocParam {
            type_name: ty,
            name: String::new(),
            description: rest.to_string(),
            variadic,
        }
    }
}

/// `[static] [ReturnType] name(params) description`.
fn parse_method(body: &str) -> Option<DocMethod> {
    let open = body.find('(')?;
    let mut depth = 0;
    let mut close = None;
    for (i, c) in body[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let head: Vec<&str> = body[..open].split_whitespace().collect();
    let (name, before) = head.split_last()?;
    let mut before: Vec<&str> = before.to_vec();
    let is_static = before.first() == Some(&"static") && !before.is_empty();
    if is_static {
        before.remove(0);
    }
    let return_type = if before.is_empty() {
        None
    } else {
        Some(before.join(" "))
    };
    Some(DocMethod {
        is_static,
        return_type,
        name: name.to_string(),
        params: body[open + 1..close].trim().to_string(),
        description: body[close + 1..].trim().to_string(),
    })
}

fn parse_annotation(name: &str, rest: &str) -> Annotation {
    let rest = rest.trim_start();
    let args = if rest.starts_with('(') {
        let mut depth = 0;
        let mut quote: Option<char> = None;
        let mut close = rest.len();
        for (i, c) in rest.char_indices() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => match c {
                    '"' | '\'' => quote = Some(c),
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = i;
                            break;
                        }
                    }
                    _ => {}
                },
            }
        }
        parse_args(&rest[1..close.max(1)])
    } else {
        Vec::new()
    };
    Annotation {
        name: name.to_string(),
        args,
    }
}

/// Split an argument list (`"/x", name="a", methods={"GET"}` or `'/x', methods: ['GET']`)
/// into named/positional arguments. Shared by annotations and PHP 8 attributes.
pub fn parse_args(s: &str) -> Vec<AnnotationArg> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    cur.push(c);
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    cur.push(c);
                }
                ',' if depth == 0 => parts.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            },
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .map(|p| collapse_ws(p.trim()))
        .filter(|p| !p.is_empty())
        .map(|p| split_named(&p))
        .collect()
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut quote: Option<char> = None;
    let mut last_ws = false;
    for c in s.chars() {
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
        }
        if c.is_whitespace() {
            if !last_ws {
                out.push(' ');
            }
            last_ws = true;
        } else {
            out.push(c);
            last_ws = false;
        }
    }
    out.replace("( ", "(")
        .replace(" )", ")")
        .replace("[ ", "[")
        .replace(" ]", "]")
}

fn split_named(p: &str) -> AnnotationArg {
    let ident_end = p
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(p.len());
    if ident_end > 0 {
        let after = p[ident_end..].trim_start();
        let is_named = (after.starts_with('=') && !after.starts_with("=="))
            || (after.starts_with(':') && !after.starts_with("::"));
        if is_named {
            return AnnotationArg {
                name: Some(p[..ident_end].to_string()),
                value: after[1..].trim().to_string(),
            };
        }
    }
    AnnotationArg {
        name: None,
        value: p.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_description_and_basic_tags() {
        let doc = parse(
            "/**
              * Calculates the total.
              * Includes taxes and
              * shipping costs.
              *
              * @param int $qty   Quantity
              * @param float|null $rate The tax
              *        rate to apply
              * @param string ...$tags
              * @return float The total
              * @throws \\InvalidArgumentException When qty < 0
              * @deprecated since 2.0, use gross()
              * @see Checkout::gross()
              * @author Jane
              */",
        );
        assert_eq!(doc.summary, "Calculates the total.");
        assert_eq!(doc.description, "Includes taxes and\nshipping costs.");
        assert_eq!(doc.params.len(), 3);
        assert_eq!(doc.params[0].type_name, "int");
        assert_eq!(doc.params[0].name, "qty");
        assert_eq!(doc.params[0].description, "Quantity");
        assert_eq!(doc.params[1].type_name, "float|null");
        assert_eq!(doc.params[1].description, "The tax\nrate to apply");
        assert!(doc.params[2].variadic);
        assert_eq!(doc.params[2].name, "tags");
        assert_eq!(doc.returns.as_ref().unwrap().type_name, "float");
        assert_eq!(doc.returns.as_ref().unwrap().description, "The total");
        assert_eq!(doc.throws[0].type_name, "\\InvalidArgumentException");
        assert_eq!(doc.deprecated.as_deref(), Some("since 2.0, use gross()"));
        assert_eq!(doc.see, vec!["Checkout::gross()"]);
        assert_eq!(doc.tags[0].name, "author");
        assert!(doc.param("$qty").is_some());
    }

    #[test]
    fn summary_without_period_ends_at_blank_line() {
        let doc = parse("/**\n * First line\n * continues here\n *\n * Body\n */");
        assert_eq!(doc.summary, "First line continues here");
        assert_eq!(doc.description, "Body");
    }

    #[test]
    fn single_line_var_and_generics() {
        let doc = parse("/** @var array<string, list<int>> $map the map */");
        let var = doc.var.unwrap();
        assert_eq!(var.type_name, "array<string, list<int>>");
        assert_eq!(var.name, "map");
        assert_eq!(var.description, "the map");

        let doc = parse("/** @var list<string> */");
        assert_eq!(doc.var.unwrap().type_name, "list<string>");
        assert_eq!(doc.summary, "");
    }

    #[test]
    fn complex_types() {
        assert_eq!(
            split_type("array{id: int, name?: string} $row rest"),
            ("array{id: int, name?: string}".into(), "$row rest".into())
        );
        assert_eq!(
            split_type("callable(int, string): void $cb"),
            ("callable(int, string): void".into(), "$cb".into())
        );
        assert_eq!(
            split_type("int | string $x"),
            ("int | string".into(), "$x".into())
        );
        assert_eq!(split_type("$x only"), ("".into(), "$x only".into()));
    }

    #[test]
    fn properties_methods_templates_generics() {
        let doc = parse(
            "/**
              * @property int $id
              * @property-read \\Illuminate\\Support\\Collection<int, Order> $orders The orders
              * @property-write string $password
              * @method static Builder|User whereEmail(string $email) Find by email
              * @method void touch()
              * @template TKey of array-key
              * @template-covariant TValue
              * @extends Model<User>
              * @implements \\IteratorAggregate<TKey, TValue>
              * @phpstan-return list<User>
              * @return array
              */",
        );
        assert_eq!(doc.properties.len(), 3);
        assert_eq!(doc.properties[1].access, PropertyAccess::Read);
        assert_eq!(
            doc.properties[1].type_name,
            "\\Illuminate\\Support\\Collection<int, Order>"
        );
        assert_eq!(doc.properties[1].description, "The orders");
        assert_eq!(doc.properties[2].access, PropertyAccess::Write);
        let m = &doc.methods[0];
        assert!(m.is_static);
        assert_eq!(m.return_type.as_deref(), Some("Builder|User"));
        assert_eq!(m.name, "whereEmail");
        assert_eq!(m.params, "string $email");
        assert_eq!(m.description, "Find by email");
        assert_eq!(doc.methods[1].return_type.as_deref(), Some("void"));
        assert_eq!(doc.templates[0].name, "TKey");
        assert_eq!(doc.templates[0].bound.as_deref(), Some("array-key"));
        assert_eq!(doc.templates[1].variance.as_deref(), Some("covariant"));
        assert_eq!(doc.generics[0].tag, "extends");
        assert_eq!(doc.generics[0].type_name, "Model<User>");
        assert_eq!(doc.generics[1].tag, "implements");
        // Static-analysis prefixed tags win over plain ones.
        assert_eq!(doc.returns.unwrap().type_name, "list<User>");
    }

    #[test]
    fn doctrine_and_symfony_annotations() {
        let doc = parse(
            r#"/**
              * The order total.
              *
              * @ORM\Column(type="decimal", precision=10, scale=2, nullable=true)
              * @ORM\ManyToMany(targetEntity="Tag")
              * @ORM\JoinTable(name="order_tags",
              *     joinColumns={@ORM\JoinColumn(name="order_id", referencedColumnName="id")}
              * )
              * @Route("/orders/{id}", name="order_show", methods={"GET","HEAD"})
              * @Assert\NotBlank
              */"#,
        );
        assert_eq!(doc.summary, "The order total.");
        assert_eq!(doc.annotations.len(), 5);
        let col = &doc.annotations[0];
        assert_eq!(col.name, "ORM\\Column");
        assert_eq!(col.arg("type").unwrap().unquoted(), "decimal");
        assert_eq!(col.arg("precision").unwrap().value, "10");
        assert_eq!(col.arg("nullable").unwrap().value, "true");
        let join = &doc.annotations[2];
        assert_eq!(join.name, "ORM\\JoinTable");
        assert_eq!(join.arg("name").unwrap().unquoted(), "order_tags");
        assert_eq!(
            join.arg("joinColumns").unwrap().value,
            r#"{@ORM\JoinColumn(name="order_id", referencedColumnName="id")}"#
        );
        let route = &doc.annotations[3];
        assert_eq!(route.name, "Route");
        assert_eq!(route.args[0].name, None);
        assert_eq!(route.args[0].unquoted(), "/orders/{id}");
        assert_eq!(route.arg("methods").unwrap().value, r#"{"GET","HEAD"}"#);
        assert_eq!(
            route.render(),
            r#"@Route("/orders/{id}", name="order_show", methods={"GET","HEAD"})"#
        );
        assert_eq!(doc.annotations[4].name, "Assert\\NotBlank");
        assert!(doc.annotations[4].args.is_empty());
    }

    #[test]
    fn attribute_style_args() {
        let args = parse_args("'/orders/{order}', name: 'orders.show', methods: ['GET', 'POST']");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].unquoted(), "/orders/{order}");
        assert_eq!(args[1].name.as_deref(), Some("name"));
        assert_eq!(args[2].value, "['GET', 'POST']");
        let args = parse_args("Foo::class, x: Bar::BAZ");
        assert_eq!(args[0].name, None);
        assert_eq!(args[0].value, "Foo::class");
        assert_eq!(args[1].value, "Bar::BAZ");
    }

    #[test]
    fn detects_docblocks() {
        assert!(is_docblock("/** x */"));
        assert!(!is_docblock("/* x */"));
        assert!(!is_docblock("// x"));
    }
}

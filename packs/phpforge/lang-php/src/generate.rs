//! "Generate" actions (Alt+Insert): constructor, getters/setters, `__toString()`,
//! override/implement method stubs and PHPDoc blocks. Pure functions over a parsed file;
//! the output is a list of text edits that respect the class' indentation, line endings
//! and PSR-12 layout.

use std::collections::{BTreeMap, HashSet};

use forge_file_templates::{names as templates, render_code, Vars};
use serde::{Deserialize, Serialize};

use crate::index::ProjectIndex;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind, UseImport, Visibility};
use crate::resolve::{resolve_type, shorten_type};
use crate::style::{apply_braces, PhpCodeStyle};
use crate::{PhpError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenerateKind {
    Constructor,
    Getters,
    Setters,
    GettersAndSetters,
    ToString,
    OverrideMethods,
    ImplementMethods,
    PhpDoc,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenerateOptions {
    /// Selected field names (without `$`).
    pub fields: Vec<String>,
    /// Selected methods as `OwnerFqn::name` (or just `name`).
    pub methods: Vec<String>,
    /// Setters return `$this` (`: static`).
    pub fluent: bool,
    /// Brace placement and blank lines around methods (the app fills them from the
    /// settings).
    pub style: PhpCodeStyle,
    /// The user's code templates by name (Settings | Editor | File and Code Templates |
    /// Code); the built-in ones apply otherwise.
    pub code_templates: BTreeMap<String, String>,
}

/// Replace `start..end` (bytes) with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub new_text: String,
}

/// Apply edits (non-overlapping) to `text`.
pub fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_string();
    for e in sorted {
        out.replace_range(e.start..e.end, &e.new_text);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldTarget {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
    pub promoted: bool,
    pub has_getter: bool,
    pub has_setter: bool,
    /// Assigned in an existing constructor.
    pub in_constructor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodTarget {
    /// `OwnerFqn::name` — pass back in [`GenerateOptions::methods`].
    pub id: String,
    pub name: String,
    pub owner: String,
    pub owner_kind: SymbolKind,
    /// Signature as rendered in the target class, e.g. `save(array $options = []): bool`.
    pub signature: String,
    pub visibility: Visibility,
    pub is_static: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTargets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_kind: Option<SymbolKind>,
    pub fields: Vec<FieldTarget>,
    pub has_constructor: bool,
    pub has_to_string: bool,
    pub override_methods: Vec<MethodTarget>,
    pub implement_methods: Vec<MethodTarget>,
    /// Name of the function/method/property at the caret that can get a PHPDoc block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub php_doc_target: Option<String>,
}

/// Supplies ancestors of a class and the imports of the file declaring a class.
pub trait ClassResolver {
    fn ancestors(&mut self, class: &ClassLike) -> Vec<ClassLike>;
    fn uses_of(&mut self, class: &ClassLike) -> Vec<UseImport>;
}

impl ClassResolver for ProjectIndex {
    fn ancestors(&mut self, class: &ClassLike) -> Vec<ClassLike> {
        ProjectIndex::ancestors(self, class)
    }

    fn uses_of(&mut self, class: &ClassLike) -> Vec<UseImport> {
        self.class_path(&class.fqn)
            .map(str::to_string)
            .and_then(|p| self.file(&p).map(|f| f.uses.clone()))
            .unwrap_or_default()
    }
}

/// Resolver over a fixed set of files (tests, single-file use).
pub struct FilesResolver(pub Vec<FileIndex>);

impl ClassResolver for FilesResolver {
    fn ancestors(&mut self, class: &ClassLike) -> Vec<ClassLike> {
        let mut out: Vec<ClassLike> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = class
            .traits
            .iter()
            .chain(&class.extends)
            .chain(&class.implements)
            .cloned()
            .collect();
        let mut i = 0;
        while i < queue.len() {
            let fqn = queue[i].clone();
            i += 1;
            if !seen.insert(fqn.to_ascii_lowercase()) {
                continue;
            }
            if let Some(c) = self
                .0
                .iter()
                .flat_map(|f| f.classes.iter())
                .find(|c| c.fqn.eq_ignore_ascii_case(&fqn))
            {
                queue.extend(
                    c.traits
                        .iter()
                        .chain(&c.extends)
                        .chain(&c.implements)
                        .cloned(),
                );
                out.push(c.clone());
            }
        }
        out
    }

    fn uses_of(&mut self, class: &ClassLike) -> Vec<UseImport> {
        self.0
            .iter()
            .find(|f| f.classes.iter().any(|c| c.fqn == class.fqn))
            .map(|f| f.uses.clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------
// Formatting helpers

pub(crate) struct Fmt {
    /// Indentation of class members.
    pub(crate) indent: String,
    /// One indentation level.
    pub(crate) unit: String,
    pub(crate) class_indent: String,
    pub(crate) crlf: bool,
    pub(crate) style: PhpCodeStyle,
}

pub(crate) fn line_start(src: &str, offset: usize) -> usize {
    src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

pub(crate) fn leading_ws(src: &str, offset: usize) -> String {
    let ls = line_start(src, offset);
    src[ls..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

impl Fmt {
    pub(crate) fn for_class(src: &str, class: &ClassLike, style: &PhpCodeStyle) -> Self {
        let class_indent = leading_ws(src, class.range.start);
        let first_member_indent = class
            .members
            .iter()
            .filter(|m| !m.promoted)
            .map(|m| leading_ws(src, m.range.start))
            .find(|w| w.len() > class_indent.len());
        let unit = match &first_member_indent {
            Some(w) => w[class_indent.len()..].to_string(),
            None if src.contains("\n\t") && !src.contains("\n    ") => "\t".to_string(),
            None => "    ".to_string(),
        };
        let indent = first_member_indent.unwrap_or_else(|| format!("{class_indent}{unit}"));
        Self {
            indent,
            unit,
            class_indent,
            crlf: src.contains("\r\n"),
            style: *style,
        }
    }

    /// Line breaks between two members: "Blank lines around method" + 1.
    pub(crate) fn member_separator(&self) -> String {
        "\n".repeat(self.style.blank_lines_around_methods + 1)
    }

    /// Indent every non-empty line of `block` (written with 4-space levels) to member level.
    pub(crate) fn indent_block(&self, block: &str) -> String {
        apply_braces(block, self.style.braces)
            .lines()
            .map(|l| {
                if l.trim().is_empty() {
                    return String::new();
                }
                let spaces = l.chars().take_while(|c| *c == ' ').count();
                format!(
                    "{}{}{}{}",
                    self.indent,
                    self.unit.repeat(spaces / 4),
                    " ".repeat(spaces % 4),
                    &l[spaces..]
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn eol(&self, s: String) -> String {
        if self.crlf {
            s.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            s
        }
    }
}

/// Insert member blocks at the end of the class body.
pub(crate) fn insert_at_end(
    src: &str,
    class: &ClassLike,
    fmt: &Fmt,
    blocks: &[String],
) -> TextEdit {
    let open = class.body_range.start;
    let close = class.body_range.end - 1;
    let body = fmt.indent_block(&blocks.join(&fmt.member_separator()));
    let inner = &src[open + 1..close];
    if inner.trim().is_empty() {
        TextEdit {
            start: open + 1,
            end: close,
            new_text: fmt.eol(format!("\n{body}\n{}", fmt.class_indent)),
        }
    } else {
        let content_end = open + 1 + inner.trim_end().len();
        TextEdit {
            start: content_end,
            end: content_end,
            new_text: fmt.eol(format!("{}{body}", fmt.member_separator())),
        }
    }
}

/// Insert after the last property/constant (or at the top of the body).
pub(crate) fn insert_after_fields(
    src: &str,
    class: &ClassLike,
    fmt: &Fmt,
    block: &str,
) -> TextEdit {
    let last_field_end = class
        .members
        .iter()
        .filter(|m| {
            !m.promoted
                && matches!(
                    m.kind,
                    SymbolKind::Property | SymbolKind::Constant | SymbolKind::EnumCase
                )
        })
        .map(|m| m.range.end)
        .max();
    let body = fmt.indent_block(block);
    match last_field_end {
        Some(end) => TextEdit {
            start: end,
            end,
            new_text: fmt.eol(format!("{}{body}", fmt.member_separator())),
        },
        None => {
            let open = class.body_range.start;
            let close = class.body_range.end - 1;
            if src[open + 1..close].trim().is_empty() {
                insert_at_end(src, class, fmt, &[block.to_string()])
            } else {
                TextEdit {
                    start: open + 1,
                    end: open + 1,
                    new_text: fmt.eol(format!("\n{body}\n")),
                }
            }
        }
    }
}

/// `first_name` / `firstName` -> `FirstName`.
pub fn studly(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn is_bool_type(t: Option<&str>) -> bool {
    matches!(
        t.map(|t| t.trim_start_matches('?').to_ascii_lowercase())
            .as_deref(),
        Some("bool" | "boolean" | "bool|null" | "null|bool")
    )
}

/// Getter name as PHPStorm generates it (`isActive` for booleans).
pub fn getter_name(field: &str, ty: Option<&str>) -> String {
    if is_bool_type(ty) {
        let starts_with_prefix = ["is", "has", "can", "should"].iter().any(|p| {
            field.len() > p.len()
                && field.starts_with(p)
                && field[p.len()..].starts_with(|c: char| c.is_uppercase() || c == '_')
        });
        if starts_with_prefix {
            return field.to_string();
        }
        return format!("is{}", studly(field));
    }
    format!("get{}", studly(field))
}

pub fn setter_name(field: &str) -> String {
    format!("set{}", studly(field))
}

/// Native type usable in a signature (docblock-only types are not).
fn native_type(m: &Member) -> Option<String> {
    m.type_hint.clone()
}

fn doc_type(m: &Member) -> Option<String> {
    m.effective_type()
}

fn has_method(class: &ClassLike, name: &str) -> bool {
    class.method(name).is_some()
}

/// Fields assigned in the constructor body (`$this->x = `) or promoted.
fn constructor_fields(src: &str, class: &ClassLike) -> HashSet<String> {
    let mut out: HashSet<String> = class
        .properties()
        .filter(|p| p.promoted)
        .map(|p| p.name.clone())
        .collect();
    if let Some(ctor) = class.method("__construct") {
        let body = &src[ctor.range.start..ctor.range.end];
        for p in class.properties() {
            if body.contains(&format!("$this->{} =", p.name))
                || body.contains(&format!("$this->{}=", p.name))
            {
                out.insert(p.name.clone());
            }
        }
    }
    out
}

fn field_targets(src: &str, class: &ClassLike) -> Vec<FieldTarget> {
    let in_ctor = constructor_fields(src, class);
    class
        .properties()
        .map(|p| {
            let ty = doc_type(p);
            FieldTarget {
                name: p.name.clone(),
                has_getter: has_method(class, &getter_name(&p.name, ty.as_deref())),
                has_setter: has_method(class, &setter_name(&p.name)),
                type_hint: ty,
                visibility: p.visibility,
                is_static: p.modifiers.is_static,
                is_readonly: p.modifiers.is_readonly,
                promoted: p.promoted,
                in_constructor: in_ctor.contains(&p.name),
            }
        })
        .collect()
}

/// Render a parameter list/return type of an ancestor method for the target file.
struct TypeMapper<'a> {
    from_ns: String,
    from_uses: Vec<UseImport>,
    to_ns: &'a str,
    to_uses: &'a [UseImport],
    owner_fqn: String,
}

impl TypeMapper<'_> {
    fn map(&self, ty: &str) -> String {
        let resolved = resolve_type(ty, &self.from_ns, &self.from_uses);
        // `self` in the parent means the parent class.
        let resolved = resolved
            .split('|')
            .map(|p| {
                if p.trim_start_matches('?').eq_ignore_ascii_case("self") {
                    format!(
                        "{}\\{}",
                        if p.starts_with('?') { "?" } else { "" },
                        self.owner_fqn
                    )
                } else {
                    p.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("|");
        shorten_type(&resolved, self.to_ns, self.to_uses)
    }
}

fn method_stub_signature(m: &Member, mapper: &TypeMapper) -> (String, Option<String>) {
    let params: Vec<String> = m
        .params
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.promoted = None;
            p.type_hint = p.type_hint.as_deref().map(|t| mapper.map(t));
            p.render()
        })
        .collect();
    let ret = m.type_hint.as_deref().map(|t| mapper.map(t));
    let sig = format!(
        "{}({}){}",
        m.name,
        params.join(", "),
        ret.as_ref().map(|r| format!(": {r}")).unwrap_or_default()
    );
    (sig, ret)
}

struct Candidate {
    owner: ClassLike,
    member: Member,
    abstract_: bool,
}

/// Inherited methods not defined in `class`, nearest declaration first.
fn inherited_methods(class: &ClassLike, resolver: &mut dyn ClassResolver) -> Vec<Candidate> {
    let mut seen: HashSet<String> = class
        .methods()
        .map(|m| m.name.to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();
    let ancestors = resolver.ancestors(class);
    // Concrete implementations anywhere in the hierarchy satisfy abstract declarations.
    let implemented: HashSet<String> = ancestors
        .iter()
        .filter(|a| a.kind != SymbolKind::Interface)
        .flat_map(|a| a.methods().filter(|m| !m.modifiers.is_abstract))
        .map(|m| m.name.to_ascii_lowercase())
        .collect();
    for anc in &ancestors {
        for m in anc.methods() {
            let key = m.name.to_ascii_lowercase();
            if m.visibility == Visibility::Private || !seen.insert(key.clone()) {
                continue;
            }
            let abstract_ = (m.modifiers.is_abstract || anc.kind == SymbolKind::Interface)
                && !implemented.contains(&key);
            out.push(Candidate {
                owner: anc.clone(),
                member: m.clone(),
                abstract_,
            });
        }
    }
    out
}

fn method_targets(
    file: &FileIndex,
    class: &ClassLike,
    resolver: &mut dyn ClassResolver,
) -> (Vec<MethodTarget>, Vec<MethodTarget>) {
    let mut overrides = Vec::new();
    let mut implements = Vec::new();
    for c in inherited_methods(class, resolver) {
        if c.member.modifiers.is_final && !c.abstract_ {
            continue;
        }
        let mapper = TypeMapper {
            from_ns: c.owner.namespace.clone(),
            from_uses: resolver.uses_of(&c.owner),
            to_ns: &class.namespace,
            to_uses: &file.uses,
            owner_fqn: c.owner.fqn.clone(),
        };
        let (signature, _) = method_stub_signature(&c.member, &mapper);
        let t = MethodTarget {
            id: format!("{}::{}", c.owner.fqn, c.member.name),
            name: c.member.name.clone(),
            owner: c.owner.fqn.clone(),
            owner_kind: c.owner.kind,
            signature,
            visibility: c.member.visibility,
            is_static: c.member.modifiers.is_static,
        };
        if c.abstract_ {
            implements.push(t);
        } else {
            overrides.push(t);
        }
    }
    (overrides, implements)
}

/// Innermost function/method/property at `offset`, else the class around it.
fn doc_target(file: &FileIndex, offset: usize) -> Option<DocTarget> {
    if let Some(class) = file.class_at(offset) {
        if let Some(m) = class
            .members
            .iter()
            .filter(|m| m.range.contains(offset) && !m.promoted)
            .min_by_key(|m| m.range.end - m.range.start)
        {
            return Some(DocTarget::Member(m.clone(), class.name.clone()));
        }
        return Some(DocTarget::Class(class.clone()));
    }
    file.functions
        .iter()
        .find(|f| f.range.contains(offset))
        .map(|f| DocTarget::Function(f.clone()))
}

enum DocTarget {
    Class(ClassLike),
    /// A member and the name of its class.
    Member(Member, String),
    Function(crate::model::FunctionDecl),
}

/// What can be generated at `offset`.
pub fn targets(
    file: &FileIndex,
    src: &str,
    offset: usize,
    resolver: &mut dyn ClassResolver,
) -> GenerateTargets {
    let mut t = GenerateTargets {
        php_doc_target: doc_target(file, offset).and_then(|d| match d {
            DocTarget::Class(c) if c.doc.is_none() => Some(c.name),
            DocTarget::Member(m, _) if m.doc.is_none() => Some(m.name),
            DocTarget::Function(f) if f.doc.is_none() => Some(f.name),
            _ => None,
        }),
        ..Default::default()
    };
    let Some(class) = file.class_at(offset) else {
        return t;
    };
    t.class_name = Some(class.name.clone());
    t.class_kind = Some(class.kind);
    t.has_constructor = class.method("__construct").is_some();
    t.has_to_string = class.method("__toString").is_some();
    if class.kind != SymbolKind::Interface {
        t.fields = field_targets(src, class);
    }
    let (o, i) = method_targets(file, class, resolver);
    t.override_methods = o;
    t.implement_methods = i;
    t
}

fn selected_fields<'a>(class: &'a ClassLike, opts: &GenerateOptions) -> Vec<&'a Member> {
    // Keep declaration order, not selection order.
    class
        .properties()
        .filter(|p| {
            opts.fields
                .iter()
                .any(|f| f.trim_start_matches('$') == p.name)
        })
        .collect()
}

fn docblock(lines: &[String]) -> String {
    let mut s = String::from("/**\n");
    for l in lines {
        if l.is_empty() {
            s.push_str(" *\n");
        } else {
            s.push_str(&format!(" * {l}\n"));
        }
    }
    s.push_str(" */\n");
    s
}

fn gen_constructor(
    src: &str,
    file: &FileIndex,
    class: &ClassLike,
    opts: &GenerateOptions,
    resolver: &mut dyn ClassResolver,
) -> Result<Vec<TextEdit>> {
    if class.method("__construct").is_some() {
        return Err(PhpError::Generate(format!(
            "Class '{}' already has a constructor",
            class.name
        )));
    }
    let fields: Vec<&Member> = selected_fields(class, opts)
        .into_iter()
        .filter(|f| !f.modifiers.is_static && !f.promoted)
        .collect();
    // Parent constructor parameters are passed through (`parent::__construct(...)`).
    let mut parent_params: Vec<String> = Vec::new();
    let mut parent_args: Vec<String> = Vec::new();
    let mut parent_doc: Vec<String> = Vec::new();
    let has_parent_ctor = if let Some(parent_fqn) = class.extends.first() {
        let ancestors = resolver.ancestors(class);
        let parent_ctor = ancestors
            .iter()
            .filter(|a| a.kind == SymbolKind::Class)
            .find_map(|a| a.method("__construct").map(|m| (a.clone(), m.clone())));
        if let Some((owner, ctor)) = parent_ctor {
            let mapper = TypeMapper {
                from_ns: owner.namespace.clone(),
                from_uses: resolver.uses_of(&owner),
                to_ns: &class.namespace,
                to_uses: &file.uses,
                owner_fqn: parent_fqn.clone(),
            };
            for p in &ctor.params {
                let mut p = p.clone();
                p.promoted = None;
                p.readonly = false;
                p.type_hint = p.type_hint.as_deref().map(|t| mapper.map(t));
                parent_doc.push(format!(
                    "@param {} ${}",
                    p.type_hint.clone().unwrap_or_else(|| "mixed".into()),
                    p.name
                ));
                parent_args.push(format!(
                    "{}${}",
                    if p.variadic { "..." } else { "" },
                    p.name
                ));
                parent_params.push(p.render());
            }
            true
        } else {
            false
        }
    } else {
        false
    };
    let mut params: Vec<String> = parent_params;
    let mut doc_lines: Vec<String> = parent_doc;
    let mut needs_doc = false;
    for f in &fields {
        let native = native_type(f);
        let ty = doc_type(f);
        if native.is_none() {
            needs_doc = true;
        }
        doc_lines.push(format!(
            "@param {} ${}",
            ty.clone().unwrap_or_else(|| "mixed".into()),
            f.name
        ));
        params.push(match native {
            Some(t) => format!("{t} ${}", f.name),
            None => format!("${}", f.name),
        });
    }
    // Variadic parameters must come last.
    if let Some(pos) = params.iter().position(|p| p.contains("...$")) {
        if pos + 1 < params.len() {
            let v = params.remove(pos);
            params.push(v);
        }
    }
    let mut block = String::new();
    if needs_doc {
        block.push_str(&docblock(&doc_lines));
    }
    block.push_str(&format!(
        "public function __construct({})\n{{\n",
        params.join(", ")
    ));
    if has_parent_ctor {
        block.push_str(&format!(
            "    parent::__construct({});\n",
            parent_args.join(", ")
        ));
    }
    for f in &fields {
        block.push_str(&format!("    $this->{0} = ${0};\n", f.name));
    }
    block.push('}');
    let fmt = Fmt::for_class(src, class, &opts.style);
    Ok(vec![insert_after_fields(src, class, &fmt, &block)])
}

fn getter_block(f: &Member) -> String {
    let ty = doc_type(f);
    let native = native_type(f);
    let name = getter_name(&f.name, ty.as_deref());
    let mut s = String::new();
    if native.is_none() {
        if let Some(t) = &ty {
            s.push_str(&docblock(&[format!("@return {t}")]));
        }
    }
    let ret = native.map(|t| format!(": {t}")).unwrap_or_default();
    if f.modifiers.is_static {
        s.push_str(&format!(
            "public static function {name}(){ret}\n{{\n    return self::${};\n}}",
            f.name
        ));
    } else {
        s.push_str(&format!(
            "public function {name}(){ret}\n{{\n    return $this->{};\n}}",
            f.name
        ));
    }
    s
}

fn setter_block(f: &Member, fluent: bool) -> String {
    let ty = doc_type(f);
    let native = native_type(f);
    let name = setter_name(&f.name);
    let fluent = fluent && !f.modifiers.is_static;
    let mut s = String::new();
    if native.is_none() {
        if let Some(t) = &ty {
            let mut lines = vec![format!("@param {t} ${}", f.name)];
            if fluent {
                lines.push("@return $this".into());
            }
            s.push_str(&docblock(&lines));
        }
    }
    let param = match native {
        Some(t) => format!("{t} ${}", f.name),
        None => format!("${}", f.name),
    };
    let ret = if fluent { "static" } else { "void" };
    if f.modifiers.is_static {
        s.push_str(&format!(
            "public static function {name}({param}): {ret}\n{{\n    self::${0} = ${0};\n}}",
            f.name
        ));
    } else if fluent {
        s.push_str(&format!(
            "public function {name}({param}): {ret}\n{{\n    $this->{0} = ${0};\n    return $this;\n}}",
            f.name
        ));
    } else {
        s.push_str(&format!(
            "public function {name}({param}): {ret}\n{{\n    $this->{0} = ${0};\n}}",
            f.name
        ));
    }
    s
}

fn gen_accessors(
    src: &str,
    class: &ClassLike,
    opts: &GenerateOptions,
    getters: bool,
    setters: bool,
) -> Result<Vec<TextEdit>> {
    let mut blocks = Vec::new();
    for f in selected_fields(class, opts) {
        let ty = doc_type(f);
        if getters && !has_method(class, &getter_name(&f.name, ty.as_deref())) {
            blocks.push(getter_block(f));
        }
        if setters && !f.modifiers.is_readonly && !has_method(class, &setter_name(&f.name)) {
            blocks.push(setter_block(f, opts.fluent));
        }
    }
    if blocks.is_empty() {
        return Err(PhpError::Generate("No fields selected".into()));
    }
    let fmt = Fmt::for_class(src, class, &opts.style);
    Ok(vec![insert_at_end(src, class, &fmt, &blocks)])
}

fn gen_to_string(src: &str, class: &ClassLike, opts: &GenerateOptions) -> Result<Vec<TextEdit>> {
    if class.method("__toString").is_some() {
        return Err(PhpError::Generate(format!(
            "Class '{}' already has a __toString() method",
            class.name
        )));
    }
    let fields: Vec<&Member> = if opts.fields.is_empty() {
        class
            .properties()
            .filter(|p| !p.modifiers.is_static)
            .collect()
    } else {
        selected_fields(class, opts)
    };
    let mut body = format!("    return '{}{{'", class.name);
    for (i, f) in fields.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        let ty = doc_type(f).unwrap_or_default().to_ascii_lowercase();
        let value = if ty.contains("array") || ty.contains("[]") {
            format!("json_encode($this->{})", f.name)
        } else if ty.trim_start_matches('?') == "bool" {
            format!("($this->{} ? 'true' : 'false')", f.name)
        } else {
            format!("$this->{}", f.name)
        };
        body.push_str(&format!(" .\n        '{sep}{}=' . {value}", f.name));
    }
    body.push_str(" .\n        '}';");
    let block = format!("public function __toString(): string\n{{\n{body}\n}}");
    let fmt = Fmt::for_class(src, class, &opts.style);
    Ok(vec![insert_at_end(src, class, &fmt, &[block])])
}

fn gen_methods(
    src: &str,
    file: &FileIndex,
    class: &ClassLike,
    opts: &GenerateOptions,
    resolver: &mut dyn ClassResolver,
    implement: bool,
) -> Result<Vec<TextEdit>> {
    let candidates = inherited_methods(class, resolver);
    let wanted = |c: &Candidate| {
        opts.methods.iter().any(|id| match id.rsplit_once("::") {
            Some((owner, name)) => {
                owner
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(&c.owner.fqn)
                    && name.eq_ignore_ascii_case(&c.member.name)
            }
            None => id.eq_ignore_ascii_case(&c.member.name),
        })
    };
    let mut blocks = Vec::new();
    for c in candidates
        .iter()
        .filter(|c| c.abstract_ == implement && wanted(c))
    {
        let mapper = TypeMapper {
            from_ns: c.owner.namespace.clone(),
            from_uses: resolver.uses_of(&c.owner),
            to_ns: &class.namespace,
            to_uses: &file.uses,
            owner_fqn: c.owner.fqn.clone(),
        };
        let (sig, ret) = method_stub_signature(&c.member, &mapper);
        let st = if c.member.modifiers.is_static {
            " static"
        } else {
            ""
        };
        let head = format!("{}{st} function {sig}", c.member.visibility.as_str());
        let args: Vec<String> = c
            .member
            .params
            .iter()
            .map(|p| format!("{}${}", if p.variadic { "..." } else { "" }, p.name))
            .collect();
        let mut vars = Vars::new();
        vars.insert("NAME".into(), c.member.name.clone());
        vars.insert("CLASS_NAME".into(), class.name.clone());
        vars.insert("PARAMETERS".into(), args.join(", "));
        vars.insert("RETURN_TYPE".into(), ret.clone().unwrap_or_default());
        let template = if implement {
            templates::IMPLEMENTED_METHOD_BODY
        } else {
            templates::OVERRIDDEN_METHOD_BODY
        };
        let rendered = render_code(&opts.code_templates, template, &vars)
            .map_err(|e| PhpError::Generate(e.to_string()))?;
        let body = rendered
            .trim_end()
            .lines()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    format!("    {l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let body = if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        };
        blocks.push(format!("{head}\n{{\n{body}}}"));
    }
    if blocks.is_empty() {
        return Err(PhpError::Generate("No methods selected".into()));
    }
    let fmt = Fmt::for_class(src, class, &opts.style);
    Ok(vec![insert_at_end(src, class, &fmt, &blocks)])
}

/// Exceptions thrown directly in a body (`throw new X(...)`).
fn thrown(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("throw new ") {
        let after = &rest[i + "throw new ".len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = after;
    }
    out
}

fn param_doc(params: &[crate::model::Param]) -> String {
    params
        .iter()
        .map(|p| {
            format!(
                " * @param {} {}${}",
                p.type_hint.clone().unwrap_or_else(|| "mixed".into()),
                if p.variadic { "..." } else { "" },
                p.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn throws_doc(body: &str) -> String {
    thrown(body)
        .iter()
        .map(|t| format!(" * @throws {t}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn gen_php_doc(
    src: &str,
    file: &FileIndex,
    offset: usize,
    opts: &GenerateOptions,
) -> Result<Vec<TextEdit>> {
    let target = doc_target(file, offset).ok_or_else(|| {
        PhpError::Generate("Place the caret on a class, function or field".into())
    })?;
    let mut vars = Vars::new();
    let mut set = |k: &str, v: String| {
        vars.insert(k.to_string(), v);
    };
    let (range, doc_exists, template) = match &target {
        DocTarget::Class(c) => {
            set("NAME", c.name.clone());
            set("NAMESPACE", c.namespace.clone());
            (c.range, c.doc.is_some(), Some(templates::CLASS_DOC_COMMENT))
        }
        DocTarget::Member(m, class) => {
            set("NAME", m.name.clone());
            set("CLASS_NAME", class.clone());
            let template = match m.kind {
                SymbolKind::Method => {
                    set("PARAM_DOC", param_doc(&m.params));
                    let ty = if m.name.eq_ignore_ascii_case("__construct") {
                        "void".to_string()
                    } else {
                        m.type_hint.clone().unwrap_or_else(|| "mixed".into())
                    };
                    set("TYPE_HINT", ty);
                    set("THROWS_DOC", throws_doc(&src[m.range.start..m.range.end]));
                    Some(templates::FUNCTION_DOC_COMMENT)
                }
                SymbolKind::Property | SymbolKind::Constant => {
                    set(
                        "TYPE_HINT",
                        m.type_hint.clone().unwrap_or_else(|| "mixed".into()),
                    );
                    Some(templates::FIELD_DOC_COMMENT)
                }
                _ => None,
            };
            (m.range, m.doc.is_some(), template)
        }
        DocTarget::Function(f) => {
            set("NAME", f.name.clone());
            set("PARAM_DOC", param_doc(&f.params));
            set(
                "TYPE_HINT",
                f.return_type.clone().unwrap_or_else(|| "mixed".into()),
            );
            set("THROWS_DOC", throws_doc(&src[f.range.start..f.range.end]));
            (
                f.range,
                f.doc.is_some(),
                Some(templates::FUNCTION_DOC_COMMENT),
            )
        }
    };
    if doc_exists {
        return Err(PhpError::Generate("PHPDoc comment already exists".into()));
    }
    let rendered = match template {
        Some(t) => render_code(&opts.code_templates, t, &vars)
            .map_err(|e| PhpError::Generate(e.to_string()))?,
        None => "/**\n */".to_string(),
    };
    // Empty variables (no parameters, nothing thrown) leave empty lines behind.
    let doc: String = rendered
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| format!("{l}\n"))
        .collect();
    if doc.is_empty() {
        return Err(PhpError::Generate("The PHPDoc template is empty".into()));
    }
    let indent = leading_ws(src, range.start);
    let ls = line_start(src, range.start);
    let mut text = String::new();
    for l in doc.lines() {
        text.push_str(&indent);
        text.push_str(l);
        text.push('\n');
    }
    // The declaration may not start its line (`<?php function f() {}`).
    let (start, text) = if src[ls..range.start].trim().is_empty() {
        (ls, text)
    } else {
        (range.start, format!("{}{indent}", text.trim_start()))
    };
    let text = if src.contains("\r\n") {
        text.replace('\n', "\r\n")
    } else {
        text
    };
    Ok(vec![TextEdit {
        start,
        end: start,
        new_text: text,
    }])
}

/// Run a generate action at `offset` (bytes) in `src`.
pub fn generate(
    file: &FileIndex,
    src: &str,
    offset: usize,
    kind: GenerateKind,
    opts: &GenerateOptions,
    resolver: &mut dyn ClassResolver,
) -> Result<Vec<TextEdit>> {
    if kind == GenerateKind::PhpDoc {
        return gen_php_doc(src, file, offset, opts);
    }
    let class = file
        .class_at(offset)
        .ok_or_else(|| PhpError::Generate("Place the caret inside a class".into()))?;
    match kind {
        GenerateKind::Constructor => gen_constructor(src, file, class, opts, resolver),
        GenerateKind::Getters => gen_accessors(src, class, opts, true, false),
        GenerateKind::Setters => gen_accessors(src, class, opts, false, true),
        GenerateKind::GettersAndSetters => gen_accessors(src, class, opts, true, true),
        GenerateKind::ToString => gen_to_string(src, class, opts),
        GenerateKind::OverrideMethods => gen_methods(src, file, class, opts, resolver, false),
        GenerateKind::ImplementMethods => gen_methods(src, file, class, opts, resolver, true),
        GenerateKind::PhpDoc => unreachable!(),
    }
}

/// Short name helper used by UIs listing method owners.
pub fn owner_label(fqn: &str) -> &str {
    short_name(fqn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::index_source;

    const CHECKOUT: &str = "<?php

namespace App\\Services;

class Checkout
{
    private string $currency;
    private float $taxRate;

    public function __construct(string $currency = 'EUR', float $taxRate = 0.19)
    {
        $this->currency = $currency;
        $this->taxRate = $taxRate;
    }
}
";

    fn run(src: &str, needle: &str, kind: GenerateKind, opts: GenerateOptions) -> Result<String> {
        run_with(src, needle, kind, opts, Vec::new())
    }

    fn run_with(
        src: &str,
        needle: &str,
        kind: GenerateKind,
        opts: GenerateOptions,
        others: Vec<FileIndex>,
    ) -> Result<String> {
        let file = index_source("a.php", src);
        let offset = src.find(needle).expect("needle");
        let mut resolver = FilesResolver(others);
        let edits = generate(&file, src, offset, kind, &opts, &mut resolver)?;
        Ok(apply_edits(src, &edits))
    }

    fn fields(names: &[&str]) -> GenerateOptions {
        GenerateOptions {
            fields: names.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn naming() {
        assert_eq!(studly("first_name"), "FirstName");
        assert_eq!(studly("taxRate"), "TaxRate");
        assert_eq!(getter_name("active", Some("bool")), "isActive");
        assert_eq!(getter_name("isActive", Some("?bool")), "isActive");
        assert_eq!(getter_name("hasItems", Some("bool")), "hasItems");
        assert_eq!(getter_name("island", Some("bool")), "isIsland");
        assert_eq!(getter_name("total", Some("float")), "getTotal");
        assert_eq!(setter_name("tax_rate"), "setTaxRate");
    }

    #[test]
    fn getters_and_setters_for_checkout() {
        let out = run(
            CHECKOUT,
            "taxRate;",
            GenerateKind::GettersAndSetters,
            fields(&["currency", "taxRate"]),
        )
        .unwrap();
        let expected =
            "    public function __construct(string $currency = 'EUR', float $taxRate = 0.19)
    {
        $this->currency = $currency;
        $this->taxRate = $taxRate;
    }

    public function getCurrency(): string
    {
        return $this->currency;
    }

    public function setCurrency(string $currency): void
    {
        $this->currency = $currency;
    }

    public function getTaxRate(): float
    {
        return $this->taxRate;
    }

    public function setTaxRate(float $taxRate): void
    {
        $this->taxRate = $taxRate;
    }
}
";
        assert!(out.ends_with(expected), "{out}");
    }

    #[test]
    fn fluent_setters_and_static_fields() {
        let src = "<?php\nclass A\n{\n    protected ?int $count = null;\n    private static $registry;\n}\n";
        let out = run(
            src,
            "count",
            GenerateKind::Setters,
            GenerateOptions {
                fields: vec!["count".into(), "registry".into()],
                fluent: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(out.contains(
            "    public function setCount(?int $count): static\n    {\n        $this->count = $count;\n        return $this;\n    }"
        ), "{out}");
        assert!(out.contains(
            "    public static function setRegistry($registry): void\n    {\n        self::$registry = $registry;\n    }\n}\n"
        ), "{out}");
    }

    #[test]
    fn getters_use_docblock_types_and_skip_existing() {
        let src = "<?php\nclass A\n{\n    /** @var list<string> */\n    private $tags = [];\n    private bool $active;\n    private $x;\n\n    public function getX() { return $this->x; }\n}\n";
        let out = run(
            src,
            "tags",
            GenerateKind::Getters,
            fields(&["tags", "active", "x"]),
        )
        .unwrap();
        assert!(out.contains(
            "    /**\n     * @return list<string>\n     */\n    public function getTags()\n    {\n        return $this->tags;\n    }"
        ), "{out}");
        assert!(out.contains("    public function isActive(): bool\n"));
        assert_eq!(out.matches("function getX").count(), 1);
    }

    #[test]
    fn readonly_fields_get_no_setter() {
        let src =
            "<?php\nclass A\n{\n    public function __construct(private readonly int $id) {}\n}\n";
        let err = run(src, "class A", GenerateKind::Setters, fields(&["id"])).unwrap_err();
        assert_eq!(err.to_string(), "No fields selected");
        let out = run(src, "class A", GenerateKind::Getters, fields(&["id"])).unwrap();
        assert!(out.contains("public function getId(): int"));
    }

    #[test]
    fn empty_class_bodies() {
        let src = "<?php\nclass A {}\n";
        let out = run(src, "A", GenerateKind::ToString, GenerateOptions::default()).unwrap();
        assert_eq!(
            out,
            "<?php\nclass A {\n    public function __toString(): string\n    {\n        return 'A{' .\n            '}';\n    }\n}\n"
        );
    }

    #[test]
    fn constructor_after_fields_with_tabs_and_crlf() {
        let src = "<?php\r\nnamespace X;\r\n\r\nclass Point\r\n{\r\n\tprivate int $x;\r\n\t/** @var float */\r\n\tprivate $y;\r\n\tpublic static $count;\r\n\r\n\tpublic function len(): float { return 0; }\r\n}\r\n";
        let out = run(
            src,
            "Point",
            GenerateKind::Constructor,
            fields(&["x", "y", "count"]),
        )
        .unwrap();
        let expected = "\tpublic static $count;\r\n\r\n\t/**\r\n\t * @param int $x\r\n\t * @param float $y\r\n\t */\r\n\tpublic function __construct(int $x, $y)\r\n\t{\r\n\t\t$this->x = $x;\r\n\t\t$this->y = $y;\r\n\t}\r\n\r\n\tpublic function len()";
        assert!(out.contains(expected), "{out:?}");
    }

    #[test]
    fn constructor_rejects_existing_and_calls_parent() {
        let err = run(
            CHECKOUT,
            "Checkout",
            GenerateKind::Constructor,
            fields(&["currency"]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("already has a constructor"));

        let base = index_source(
            "b.php",
            "<?php\nnamespace App;\nuse Psr\\Log\\LoggerInterface;\nabstract class Base { public function __construct(protected LoggerInterface $logger, string ...$tags) {} }\n",
        );
        let src = "<?php\nnamespace App\\Sub;\nuse App\\Base;\nclass Child extends Base\n{\n    private int $n;\n}\n";
        let out = run_with(
            src,
            "Child",
            GenerateKind::Constructor,
            fields(&["n"]),
            vec![base],
        )
        .unwrap();
        assert!(out.contains(
            "    public function __construct(\\Psr\\Log\\LoggerInterface $logger, int $n, string ...$tags)\n    {\n        parent::__construct($logger, ...$tags);\n        $this->n = $n;\n    }"
        ), "{out}");
    }

    #[test]
    fn constructor_in_class_without_fields_goes_to_top() {
        let src = "<?php\nclass A\n{\n    public function f() {}\n}\n";
        let out = run(
            src,
            "A",
            GenerateKind::Constructor,
            GenerateOptions::default(),
        )
        .unwrap();
        assert_eq!(
            out,
            "<?php\nclass A\n{\n    public function __construct()\n    {\n    }\n\n    public function f() {}\n}\n"
        );
    }

    #[test]
    fn to_string_formats_fields() {
        let out = run(
            CHECKOUT,
            "Checkout",
            GenerateKind::ToString,
            GenerateOptions::default(),
        )
        .unwrap();
        assert!(out.contains(
            "    public function __toString(): string\n    {\n        return 'Checkout{' .\n            'currency=' . $this->currency .\n            ', taxRate=' . $this->taxRate .\n            '}';\n    }\n}\n"
        ), "{out}");
    }

    fn hierarchy() -> Vec<FileIndex> {
        vec![
            index_source(
                "Model.php",
                "<?php\nnamespace Lib;\nuse Lib\\Support\\Collection;\nabstract class Model implements \\JsonSerializable, HasName\n{\n    public function save(array $options = []): bool {}\n    public function all(): Collection {}\n    protected function boot(): void {}\n    final public function key(): string {}\n    private function secret() {}\n    abstract protected function table(): string;\n    public static function make(self ...$items): static {}\n}\n",
            ),
            index_source(
                "HasName.php",
                "<?php\nnamespace Lib;\ninterface HasName\n{\n    public function name(?Model $m = null): string;\n    public function save(array $options = []): bool;\n}\n",
            ),
        ]
    }

    const USER: &str = "<?php\nnamespace App;\n\nuse Lib\\Model;\nuse Lib\\Support\\Collection;\n\nclass User extends Model\n{\n    public function all(): Collection {}\n}\n";

    #[test]
    fn lists_override_and_implement_targets() {
        let file = index_source("u.php", USER);
        let mut r = FilesResolver(hierarchy());
        let t = targets(&file, USER, USER.find("class User").unwrap() + 3, &mut r);
        assert_eq!(t.class_name.as_deref(), Some("User"));
        let overrides: Vec<&str> = t
            .override_methods
            .iter()
            .map(|m| m.signature.as_str())
            .collect();
        assert_eq!(
            overrides,
            vec![
                "save(array $options = []): bool",
                "boot(): void",
                "make(Model ...$items): static"
            ]
        );
        let implement: Vec<(&str, &str)> = t
            .implement_methods
            .iter()
            .map(|m| (m.id.as_str(), m.signature.as_str()))
            .collect();
        assert_eq!(
            implement,
            vec![
                ("Lib\\Model::table", "table(): string"),
                ("Lib\\HasName::name", "name(?Model $m = null): string"),
            ]
        );
    }

    #[test]
    fn override_and_implement_stubs() {
        let out = run_with(
            USER,
            "class User",
            GenerateKind::OverrideMethods,
            GenerateOptions {
                methods: vec!["Lib\\Model::save".into(), "boot".into()],
                ..Default::default()
            },
            hierarchy(),
        )
        .unwrap();
        assert!(out.ends_with(
            "    public function all(): Collection {}\n\n    public function save(array $options = []): bool\n    {\n        return parent::save($options);\n    }\n\n    protected function boot(): void\n    {\n        parent::boot();\n    }\n}\n"
        ), "{out}");

        let out = run_with(
            USER,
            "class User",
            GenerateKind::ImplementMethods,
            GenerateOptions {
                methods: vec!["Lib\\Model::table".into(), "Lib\\HasName::name".into()],
                ..Default::default()
            },
            hierarchy(),
        )
        .unwrap();
        assert!(out.contains(
            "    protected function table(): string\n    {\n        // TODO: Implement table() method.\n    }\n\n    public function name(?Model $m = null): string\n    {\n        // TODO: Implement name() method.\n    }\n}\n"
        ), "{out}");
    }

    #[test]
    fn php_doc_for_methods_functions_and_fields() {
        let src = "<?php\nclass A\n{\n    private $x;\n\n    #[Pure]\n    public function f(int $a, ...$rest): ?string\n    {\n        throw new \\InvalidArgumentException('x');\n    }\n\n    public function g(): void {}\n}\nfunction h($v) {}\n";
        let out = run(
            src,
            "$rest",
            GenerateKind::PhpDoc,
            GenerateOptions::default(),
        )
        .unwrap();
        assert!(out.contains(
            "\n    /**\n     * @param int $a\n     * @param mixed ...$rest\n     * @return ?string\n     * @throws \\InvalidArgumentException\n     */\n    #[Pure]\n    public function f("
        ), "{out}");
        let out = run(src, "g()", GenerateKind::PhpDoc, GenerateOptions::default()).unwrap();
        assert!(
            out.contains("    /**\n     */\n    public function g()"),
            "{out}"
        );
        let out = run(src, "h(", GenerateKind::PhpDoc, GenerateOptions::default()).unwrap();
        assert!(out.contains("/**\n * @param mixed $v\n * @return mixed\n */\nfunction h($v)"));
        let out = run(src, "$x", GenerateKind::PhpDoc, GenerateOptions::default()).unwrap();
        assert!(out.contains("    /**\n     * @var mixed\n     */\n    private $x;"));

        let documented = "<?php\n/** Doc */\nfunction h() {}\n";
        assert!(run(
            documented,
            "h(",
            GenerateKind::PhpDoc,
            GenerateOptions::default()
        )
        .is_err());
    }

    #[test]
    fn php_doc_for_classes() {
        let src = "<?php\nnamespace App\\Models;\n\nfinal class Order\n{\n\n    private $x;\n}\n";
        let out = run(
            src,
            "\n\n    private",
            GenerateKind::PhpDoc,
            GenerateOptions::default(),
        )
        .unwrap();
        assert!(
            out.contains("namespace App\\Models;\n\n/**\n * Class Order\n * @package App\\Models\n */\nfinal class Order\n"),
            "{out}"
        );
    }

    #[test]
    fn code_templates_shape_the_generated_code() {
        let templates: BTreeMap<String, String> = [
            (
                templates::IMPLEMENTED_METHOD_BODY.to_string(),
                "throw new \\LogicException('${CLASS_NAME}::${NAME}(${PARAMETERS}) is not implemented');\n".to_string(),
            ),
            (
                templates::OVERRIDDEN_METHOD_BODY.to_string(),
                "$result = parent::${NAME}(${PARAMETERS});\n\nreturn $result;".to_string(),
            ),
            (
                templates::FUNCTION_DOC_COMMENT.to_string(),
                "/**\n * ${NAME}\n${PARAM_DOC}\n */".to_string(),
            ),
            (templates::FIELD_DOC_COMMENT.to_string(), "#if ($X".to_string()),
        ]
        .into();
        let opts = |methods: &[&str]| GenerateOptions {
            methods: methods.iter().map(|m| m.to_string()).collect(),
            code_templates: templates.clone(),
            ..Default::default()
        };
        let out = run_with(
            USER,
            "class User",
            GenerateKind::ImplementMethods,
            opts(&["Lib\\Model::table"]),
            hierarchy(),
        )
        .unwrap();
        assert!(out.contains("    {\n        throw new \\LogicException('User::table() is not implemented');\n    }"), "{out}");
        let out = run_with(
            USER,
            "class User",
            GenerateKind::OverrideMethods,
            opts(&["Lib\\Model::save"]),
            hierarchy(),
        )
        .unwrap();
        assert!(out.contains("    {\n        $result = parent::save($options);\n\n        return $result;\n    }"), "{out}");
        let src = "<?php\nclass A\n{\n    private $x;\n\n    public function f(int $a) {}\n}\n";
        let out = run(src, "f(", GenerateKind::PhpDoc, opts(&[])).unwrap();
        assert!(
            out.contains(
                "    /**\n     * f\n     * @param int $a\n     */\n    public function f("
            ),
            "{out}"
        );
        let err = run(src, "$x", GenerateKind::PhpDoc, opts(&[])).unwrap_err();
        assert!(err.to_string().contains("PHP Field Doc Comment"), "{err}");
    }

    #[test]
    fn targets_report_fields_and_flags() {
        let file = index_source("c.php", CHECKOUT);
        let mut r = FilesResolver(Vec::new());
        let t = targets(&file, CHECKOUT, CHECKOUT.find("taxRate;").unwrap(), &mut r);
        assert!(t.has_constructor);
        assert!(!t.has_to_string);
        assert_eq!(t.fields.len(), 2);
        assert_eq!(t.fields[0].name, "currency");
        assert_eq!(t.fields[0].type_hint.as_deref(), Some("string"));
        assert!(t.fields[0].in_constructor);
        assert!(!t.fields[0].has_getter);
        assert_eq!(t.php_doc_target.as_deref(), Some("taxRate"));

        let t = targets(&file, CHECKOUT, 3, &mut r);
        assert!(t.class_name.is_none());
    }

    #[test]
    fn outside_class_is_an_error() {
        let err = run(
            "<?php\n$x = 1;\n",
            "$x",
            GenerateKind::Getters,
            GenerateOptions::default(),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "Place the caret inside a class");
    }
}

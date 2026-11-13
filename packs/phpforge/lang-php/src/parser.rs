//! Tree-sitter based PHP file indexer: turns source into a [`FileIndex`].

use tree_sitter::{Node, Parser, Tree};

use crate::docblock::{self, AnnotationArg, DocBlock};
use crate::model::*;
use crate::resolve::resolve_class_name;

/// Create a parser for PHP files (with the leading `<?php` / inline HTML grammar).
pub fn new_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("tree-sitter-php grammar is compatible");
    parser
}

pub fn parse_tree(parser: &mut Parser, source: &str) -> Option<Tree> {
    parser.parse(source, None)
}

/// Index one file. `path` is stored verbatim in the result.
pub fn index_source(path: &str, source: &str) -> FileIndex {
    let mut parser = new_parser();
    index_with(&mut parser, path, source)
}

/// Index one file reusing a parser (one per thread).
pub fn index_with(parser: &mut Parser, path: &str, source: &str) -> FileIndex {
    let mut index = FileIndex {
        path: path.to_string(),
        ..Default::default()
    };
    let Some(tree) = parse_tree(parser, source) else {
        return index;
    };
    let mut ctx = Ctx {
        src: source,
        namespace: String::new(),
        index: &mut index,
    };
    ctx.walk_statements(tree.root_node());
    index
}

struct Ctx<'a> {
    src: &'a str,
    namespace: String,
    index: &'a mut FileIndex,
}

pub(crate) fn range_of(node: Node) -> Range {
    let p = node.start_position();
    Range {
        start: node.start_byte(),
        end: node.end_byte(),
        line: p.row as u32,
        column: p.column as u32,
    }
}

impl<'a> Ctx<'a> {
    fn text(&self, node: Node) -> &'a str {
        &self.src[node.byte_range()]
    }

    fn resolve(&self, name: &str) -> String {
        resolve_class_name(name, &self.namespace, &self.index.uses)
    }

    fn qualify(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }

    fn walk_statements(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in children {
            match child.kind() {
                "namespace_definition" => {
                    let name = child
                        .child_by_field_name("name")
                        .map(|n| self.text(n).to_string())
                        .unwrap_or_default();
                    self.namespace = name.clone();
                    self.index.namespaces.push(NamespaceDecl {
                        name,
                        range: range_of(child),
                    });
                    if let Some(body) = child.child_by_field_name("body") {
                        self.walk_statements(body);
                        self.namespace.clear();
                    }
                }
                "namespace_use_declaration" => self.use_declaration(child),
                "class_declaration"
                | "interface_declaration"
                | "trait_declaration"
                | "enum_declaration" => {
                    if let Some(c) = self.class_like(child) {
                        self.index.classes.push(c);
                    }
                }
                "function_definition" => {
                    if let Some(f) = self.function(child) {
                        self.index.functions.push(f);
                    }
                }
                "const_declaration" => {
                    let doc = self.doc_for(child);
                    let mut c = child.walk();
                    for el in child.named_children(&mut c) {
                        if el.kind() != "const_element" {
                            continue;
                        }
                        let (name, value) = self.const_element(el);
                        if let Some(name) = name {
                            self.index.constants.push(ConstantDecl {
                                fqn: self.qualify(&name),
                                name,
                                range: range_of(el),
                                value,
                                doc: doc.clone(),
                            });
                        }
                    }
                }
                // Declarations wrapped in blocks or conditionals (`if (!function_exists(...))`).
                "compound_statement" | "if_statement" | "else_clause" | "declare_statement" => {
                    self.walk_statements(child)
                }
                _ => {}
            }
        }
    }

    fn use_declaration(&mut self, node: Node) {
        let head = self.text(node).trim_start_matches("use").trim_start();
        let kind = match head.split_whitespace().next() {
            Some("function") => UseKind::Function,
            Some("const") => UseKind::Const,
            _ => UseKind::Class,
        };
        let mut prefix = String::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "namespace_name" => prefix = self.text(child).to_string(),
                "namespace_use_clause" => self.use_clause(child, "", kind),
                "namespace_use_group" => {
                    let mut c = child.walk();
                    for clause in child.named_children(&mut c) {
                        if clause.kind() == "namespace_use_clause" {
                            let clause_kind = match self.text(clause).split_whitespace().next() {
                                Some("function") => UseKind::Function,
                                Some("const") => UseKind::Const,
                                _ => kind,
                            };
                            self.use_clause(clause, &prefix, clause_kind);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn use_clause(&mut self, clause: Node, prefix: &str, kind: UseKind) {
        let alias = clause
            .child_by_field_name("alias")
            .map(|a| self.text(a).to_string());
        let mut c = clause.walk();
        let name = clause
            .named_children(&mut c)
            .find(|n| {
                matches!(n.kind(), "name" | "qualified_name")
                    && Some(*n) != clause.child_by_field_name("alias")
            })
            .map(|n| self.text(n).trim_start_matches('\\').to_string());
        if let Some(name) = name {
            let fqn = if prefix.is_empty() {
                name
            } else {
                format!("{}\\{name}", prefix.trim_matches('\\'))
            };
            self.index.uses.push(UseImport {
                kind,
                fqn,
                alias,
                range: range_of(clause),
            });
        }
    }

    /// Docblock comment directly preceding `node` (skipping nothing but whitespace).
    fn doc_for(&self, node: Node) -> Option<DocBlock> {
        let prev = node.prev_sibling()?;
        if prev.kind() != "comment" {
            return None;
        }
        let text = self.text(prev);
        docblock::is_docblock(text).then(|| docblock::parse(text))
    }

    fn attributes(&self, node: Node) -> Vec<Attribute> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for list in node.children(&mut cursor) {
            if list.kind() != "attribute_list" {
                continue;
            }
            let mut c1 = list.walk();
            for group in list.named_children(&mut c1) {
                let mut c2 = group.walk();
                for attr in group.named_children(&mut c2) {
                    if attr.kind() != "attribute" {
                        continue;
                    }
                    let mut c3 = attr.walk();
                    let Some(name_node) = attr
                        .named_children(&mut c3)
                        .find(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))
                    else {
                        continue;
                    };
                    let name = self.text(name_node).to_string();
                    let args = attr
                        .child_by_field_name("parameters")
                        .map(|p| self.arguments(p))
                        .unwrap_or_default();
                    out.push(Attribute {
                        fqn: self.resolve(&name),
                        name,
                        args,
                        range: range_of(attr),
                    });
                }
            }
        }
        out
    }

    fn arguments(&self, node: Node) -> Vec<AnnotationArg> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for arg in node.named_children(&mut cursor) {
            if arg.kind() != "argument" {
                continue;
            }
            let name_node = arg.child_by_field_name("name");
            let mut c = arg.walk();
            let value = arg
                .named_children(&mut c)
                .filter(|n| Some(*n) != name_node)
                .last()
                .map(|n| collapse(self.text(n)))
                .unwrap_or_default();
            out.push(AnnotationArg {
                name: name_node.map(|n| self.text(n).to_string()),
                value,
            });
        }
        out
    }

    fn modifiers(&self, node: Node) -> (Option<Visibility>, Modifiers) {
        let mut vis = None;
        let mut m = Modifiers::default();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "visibility_modifier" => {
                    // `private(set)` asymmetric visibility: take the leading keyword.
                    let t = self.text(child);
                    let kw = t.split('(').next().unwrap_or(t);
                    if vis.is_none() {
                        vis = Visibility::parse(kw.trim());
                    }
                }
                "static_modifier" => m.is_static = true,
                "abstract_modifier" => m.is_abstract = true,
                "final_modifier" => m.is_final = true,
                "readonly_modifier" => m.is_readonly = true,
                _ => {}
            }
        }
        (vis, m)
    }

    fn names_in(&self, node: Node) -> Vec<String> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter(|n| matches!(n.kind(), "name" | "qualified_name" | "relative_name"))
            .map(|n| self.resolve(self.text(n)))
            .collect()
    }

    fn class_like(&mut self, node: Node) -> Option<ClassLike> {
        let kind = match node.kind() {
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "trait_declaration" => SymbolKind::Trait,
            _ => SymbolKind::Enum,
        };
        let name_node = node.child_by_field_name("name")?;
        let name = self.text(name_node).to_string();
        let body = node.child_by_field_name("body")?;
        let (_, modifiers) = self.modifiers(node);
        let mut class = ClassLike {
            kind,
            fqn: self.qualify(&name),
            name,
            namespace: self.namespace.clone(),
            range: range_of(node),
            name_range: range_of(name_node),
            body_range: range_of(body),
            modifiers,
            extends: Vec::new(),
            implements: Vec::new(),
            traits: Vec::new(),
            backing_type: None,
            members: Vec::new(),
            doc: self.doc_for(node),
            attributes: self.attributes(node),
        };
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "base_clause" => class.extends = self.names_in(child),
                "class_interface_clause" => class.implements = self.names_in(child),
                "primitive_type" => class.backing_type = Some(self.text(child).to_string()),
                _ => {}
            }
        }
        let mut cursor = body.walk();
        let members: Vec<Node> = body.named_children(&mut cursor).collect();
        for m in members {
            match m.kind() {
                "method_declaration" => self.method(m, kind, &mut class.members),
                "property_declaration" => self.property(m, &mut class.members),
                "const_declaration" => self.class_const(m, &mut class.members),
                "enum_case" => {
                    if let Some(n) = m.child_by_field_name("name") {
                        class.members.push(Member {
                            kind: SymbolKind::EnumCase,
                            name: self.text(n).to_string(),
                            range: range_of(m),
                            name_range: range_of(n),
                            visibility: Visibility::Public,
                            modifiers: Modifiers::default(),
                            type_hint: None,
                            params: Vec::new(),
                            value: m
                                .child_by_field_name("value")
                                .map(|v| self.text(v).to_string()),
                            doc: self.doc_for(m),
                            attributes: self.attributes(m),
                            promoted: false,
                            body_start: None,
                        });
                    }
                }
                "use_declaration" => {
                    class.traits.extend(self.names_in(m));
                }
                _ => {}
            }
        }
        Some(class)
    }

    fn params<'t>(&self, node: Node<'t>) -> Vec<(Param, Node<'t>)> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for p in node.named_children(&mut cursor) {
            let kind = p.kind();
            if !matches!(
                kind,
                "simple_parameter" | "variadic_parameter" | "property_promotion_parameter"
            ) {
                continue;
            }
            let name = p
                .child_by_field_name("name")
                .map(|n| {
                    self.text(n)
                        .trim_start_matches('&')
                        .trim_start_matches('$')
                        .to_string()
                })
                .unwrap_or_default();
            let promoted = (kind == "property_promotion_parameter").then(|| {
                p.child_by_field_name("visibility")
                    .and_then(|v| Visibility::parse(self.text(v).split('(').next().unwrap_or("")))
                    .unwrap_or(Visibility::Public)
            });
            let mut by_ref = p.child_by_field_name("reference_modifier").is_some();
            if let Some(n) = p.child_by_field_name("name") {
                by_ref |= n.kind() == "by_ref";
            }
            out.push((
                Param {
                    name,
                    type_hint: p
                        .child_by_field_name("type")
                        .map(|t| collapse(self.text(t))),
                    default: p
                        .child_by_field_name("default_value")
                        .map(|d| collapse(self.text(d))),
                    variadic: kind == "variadic_parameter",
                    by_ref,
                    promoted,
                    readonly: p.child_by_field_name("readonly").is_some(),
                    attributes: self.attributes(p),
                },
                p,
            ));
        }
        out
    }

    fn method(&self, node: Node, owner: SymbolKind, out: &mut Vec<Member>) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let (vis, mut modifiers) = self.modifiers(node);
        if owner == SymbolKind::Interface {
            modifiers.is_abstract = true;
        }
        let params = node
            .child_by_field_name("parameters")
            .map(|p| self.params(p))
            .unwrap_or_default();
        let doc = self.doc_for(node);
        let name = self.text(name_node).to_string();
        // Promoted constructor parameters are properties too.
        if name.eq_ignore_ascii_case("__construct") {
            for (p, pnode) in params.iter().filter(|(p, _)| p.promoted.is_some()) {
                let doc_param = doc
                    .as_ref()
                    .and_then(|d| d.param(&p.name))
                    .map(|dp| DocBlock {
                        summary: dp.description.clone(),
                        var: Some(dp.clone()),
                        ..Default::default()
                    });
                out.push(Member {
                    kind: SymbolKind::Property,
                    name: p.name.clone(),
                    range: range_of(*pnode),
                    name_range: pnode
                        .child_by_field_name("name")
                        .map(range_of)
                        .unwrap_or_else(|| range_of(*pnode)),
                    visibility: p.promoted.unwrap_or_default(),
                    modifiers: Modifiers {
                        is_readonly: p.readonly || modifiers.is_readonly,
                        ..Default::default()
                    },
                    type_hint: p.type_hint.clone(),
                    params: Vec::new(),
                    value: p.default.clone(),
                    doc: doc_param,
                    attributes: self.attributes(*pnode),
                    promoted: true,
                    body_start: None,
                });
            }
        }
        out.push(Member {
            kind: SymbolKind::Method,
            name,
            range: range_of(node),
            name_range: range_of(name_node),
            visibility: vis.unwrap_or_default(),
            modifiers,
            type_hint: node
                .child_by_field_name("return_type")
                .map(|t| collapse(self.text(t))),
            params: params.into_iter().map(|(p, _)| p).collect(),
            value: None,
            doc,
            attributes: self.attributes(node),
            promoted: false,
            body_start: node.child_by_field_name("body").map(|b| b.start_byte() + 1),
        });
    }

    fn property(&self, node: Node, out: &mut Vec<Member>) {
        let (vis, modifiers) = self.modifiers(node);
        let type_hint = node
            .child_by_field_name("type")
            .map(|t| collapse(self.text(t)));
        let doc = self.doc_for(node);
        let attributes = self.attributes(node);
        let mut cursor = node.walk();
        for el in node.named_children(&mut cursor) {
            if el.kind() != "property_element" {
                continue;
            }
            let Some(name_node) = el.child_by_field_name("name") else {
                continue;
            };
            out.push(Member {
                kind: SymbolKind::Property,
                name: self.text(name_node).trim_start_matches('$').to_string(),
                range: range_of(node),
                name_range: range_of(name_node),
                visibility: vis.unwrap_or_default(),
                modifiers,
                type_hint: type_hint.clone(),
                params: Vec::new(),
                value: el
                    .child_by_field_name("default_value")
                    .map(|d| collapse(self.text(d))),
                doc: doc.clone(),
                attributes: attributes.clone(),
                promoted: false,
                body_start: None,
            });
        }
    }

    fn const_element(&self, el: Node) -> (Option<String>, Option<String>) {
        let mut c = el.walk();
        let mut name = None;
        let mut value = None;
        for n in el.named_children(&mut c) {
            if n.kind() == "name" && name.is_none() {
                name = Some(self.text(n).to_string());
            } else {
                value = Some(collapse(self.text(n)));
            }
        }
        (name, value)
    }

    fn class_const(&self, node: Node, out: &mut Vec<Member>) {
        let (vis, modifiers) = self.modifiers(node);
        let doc = self.doc_for(node);
        let attributes = self.attributes(node);
        let type_hint = node
            .child_by_field_name("type")
            .map(|t| collapse(self.text(t)));
        let mut cursor = node.walk();
        for el in node.named_children(&mut cursor) {
            if el.kind() != "const_element" {
                continue;
            }
            let (Some(name), value) = self.const_element(el) else {
                continue;
            };
            let mut c = el.walk();
            let name_range = el
                .named_children(&mut c)
                .find(|n| n.kind() == "name")
                .map(range_of)
                .unwrap_or_else(|| range_of(el));
            out.push(Member {
                kind: SymbolKind::Constant,
                name,
                range: range_of(node),
                name_range,
                visibility: vis.unwrap_or_default(),
                modifiers,
                type_hint: type_hint.clone(),
                params: Vec::new(),
                value,
                doc: doc.clone(),
                attributes: attributes.clone(),
                promoted: false,
                body_start: None,
            });
        }
    }

    fn function(&self, node: Node) -> Option<FunctionDecl> {
        let name_node = node.child_by_field_name("name")?;
        let name = self.text(name_node).to_string();
        Some(FunctionDecl {
            fqn: self.qualify(&name),
            name,
            namespace: self.namespace.clone(),
            range: range_of(node),
            name_range: range_of(name_node),
            params: node
                .child_by_field_name("parameters")
                .map(|p| self.params(p).into_iter().map(|(p, _)| p).collect())
                .unwrap_or_default(),
            return_type: node
                .child_by_field_name("return_type")
                .map(|t| collapse(self.text(t))),
            doc: self.doc_for(node),
            attributes: self.attributes(node),
        })
    }
}

/// Collapse runs of whitespace (multi-line defaults/attribute args) to single spaces.
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !ws {
                out.push(' ');
            }
            ws = true;
        } else {
            out.push(c);
            ws = false;
        }
    }
    out.replace("[ ", "[")
        .replace(" ]", "]")
        .replace("( ", "(")
        .replace(" )", ")")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<?php

declare(strict_types=1);

namespace App\Entity;

use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Routing\Attribute\Route;
use App\Contracts\{HasTotal, Priceable as P};
use function App\helpers\money;

/**
 * A customer order.
 *
 * @ORM\Entity(repositoryClass="App\Repository\OrderRepository")
 */
#[ORM\Entity]
#[ORM\Table(name: 'orders')]
final class Order extends BaseEntity implements HasTotal, P
{
    use Timestamps;

    public const STATUS_NEW = 'new';
    private const int MAX = 10;

    /** @var list<Line> */
    private array $lines = [];

    #[ORM\Column(type: 'string', length: 3)]
    protected ?string $currency = null;

    public static int $count = 0, $other;

    public function __construct(
        private readonly int $id,
        /** ignored */
        protected string $name = 'x',
        float $plain = 1.0,
    ) {
    }

    /**
     * Order total.
     *
     * @return float The gross total
     */
    #[Route('/orders/{id}', name: 'orders.show', methods: ['GET'])]
    public function total(int $precision = 2, string ...$tags): float
    {
        return 0.0;
    }

    abstract protected static function make(): static;

    final public function &ref(array &$a) {}
}

interface Shape extends \Countable, \JsonSerializable
{
    public function area(): float;
}

trait Timestamps
{
    public ?\DateTimeImmutable $createdAt = null;
}

enum Suit: string implements HasLabel
{
    case Hearts = 'H';
    case Spades = 'S';

    const Wild = self::Spades;

    public function label(): string { return ucfirst($this->name); }
}

function helper(int $a, $b = null): ?string { return null; }

const VERSION = '1.0';
"#;

    fn idx() -> FileIndex {
        index_source("src/Order.php", SRC)
    }

    #[test]
    fn namespaces_and_imports() {
        let i = idx();
        assert_eq!(i.namespaces.len(), 1);
        assert_eq!(i.namespaces[0].name, "App\\Entity");
        let uses: Vec<(String, Option<String>, UseKind)> = i
            .uses
            .iter()
            .map(|u| (u.fqn.clone(), u.alias.clone(), u.kind))
            .collect();
        assert_eq!(
            uses,
            vec![
                (
                    "Doctrine\\ORM\\Mapping".into(),
                    Some("ORM".into()),
                    UseKind::Class
                ),
                (
                    "Symfony\\Component\\Routing\\Attribute\\Route".into(),
                    None,
                    UseKind::Class
                ),
                ("App\\Contracts\\HasTotal".into(), None, UseKind::Class),
                (
                    "App\\Contracts\\Priceable".into(),
                    Some("P".into()),
                    UseKind::Class
                ),
                ("App\\helpers\\money".into(), None, UseKind::Function),
            ]
        );
    }

    #[test]
    fn class_header_doc_and_attributes() {
        let i = idx();
        let c = &i.classes[0];
        assert_eq!(c.kind, SymbolKind::Class);
        assert_eq!(c.fqn, "App\\Entity\\Order");
        assert!(c.modifiers.is_final);
        assert_eq!(c.extends, vec!["App\\Entity\\BaseEntity"]);
        assert_eq!(
            c.implements,
            vec!["App\\Contracts\\HasTotal", "App\\Contracts\\Priceable"]
        );
        assert_eq!(c.traits, vec!["App\\Entity\\Timestamps"]);
        let doc = c.doc.as_ref().unwrap();
        assert_eq!(doc.summary, "A customer order.");
        assert_eq!(doc.annotations[0].name, "ORM\\Entity");
        assert_eq!(c.attributes.len(), 2);
        assert_eq!(c.attributes[0].fqn, "Doctrine\\ORM\\Mapping\\Entity");
        assert_eq!(c.attributes[1].render(), "#[ORM\\Table(name: 'orders')]");
        assert_eq!(&SRC[c.body_range.start..c.body_range.start + 1], "{");
        assert_eq!(&SRC[c.body_range.end - 1..c.body_range.end], "}");
        assert_eq!(
            c.signature(),
            "final class Order extends BaseEntity implements HasTotal, Priceable"
        );
    }

    #[test]
    fn constants_and_properties() {
        let i = idx();
        let c = &i.classes[0];
        let k = c.member(SymbolKind::Constant, "STATUS_NEW").unwrap();
        assert_eq!(k.value.as_deref(), Some("'new'"));
        let max = c.member(SymbolKind::Constant, "MAX").unwrap();
        assert_eq!(max.visibility, Visibility::Private);
        assert_eq!(max.type_hint.as_deref(), Some("int"));

        let lines = c.member(SymbolKind::Property, "lines").unwrap();
        assert_eq!(lines.visibility, Visibility::Private);
        assert_eq!(lines.type_hint.as_deref(), Some("array"));
        assert_eq!(lines.value.as_deref(), Some("[]"));
        assert_eq!(
            lines.doc.as_ref().unwrap().var.as_ref().unwrap().type_name,
            "list<Line>"
        );

        let cur = c.member(SymbolKind::Property, "currency").unwrap();
        assert_eq!(cur.visibility, Visibility::Protected);
        assert_eq!(cur.type_hint.as_deref(), Some("?string"));
        assert_eq!(cur.attributes[0].name, "ORM\\Column");
        assert_eq!(cur.attributes[0].args[1].name.as_deref(), Some("length"));

        let count = c.member(SymbolKind::Property, "count").unwrap();
        assert!(count.modifiers.is_static);
        assert!(
            c.member(SymbolKind::Property, "other")
                .unwrap()
                .modifiers
                .is_static
        );
    }

    #[test]
    fn promoted_constructor_params() {
        let i = idx();
        let c = &i.classes[0];
        let id = c.member(SymbolKind::Property, "id").unwrap();
        assert!(id.promoted);
        assert!(id.modifiers.is_readonly);
        assert_eq!(id.visibility, Visibility::Private);
        assert_eq!(id.type_hint.as_deref(), Some("int"));
        let name = c.member(SymbolKind::Property, "name").unwrap();
        assert_eq!(name.visibility, Visibility::Protected);
        assert_eq!(name.value.as_deref(), Some("'x'"));
        assert!(c.member(SymbolKind::Property, "plain").is_none());
        let ctor = c.method("__construct").unwrap();
        assert_eq!(ctor.params.len(), 3);
        assert_eq!(ctor.params[0].promoted, Some(Visibility::Private));
        assert_eq!(ctor.params[2].promoted, None);
    }

    #[test]
    fn methods() {
        let i = idx();
        let c = &i.classes[0];
        let total = c.method("total").unwrap();
        assert_eq!(total.type_hint.as_deref(), Some("float"));
        assert_eq!(total.params.len(), 2);
        assert_eq!(total.params[0].default.as_deref(), Some("2"));
        assert!(total.params[1].variadic);
        assert_eq!(
            total.signature(),
            "public function total(int $precision = 2, string ...$tags): float"
        );
        assert_eq!(total.doc.as_ref().unwrap().summary, "Order total.");
        let route = &total.attributes[0];
        assert_eq!(route.fqn, "Symfony\\Component\\Routing\\Attribute\\Route");
        assert_eq!(route.args[0].unquoted(), "/orders/{id}");
        assert_eq!(route.args[2].value, "['GET']");
        assert!(total.body_start.is_some());

        let make = c.method("make").unwrap();
        assert!(make.modifiers.is_abstract && make.modifiers.is_static);
        assert_eq!(make.visibility, Visibility::Protected);
        assert_eq!(make.type_hint.as_deref(), Some("static"));
        assert!(make.body_start.is_none());

        let r = c.method("ref").unwrap();
        assert!(r.modifiers.is_final);
        assert!(r.params[0].by_ref);
    }

    #[test]
    fn interfaces_traits_enums_functions() {
        let i = idx();
        let shape = &i.classes[1];
        assert_eq!(shape.kind, SymbolKind::Interface);
        assert_eq!(shape.extends, vec!["Countable", "JsonSerializable"]);
        assert!(shape.method("area").unwrap().modifiers.is_abstract);

        let tr = &i.classes[2];
        assert_eq!(tr.kind, SymbolKind::Trait);
        assert_eq!(
            tr.member(SymbolKind::Property, "createdAt")
                .unwrap()
                .type_hint
                .as_deref(),
            Some("?\\DateTimeImmutable")
        );

        let e = &i.classes[3];
        assert_eq!(e.kind, SymbolKind::Enum);
        assert_eq!(e.backing_type.as_deref(), Some("string"));
        assert_eq!(e.implements, vec!["App\\Entity\\HasLabel"]);
        let cases: Vec<&str> = e
            .members
            .iter()
            .filter(|m| m.kind == SymbolKind::EnumCase)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(cases, vec!["Hearts", "Spades"]);
        assert_eq!(
            e.member(SymbolKind::EnumCase, "Hearts")
                .unwrap()
                .value
                .as_deref(),
            Some("'H'")
        );
        assert!(e.member(SymbolKind::Constant, "Wild").is_some());
        assert!(e.method("label").is_some());

        assert_eq!(i.functions.len(), 1);
        let f = &i.functions[0];
        assert_eq!(f.fqn, "App\\Entity\\helper");
        assert_eq!(f.signature(), "function helper(int $a, $b = null): ?string");
        assert_eq!(i.constants[0].fqn, "App\\Entity\\VERSION");
    }

    #[test]
    fn braced_namespaces_and_lookup_helpers() {
        let src =
            "<?php\nnamespace A { class X {} }\nnamespace B { use A\\X; class Y extends X {} }\n";
        let i = index_source("x.php", src);
        assert_eq!(i.classes[0].fqn, "A\\X");
        assert_eq!(i.classes[1].fqn, "B\\Y");
        assert_eq!(i.classes[1].extends, vec!["A\\X"]);
        let off = src.find("class Y").unwrap() + 8;
        assert_eq!(i.class_at(off).unwrap().name, "Y");
        assert_eq!(i.namespace_at(off), "B");
    }

    #[test]
    fn tolerates_broken_code() {
        let i = index_source("x.php", "<?php\nclass A {\n public function f( {\n");
        assert!(i.classes.len() <= 1);
    }
}

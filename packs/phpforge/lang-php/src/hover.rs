//! Index-based hover for project symbols: signature + attributes + parsed docblock as
//! markdown. Complements a language server's hover (which covers built-ins).

use serde::{Deserialize, Serialize};

use crate::docblock::{DocBlock, PropertyAccess};
use crate::index::ProjectIndex;
use crate::model::{Attribute, ClassLike, FileIndex, FunctionDecl, Member, SymbolKind, UseKind};
use crate::parser;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub markdown: String,
    /// Byte range of the hovered word.
    pub start: usize,
    pub end: usize,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'
}

/// Word (identifier, qualified name or variable) around `offset`.
pub(crate) fn word_at(src: &str, offset: usize) -> Option<(usize, usize)> {
    let offset = offset.min(src.len());
    if !src.is_char_boundary(offset) {
        return None;
    }
    let start = src[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(offset);
    let end = src[offset..]
        .char_indices()
        .find(|(_, c)| !is_ident(*c))
        .map(|(i, _)| offset + i)
        .unwrap_or(src.len());
    (start < end).then_some((start, end))
}

/// Markdown body for a docblock (without signature).
pub fn render_doc(doc: &DocBlock) -> String {
    let mut out = Vec::<String>::new();
    if let Some(d) = &doc.deprecated {
        out.push(if d.is_empty() {
            "**Deprecated.**".to_string()
        } else {
            format!("**Deprecated:** {d}")
        });
    }
    if !doc.summary.is_empty() {
        out.push(doc.summary.clone());
    }
    if !doc.description.is_empty() {
        out.push(doc.description.clone());
    }
    let mut sections: Vec<String> = Vec::new();
    if !doc.templates.is_empty() {
        let items: Vec<String> = doc
            .templates
            .iter()
            .map(|t| match &t.bound {
                Some(b) => format!("`{}` of `{b}`", t.name),
                None => format!("`{}`", t.name),
            })
            .collect();
        sections.push(format!("**Template:** {}", items.join(", ")));
    }
    if !doc.params.is_empty() {
        let mut s = String::from("**Params:**");
        for p in &doc.params {
            s.push_str(&format!("\n- `{}` `${}`", p.type_name, p.name).replace("`` ", ""));
            if !p.description.is_empty() {
                s.push_str(&format!(" — {}", p.description.replace('\n', " ")));
            }
        }
        sections.push(s);
    }
    if let Some(v) = &doc.var {
        let mut s = format!("**Type:** `{}`", v.type_name);
        if !v.description.is_empty() {
            s.push_str(&format!(" — {}", v.description.replace('\n', " ")));
        }
        sections.push(s);
    }
    if let Some(r) = &doc.returns {
        let mut s = format!("**Returns:** `{}`", r.type_name);
        if !r.description.is_empty() {
            s.push_str(&format!(" — {}", r.description.replace('\n', " ")));
        }
        sections.push(s);
    }
    if !doc.throws.is_empty() {
        let items: Vec<String> = doc
            .throws
            .iter()
            .map(|t| {
                if t.description.is_empty() {
                    format!("`{}`", t.type_name)
                } else {
                    format!("`{}` — {}", t.type_name, t.description)
                }
            })
            .collect();
        sections.push(format!("**Throws:** {}", items.join(", ")));
    }
    if !doc.properties.is_empty() {
        let mut s = String::from("**Properties:**");
        for p in &doc.properties {
            let access = match p.access {
                PropertyAccess::Read => " *(read-only)*",
                PropertyAccess::Write => " *(write-only)*",
                PropertyAccess::ReadWrite => "",
            };
            s.push_str(&format!("\n- `{} ${}`{access}", p.type_name, p.name));
        }
        sections.push(s);
    }
    if !doc.methods.is_empty() {
        let mut s = String::from("**Methods:**");
        for m in &doc.methods {
            let st = if m.is_static { "static " } else { "" };
            let ret = m
                .return_type
                .as_ref()
                .map(|r| format!(": {r}"))
                .unwrap_or_default();
            s.push_str(&format!("\n- `{st}{}({}){ret}`", m.name, m.params));
        }
        sections.push(s);
    }
    if !doc.annotations.is_empty() {
        let items: Vec<String> = doc
            .annotations
            .iter()
            .map(|a| format!("- `{}`", a.render()))
            .collect();
        sections.push(format!("**Annotations:**\n{}", items.join("\n")));
    }
    if !doc.see.is_empty() {
        sections.push(format!("**See:** {}", doc.see.join(", ")));
    }
    out.extend(sections);
    out.join("\n\n")
}

fn code_block(attributes: &[Attribute], signature: &str) -> String {
    let mut s = String::from("```php\n");
    for a in attributes {
        s.push_str(&a.render());
        s.push('\n');
    }
    s.push_str(signature);
    s.push_str("\n```");
    s
}

fn join_parts(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn class_markdown(c: &ClassLike) -> String {
    let location = if c.namespace.is_empty() {
        String::new()
    } else {
        format!("`{}`", c.namespace)
    };
    join_parts(vec![
        location,
        code_block(&c.attributes, &c.signature()),
        c.doc.as_ref().map(render_doc).unwrap_or_default(),
    ])
}

pub fn member_markdown(owner: &ClassLike, m: &Member) -> String {
    let doc = m.doc.as_ref().map(render_doc).unwrap_or_default();
    join_parts(vec![
        format!("`{}`", owner.fqn),
        code_block(&m.attributes, &m.signature()),
        doc,
    ])
}

pub fn function_markdown(f: &FunctionDecl) -> String {
    let location = if f.namespace.is_empty() {
        String::new()
    } else {
        format!("`{}`", f.namespace)
    };
    join_parts(vec![
        location,
        code_block(&f.attributes, &f.signature()),
        f.doc.as_ref().map(render_doc).unwrap_or_default(),
    ])
}

/// Find a member on `class` or its ancestors.
pub(crate) fn find_member(
    index: &mut ProjectIndex,
    class: &ClassLike,
    name: &str,
    kinds: &[SymbolKind],
) -> Option<(ClassLike, Member)> {
    let find = |c: &ClassLike| {
        c.members
            .iter()
            .find(|m| {
                kinds.contains(&m.kind)
                    && if m.kind == SymbolKind::Method {
                        m.name.eq_ignore_ascii_case(name)
                    } else {
                        m.name == name
                    }
            })
            .cloned()
    };
    if let Some(m) = find(class) {
        return Some((class.clone(), m));
    }
    for anc in index.ancestors(class) {
        if let Some(m) = find(&anc) {
            return Some((anc, m));
        }
    }
    None
}

/// Hover at `offset` (bytes) in `source`, the current content of `rel`.
pub fn hover(index: &mut ProjectIndex, rel: &str, source: &str, offset: usize) -> Option<Hover> {
    let file = parser::index_source(rel, source);
    let (start, end) = word_at(source, offset)?;
    let word = &source[start..end];
    let md = hover_markdown(index, &file, source, start, end, word)?;
    Some(Hover {
        markdown: md,
        start,
        end,
    })
}

fn hover_markdown(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    start: usize,
    end: usize,
    word: &str,
) -> Option<String> {
    // 1. Declarations in this file.
    for c in &file.classes {
        if c.name_range.start <= start && end <= c.name_range.end {
            return Some(class_markdown(c));
        }
        for m in &c.members {
            if m.name_range.start <= start && end <= m.name_range.end {
                return Some(member_markdown(c, m));
            }
        }
    }
    for f in &file.functions {
        if f.name_range.start <= start && end <= f.name_range.end {
            return Some(function_markdown(f));
        }
    }

    let before = src[..start].trim_end();
    let after = src[end..].trim_start();
    let is_call = after.starts_with('(');
    let enclosing = file.class_at(start).cloned();

    // 2. Member access: `$this->x`, `self::X`, `Foo::bar()`.
    let access = if let Some(b) = before.strip_suffix("?->") {
        Some((b, "->"))
    } else if let Some(b) = before.strip_suffix("->") {
        Some((b, "->"))
    } else {
        before.strip_suffix("::").map(|b| (b, "::"))
    };
    if let Some((recv_src, op)) = access {
        let recv_start = recv_src
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_ident(*c))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(recv_src.len());
        let recv = &recv_src[recv_start..];
        let class = match recv.to_ascii_lowercase().as_str() {
            "$this" | "self" | "static" => enclosing.clone(),
            "parent" => enclosing
                .as_ref()
                .and_then(|c| c.extends.first().cloned())
                .and_then(|p| index.resolve_class(&p)),
            r if !r.starts_with('$') && !r.is_empty() => {
                let fqn = file.resolve_class(recv, start);
                index.resolve_class(&fqn)
            }
            // Variables and call chains, also through `.phpstorm.meta.php`.
            _ => crate::navigate::receiver_types(index, file, src, recv_src.len(), 0)
                .into_iter()
                .find_map(|fqn| crate::navigate::find_class(index, file, &fqn)),
        }?;
        let name = word.trim_start_matches('$');
        let kinds: &[SymbolKind] = match (op, is_call) {
            (_, true) => &[SymbolKind::Method],
            ("->", false) => &[SymbolKind::Property],
            _ if word.starts_with('$') => &[SymbolKind::Property],
            _ if word == "class" => return Some(class_markdown(&class)),
            _ => &[SymbolKind::Constant, SymbolKind::EnumCase],
        };
        let (owner, m) = find_member(index, &class, name, kinds)?;
        return Some(member_markdown(&owner, &m));
    }
    if word.starts_with('$') {
        return None;
    }

    // 3. Function call.
    let prev_word = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    if is_call
        && !prev_word.eq_ignore_ascii_case("new")
        && !prev_word.eq_ignore_ascii_case("function")
    {
        let ns = file.namespace_at(start);
        let bare = word.trim_start_matches('\\');
        let imported = file
            .uses
            .iter()
            .find(|u| u.kind == UseKind::Function && u.local_name().eq_ignore_ascii_case(bare))
            .map(|u| u.fqn.clone())
            .unwrap_or_default();
        let candidates = [
            imported,
            if ns.is_empty() || word.starts_with('\\') {
                bare.to_string()
            } else {
                format!("{ns}\\{bare}")
            },
            bare.to_string(),
        ];
        for cand in candidates {
            if let Some(f) = index
                .files()
                .flat_map(|f| f.functions.iter())
                .find(|f| f.fqn.eq_ignore_ascii_case(&cand))
            {
                return Some(function_markdown(f));
            }
        }
        return None;
    }

    // 4. Class reference (type hints, `new X`, `X::class`, attributes, extends, ...).
    let fqn = file.resolve_class(word, start);
    if let Some(c) = index.resolve_class(&fqn) {
        return Some(class_markdown(&c));
    }
    // Same-file class not yet indexed (unsaved buffer).
    file.classes
        .iter()
        .find(|c| c.fqn.eq_ignore_ascii_case(&fqn))
        .map(class_markdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexConfig;
    use std::fs;

    fn setup() -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        fs::create_dir_all(r.join("app")).unwrap();
        fs::write(
            r.join("app/Order.php"),
            r#"<?php
namespace App\Models;

/**
 * A customer order.
 *
 * @property-read int $id
 * @ORM\Entity(repositoryClass="OrderRepository")
 */
#[ORM\Table(name: 'orders')]
class Order extends Model
{
    public const PAID = 'paid';

    /** @var float Gross total */
    public $total;

    /**
     * Mark as paid.
     *
     * @param string $ref Payment reference
     * @return static
     * @throws \LogicException When already paid
     * @deprecated Use pay()
     */
    public function markPaid(string $ref): static {}
}

/** Base model. */
abstract class Model
{
    public function save(): bool {}
}

/** Formats money. */
function money(float $v): string {}
"#,
        )
        .unwrap();
        let mut idx = ProjectIndex::new(r, IndexConfig::default());
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    const USER: &str = r#"<?php
namespace App\Http;

use App\Models\Order;
use Symfony\Component\Routing\Attribute\Route;
use function App\Models\money;

class OrderController
{
    #[Route('/orders/{id}', name: 'orders.show', methods: ['GET'])]
    public function show(Order $order): string
    {
        $order->markPaid('x');
        $order->save();
        echo Order::PAID . $order->total;
        return money(1.0);
    }
}
"#;

    fn hover_on(idx: &mut ProjectIndex, needle: &str, delta: usize) -> Option<Hover> {
        let off = USER.find(needle).unwrap() + delta;
        hover(idx, "app/Http/OrderController.php", USER, off)
    }

    #[test]
    fn hovers_class_references_with_doc_and_attributes() {
        let (_d, mut idx) = setup();
        let h = hover_on(&mut idx, "Order $order", 2).unwrap();
        assert!(h.markdown.contains("`App\\Models`"));
        assert!(h
            .markdown
            .contains("```php\n#[ORM\\Table(name: 'orders')]\nclass Order extends Model\n```"));
        assert!(h.markdown.contains("A customer order."));
        assert!(h.markdown.contains("`int $id` *(read-only)*"));
        assert!(h
            .markdown
            .contains("`@ORM\\Entity(repositoryClass=\"OrderRepository\")`"));
        assert_eq!(&USER[h.start..h.end], "Order");
    }

    #[test]
    fn hovers_own_method_declaration_with_route_attribute() {
        let (_d, mut idx) = setup();
        let h = hover_on(&mut idx, "show(", 1).unwrap();
        assert!(h.markdown.contains(
            "#[Route('/orders/{id}', name: 'orders.show', methods: ['GET'])]\npublic function show(Order $order): string"
        ));
        assert!(h.markdown.starts_with("`App\\Http\\OrderController`"));
    }

    #[test]
    fn hovers_members_through_variables_and_static_access() {
        let (_d, mut idx) = setup();
        // `$order->markPaid` — variable receivers are unknown to the index.
        assert!(hover_on(&mut idx, "markPaid", 2).is_none());
        let h = hover_on(&mut idx, "PAID .", 1).unwrap();
        assert!(h.markdown.contains("public const PAID = 'paid'"));
    }

    #[test]
    fn hovers_functions() {
        let (_d, mut idx) = setup();
        let h = hover_on(&mut idx, "money(1.0)", 1).unwrap();
        assert!(h.markdown.contains("function money(float $v): string"));
        assert!(h.markdown.contains("Formats money."));
    }

    #[test]
    fn hovers_members_via_this_and_parents() {
        let (_d, mut idx) = setup();
        let src = "<?php\nnamespace App\\Models;\nclass Special extends Order {\n function f() { $this->markPaid('a'); $this->save(); return $this->total; }\n}\n";
        let off = src.find("markPaid").unwrap() + 1;
        let h = hover(&mut idx, "x.php", src, off).unwrap();
        assert!(h.markdown.contains("`App\\Models\\Order`"));
        assert!(h.markdown.contains("**Deprecated:** Use pay()"));
        assert!(h.markdown.contains("- `string` `$ref` — Payment reference"));
        assert!(h.markdown.contains("**Returns:** `static`"));
        assert!(h
            .markdown
            .contains("**Throws:** `\\LogicException` — When already paid"));

        let off = src.find("save").unwrap();
        let h = hover(&mut idx, "x.php", src, off).unwrap();
        assert!(h.markdown.contains("`App\\Models\\Model`"));

        let off = src.find("total;").unwrap();
        let h = hover(&mut idx, "x.php", src, off).unwrap();
        assert!(h.markdown.contains("public $total"));
        assert!(h.markdown.contains("Gross total"));
        assert!(h.markdown.contains("**Type:** `float`"));
    }

    #[test]
    fn nothing_on_whitespace_or_unknown() {
        let (_d, mut idx) = setup();
        assert!(hover(&mut idx, "x.php", "<?php\n\n  \n", 7).is_none());
        assert!(hover(&mut idx, "x.php", "<?php\nUnknownThing::x();", 8).is_none());
    }
}

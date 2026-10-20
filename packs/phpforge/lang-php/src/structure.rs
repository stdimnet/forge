//! File structure for the Structure tool window (Alt+7).

use serde::{Deserialize, Serialize};

use crate::model::{short_name, ClassLike, FileIndex, Member, Modifiers, SymbolKind, Visibility};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureNode {
    pub name: String,
    pub kind: SymbolKind,
    /// Text shown after the name: `(int $a): string` for methods, `: string` for fields.
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(default)]
    pub modifiers: Modifiers,
    /// Byte offset of the name (converted to UTF-16 at the IPC boundary).
    pub offset: usize,
    pub line: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    /// Short name of the declaring class for inherited members.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherited_from: Option<String>,
    /// File of an inherited member's declaring class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<StructureNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStructure {
    pub path: String,
    pub nodes: Vec<StructureNode>,
}

/// Detail text for a member, in PHPStorm's Structure view style.
pub fn member_detail(m: &Member) -> String {
    match m.kind {
        SymbolKind::Method => {
            let params: Vec<String> = m.params.iter().map(|p| p.render()).collect();
            let ret = m
                .effective_type()
                .map(|t| format!(": {t}"))
                .unwrap_or_default();
            format!("({}){ret}", params.join(", "))
        }
        SymbolKind::Property => m
            .effective_type()
            .map(|t| format!(": {t}"))
            .unwrap_or_default(),
        SymbolKind::Constant | SymbolKind::EnumCase => m
            .value
            .as_ref()
            .map(|v| format!(" = {v}"))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn member_node(m: &Member, inherited: Option<(&str, &str)>) -> StructureNode {
    StructureNode {
        name: m.name.clone(),
        kind: m.kind,
        detail: member_detail(m),
        visibility: (m.kind != SymbolKind::EnumCase).then_some(m.visibility),
        modifiers: m.modifiers,
        offset: m.name_range.start,
        line: m.name_range.line,
        deprecated: m.doc.as_ref().is_some_and(|d| d.is_deprecated()),
        inherited_from: inherited.map(|(owner, _)| short_name(owner).to_string()),
        path: inherited.map(|(_, path)| path.to_string()),
        children: Vec::new(),
    }
}

fn class_node(c: &ClassLike) -> StructureNode {
    let mut detail = String::new();
    if let Some(p) = c.extends.first().filter(|_| c.kind == SymbolKind::Class) {
        detail = format!(" extends {}", short_name(p));
    }
    StructureNode {
        name: c.name.clone(),
        kind: c.kind,
        detail,
        visibility: None,
        modifiers: c.modifiers,
        offset: c.name_range.start,
        line: c.name_range.line,
        deprecated: c.doc.as_ref().is_some_and(|d| d.is_deprecated()),
        inherited_from: None,
        path: None,
        // Source order, like PHPStorm without "Sort alphabetically".
        children: c.members.iter().map(|m| member_node(m, None)).collect(),
    }
}

/// Build the structure of a file. `inherited(class)` returns (owner FQN, owner path, member)
/// triples to append to a class' children, flagged as inherited.
pub fn build(
    file: &FileIndex,
    mut inherited: impl FnMut(&ClassLike) -> Vec<(String, String, Member)>,
) -> FileStructure {
    let mut nodes: Vec<StructureNode> = Vec::new();
    for c in &file.classes {
        let mut node = class_node(c);
        for (owner, path, m) in inherited(c) {
            node.children
                .push(member_node(&m, Some((owner.as_str(), path.as_str()))));
        }
        nodes.push(node);
    }
    for f in &file.functions {
        let params: Vec<String> = f.params.iter().map(|p| p.render()).collect();
        nodes.push(StructureNode {
            name: f.name.clone(),
            kind: SymbolKind::Function,
            detail: format!(
                "({}){}",
                params.join(", "),
                f.return_type
                    .as_ref()
                    .map(|t| format!(": {t}"))
                    .unwrap_or_default()
            ),
            visibility: None,
            modifiers: Modifiers::default(),
            offset: f.name_range.start,
            line: f.name_range.line,
            deprecated: f.doc.as_ref().is_some_and(|d| d.is_deprecated()),
            inherited_from: None,
            path: None,
            children: Vec::new(),
        });
    }
    for k in &file.constants {
        nodes.push(StructureNode {
            name: k.name.clone(),
            kind: SymbolKind::Constant,
            detail: k
                .value
                .as_ref()
                .map(|v| format!(" = {v}"))
                .unwrap_or_default(),
            visibility: None,
            modifiers: Modifiers::default(),
            offset: k.range.start,
            line: k.range.line,
            deprecated: false,
            inherited_from: None,
            path: None,
            children: Vec::new(),
        });
    }
    nodes.sort_by_key(|n| n.offset);
    FileStructure {
        path: file.path.clone(),
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::index_source;

    #[test]
    fn builds_tree_with_details() {
        let src = "<?php\nnamespace App;\nclass Checkout extends Base {\n  public const RATE = 0.19;\n  private string $currency;\n  /** @var float */\n  protected $taxRate;\n  public function __construct(private readonly Cart $cart) {}\n  /** @deprecated */\n  public static function total(int $q = 1): float {}\n}\nfunction helper(): void {}\n";
        let file = index_source("a.php", src);
        let s = build(&file, |_| Vec::new());
        assert_eq!(s.nodes.len(), 2);
        let c = &s.nodes[0];
        assert_eq!(c.name, "Checkout");
        assert_eq!(c.detail, " extends Base");
        let names: Vec<(&str, &str)> = c
            .children
            .iter()
            .map(|n| (n.name.as_str(), n.detail.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("RATE", " = 0.19"),
                ("currency", ": string"),
                ("taxRate", ": float"),
                ("cart", ": Cart"),
                ("__construct", "(Cart $cart)"),
                ("total", "(int $q = 1): float"),
            ]
        );
        let total = &c.children[5];
        assert!(total.deprecated);
        assert!(total.modifiers.is_static);
        assert_eq!(total.visibility, Some(Visibility::Public));
        assert_eq!(&src[total.offset..total.offset + 5], "total");
        assert_eq!(s.nodes[1].name, "helper");
        assert_eq!(s.nodes[1].detail, "(): void");
    }

    #[test]
    fn appends_inherited_members() {
        let file = index_source("a.php", "<?php\nclass A extends B { function own() {} }\n");
        let parent = index_source("b.php", "<?php\nclass B { public function save() {} }\n");
        let m = parent.classes[0].members[0].clone();
        let s = build(&file, |_| vec![("B".into(), "b.php".into(), m.clone())]);
        let kids = &s.nodes[0].children;
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[1].inherited_from.as_deref(), Some("B"));
        assert_eq!(kids[1].path.as_deref(), Some("b.php"));
    }
}

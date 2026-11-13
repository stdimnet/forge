//! Declarations found with tree-sitter grammars, so a `class` inside a comment or a string is not
//! a class and methods of JavaScript and TypeScript classes are found. Other languages, and files
//! a grammar cannot make sense of, keep the line-pattern extractor of `forge-search-core`.

use std::cell::RefCell;
use std::collections::HashMap;

use forge_search_core::{RawSymbol, SymbolKind as Kind};
use tree_sitter::{Language, Node, Parser};

const MAX_BYTES: usize = 256 * 1024;
const MAX_SYMBOLS: usize = 5000;
const MAX_DEPTH: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Lang {
    Php,
    JavaScript,
    TypeScript,
    Tsx,
    Lua,
}

fn lang_of(path: &str) -> Option<Lang> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "php" | "phtml" | "inc" => Lang::Php,
        "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
        "ts" | "mts" | "cts" => Lang::TypeScript,
        "tsx" => Lang::Tsx,
        "lua" => Lang::Lua,
        _ => return None,
    })
}

fn language(lang: Lang) -> Language {
    match lang {
        Lang::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Lua => tree_sitter_lua::LANGUAGE.into(),
    }
}

thread_local! {
    static PARSERS: RefCell<HashMap<Lang, Parser>> = RefCell::new(HashMap::new());
}

/// The declarations of `bytes`, or `None` when no grammar covers the file or it cannot be parsed.
pub(crate) fn symbols(path: &str, bytes: &[u8]) -> Option<Vec<RawSymbol>> {
    let lang = lang_of(path)?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let tree = PARSERS.with(|parsers| {
        let mut parsers = parsers.borrow_mut();
        let parser = parsers.entry(lang).or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&language(lang))
                .expect("bundled grammar");
            parser
        });
        parser.parse(bytes, None)
    })?;
    let mut out = Vec::new();
    visit(tree.root_node(), bytes, lang, None, 0, &mut out);
    Some(out)
}

fn text<'a>(node: Node, src: &'a [u8]) -> Option<&'a str> {
    node.utf8_text(src)
        .ok()
        .filter(|t| !t.is_empty() && t.len() <= 120)
}

fn name_of<'a>(node: Node, src: &'a [u8]) -> Option<&'a str> {
    text(node.child_by_field_name("name")?, src)
}

fn first_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    found
}

fn is_type(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Class
            | Kind::Interface
            | Kind::Trait
            | Kind::Enum
            | Kind::Struct
            | Kind::Module
            | Kind::TypeAlias
    )
}

/// The names a node declares, with their kinds.
fn declaration<'a>(
    node: Node<'a>,
    src: &'a [u8],
    lang: Lang,
    container: Option<usize>,
) -> Vec<(&'a str, Kind)> {
    let one = |kind: Kind| {
        name_of(node, src)
            .map(|n| vec![(n, kind)])
            .unwrap_or_default()
    };
    match lang {
        Lang::Php => match node.kind() {
            "class_declaration" => one(Kind::Class),
            "interface_declaration" => one(Kind::Interface),
            "trait_declaration" => one(Kind::Trait),
            "enum_declaration" => one(Kind::Enum),
            "function_definition" => one(Kind::Function),
            "method_declaration" => one(Kind::Method),
            "enum_case" => one(Kind::Constant),
            "const_declaration" => {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .filter(|c| c.kind() == "const_element")
                    .filter_map(|c| first_child_of_kind(c, "name").and_then(|n| text(n, src)))
                    .map(|n| (n, Kind::Constant))
                    .collect()
            }
            "property_declaration" => {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .filter(|c| c.kind() == "property_element")
                    .filter_map(|c| {
                        first_child_of_kind(c, "variable_name").and_then(|n| text(n, src))
                    })
                    .map(|n| (n, Kind::Property))
                    .collect()
            }
            _ => Vec::new(),
        },
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => match node.kind() {
            "class_declaration" | "abstract_class_declaration" => one(Kind::Class),
            "interface_declaration" => one(Kind::Interface),
            "enum_declaration" => one(Kind::Enum),
            "type_alias_declaration" => one(Kind::TypeAlias),
            "internal_module" | "module" => one(Kind::Module),
            "function_declaration" | "generator_function_declaration" => one(Kind::Function),
            "method_definition" => one(Kind::Method),
            "variable_declarator" => {
                let function_value = node.child_by_field_name("value").is_some_and(|v| {
                    matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function"
                    )
                });
                if function_value && container.is_none() {
                    one(Kind::Function)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        },
        Lang::Lua => match node.kind() {
            "function_declaration" => one(Kind::Function),
            _ => Vec::new(),
        },
    }
}

fn visit(
    node: Node,
    src: &[u8],
    lang: Lang,
    container: Option<usize>,
    depth: usize,
    out: &mut Vec<RawSymbol>,
) {
    if depth > MAX_DEPTH || out.len() >= MAX_SYMBOLS {
        return;
    }
    let mut inner = container;
    for (name, kind) in declaration(node, src, lang, container) {
        out.push(RawSymbol {
            name: name.to_string(),
            kind,
            line: node.start_position().row as u32 + 1,
            container,
        });
        if is_type(kind) {
            inner = Some(out.len() - 1);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, src, lang, inner, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(path: &str, src: &str) -> Vec<(String, Kind, Option<usize>)> {
        symbols(path, src.as_bytes())
            .unwrap()
            .into_iter()
            .map(|s| (s.name, s.kind, s.container))
            .collect()
    }

    #[test]
    fn php_declarations_ignore_comments_and_strings() {
        let src = "<?php\n/*\nclass Fake {}\n*/\n$s = \"function nope() {}\";\nclass Real {\n    const MAX = 1;\n    public int $count = 0;\n    public function go() {}\n}\nfunction helper() {}\n";
        let found = names("a.php", src);
        assert!(found.iter().all(|(n, ..)| n != "Fake" && n != "nope"));
        assert!(found.contains(&("Real".into(), Kind::Class, None)));
        assert!(found.contains(&("MAX".into(), Kind::Constant, Some(0))));
        assert!(found.contains(&("$count".into(), Kind::Property, Some(0))));
        assert!(found.contains(&("go".into(), Kind::Method, Some(0))));
        assert!(found.contains(&("helper".into(), Kind::Function, None)));
    }

    #[test]
    fn typescript_finds_class_methods_types_and_arrow_functions() {
        let src = "export class Cart {\n  add(item: Item) {}\n}\ninterface Item { id: number }\ntype Id = number;\nconst total = (a: number) => a;\n// class Ghost {}\n";
        let found = names("cart.ts", src);
        assert!(found.contains(&("Cart".into(), Kind::Class, None)));
        assert!(found.contains(&("add".into(), Kind::Method, Some(0))));
        assert!(found.contains(&("Item".into(), Kind::Interface, None)));
        assert!(found.contains(&("Id".into(), Kind::TypeAlias, None)));
        assert!(found.contains(&("total".into(), Kind::Function, None)));
        assert!(found.iter().all(|(n, ..)| n != "Ghost"));
    }

    #[test]
    fn javascript_and_lua() {
        let found = names("a.js", "class A { run() {} }\nfunction f() {}\n");
        assert!(found.contains(&("run".into(), Kind::Method, Some(0))));
        assert!(found.contains(&("f".into(), Kind::Function, None)));
        let lua = names(
            "a.lua",
            "-- function ghost() end\nlocal function make() end\nfunction M.go() end\n",
        );
        assert!(lua.iter().any(|(n, ..)| n == "make"));
        assert!(lua.iter().any(|(n, ..)| n == "M.go"));
        assert!(lua.iter().all(|(n, ..)| n != "ghost"));
    }

    #[test]
    fn unknown_languages_are_left_to_the_line_extractor() {
        assert!(symbols("a.rs", b"fn main() {}").is_none());
        assert!(symbols("big.php", &vec![b' '; MAX_BYTES + 1]).is_none());
    }
}

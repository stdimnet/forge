//! Refactor This (Ctrl+Alt+Shift+T) for PHP: the refactorings that apply to the selection or
//! to the element at the caret, as PHPStorm action ids in PHPStorm's order. A selected
//! expression or statements offer the Extract / Introduce refactorings; a class offers
//! Rename, Move, Copy, Safe Delete, Extract Interface / Superclass and Pull Members Up / Push
//! Members Down; a method Rename, Change Signature, Make Static, Inline, Move (static),
//! Safe Delete and Pull Up / Push Down; an expression at the caret the Introduce refactorings.

use super::hierarchy::class_members;
use super::introduce::{self, IntroduceKind};
use super::{extract_method, inline, make_static, parse, safe_delete, signature, text};

/// Refactor This's order.
pub const ORDER: &[&str] = &[
    "RenameElement",
    "ChangeSignature",
    "Move",
    "CopyElement",
    "SafeDelete",
    "IntroduceVariable",
    "IntroduceConstant",
    "IntroduceField",
    "IntroduceParameter",
    "ExtractMethod",
    "ExtractInterface",
    "ExtractSuperclass",
    "Inline",
    "MembersPullUp",
    "MemberPushDown",
    "MakeStatic",
];

const INTRODUCE: [(IntroduceKind, &str); 4] = [
    (IntroduceKind::Variable, "IntroduceVariable"),
    (IntroduceKind::Constant, "IntroduceConstant"),
    (IntroduceKind::Field, "IntroduceField"),
    (IntroduceKind::Parameter, "IntroduceParameter"),
];

fn ordered(found: Vec<&'static str>) -> Vec<String> {
    ORDER
        .iter()
        .filter(|id| found.contains(id))
        .map(|id| id.to_string())
        .collect()
}

/// The Introduce refactorings for the expression(s) at `start..end`.
fn introductions(src: &str, start: usize, end: usize, out: &mut Vec<&'static str>) {
    for (kind, id) in INTRODUCE {
        let ok = introduce::prepare(src, start, end, kind)
            .is_ok_and(|p| p.info.is_some() || !p.expressions.is_empty());
        if ok {
            out.push(id);
        }
    }
}

/// Class-level refactorings of the class around `pos`.
fn class_level(src: &str, pos: usize, out: &mut Vec<&'static str>) {
    let Some(info) = class_members(src, pos) else {
        return;
    };
    match info.kind.as_str() {
        "class" => {
            out.extend(["ExtractInterface", "ExtractSuperclass", "MemberPushDown"]);
            if info.parent.is_some() || !info.interfaces.is_empty() {
                out.push("MembersPullUp");
            }
        }
        "enum" => out.push("ExtractInterface"),
        "interface" => {
            out.push("MemberPushDown");
            if !info.interfaces.is_empty() {
                out.push("MembersPullUp");
            }
        }
        _ => {}
    }
}

/// Pull Members Up / Push Members Down for a member of the class around `pos`.
fn member_moves(src: &str, pos: usize, out: &mut Vec<&'static str>) {
    let mut class = Vec::new();
    class_level(src, pos, &mut class);
    out.extend(
        class
            .into_iter()
            .filter(|id| matches!(*id, "MembersPullUp" | "MemberPushDown")),
    );
}

/// The refactorings that apply at `start..end` (a caret when equal), as action ids.
pub fn refactorings_at(src: &str, start: usize, end: usize) -> Vec<String> {
    let mut found: Vec<&'static str> = Vec::new();
    let (start, end) = (start.min(src.len()), end.min(src.len()));
    if start < end && !src[start..end].trim().is_empty() {
        if extract_method::prepare(src, start, end).is_ok() {
            found.push("ExtractMethod");
        }
        introductions(src, start, end, &mut found);
        if !found.is_empty() {
            return ordered(found);
        }
    }
    let pos = start;
    if let Ok(target) = safe_delete::target_at(src, pos) {
        found.extend(["RenameElement", "SafeDelete"]);
        match target.kind.as_str() {
            "class" | "interface" | "trait" | "enum" => {
                found.extend(["Move", "CopyElement"]);
                class_level(src, pos, &mut found);
            }
            "method" => {
                found.push("ChangeSignature");
                if make_static::make_static_at(src, pos).is_ok() {
                    found.push("MakeStatic");
                }
                if inline::target_at(src, pos).is_ok() {
                    found.push("Inline");
                }
                if super::move_members::members_at(src, pos).is_some_and(|m| m.selected.is_some()) {
                    found.push("Move");
                }
                member_moves(src, pos, &mut found);
            }
            "function" => {
                found.extend(["ChangeSignature", "Move"]);
                if inline::target_at(src, pos).is_ok() {
                    found.push("Inline");
                }
            }
            "property" | "constant" => {
                if super::move_members::members_at(src, pos).is_some_and(|m| m.selected.is_some()) {
                    found.push("Move");
                }
                if target.kind == "constant" && inline::target_at(src, pos).is_ok() {
                    found.push("Inline");
                }
                member_moves(src, pos, &mut found);
            }
            "parameter" => found.push("ChangeSignature"),
            _ => {}
        }
        return ordered(found);
    }
    // In code: the expression and the element at the caret.
    introductions(src, pos, pos, &mut found);
    if inline::target_at(src, pos).is_ok() {
        found.push("Inline");
    }
    if on_name(src, pos) {
        found.push("RenameElement");
    }
    if found.is_empty() {
        // Between the members of a class.
        if let Some(info) = class_members(src, pos) {
            if info.selected.is_none() {
                class_level(src, pos, &mut found);
                found.extend(["RenameElement", "Move", "CopyElement", "SafeDelete"]);
            }
        }
    }
    if found.is_empty() && signature::signature_at(src, pos).is_ok() {
        found.push("ChangeSignature");
    }
    ordered(found)
}

/// The caret is on a name (a variable, a call, a class reference).
fn on_name(src: &str, pos: usize) -> bool {
    let Some(tree) = parse(src) else {
        return false;
    };
    [pos, pos.saturating_sub(1)].into_iter().any(|at| {
        tree.root_node()
            .descendant_for_byte_range(at, at)
            .is_some_and(|n| {
                matches!(n.kind(), "name" | "variable_name")
                    && !text(n, src).eq_ignore_ascii_case("$this")
                    || n.parent()
                        .is_some_and(|p| p.kind() == "variable_name" && text(p, src) != "$this")
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::test_util::sel;

    const SRC: &str = "<?php\nnamespace App;\n\nclass Cart extends Base implements Countable\n{\n    public const MAX = 10;\n    private array $items = [];\n\n    public function total(int $tax): int\n    {\n        $sum = array_sum($this->items) * 2;\n        return $sum + $tax;\n    }\n\n    public static function make(): self\n    {\n        return new self();\n    }\n}\n";

    fn at(marked: &str) -> Vec<String> {
        let (src, s, e) = sel(marked);
        refactorings_at(&src, s, e)
    }

    fn mark(word: &str, offset: usize) -> String {
        let i = SRC.find(word).unwrap() + offset;
        format!("{}‸{}", &SRC[..i], &SRC[i..])
    }

    #[test]
    fn on_a_class() {
        assert_eq!(
            at(&mark("Cart", 1)),
            [
                "RenameElement",
                "Move",
                "CopyElement",
                "SafeDelete",
                "ExtractInterface",
                "ExtractSuperclass",
                "MembersPullUp",
                "MemberPushDown"
            ]
        );
    }

    #[test]
    fn on_a_method() {
        assert_eq!(
            at(&mark("total", 1)),
            [
                "RenameElement",
                "ChangeSignature",
                "SafeDelete",
                "Inline",
                "MembersPullUp",
                "MemberPushDown",
                "MakeStatic"
            ]
        );
        // A static method moves (Move Members) and cannot be made static.
        let found = at(&mark("make", 1));
        assert!(found.contains(&"Move".to_string()));
        assert!(!found.contains(&"MakeStatic".to_string()));
    }

    #[test]
    fn on_an_expression_or_selection() {
        let found = at(&mark("array_sum", 2));
        assert!(
            found.contains(&"IntroduceVariable".to_string()),
            "{found:?}"
        );
        assert!(found.contains(&"IntroduceField".to_string()));
        assert!(!found.contains(&"ExtractInterface".to_string()));
        let i = SRC.find("array_sum").unwrap();
        let j = SRC[i..].find(" * 2").unwrap() + i;
        let found = refactorings_at(SRC, i, j);
        assert!(found.contains(&"ExtractMethod".to_string()), "{found:?}");
        assert!(found.contains(&"IntroduceVariable".to_string()));
        assert!(!found.contains(&"RenameElement".to_string()));
        // On a local variable: Rename and Inline.
        let found = at(&mark("$sum +", 2));
        assert!(found.contains(&"RenameElement".to_string()), "{found:?}");
        assert!(found.contains(&"Inline".to_string()), "{found:?}");
    }

    #[test]
    fn on_a_property_and_between_members() {
        let found = at(&mark("$items", 2));
        assert_eq!(
            found,
            [
                "RenameElement",
                "SafeDelete",
                "MembersPullUp",
                "MemberPushDown"
            ]
        );
        let found = at(&mark("\n\n    public function total", 1));
        assert!(
            found.contains(&"ExtractSuperclass".to_string()),
            "{found:?}"
        );
        assert!(at("<?php\n‸").is_empty());
    }
}

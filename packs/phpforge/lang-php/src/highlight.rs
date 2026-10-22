//! Highlight Usages in File (Ctrl+Shift+F7) and the highlighting of the element at the
//! caret on the PHP index, when no language server answers `documentHighlight`: the usages
//! of the symbol in the current file (read / write), and on `implements` / `extends` the
//! methods of the class that implement or override the parents' methods, like PHPStorm.
//!
//! Offsets inside are UTF-8 bytes; ranges handed out are LSP ranges.

use std::collections::HashSet;

use forge_codeintel::{
    classify_ranges, HighlightAnswer, HighlightKind, HighlightTarget, Range, UsageHighlight,
};

use crate::hover::word_at;
use crate::index::ProjectIndex;
use crate::model::{short_name, FileIndex, SymbolKind};
use crate::navigate::{
    declaration_range, element_kind, find_class, search_symbol, sources, usage_symbol,
    variable_usages, Lines, SymbolRef,
};
use crate::parser;

/// The methods of the class around `at` (the `implements` / `extends` keyword in its
/// header) that implement or override methods of the parents after the keyword.
fn parent_methods(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    at: usize,
    keyword: &str,
) -> Option<HighlightAnswer> {
    let class = file.class_at(at)?.clone();
    if at >= class.body_range.start {
        return None;
    }
    // `class A extends B implements C`: the keyword picks its own list.
    let parents = if keyword == "implements" {
        &class.implements
    } else {
        &class.extends
    };
    let mut names: HashSet<String> = HashSet::new();
    for fqn in parents {
        let Some(parent) = find_class(index, file, fqn) else {
            continue;
        };
        let mut lineage = vec![parent.clone()];
        lineage.extend(index.ancestors(&parent));
        for c in lineage {
            names.extend(c.methods().map(|m| m.name.to_ascii_lowercase()));
        }
    }
    let lines = Lines::new(src);
    let highlights = class
        .methods()
        .filter(|m| names.contains(&m.name.to_ascii_lowercase()))
        .map(|m| UsageHighlight {
            range: lines.range(m.name_range.start, m.name_range.end),
            kind: HighlightKind::Read,
        })
        .collect();
    let implements = keyword == "implements" || class.kind == SymbolKind::Interface;
    let title = parents
        .iter()
        .map(|p| short_name(p).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(HighlightAnswer::new(
        if implements {
            HighlightTarget::ImplementingMethods
        } else {
            HighlightTarget::OverridingMethods
        },
        title,
        highlights,
        "index",
    ))
}

/// Usages of the symbol at byte `offset` in the current file, the declaration (when it is
/// in the file) as a write.
pub fn highlights(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Option<HighlightAnswer> {
    let current = parser::index_source(rel, src);
    if let Some((s, e)) = word_at(src, offset) {
        let word = &src[s..e];
        if word == "implements" || word == "extends" {
            return parent_methods(index, &current, src, s, word);
        }
    }
    let symbol = usage_symbol(index, &current, src, offset)?;
    let element = element_kind(index, &current, &symbol);
    let lines = Lines::new(src);
    let ranges: Vec<Range> = match &symbol {
        SymbolRef::Variable { name, declared_at } => {
            variable_usages(&current, rel, src, name, *declared_at)
        }
        _ => {
            let mut sources = sources(index, rel, src);
            let paths = [rel.to_string()];
            search_symbol(index, rel, &current, &mut sources, &symbol, &paths, false)
        }
    }
    .into_iter()
    .map(|u| u.range)
    .collect();
    let declaration = declaration_range(index, &current, &symbol)
        .filter(|(path, _)| path == rel)
        .map(|(_, r)| {
            let end = match &symbol {
                // Constants' ranges cover `NAME = value`.
                SymbolRef::Constant { fqn } => r.start + short_name(fqn).len(),
                _ => r.end,
            };
            lines.range(r.start, end.min(src.len()))
        });
    let highlights = classify_ranges(src, &ranges, declaration);
    if highlights.is_empty() {
        return None;
    }
    Some(
        HighlightAnswer::new(
            HighlightTarget::Usages,
            symbol.search_word(),
            highlights,
            "index",
        )
        .with_element(element),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexConfig;
    use forge_codeintel::{ElementKind, Position};

    const GATEWAY: &str = "<?php\nnamespace App\\Payments;\n\ninterface Gateway\n{\n    public function charge(int $cents): bool;\n    public function refund(int $cents): void;\n}\n";
    const STRIPE: &str = "<?php\nnamespace App\\Payments;\n\nclass Stripe implements Gateway\n{\n    private int $total = 0;\n\n    public function charge(int $cents): bool\n    {\n        $this->total += $cents;\n        return $this->total > 0;\n    }\n\n    public function refund(int $cents): void\n    {\n        $left = $this->total - $cents;\n        $this->total = $left;\n    }\n\n    public function name(): string\n    {\n        return 'stripe';\n    }\n}\n";

    fn project() -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("app/Payments")).unwrap();
        std::fs::write(root.join("app/Payments/Gateway.php"), GATEWAY).unwrap();
        std::fs::write(root.join("app/Payments/Stripe.php"), STRIPE).unwrap();
        let mut idx = ProjectIndex::new(
            root,
            IndexConfig {
                cache_file: Some(root.join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn at(src: &str, needle: &str, n: usize) -> usize {
        src.match_indices(needle).nth(n).unwrap().0 + 1
    }

    fn lines_of(a: &HighlightAnswer) -> Vec<(u32, HighlightKind)> {
        a.highlights
            .iter()
            .map(|h| (h.range.start.line, h.kind))
            .collect()
    }

    #[test]
    fn property_reads_and_writes_in_the_file() {
        let (_d, mut idx) = project();
        let rel = "app/Payments/Stripe.php";
        let a = highlights(&mut idx, rel, STRIPE, at(STRIPE, "total", 1)).unwrap();
        assert_eq!(a.title, "total");
        assert_eq!(a.element, ElementKind::Property);
        assert_eq!(
            lines_of(&a),
            vec![
                (5, HighlightKind::Write),
                (9, HighlightKind::Write),
                (10, HighlightKind::Read),
                (15, HighlightKind::Read),
                (16, HighlightKind::Write),
            ]
        );
        assert_eq!(a.summary, "5 usages of 'total' found");
        let v = highlights(&mut idx, rel, STRIPE, at(STRIPE, "$left", 1)).unwrap();
        assert_eq!(
            lines_of(&v),
            vec![(15, HighlightKind::Write), (16, HighlightKind::Read)]
        );
        assert_eq!(v.element, ElementKind::Variable);
    }

    #[test]
    fn find_usages_options_add_inheritors_and_member_usages() {
        use crate::navigate::find_usages_with;
        use forge_codeintel::{UsageKind, UsageOptions};
        let (_d, mut idx) = project();
        let rel = "app/Payments/Gateway.php";
        let options = UsageOptions {
            interface_usages: false,
            interface_implementing_classes: true,
            ..Default::default()
        };
        let u =
            find_usages_with(&mut idx, rel, GATEWAY, at(GATEWAY, "Gateway", 0), &options).unwrap();
        assert_eq!(u.element, ElementKind::Interface);
        let found: Vec<(&str, Option<UsageKind>)> =
            u.usages.iter().map(|l| (l.path.as_str(), l.kind)).collect();
        assert_eq!(
            found,
            vec![(
                "app/Payments/Stripe.php",
                Some(UsageKind::ImplementingClass)
            )]
        );

        let options = UsageOptions {
            method_overriding_methods: true,
            ..Default::default()
        };
        let u =
            find_usages_with(&mut idx, rel, GATEWAY, at(GATEWAY, "charge", 0), &options).unwrap();
        assert_eq!(u.element, ElementKind::Method);
        assert!(u
            .usages
            .iter()
            .any(|l| l.kind == Some(UsageKind::ImplementingMethod) && l.range.start.line == 7));

        let rel = "app/Payments/Stripe.php";
        let options = UsageOptions {
            class_usages: false,
            class_field_usages: true,
            ..Default::default()
        };
        let u = find_usages_with(&mut idx, rel, STRIPE, at(STRIPE, "Stripe", 0), &options).unwrap();
        assert_eq!(u.usages.len(), 4, "usages of $total: {:?}", u.usages);
    }

    #[test]
    fn implements_highlights_the_implementing_methods() {
        let (_d, mut idx) = project();
        let rel = "app/Payments/Stripe.php";
        let a = highlights(&mut idx, rel, STRIPE, at(STRIPE, "implements", 0)).unwrap();
        assert_eq!(a.target, HighlightTarget::ImplementingMethods);
        assert_eq!(a.title, "Gateway");
        let starts: Vec<Position> = a.highlights.iter().map(|h| h.range.start).collect();
        assert_eq!(starts, vec![Position::new(7, 20), Position::new(13, 20)]);
        assert_eq!(a.summary, "2 methods implementing 'Gateway' found");
    }
}

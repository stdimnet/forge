//! Built-in PHP intentions for Show Context Actions (Alt+Enter), listed next to the
//! language server's quick fixes: import an unknown class, replace a qualifier with an
//! import, generate accessors / method stubs / a PHPDoc comment for the element at the
//! caret, and "Generate..." (Alt+Insert). Pure functions over the parsed file and the index.

use forge_codeintel::{Caret, WorkspaceEdit};
use serde::{Deserialize, Serialize};

use crate::generate::{self, GenerateKind, GenerateOptions, TextEdit};
use crate::hover::word_at;
use crate::index::ProjectIndex;
use crate::model::{short_name, FileIndex, SymbolKind, UseKind};
use crate::navigate::ExternalSymbol;
use crate::parser;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentionKind {
    /// Fixes a problem (PHPStorm's red bulb), listed first.
    QuickFix,
    /// Changes working code (yellow bulb).
    #[default]
    Intention,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intention {
    pub title: String,
    pub kind: IntentionKind,
    /// Edits of the current file (bytes here; UTF-16 at the IPC boundary).
    pub edits: Vec<TextEdit>,
    /// A UI action to run instead of edits, e.g. `Generate` (Alt+Insert).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Arguments of `action` (Create class: the class to create).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Edits of other files (LSP positions), applied together with `edits`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_edit: Option<WorkspaceEdit>,
    /// Where the caret goes afterwards (positions after the edit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caret: Option<Caret>,
    /// The symbol the action creates, as the PHP stubs would know it: the action is dropped
    /// when PHP itself declares it (`strlen()`, `\Exception::getMessage()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<ExternalSymbol>,
}

/// The edit adding `use <fqn>;` to the imports: in alphabetical order among the existing
/// class imports, else after the namespace declaration, else after `<?php`.
pub fn add_import(file: &FileIndex, src: &str, fqn: &str) -> TextEdit {
    let line = format!("use {fqn};");
    let insert = |at: usize, text: String| TextEdit {
        start: at,
        end: at,
        new_text: text,
    };
    let mut imports: Vec<_> = file
        .uses
        .iter()
        .filter(|u| u.kind == UseKind::Class)
        .collect();
    imports.sort_by_key(|u| u.range.start);
    if let Some(next) = imports
        .iter()
        .find(|u| u.fqn.to_ascii_lowercase() > fqn.to_ascii_lowercase())
    {
        let line_start = src[..next.range.start].rfind('\n').map_or(0, |i| i + 1);
        return insert(line_start, format!("{line}\n"));
    }
    if let Some(last) = imports.last() {
        let line_end = src[last.range.start..]
            .find('\n')
            .map_or(src.len(), |i| last.range.start + i);
        return insert(line_end, format!("\n{line}"));
    }
    if let Some(ns) = file.namespaces.first() {
        if let Some(semi) = src[ns.range.start..].find(';') {
            return insert(ns.range.start + semi + 1, format!("\n\n{line}"));
        }
    }
    let after_open_tag = src.find('\n').unwrap_or(src.len());
    insert(after_open_tag, format!("\n\n{line}"))
}

fn is_declaration(file: &FileIndex, start: usize, end: usize) -> bool {
    let inside = |r: &crate::model::Range| r.start <= start && end <= r.end;
    file.classes
        .iter()
        .any(|c| inside(&c.name_range) || c.members.iter().any(|m| inside(&m.name_range)))
        || file.functions.iter().any(|f| inside(&f.name_range))
}

fn import_intentions(
    index: &ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Vec<Intention> {
    let Some((start, end)) = word_at(src, offset) else {
        return vec![];
    };
    let word = &src[start..end];
    let before = src[..start].trim_end();
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_head = src[line_start..start].trim_start();
    let starts_upper = word
        .trim_start_matches('\\')
        .chars()
        .next()
        .is_some_and(char::is_uppercase);
    if word.starts_with('$')
        || !starts_upper
        || before.ends_with("->")
        || before.ends_with("::")
        || line_head.starts_with("use ")
        || line_head.starts_with("namespace ")
        || is_declaration(file, start, end)
    {
        return vec![];
    }
    let imported = |short: &str| {
        file.uses
            .iter()
            .find(|u| u.kind == UseKind::Class && u.local_name().eq_ignore_ascii_case(short))
    };

    // `\App\Models\Order` -> `Order` plus an import.
    if let Some(fqn) = word.strip_prefix('\\').filter(|w| w.contains('\\')) {
        let short = short_name(fqn);
        let mut edits = vec![TextEdit {
            start,
            end,
            new_text: short.to_string(),
        }];
        match imported(short) {
            Some(u) if !u.fqn.eq_ignore_ascii_case(fqn) => return vec![],
            Some(_) => {}
            None => edits.push(add_import(file, src, fqn)),
        }
        return vec![Intention {
            title: "Replace qualifier with an import".into(),
            kind: IntentionKind::Intention,
            edits,
            ..Default::default()
        }];
    }
    if word.contains('\\') {
        return vec![];
    }
    // An unknown class with candidates in the index: "Import class".
    let fqn = file.resolve_class(word, start);
    let known = index.find_class(&fqn).is_some()
        || file
            .classes
            .iter()
            .any(|c| c.fqn.eq_ignore_ascii_case(&fqn));
    if known || imported(word).is_some() {
        return vec![];
    }
    let mut candidates: Vec<String> = index
        .classes()
        .filter(|c| c.name.eq_ignore_ascii_case(word))
        .map(|c| c.fqn.clone())
        .collect();
    candidates.sort();
    candidates.dedup();
    candidates
        .into_iter()
        .map(|fqn| Intention {
            title: format!("Import class '{fqn}'"),
            kind: IntentionKind::QuickFix,
            edits: vec![add_import(file, src, &fqn)],
            ..Default::default()
        })
        .collect()
}

fn generate_intention(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
    title: &str,
    kind: GenerateKind,
    options: GenerateOptions,
) -> Option<Intention> {
    let edits = generate::generate(file, src, offset, kind, &options, index).ok()?;
    Some(Intention {
        title: title.to_string(),
        kind: IntentionKind::Intention,
        edits,
        ..Default::default()
    })
}

/// "Add 'readonly' modifier" on a typed, non-static property without a default value
/// (PHP 8.1): `private int $id` → `private readonly int $id`, also for promoted properties.
fn readonly_intention(src: &str, p: &crate::model::Member) -> Option<Intention> {
    if p.modifiers.is_readonly
        || p.modifiers.is_static
        || p.value.is_some()
        || p.type_hint.is_none()
    {
        return None;
    }
    let head = src.get(p.range.start..p.name_range.start)?;
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let end = ["public", "protected", "private"]
        .iter()
        .filter_map(|kw| {
            head.match_indices(kw)
                .filter(|(i, _)| {
                    !head[..*i].chars().next_back().is_some_and(ident)
                        && !head[i + kw.len()..].chars().next().is_some_and(ident)
                })
                .map(|(i, _)| i + kw.len())
                .last()
        })
        .max()?;
    let at = p.range.start + end;
    Some(Intention {
        title: "Add 'readonly' modifier".into(),
        kind: IntentionKind::Intention,
        edits: vec![TextEdit {
            start: at,
            end: at,
            new_text: " readonly".into(),
        }],
        ..Default::default()
    })
}

/// Intentions at `offset` (bytes) of `src`, the content of `rel`. `options` carries the code
/// style and the user's code templates for generated code.
pub fn intentions(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
    options: &GenerateOptions,
) -> Vec<Intention> {
    let file = parser::index_source(rel, src);
    let mut out = import_intentions(index, &file, src, offset);
    out.extend(crate::create::create_from_usage(
        index, &file, rel, src, offset, options,
    ));
    let targets = generate::targets(&file, src, offset, index);
    let Some(class) = file.class_at(offset).cloned() else {
        return out;
    };

    // On a property declaration: accessors it does not have yet.
    if let Some(p) = class
        .members
        .iter()
        .find(|m| m.kind == SymbolKind::Property && m.range.contains(offset))
    {
        if let Some(f) = targets.fields.iter().find(|f| f.name == p.name) {
            let fields = || GenerateOptions {
                fields: vec![f.name.clone()],
                ..options.clone()
            };
            let can_set = !f.has_setter && !f.is_readonly;
            let mut add = |title: &str, kind| {
                if let Some(i) =
                    generate_intention(index, &file, src, offset, title, kind, fields())
                {
                    out.push(i);
                }
            };
            if !f.has_getter && can_set {
                add(
                    "Generate getter and setter",
                    GenerateKind::GettersAndSetters,
                );
            }
            if !f.has_getter {
                add("Generate getter", GenerateKind::Getters);
            }
            if can_set {
                add("Generate setter", GenerateKind::Setters);
            }
        }
        if let Some(i) = readonly_intention(src, p) {
            out.push(i);
        }
    }

    // On the class name: stubs for abstract / interface methods it does not implement.
    if class.name_range.contains(offset) && !targets.implement_methods.is_empty() {
        let options = GenerateOptions {
            methods: targets
                .implement_methods
                .iter()
                .map(|m| m.id.clone())
                .collect(),
            ..options.clone()
        };
        if let Some(mut i) = generate_intention(
            index,
            &file,
            src,
            offset,
            "Add method stubs",
            GenerateKind::ImplementMethods,
            options,
        ) {
            i.kind = IntentionKind::QuickFix;
            out.push(i);
        }
    }

    if let Some(name) = &targets.php_doc_target {
        if let Some(i) = generate_intention(
            index,
            &file,
            src,
            offset,
            &format!("Generate PHPDoc for '{name}'"),
            GenerateKind::PhpDoc,
            options.clone(),
        ) {
            out.push(i);
        }
    }

    out.push(Intention {
        title: "Generate...".into(),
        kind: IntentionKind::Intention,
        edits: vec![],
        action: Some("Generate".into()),
        ..Default::default()
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;
    use crate::IndexConfig;
    use std::fs;

    fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        for (p, t) in files {
            let path = dir.path().join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, t).unwrap();
        }
        let mut idx = ProjectIndex::new(
            dir.path(),
            IndexConfig {
                cache_file: Some(dir.path().join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    const ORDER: &str = "<?php\nnamespace App\\Models;\n\nclass Order {}\n";
    const LEGACY: &str = "<?php\nnamespace Legacy;\n\nclass Order {}\n";

    fn titles(i: &[Intention]) -> Vec<&str> {
        i.iter().map(|i| i.title.as_str()).collect()
    }

    #[test]
    fn imports_unknown_classes_in_order() {
        let (_d, mut idx) = index_with(&[
            ("app/Models/Order.php", ORDER),
            ("legacy/Order.php", LEGACY),
        ]);
        let src = "<?php\n\nnamespace App\\Http;\n\nuse App\\Models\\Address;\nuse Illuminate\\Http\\Request;\n\nfunction show(Order $o) {}\n";
        let at = src.find("Order $o").unwrap() + 2;
        let found = intentions(
            &mut idx,
            "app/Http/show.php",
            src,
            at,
            &GenerateOptions::default(),
        );
        assert_eq!(
            titles(&found)[..2],
            [
                "Import class 'App\\Models\\Order'",
                "Import class 'Legacy\\Order'"
            ]
        );
        assert_eq!(found[0].kind, IntentionKind::QuickFix);
        let fixed = apply_edits(src, &found[0].edits);
        assert!(fixed.contains(
            "use App\\Models\\Address;\nuse App\\Models\\Order;\nuse Illuminate\\Http\\Request;\n"
        ));

        // Known classes get no import.
        let at = fixed.find("Order $o").unwrap() + 2;
        let again = intentions(
            &mut idx,
            "app/Http/show.php",
            &fixed,
            at,
            &GenerateOptions::default(),
        );
        assert!(!titles(&again).iter().any(|t| t.starts_with("Import class")));
    }

    #[test]
    fn replaces_a_qualifier_with_an_import() {
        let (_d, mut idx) = index_with(&[("app/Models/Order.php", ORDER)]);
        let src = "<?php\nnamespace App\\Http;\n\nfunction f(): \\App\\Models\\Order {}\n";
        let at = src.find("Models\\Order {").unwrap();
        let found = intentions(&mut idx, "f.php", src, at, &GenerateOptions::default());
        assert_eq!(found[0].title, "Replace qualifier with an import");
        assert_eq!(
            apply_edits(src, &found[0].edits),
            "<?php\nnamespace App\\Http;\n\nuse App\\Models\\Order;\n\nfunction f(): Order {}\n"
        );
    }

    #[test]
    fn offers_accessors_stubs_phpdoc_and_generate() {
        let (_d, mut idx) = index_with(&[(
            "app/Contracts/Payable.php",
            "<?php\nnamespace App\\Contracts;\n\ninterface Payable\n{\n    public function pay(int $cents): void;\n}\n",
        )]);
        let src = "<?php\nnamespace App;\n\nuse App\\Contracts\\Payable;\n\nclass Invoice implements Payable\n{\n    private int $total = 0;\n\n    public function getTotal(): int\n    {\n        return $this->total;\n    }\n}\n";
        let at = src.find("$total = 0").unwrap();
        let found = intentions(
            &mut idx,
            "app/Invoice.php",
            src,
            at,
            &GenerateOptions::default(),
        );
        assert_eq!(
            titles(&found),
            [
                "Generate setter",
                "Generate PHPDoc for 'total'",
                "Generate..."
            ]
        );
        let with_setter = apply_edits(src, &found[0].edits);
        assert!(with_setter.contains("public function setTotal(int $total): void"));
        assert_eq!(found[2].action.as_deref(), Some("Generate"));

        let at = src.find("Invoice implements").unwrap() + 1;
        let found = intentions(
            &mut idx,
            "app/Invoice.php",
            src,
            at,
            &GenerateOptions::default(),
        );
        assert_eq!(found[0].title, "Add method stubs");
        assert!(apply_edits(src, &found[0].edits).contains("public function pay(int $cents): void"));

        let at = src.find("getTotal").unwrap() + 1;
        let found = intentions(
            &mut idx,
            "app/Invoice.php",
            src,
            at,
            &GenerateOptions::default(),
        );
        assert!(titles(&found).contains(&"Generate PHPDoc for 'getTotal'"));
    }

    #[test]
    fn makes_typed_properties_readonly() {
        let (_d, mut idx) = index_with(&[]);
        let src = "<?php\nclass Money\n{\n    private static int $count;\n    protected string $currency;\n    public ?int $cents = null;\n\n    public function __construct(private int $amount) {}\n}\n";
        let readonly = |at: &str, idx: &mut ProjectIndex| {
            let offset = src.find(at).unwrap();
            intentions(
                idx,
                "app/Money.php",
                src,
                offset,
                &GenerateOptions::default(),
            )
            .into_iter()
            .find(|i| i.title == "Add 'readonly' modifier")
        };
        let i = readonly("$currency", &mut idx).expect("typed property");
        assert!(apply_edits(src, &i.edits).contains("protected readonly string $currency;"));
        let i = readonly("$amount", &mut idx).expect("promoted property");
        assert!(apply_edits(src, &i.edits).contains("(private readonly int $amount)"));
        assert!(readonly("$count", &mut idx).is_none(), "static");
        assert!(readonly("$cents", &mut idx).is_none(), "default value");
    }
}

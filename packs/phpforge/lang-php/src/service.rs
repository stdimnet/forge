//! Editor-facing PHP queries on a [`ProjectIndex`]: the same answers wherever the index
//! lives (in the desktop app or in the remote development agent). Offsets crossing this
//! boundary are UTF-16 (JS strings); the index works in bytes, so every query converts.

use forge_codeintel::{HighlightAnswer, UsageOptions};

use crate::generate::{self, GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use crate::hierarchy::{self, HierarchyQuery, PhpHierarchy};
use crate::highlight;
use crate::hover::{self, Hover};
use crate::index::ProjectIndex;
use crate::intentions::{self, Intention};
use crate::navigate::{self, NavKind, NavTarget, PhpDefinition, PhpLineMarker, PhpUsages};
use crate::offsets::{byte_to_utf16, utf16_to_byte};
use crate::parser;
use crate::structure::{FileStructure, StructureNode};

fn convert_nodes(text: &str, nodes: &mut [StructureNode]) {
    for n in nodes {
        // Inherited members live in other files; the UI navigates to them by line.
        if n.path.is_none() {
            n.offset = byte_to_utf16(text, n.offset);
        }
        convert_nodes(text, &mut n.children);
    }
}

/// The structure of `text` (the content of `path`), optionally with inherited members.
pub fn file_structure(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    inherited: bool,
) -> FileStructure {
    let file = parser::index_source(path, text);
    let mut s = idx.file_structure(&file, inherited);
    convert_nodes(text, &mut s.nodes);
    s
}

/// Hover at a UTF-16 `offset`, with a UTF-16 range.
pub fn hover(idx: &mut ProjectIndex, path: &str, text: &str, offset: usize) -> Option<Hover> {
    let byte = utf16_to_byte(text, offset);
    hover::hover(idx, path, text, byte).map(|h| Hover {
        start: byte_to_utf16(text, h.start),
        end: byte_to_utf16(text, h.end),
        markdown: h.markdown,
    })
}

/// What the "Generate" popup offers at a UTF-16 `offset`.
pub fn generate_targets(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
) -> GenerateTargets {
    let file = parser::index_source(path, text);
    let byte = utf16_to_byte(text, offset);
    generate::targets(&file, text, byte, idx)
}

/// The edits of a "Generate" action, with UTF-16 ranges.
pub fn generate(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
    kind: GenerateKind,
    options: &GenerateOptions,
) -> crate::Result<Vec<TextEdit>> {
    let file = parser::index_source(path, text);
    let byte = utf16_to_byte(text, offset);
    let edits = generate::generate(&file, text, byte, kind, options, idx)?;
    Ok(edits_to_utf16(text, edits))
}

fn edits_to_utf16(text: &str, edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits
        .into_iter()
        .map(|e| TextEdit {
            start: byte_to_utf16(text, e.start),
            end: byte_to_utf16(text, e.end),
            new_text: e.new_text,
        })
        .collect()
}

/// Find Usages from the index for the symbol at a UTF-16 `offset` (LSP positions out).
pub fn usages(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
    options: &UsageOptions,
) -> Option<PhpUsages> {
    navigate::find_usages_with(idx, path, text, utf16_to_byte(text, offset), options)
}

/// Highlight Usages in File at a UTF-16 `offset` (see [`crate::highlight`]).
pub fn highlights(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
) -> Option<HighlightAnswer> {
    highlight::highlights(idx, path, text, utf16_to_byte(text, offset))
}

/// Go to Implementation / Super Method / Type Declaration at a UTF-16 `offset`.
pub fn navigate(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
    kind: NavKind,
) -> Vec<NavTarget> {
    navigate::navigate(idx, path, text, utf16_to_byte(text, offset), kind)
}

/// Go to Declaration at a UTF-16 `offset` (LSP positions out).
pub fn definition(idx: &mut ProjectIndex, path: &str, text: &str, offset: usize) -> PhpDefinition {
    navigate::definition(idx, path, text, utf16_to_byte(text, offset))
}

/// Type, Method or Call Hierarchy at a UTF-16 `offset` (LSP positions out).
pub fn hierarchy(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
    query: HierarchyQuery,
) -> PhpHierarchy {
    hierarchy::hierarchy(idx, path, text, utf16_to_byte(text, offset), query)
}

/// The implemented / overridden gutter markers of a file.
pub fn line_markers(idx: &mut ProjectIndex, path: &str, text: &str) -> Vec<PhpLineMarker> {
    navigate::line_markers(idx, path, text)
}

/// Show Context Actions: the built-in intentions at a UTF-16 `offset`, UTF-16 edits (the
/// workspace edits and carets of Create from usage are LSP positions already).
pub fn intentions(
    idx: &mut ProjectIndex,
    path: &str,
    text: &str,
    offset: usize,
    options: &GenerateOptions,
) -> Vec<Intention> {
    crate::imports::with_style(options.style.imports, || {
        intentions::intentions(idx, path, text, utf16_to_byte(text, offset), options)
    })
    .into_iter()
    .map(|mut i| {
        i.edits = edits_to_utf16(text, std::mem::take(&mut i.edits));
        i
    })
    .collect()
}

/// The edit importing `fqn` into `text` (the content of `path`), UTF-16 offsets: nothing
/// when the class is in the file's namespace or imported already. Create class uses it when
/// the new class went to another namespace than the usage implied.
pub fn import_class(path: &str, text: &str, fqn: &str) -> Vec<TextEdit> {
    let fqn = fqn.trim_start_matches('\\');
    let file = parser::index_source(path, text);
    let ns = fqn.rsplit_once('\\').map_or("", |(ns, _)| ns);
    let short = crate::model::short_name(fqn);
    let in_namespace = file
        .namespaces
        .first()
        .map_or("", |n| n.name.as_str())
        .eq_ignore_ascii_case(ns);
    let imported = file.uses.iter().any(|u| {
        u.kind == crate::model::UseKind::Class && u.local_name().eq_ignore_ascii_case(short)
    });
    if in_namespace || imported {
        return vec![];
    }
    edits_to_utf16(
        text,
        intentions::add_import(&file, text, fqn)
            .into_iter()
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;

    #[test]
    fn structure_offsets_are_utf16() {
        let dir = tempfile::tempdir().unwrap();
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        let text = "<?php\n// ü\nclass A { public function b() {} }\n";
        let s = file_structure(&mut idx, "a.php", text, false);
        let name_byte = text.find("class A").unwrap() + 6;
        // "ü" is two bytes but one UTF-16 unit.
        assert_eq!(s.nodes[0].offset, name_byte - 1);
    }

    #[test]
    fn imports_a_created_class_once() {
        let text = "<?php\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nnew Ünit();\n";
        let edits = import_class("a.php", text, "App\\Support\\Ünit");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "\nuse App\\Support\\Ünit;");
        assert!(import_class("a.php", text, "App\\Http\\Ünit").is_empty());
        assert!(import_class("a.php", text, "Other\\User").is_empty());
    }
}

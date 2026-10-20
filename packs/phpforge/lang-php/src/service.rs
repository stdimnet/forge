//! Editor-facing PHP queries on a [`ProjectIndex`]: the same answers wherever the index
//! lives (in the desktop app or in the remote development agent). Offsets crossing this
//! boundary are UTF-16 (JS strings); the index works in bytes, so every query converts.

use crate::generate::{self, GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use crate::hover::{self, Hover};
use crate::index::ProjectIndex;
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
    Ok(edits
        .into_iter()
        .map(|e| TextEdit {
            start: byte_to_utf16(text, e.start),
            end: byte_to_utf16(text, e.end),
            new_text: e.new_text,
        })
        .collect())
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
}

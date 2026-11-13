//! The site functions with editor offsets (UTF-16 code units, as JavaScript strings and
//! CodeMirror count), as the IPC commands and the browser mock expose them.

use serde::{Deserialize, Serialize};

use crate::dql::Item;
use crate::model::{Location, Snapshot};
use crate::site::{self, SiteFinder, SourceDiagnostic, SourceHighlight};

/// UTF-16 offset of byte offset `byte` in `text`.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].encode_utf16().count()
}

/// Byte offset of UTF-16 offset `units` in `text`.
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DqlCompletion {
    /// Where the replaced text starts.
    pub from: usize,
    pub items: Vec<Item>,
}

pub fn complete(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Option<DqlCompletion> {
    let byte = utf16_to_byte(src, offset);
    site::complete(finder, snap, path, src, byte).map(|(from, c)| DqlCompletion {
        from: byte_to_utf16(src, from),
        items: c.items,
    })
}

pub fn goto(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
    offset: usize,
) -> Vec<Location> {
    site::goto(finder, snap, path, src, utf16_to_byte(src, offset))
}

pub fn diagnostics(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
) -> Vec<SourceDiagnostic> {
    site::diagnostics(finder, snap, path, src)
        .into_iter()
        .map(|mut d| {
            d.start = byte_to_utf16(src, d.start);
            d.end = byte_to_utf16(src, d.end);
            d
        })
        .collect()
}

pub fn highlight(
    finder: &dyn SiteFinder,
    snap: &Snapshot,
    path: &str,
    src: &str,
) -> SourceHighlight {
    let mut h = site::highlight(finder, snap, path, src);
    for r in &mut h.injected {
        *r = (byte_to_utf16(src, r.0), byte_to_utf16(src, r.1));
    }
    for s in &mut h.spans {
        s.from = byte_to_utf16(src, s.from);
        s.to = byte_to_utf16(src, s.to);
    }
    h
}

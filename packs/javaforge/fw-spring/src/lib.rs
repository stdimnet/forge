//! JavaForge's Spring Boot and Jakarta EE support, like IntelliJ IDEA Ultimate's.
//! Pure Rust: the caller hands in file names and texts, so the desktop backend and the
//! browser mode (WebAssembly) share it. Nothing here runs the application.
//!
//! - [`scan`] reads Java and Kotlin declarations with their annotations and arguments.
//! - [`config`] reads `application.properties` / `application.yml` (and profile variants);
//!   [`metadata`] holds `spring-configuration-metadata.json`; [`assist`] completes,
//!   documents, inspects and navigates configuration keys.

use serde::{Deserialize, Serialize};

pub mod assist;
pub mod config;
#[cfg(feature = "jars")]
pub mod jars;
pub mod metadata;
pub mod model;
pub mod scan;

/// A place in a project file (0-based line and column).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// Byte offset of a UTF-16 offset (JS strings) in `text`.
pub fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// UTF-16 offset of a byte offset in `text`.
pub fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().map(char::len_utf16).sum()
}

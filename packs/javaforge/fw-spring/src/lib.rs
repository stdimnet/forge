//! JavaForge's Spring Boot and Jakarta EE support, like IntelliJ IDEA Ultimate's.
//! Pure Rust: the caller hands in file names and texts, so the desktop backend and the
//! browser mode (WebAssembly) share it. Nothing here runs the application.
//!
//! - [`scan`] reads Java and Kotlin declarations with their annotations and arguments.
//! - [`config`] reads `application.properties` / `application.yml` (and profile variants);
//!   [`metadata`] holds `spring-configuration-metadata.json`; [`assist`] completes,
//!   documents, inspects and navigates configuration keys; [`jars`] (feature `jars`) reads
//!   the metadata out of the dependency jars in the local Maven / Gradle caches.
//! - [`model`] is the project's Spring / Jakarta EE model: beans and injection points,
//!   Spring MVC and JAX-RS endpoints, JPA entities, `@ConfigurationProperties` classes,
//!   APIs, profiles, and the gutter markers of a file; [`views`] merges it with what a
//!   running application reports and writes "Open in HTTP Client" requests.
//! - [`run`] is the Spring Boot run configuration and reads its console output;
//!   [`actuator`] parses (and with feature `net` fetches) health, beans and mappings;
//!   [`dashboard`] builds the run dashboard's Services nodes and details.

use serde::{Deserialize, Serialize};

pub mod actuator;
pub mod assist;
pub mod config;
pub mod dashboard;
#[cfg(feature = "jars")]
pub mod jars;
pub mod metadata;
pub mod model;
pub mod run;
pub mod scan;
pub mod views;

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
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

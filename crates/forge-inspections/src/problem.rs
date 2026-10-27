//! A problem an inspection reports, with its quick fixes.

use serde::{Deserialize, Serialize};

/// A change of one file: replace the bytes `start..end` with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEdit {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// A quick fix (Alt+Enter, "Apply fix" in Inspection Results, Code Cleanup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub title: String,
    pub edits: Vec<FileEdit>,
    /// Safe to apply without looking (Code Cleanup applies only these).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub safe: bool,
}

/// A place in a file (0-based line, UTF-16 columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// A problem: providers fill `start`/`end` (bytes); the runner adds the positions, the
/// effective severity and the suppress actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// The inspection id (`PhpUnusedAssignment`).
    pub inspection: String,
    pub path: String,
    /// Byte offsets in the file.
    pub start: usize,
    pub end: usize,
    pub message: String,
    /// The level the provider reports (external tools); the profile decides otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
    /// Other places that belong to it (the other copies of a duplicate).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Location>,
    /// Shown greyed out (unused code).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unnecessary: bool,
    /// Shown struck through (deprecated symbols).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

impl Problem {
    pub fn new(
        inspection: &str,
        path: &str,
        start: usize,
        end: usize,
        message: impl Into<String>,
    ) -> Self {
        Problem {
            inspection: inspection.into(),
            path: path.into(),
            start,
            end,
            message: message.into(),
            severity: None,
            fixes: Vec::new(),
            related: Vec::new(),
            unnecessary: false,
            deprecated: false,
        }
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }
}

/// A short fingerprint of a file's text (FNV-1a), so fixes are not applied to a file that
/// changed since it was inspected.
pub fn stamp(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

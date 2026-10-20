//! Offsets and positions. Scanners work in UTF-8 byte offsets; the UI (CodeMirror, JS
//! strings) uses UTF-16 code units and 0-based lines/columns. Convert at the boundary.

use serde::{Deserialize, Serialize};

/// A position in a project file: workspace-relative path, 0-based line and UTF-16 column.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

impl Location {
    /// The location of byte offset `byte` in `text`.
    pub fn at(path: &str, text: &str, byte: usize) -> Self {
        let (line, column) = line_col(text, byte);
        Location {
            path: path.to_string(),
            line,
            column,
        }
    }

    /// The start of a file.
    pub fn file(path: &str) -> Self {
        Location {
            path: path.to_string(),
            line: 0,
            column: 0,
        }
    }
}

/// UTF-16 offset -> byte offset (clamped to the text, snapped to a char boundary).
pub fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Byte offset -> UTF-16 offset.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let b = floor_boundary(text, byte);
    text[..b].chars().map(char::len_utf16).sum()
}

/// 0-based line and UTF-16 column of a byte offset.
pub fn line_col(text: &str, byte: usize) -> (u32, u32) {
    let b = floor_boundary(text, byte);
    let before = &text[..b];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column: usize = before[line_start..].chars().map(char::len_utf16).sum();
    (line as u32, column as u32)
}

/// 0-based line of a byte offset.
pub fn line_of(text: &str, byte: usize) -> u32 {
    line_col(text, byte).0
}

fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    b
}

/// Identifier characters of JavaScript (ASCII plus `$`; non-ASCII letters count too).
pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

/// The identifier that ends at byte `start` (exclusive) and the one starting at `end`.
pub fn ident_at(text: &str, byte: usize) -> Option<(usize, usize)> {
    let byte = floor_boundary(text, byte);
    let start = text[..byte]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident_char(*c))
        .last()
        .map_or(byte, |(i, _)| i);
    let end = byte
        + text[byte..]
            .char_indices()
            .find(|(_, c)| !is_ident_char(*c))
            .map_or(text.len() - byte, |(i, _)| i);
    (start < end).then_some((start, end))
}

/// `UserCard` -> `user-card`, `HTMLView` -> `html-view`.
pub fn kebab_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let prev_upper = i > 0 && chars[i - 1].is_uppercase();
            if i > 0 && (prev_lower || (prev_upper && next_lower)) {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

/// `user-card` / `user_card` / `userCard` -> `UserCard`.
pub fn pascal_case(name: &str) -> String {
    name.split(['-', '_', '.', ' '])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

/// `user-name` -> `userName`.
pub fn camel_case(name: &str) -> String {
    let p = pascal_case(name);
    let mut c = p.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or_default()
}

/// Parent directory of a workspace-relative path (`""` for the root).
pub fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// File name without directories and the last extension: `src/UserCard.vue` -> `UserCard`.
pub fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').map_or(name, |i| &name[..i])
}

/// Join `rel` (`./a`, `../b/c`, `d`) onto `dir` and normalise `.` and `..`.
pub fn join_path(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = if rel.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_round_trip() {
        let s = "a€b😀c\nx";
        assert_eq!(utf16_to_byte(s, 2), 4);
        assert_eq!(byte_to_utf16(s, 9), 5);
        assert_eq!(line_col(s, s.len()), (1, 1));
        assert_eq!(line_col(s, 9), (0, 5));
    }

    #[test]
    fn cases() {
        assert_eq!(kebab_case("UserCard"), "user-card");
        assert_eq!(kebab_case("HTMLView"), "html-view");
        assert_eq!(kebab_case("Item2Row"), "item2-row");
        assert_eq!(pascal_case("user-card"), "UserCard");
        assert_eq!(pascal_case("UserCard"), "UserCard");
        assert_eq!(camel_case("user-name"), "userName");
    }

    #[test]
    fn paths() {
        assert_eq!(join_path("src/app", "./user.html"), "src/app/user.html");
        assert_eq!(join_path("src/app", "../shared/x"), "src/shared/x");
        assert_eq!(join_path("", "a/b"), "a/b");
        assert_eq!(file_stem("src/UserCard.vue"), "UserCard");
        assert_eq!(parent_dir("package.json"), "");
        assert_eq!(parent_dir("apps/web/package.json"), "apps/web");
    }

    #[test]
    fn identifier_at_offset() {
        let s = "<UserCard :user=\"u\"/>";
        assert_eq!(ident_at(s, 3), Some((1, 9)));
        assert_eq!(ident_at(s, 9), Some((1, 9)));
        assert_eq!(ident_at(s, 0), None);
    }
}

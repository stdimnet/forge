//! Scratch names: PHPStorm numbers new scratches `scratch.php`, `scratch_1.php`,
//! `scratch_2.php`, … and scratch buffers `buffer1.txt` … `buffer5.txt` (the oldest buffer is
//! reused once all five exist).

use crate::{Error, Result};

/// Number of scratch buffers before the oldest one is reused.
pub const MAX_BUFFERS: u32 = 5;

/// Base name of new scratches of a language (`rest-api.http` for HTTP requests, as in
/// PHPStorm, `scratch.<ext>` otherwise).
pub fn base_name(extension: &str) -> &'static str {
    match extension {
        "http" | "rest" => "rest-api",
        _ => "scratch",
    }
}

/// The first free name `base.ext`, `base_1.ext`, `base_2.ext`, … (case-insensitive, since
/// scratches may live on a case-insensitive file system).
pub fn next_name(existing: &[String], base: &str, extension: &str) -> String {
    let taken = |name: &str| existing.iter().any(|e| e.eq_ignore_ascii_case(name));
    let file = |suffix: String| {
        if extension.is_empty() {
            format!("{base}{suffix}")
        } else {
            format!("{base}{suffix}.{extension}")
        }
    };
    let first = file(String::new());
    if !taken(&first) {
        return first;
    }
    (1..)
        .map(|n| file(format!("_{n}")))
        .find(|name| !taken(name))
        .expect("an unused name exists")
}

/// The buffer to use for "New Scratch Buffer": the lowest free `bufferN.txt` (N ≤
/// [`MAX_BUFFERS`]), else the least recently modified buffer. `existing` holds names and
/// modification times. Returns the name and whether it is reused (and should be emptied).
pub fn next_buffer(existing: &[(String, u64)]) -> (String, bool) {
    let buffers: Vec<(u32, u64)> = existing
        .iter()
        .filter_map(|(name, modified)| buffer_number(name).map(|n| (n, *modified)))
        .collect();
    if let Some(free) = (1..=MAX_BUFFERS).find(|n| !buffers.iter().any(|(b, _)| b == n)) {
        return (buffer_name(free), false);
    }
    let oldest = buffers
        .iter()
        .filter(|(n, _)| *n <= MAX_BUFFERS)
        .min_by_key(|(n, modified)| (*modified, *n))
        .map(|(n, _)| *n)
        .unwrap_or(1);
    (buffer_name(oldest), true)
}

fn buffer_name(n: u32) -> String {
    format!("buffer{n}.txt")
}

/// `buffer3.txt` → 3.
pub fn buffer_number(name: &str) -> Option<u32> {
    let n = name.strip_prefix("buffer")?.strip_suffix(".txt")?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

/// A scratch name must be a plain file name: no separators, no `.`/`..`, no control
/// characters and nothing Windows refuses.
pub fn validate_name(name: &str) -> Result<()> {
    let invalid = |why: &str| {
        Err(Error::Invalid(format!(
            "'{name}' is not a valid name: {why}"
        )))
    };
    if name.trim().is_empty() {
        return Err(Error::Invalid("the name is empty".into()));
    }
    if name != name.trim() {
        return invalid("it starts or ends with a space");
    }
    if name == "." || name == ".." {
        return invalid("it is reserved");
    }
    if let Some(c) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return invalid(&format!("it contains '{}'", c.escape_default()));
    }
    if name.ends_with('.') {
        return invalid("it ends with a dot");
    }
    if name.len() > 200 {
        return invalid("it is too long");
    }
    Ok(())
}

/// Extension of a scratch name (`scratch.blade.php` → `php`), lowercase.
pub fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn numbers_new_scratches() {
        assert_eq!(next_name(&[], "scratch", "php"), "scratch.php");
        assert_eq!(
            next_name(&names(&["scratch.php"]), "scratch", "php"),
            "scratch_1.php"
        );
        assert_eq!(
            next_name(
                &names(&["scratch.php", "scratch_1.php", "scratch_3.php"]),
                "scratch",
                "php"
            ),
            "scratch_2.php"
        );
        // Other extensions do not count; case does.
        assert_eq!(
            next_name(&names(&["scratch.js", "Scratch.PHP"]), "scratch", "php"),
            "scratch_1.php"
        );
        assert_eq!(next_name(&names(&["scratch"]), "scratch", ""), "scratch_1");
        assert_eq!(base_name("http"), "rest-api");
        assert_eq!(base_name("php"), "scratch");
    }

    #[test]
    fn buffers_fill_up_then_rotate() {
        assert_eq!(next_buffer(&[]), ("buffer1.txt".into(), false));
        let some = vec![
            ("buffer1.txt".to_string(), 5),
            ("buffer3.txt".to_string(), 1),
        ];
        assert_eq!(next_buffer(&some), ("buffer2.txt".into(), false));
        let all: Vec<(String, u64)> = (1..=5)
            .map(|n| (format!("buffer{n}.txt"), [50, 20, 90, 10, 30][n - 1]))
            .collect();
        assert_eq!(next_buffer(&all), ("buffer4.txt".into(), true));
        assert_eq!(buffer_number("buffer12.txt"), Some(12));
        assert_eq!(buffer_number("buffer.txt"), None);
        assert_eq!(buffer_number("buffer1.php"), None);
    }

    #[test]
    fn validates_names() {
        assert!(validate_name("scratch_1.php").is_ok());
        assert!(validate_name("my notes.md").is_ok());
        for bad in [
            "",
            " ",
            "a/b.php",
            "..",
            "a\\b",
            "x.",
            " lead.php",
            "a:b",
            "nul\0",
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn extensions() {
        assert_eq!(extension("scratch.PHP"), "php");
        assert_eq!(extension("rest-api.http"), "http");
        assert_eq!(extension(".env"), "");
        assert_eq!(extension("Makefile"), "");
    }
}

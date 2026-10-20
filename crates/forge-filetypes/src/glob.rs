//! The two pattern flavours of file type settings.

/// `*` matches any run of characters; everything else is literal (case-sensitive).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut tail) = text.strip_prefix(first) else {
        return false;
    };
    let rest: Vec<&str> = parts.collect();
    let Some((last, middle)) = rest.split_last() else {
        return tail.is_empty();
    };
    for part in middle {
        match tail.find(part) {
            Some(i) => tail = &tail[i + part.len()..],
            None => return false,
        }
    }
    tail.ends_with(last)
}

/// Case-insensitive glob with `*` (any run) and `?` (one character).
fn glob_match(pattern: &[char], text: &[char]) -> bool {
    // Iterative matcher with backtracking to the last `*`.
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// Match a path against a `;`-separated list of simple globs (`*`, `?`), e.g.
/// `*.md; *.txt`. Patterns with a `/` match the whole path, others the file name.
pub fn matches_glob_list(path: &str, patterns: &str) -> bool {
    let lower = |s: &str| -> Vec<char> { s.chars().flat_map(char::to_lowercase).collect() };
    let name = path.rsplit('/').next().unwrap_or(path);
    patterns
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .any(|p| {
            let target = if p.contains('/') { path } else { name };
            glob_match(&lower(p), &lower(target))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("a*b*c", "aXbYc"));
        assert!(wildcard_match("*.yml", "x.yml"));
        assert!(!wildcard_match("ab*ba", "aba"));
        assert!(wildcard_match("exact", "exact"));
        assert!(!wildcard_match("exact", "exactly"));
    }

    #[test]
    fn glob_lists() {
        assert!(matches_glob_list("docs/README.md", "*.md; *.txt"));
        assert!(!matches_glob_list("src/App.php", "*.md; *.txt"));
        assert!(matches_glob_list("a/b.TXT", "?.txt"));
        assert!(matches_glob_list("legacy/latex/x.tpl", "legacy/latex/*"));
        assert!(!matches_glob_list("other/latex/x.tpl", "legacy/latex/*"));
        assert!(!matches_glob_list("x", " ; "));
    }
}

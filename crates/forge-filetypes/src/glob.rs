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

/// Expand `{a,b}` alternatives (nested too): `{**/*,*}.{js,ts}` gives `**/*.js`, `**/*.ts`,
/// `*.js` and `*.ts`. Unbalanced braces stay literal.
pub fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    let mut commas = Vec::new();
    for (i, c) in pattern[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            ',' if depth == 1 => commas.push(open + i),
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    let mut bounds = vec![open];
    bounds.extend(&commas);
    bounds.push(close);
    let mut out = Vec::new();
    for w in bounds.windows(2) {
        let alt = &pattern[w[0] + 1..w[1]];
        out.extend(expand_braces(&format!("{head}{alt}{tail}")));
    }
    out
}

/// Match a path against a `;`-separated list of simple globs (`*`, `?`, `{a,b}`), e.g.
/// `*.md; *.txt` or PHPStorm's `{**/*,*}.{js,ts}`. Patterns with a `/` match the whole path
/// (`*` and `**` cross folders), others the file name.
pub fn matches_glob_list(path: &str, patterns: &str) -> bool {
    let lower = |s: &str| -> Vec<char> { s.chars().flat_map(char::to_lowercase).collect() };
    let name = path.rsplit('/').next().unwrap_or(path);
    patterns
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .flat_map(expand_braces)
        .any(|p| {
            // `**/` also matches no folder at all.
            let p = p
                .strip_prefix("**/")
                .filter(|_| !path.contains('/'))
                .unwrap_or(&p);
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

    #[test]
    fn braces_and_globstars() {
        let eslint = "{**/*,*}.{js,ts,jsx,tsx,cjs,mjs,vue,html}";
        assert!(matches_glob_list("src/App.tsx", eslint));
        assert!(matches_glob_list("eslint.config.mjs", eslint));
        assert!(!matches_glob_list("src/app.css", eslint));
        assert!(matches_glob_list("a.scss", "**/*.{css,scss}"));
        assert_eq!(expand_braces("a{b,{c,d}}e"), vec!["abe", "ace", "ade"]);
        assert_eq!(expand_braces("x{y"), vec!["x{y"]);
    }
}

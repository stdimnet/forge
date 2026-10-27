//! Just enough about each language to find comments, strings and statements: comment and
//! string syntax, how statements are delimited and how a suppression comment is written.
//! Picked by file name; unknown files use C-style line comments.

/// How statements are delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Braces and semicolons; `semicolons: false` also ends statements at line breaks
    /// (JavaScript, Go, Kotlin, ...).
    Braces { semicolons: bool },
    /// Indentation (Python, YAML).
    Indent,
    /// One statement per line (markup, shell, Lua, config files).
    Line,
}

/// How a suppression comment is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    /// `/** @noinspection Id */` (PHP).
    PhpDoc,
    /// `// noinspection Id`.
    Line(&'static str),
    /// `/* noinspection Id */`, `<!-- noinspection Id -->`.
    Wrap(&'static str, &'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Lang {
    pub id: &'static str,
    pub line: &'static [&'static str],
    pub block: &'static [(&'static str, &'static str)],
    pub quotes: &'static [u8],
    /// PHP: code only between `<?php` and `?>`, `#[` is an attribute, heredocs.
    pub php: bool,
    /// Rust: `'a` is a lifetime unless it closes like a char literal.
    pub rust: bool,
    /// Python: triple-quoted strings.
    pub triple: bool,
    pub mode: Mode,
    pub insert: Insert,
}

const C_LINE: &[&str] = &["//"];
const C_BLOCK: &[(&str, &str)] = &[("/*", "*/")];
const HASH: &[&str] = &["#"];
const NONE: &[&str] = &[];
const NO_BLOCK: &[(&str, &str)] = &[];

const fn lang(id: &'static str, mode: Mode, insert: Insert) -> Lang {
    Lang {
        id,
        line: C_LINE,
        block: C_BLOCK,
        quotes: b"'\"",
        php: false,
        rust: false,
        triple: false,
        mode,
        insert,
    }
}

const SEMI: Mode = Mode::Braces { semicolons: true };
const LOOSE: Mode = Mode::Braces { semicolons: false };

/// The language of a file.
pub fn for_path(path: &str) -> Lang {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    if name.ends_with(".blade.php") {
        return Lang {
            line: NONE,
            block: &[("{{--", "--}}"), ("<!--", "-->")],
            ..lang("blade", Mode::Line, Insert::Wrap("{{-- ", " --}}"))
        };
    }
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "php" | "phtml" | "inc" | "module" => Lang {
            line: &["//", "#"],
            php: true,
            ..lang("php", SEMI, Insert::PhpDoc)
        },
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" => Lang {
            quotes: b"'\"`",
            ..lang("javascript", LOOSE, Insert::Line("// "))
        },
        "py" | "pyi" | "pyw" => Lang {
            line: HASH,
            block: NO_BLOCK,
            triple: true,
            ..lang("python", Mode::Indent, Insert::Line("# "))
        },
        "rs" => Lang {
            rust: true,
            ..lang("rust", LOOSE, Insert::Line("// "))
        },
        "go" => Lang {
            quotes: b"'\"`",
            ..lang("go", LOOSE, Insert::Line("// "))
        },
        "java" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "cs" | "m" | "mm" => {
            lang("c", SEMI, Insert::Line("// "))
        }
        "kt" | "kts" | "scala" | "sc" | "groovy" | "gradle" | "swift" | "dart" => Lang {
            quotes: b"'\"`",
            ..lang("jvm", LOOSE, Insert::Line("// "))
        },
        "css" => Lang {
            line: NONE,
            ..lang("css", SEMI, Insert::Wrap("/* ", " */"))
        },
        "scss" | "less" => lang("css", SEMI, Insert::Line("// ")),
        "html" | "htm" | "xml" | "svg" | "vue" | "xhtml" | "xsd" | "xsl" => Lang {
            line: NONE,
            block: &[("<!--", "-->")],
            quotes: b"",
            ..lang("markup", Mode::Line, Insert::Wrap("<!-- ", " -->"))
        },
        "twig" => Lang {
            line: NONE,
            block: &[("{#", "#}"), ("<!--", "-->")],
            quotes: b"",
            ..lang("twig", Mode::Line, Insert::Wrap("{# ", " #}"))
        },
        "yaml" | "yml" => Lang {
            line: HASH,
            block: NO_BLOCK,
            ..lang("yaml", Mode::Indent, Insert::Line("# "))
        },
        "sh" | "bash" | "zsh" | "rb" | "toml" | "pl" | "r" | "cmake" | "properties" | "conf"
        | "mk" | "nix" | "tf" | "ps1" => Lang {
            line: HASH,
            block: NO_BLOCK,
            ..lang("hash", Mode::Line, Insert::Line("# "))
        },
        "ini" => Lang {
            line: &[";", "#"],
            block: NO_BLOCK,
            ..lang("ini", Mode::Line, Insert::Line("; "))
        },
        "lua" => Lang {
            line: &["--"],
            block: &[("--[[", "]]")],
            ..lang("lua", Mode::Line, Insert::Line("-- "))
        },
        "sql" | "dql" => Lang {
            line: &["--", "#"],
            ..lang("sql", SEMI, Insert::Line("-- "))
        },
        "md" | "markdown" | "txt" | "json" | "csv" => Lang {
            line: NONE,
            block: NO_BLOCK,
            quotes: b"",
            ..lang("text", Mode::Line, Insert::Line("// "))
        },
        _ if name == "dockerfile" || name == "makefile" || name.starts_with(".env") => Lang {
            line: HASH,
            block: NO_BLOCK,
            ..lang("hash", Mode::Line, Insert::Line("# "))
        },
        _ => lang("c", LOOSE, Insert::Line("// ")),
    }
}

/// What a byte of the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Code,
    Comment,
    String,
    /// Outside the language's code (HTML around PHP, the `<?php` tags).
    Outside,
}

/// The text split into code, comments and strings.
#[derive(Debug, Clone)]
pub struct Lexed {
    pub kinds: Vec<Kind>,
    /// Comment ranges (bytes).
    pub comments: Vec<(usize, usize)>,
}

impl Lexed {
    pub fn is_code(&self, i: usize) -> bool {
        self.kinds.get(i) == Some(&Kind::Code)
    }
}

fn starts(b: &[u8], i: usize, s: &str) -> bool {
    b[i..].starts_with(s.as_bytes())
}

/// Split `text` into code, comments and strings.
pub fn lex(text: &str, lang: &Lang) -> Lexed {
    let b = text.as_bytes();
    let n = b.len();
    let mut kinds = vec![Kind::Code; n];
    let mut comments = Vec::new();
    let mut i = 0;
    let mut outside = lang.php;
    let fill = |kinds: &mut Vec<Kind>, from: usize, to: usize, k: Kind| {
        for x in &mut kinds[from..to.min(n)] {
            *x = k;
        }
    };
    while i < n {
        if outside {
            // HTML until `<?php` / `<?=` / `<?`.
            match text[i..].find("<?") {
                Some(p) => {
                    let start = i + p;
                    let mut end = start + 2;
                    if starts(b, end, "php") {
                        end += 3;
                    } else if starts(b, end, "=") {
                        end += 1;
                    }
                    fill(&mut kinds, i, end, Kind::Outside);
                    i = end;
                    outside = false;
                }
                None => {
                    fill(&mut kinds, i, n, Kind::Outside);
                    break;
                }
            }
            continue;
        }
        if lang.php && starts(b, i, "?>") {
            fill(&mut kinds, i, i + 2, Kind::Outside);
            i += 2;
            outside = true;
            continue;
        }
        if let Some((open, close)) = lang.block.iter().find(|(o, _)| starts(b, i, o)) {
            let end = text[i + open.len()..]
                .find(close)
                .map_or(n, |p| i + open.len() + p + close.len());
            fill(&mut kinds, i, end, Kind::Comment);
            comments.push((i, end));
            i = end;
            continue;
        }
        if let Some(_l) = lang
            .line
            .iter()
            .find(|l| starts(b, i, l) && !(lang.php && **l == "#" && starts(b, i, "#[")))
        {
            let mut end = text[i..].find('\n').map_or(n, |p| i + p);
            if lang.php {
                if let Some(p) = text[i..end].find("?>") {
                    end = i + p;
                }
            }
            fill(&mut kinds, i, end, Kind::Comment);
            comments.push((i, end));
            i = end;
            continue;
        }
        if lang.php && starts(b, i, "<<<") {
            if let Some(end) = heredoc_end(text, i) {
                fill(&mut kinds, i, end, Kind::String);
                i = end;
                continue;
            }
        }
        let c = b[i];
        if lang.quotes.contains(&c) {
            if lang.rust && c == b'\'' && !rust_char(b, i) {
                i += 1;
                continue;
            }
            let end = if lang.triple && (starts(b, i, "\"\"\"") || starts(b, i, "'''")) {
                let q = &text[i..i + 3];
                text[i + 3..].find(q).map_or(n, |p| i + 3 + p + 3)
            } else {
                string_end(b, i)
            };
            fill(&mut kinds, i, end, Kind::String);
            i = end;
            continue;
        }
        i += 1;
    }
    Lexed { kinds, comments }
}

/// The end of a quoted string starting at `i` (backslash escapes; an unclosed `'`/`"`
/// ends at the line break).
fn string_end(b: &[u8], i: usize) -> usize {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == q => return j + 1,
            b'\n' if q != b'`' => return j,
            _ => j += 1,
        }
    }
    b.len()
}

/// `'x'` or `'\n'` (a char literal), not a lifetime `'a`.
fn rust_char(b: &[u8], i: usize) -> bool {
    match b.get(i + 1) {
        Some(b'\\') => true,
        Some(_) => {
            // One (possibly multi-byte) character, then the closing quote.
            let rest = &b[i + 1..];
            let len = std::str::from_utf8(&rest[..rest.len().min(4)])
                .ok()
                .or_else(|| std::str::from_utf8(&rest[..rest.len().min(3)]).ok())
                .or_else(|| std::str::from_utf8(&rest[..rest.len().min(2)]).ok())
                .or_else(|| std::str::from_utf8(&rest[..1]).ok())
                .and_then(|s| s.chars().next())
                .map_or(1, char::len_utf8);
            b.get(i + 1 + len) == Some(&b'\'')
        }
        None => false,
    }
}

/// The end of a PHP heredoc / nowdoc starting at `i` (`<<<EOT`, `<<<'EOT'`).
fn heredoc_end(text: &str, i: usize) -> Option<usize> {
    let rest = &text[i + 3..];
    let line_end = rest.find('\n')?;
    let label = rest[..line_end].trim().trim_matches(['\'', '"']);
    if label.is_empty() || !label.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let mut pos = i + 3 + line_end + 1;
    for line in text[pos..].split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with(label)
            && !t[label.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            let lead = line.len() - t.len();
            return Some(pos + lead + label.len());
        }
        pos += line.len();
    }
    Some(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(path: &str, text: &str) -> String {
        let l = lex(text, &for_path(path));
        l.kinds
            .iter()
            .map(|k| match k {
                Kind::Code => 'c',
                Kind::Comment => '/',
                Kind::String => 's',
                Kind::Outside => '_',
            })
            .collect()
    }

    #[test]
    fn php_tags_attributes_and_heredocs() {
        let k = kinds("a.php", "<?php $a = 'x'; # c\n#[Attr] ?>html");
        assert!(k.starts_with("_____"), "{k}");
        assert_eq!(&k[11..14], "sss");
        assert_eq!(&k[16..19], "///");
        assert_eq!(&k[20..27], "ccccccc", "attribute is code: {k}");
        assert!(k.ends_with("______"));
        let h = "<?php $x = <<<EOT\n { }\n  EOT;\n$y;";
        let k = kinds("a.php", h);
        let semi = h.find("EOT;").unwrap() + 3;
        assert_eq!(&k[semi..semi + 1], "c");
        assert_eq!(&k[h.find('{').unwrap()..h.find('{').unwrap() + 1], "s");
    }

    #[test]
    fn strings_comments_and_lifetimes() {
        let k = kinds("a.rs", "fn f<'a>(x: &'a str) -> char { 'x' } // c");
        assert!(!k.contains('s') || k.matches('s').count() == 3, "{k}");
        let k = kinds("a.py", "x = '''a\n{'''  # c");
        assert_eq!(k.matches('s').count(), 9);
        assert!(k.ends_with("///"));
        let k = kinds("a.ts", "let s = `a ${b}`; /* x */");
        assert!(k.contains("ssssssss") && !k.contains("sssssssss"), "{k}");
        assert_eq!(for_path("x/View.blade.php").id, "blade");
        assert_eq!(for_path("Dockerfile").id, "hash");
    }
}

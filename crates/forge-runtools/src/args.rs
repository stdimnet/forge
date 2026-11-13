//! Splitting and quoting of argument strings as typed into run configuration fields
//! ("Arguments", "Interpreter options", "Test runner options").

/// Split a command-line string into arguments. Supports single and double quotes and
/// backslash escapes outside single quotes, like a POSIX shell (without expansion).
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\')) => {
                            cur.push(chars.next().expect("peeked"));
                        }
                        c => cur.push(c),
                    }
                }
            }
            '\\' if matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == '"' || *c == '\'' || *c == '\\') =>
            {
                in_word = true;
                cur.push(chars.next().expect("peeked"));
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

/// Quote an argument for display (the command line shown at the top of a run).
pub fn quote_arg(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,\\".contains(c));
    if safe {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_and_quoted_words() {
        assert_eq!(split_args("  a  b c "), vec!["a", "b", "c"]);
        assert_eq!(
            split_args(r#"--filter "test with space" 'x y' z\ w"#),
            vec!["--filter", "test with space", "x y", "z w"]
        );
        assert_eq!(split_args(r#""say \"hi\"""#), vec![r#"say "hi""#]);
        assert_eq!(split_args(r"C:\path\file.php"), vec![r"C:\path\file.php"]);
        assert_eq!(split_args("''"), vec![""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(quote_arg("vendor/bin/phpunit"), "vendor/bin/phpunit");
        assert_eq!(quote_arg("a b"), "'a b'");
        assert_eq!(quote_arg("it's"), r"'it'\''s'");
        assert_eq!(quote_arg(""), "''");
    }
}

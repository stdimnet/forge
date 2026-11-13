//! A small, tolerant C# tokenizer for the ASP.NET Core, EF Core and Aspire models: names,
//! punctuation and string literals with their values (regular, verbatim, raw and
//! interpolated strings), each with its 0-based line. Comments and preprocessor lines are
//! skipped. It does not parse C#; the models walk the tokens.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Punct(char),
    /// A string literal's value (interpolations kept as written).
    Str(String),
    /// A number or character literal.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

impl Token {
    pub fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_ident(&self, name: &str) -> bool {
        self.ident() == Some(name)
    }

    pub fn is(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }

    pub fn string(&self) -> Option<&str> {
        match &self.tok {
            Tok::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    let mut line_start = true;
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' && line_start {
            // Preprocessor directive.
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        line_start = false;
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i < n && !(chars[i] == '*' && i + 1 < n && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        let mut j = i;
        let mut verbatim = false;
        while j < n && j < i + 3 && (chars[j] == '$' || chars[j] == '@') {
            verbatim |= chars[j] == '@';
            j += 1;
        }
        if j < n && chars[j] == '"' && (j > i || c == '"') {
            let start_line = line;
            let mut quotes = 0;
            while j + quotes < n && chars[j + quotes] == '"' {
                quotes += 1;
            }
            let mut value = String::new();
            if quotes >= 3 {
                let mut k = j + quotes;
                while k < n {
                    if chars[k] == '"' {
                        let mut q = 0;
                        while k + q < n && chars[k + q] == '"' {
                            q += 1;
                        }
                        if q >= quotes {
                            k += q;
                            break;
                        }
                        value.extend(&chars[k..k + q]);
                        k += q;
                        continue;
                    }
                    if chars[k] == '\n' {
                        line += 1;
                    }
                    value.push(chars[k]);
                    k += 1;
                }
                i = k;
                value = value.trim().to_string();
            } else if quotes == 2 && !(verbatim && j + 2 < n && chars[j + 2] == '"') {
                i = j + 2;
            } else {
                let mut k = j + 1;
                while k < n {
                    let ch = chars[k];
                    if verbatim {
                        if ch == '"' {
                            if k + 1 < n && chars[k + 1] == '"' {
                                value.push('"');
                                k += 2;
                                continue;
                            }
                            break;
                        }
                        if ch == '\n' {
                            line += 1;
                        }
                    } else {
                        if ch == '\\' && k + 1 < n {
                            value.push(match chars[k + 1] {
                                'n' => '\n',
                                't' => '\t',
                                other => other,
                            });
                            k += 2;
                            continue;
                        }
                        if ch == '"' || ch == '\n' {
                            break;
                        }
                    }
                    value.push(ch);
                    k += 1;
                }
                i = k + 1;
            }
            out.push(Token {
                tok: Tok::Str(value),
                line: start_line,
            });
            continue;
        }
        if c == '\'' {
            let mut k = i + 1;
            if k < n && chars[k] == '\\' {
                k += 2;
            } else {
                k += 1;
            }
            while k < n && k < i + 10 && chars[k] != '\'' && chars[k] != '\n' {
                k += 1;
            }
            out.push(Token {
                tok: Tok::Other,
                line,
            });
            i = if k < n && chars[k] == '\'' {
                k + 1
            } else {
                i + 1
            };
            continue;
        }
        if c.is_ascii_digit() {
            let mut k = i;
            while k < n && (chars[k].is_alphanumeric() || chars[k] == '.' || chars[k] == '_') {
                k += 1;
            }
            out.push(Token {
                tok: Tok::Other,
                line,
            });
            i = k;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut k = i;
            while k < n && (chars[k].is_alphanumeric() || chars[k] == '_') {
                k += 1;
            }
            out.push(Token {
                tok: Tok::Ident(chars[i..k].iter().collect()),
                line,
            });
            i = k;
            continue;
        }
        if c == '@' && i + 1 < n && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_') {
            i += 1;
            continue;
        }
        out.push(Token {
            tok: Tok::Punct(c),
            line,
        });
        i += 1;
    }
    out
}

/// The index after the bracket that closes the one at `open` (`(`, `[`, `{`, `<`).
pub fn matching(tokens: &[Token], open: usize) -> usize {
    let (o, c) = match tokens.get(open).map(|t| &t.tok) {
        Some(Tok::Punct('(')) => ('(', ')'),
        Some(Tok::Punct('[')) => ('[', ']'),
        Some(Tok::Punct('{')) => ('{', '}'),
        Some(Tok::Punct('<')) => ('<', '>'),
        _ => return open + 1,
    };
    let mut depth = 0i32;
    let mut i = open;
    while i < tokens.len() {
        if tokens[i].is(o) {
            depth += 1;
        } else if tokens[i].is(c) {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        } else if o == '<' && (tokens[i].is(';') || tokens[i].is('{') || tokens[i].is(')')) {
            // Not a generic argument list after all.
            return open + 1;
        }
        i += 1;
    }
    tokens.len()
}

/// Index ranges of the arguments between `open` (a bracket) and its closing bracket,
/// split at top-level commas.
pub fn argument_ranges(tokens: &[Token], open: usize) -> Vec<(usize, usize)> {
    let end = matching(tokens, open).saturating_sub(1);
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = open + 1;
    let mut i = open + 1;
    while i < end {
        let t = &tokens[i];
        if t.is('(') || t.is('[') || t.is('{') {
            depth += 1;
        } else if t.is(')') || t.is(']') || t.is('}') {
            depth -= 1;
        } else if t.is('<') {
            let m = matching(tokens, i);
            if m > i + 1 && m <= end {
                i = m;
                continue;
            }
        } else if t.is(',') && depth == 0 {
            out.push((start, i));
            start = i + 1;
        }
        i += 1;
    }
    if start < end {
        out.push((start, end));
    }
    out
}

/// The arguments between `open` (a bracket) and its closing bracket.
pub fn arguments(tokens: &[Token], open: usize) -> Vec<&[Token]> {
    argument_ranges(tokens, open)
        .into_iter()
        .map(|(a, b)| &tokens[a..b])
        .collect()
}

/// Tokens written back as text (types and expressions in the models).
pub fn text(tokens: &[Token]) -> String {
    let mut s = String::new();
    for (i, t) in tokens.iter().enumerate() {
        let piece = match &t.tok {
            Tok::Ident(x) => x.clone(),
            Tok::Punct(c) => c.to_string(),
            Tok::Str(x) => format!("\"{x}\""),
            Tok::Other => "…".into(),
        };
        let word = matches!(t.tok, Tok::Ident(_));
        let prev_word = i > 0 && matches!(tokens[i - 1].tok, Tok::Ident(_));
        if word && prev_word || t.is('=') || i > 0 && tokens[i - 1].is(',') {
            s.push(' ');
        }
        s.push_str(&piece);
        if t.is('=') {
            s.push(' ');
        }
    }
    s.trim().to_string()
}

/// One attribute of an `[A, B(…)]` list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribute {
    /// Without namespace and `Attribute` suffix.
    pub name: String,
    /// String arguments in order.
    pub strings: Vec<String>,
    /// `Name = "value"` arguments.
    pub named: Vec<(String, String)>,
    /// `typeof(X)` arguments.
    pub types: Vec<String>,
}

/// Parse the attribute list starting at `open` (`[`); returns the attributes and the
/// index after `]`.
pub fn attribute_list(tokens: &[Token], open: usize) -> (Vec<Attribute>, usize) {
    let end = matching(tokens, open);
    let mut out = Vec::new();
    let inner = &tokens[open + 1..end.saturating_sub(1).max(open + 1)];
    // `[return: …]`, `[assembly: …]`.
    let inner = match inner.iter().position(|t| t.is(':')) {
        Some(p) if p == 1 && inner[0].ident().is_some() => &inner[2..],
        _ => inner,
    };
    let mut i = 0;
    while i < inner.len() {
        let mut name = String::new();
        while i < inner.len() && !inner[i].is('(') && !inner[i].is(',') {
            if let Some(id) = inner[i].ident() {
                name = id.to_string();
            }
            i += 1;
        }
        let mut a = Attribute {
            name: name
                .strip_suffix("Attribute")
                .map(str::to_string)
                .unwrap_or(name),
            ..Attribute::default()
        };
        if i < inner.len() && inner[i].is('(') {
            for arg in arguments(inner, i) {
                match arg {
                    [t] if t.string().is_some() => a.strings.push(t.string().unwrap().into()),
                    [n, eq, v] if eq.is('=') && n.ident().is_some() => {
                        if let Some(s) = v.string() {
                            a.named.push((n.ident().unwrap().into(), s.into()));
                        } else if let Some(s) = v.ident() {
                            a.named.push((n.ident().unwrap().into(), s.into()));
                        }
                    }
                    [k, ..] if k.is_ident("typeof") => {
                        let ty = arg
                            .iter()
                            .skip(2)
                            .take_while(|t| !t.is(')'))
                            .filter_map(|t| t.ident())
                            .last()
                            .unwrap_or("")
                            .to_string();
                        a.types.push(ty);
                    }
                    _ => {
                        // `new[] { "GET", "POST" }` and other expressions: keep the strings.
                        a.strings
                            .extend(arg.iter().filter_map(|t| t.string().map(str::to_string)));
                    }
                }
            }
            i = matching(inner, i);
        }
        if !a.name.is_empty() {
            out.push(a);
        }
        while i < inner.len() && !inner[i].is(',') {
            i += 1;
        }
        i += 1;
    }
    (out, end)
}

pub const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "static",
    "virtual",
    "override",
    "abstract",
    "sealed",
    "async",
    "partial",
    "readonly",
    "new",
    "extern",
    "unsafe",
    "required",
    "file",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_comments() {
        let t = tokenize(
            "// x \"no\"\nvar a = \"/api\"; /* \"no\" */ var b = @\"c:\\x\"\"y\"; var r = \"\"\"\n  raw\n  \"\"\";\n#if DEBUG\nx",
        );
        let strings: Vec<&str> = t.iter().filter_map(Token::string).collect();
        assert_eq!(strings, ["/api", "c:\\x\"y", "raw"]);
        assert_eq!(t.last().unwrap().line, 5);
        assert!(!t.iter().any(|t| t.is_ident("DEBUG")));
    }

    #[test]
    fn attributes() {
        let t = tokenize(
            r#"[HttpGet("{id:int}", Name = "GetBook"), ProducesResponseType(typeof(Book), 200)]"#,
        );
        let (a, end) = attribute_list(&t, 0);
        assert_eq!(end, t.len());
        assert_eq!(a[0].name, "HttpGet");
        assert_eq!(a[0].strings, ["{id:int}"]);
        assert_eq!(a[0].named, [("Name".to_string(), "GetBook".to_string())]);
        assert_eq!(a[1].types, ["Book"]);
    }

    #[test]
    fn splits_arguments() {
        let t = tokenize(r#"Map("/a", (int id, Dictionary<string, int> d) => Results.Ok(id))"#);
        let args = arguments(&t, 1);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0][0].string(), Some("/a"));
    }
}

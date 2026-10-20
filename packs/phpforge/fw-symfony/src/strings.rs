//! String literal helpers shared by completion, navigation and the gutter: find the quoted
//! string around a cursor and the call it is an argument of (`generateUrl('|')`).

/// A quoted string literal in a line of source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringAt {
    pub quote: char,
    /// Byte offset of the opening quote.
    pub open: usize,
    /// Byte range of the content (without quotes).
    pub start: usize,
    pub end: usize,
    /// Content up to the cursor.
    pub prefix: String,
    /// Whole content.
    pub value: String,
    /// Whether the closing quote exists on the line.
    pub closed: bool,
}

/// The single- or double-quoted string containing `offset` (between the quotes, or right
/// after an unterminated opening quote). Only looks at the current line.
pub fn string_at(text: &str, offset: usize) -> Option<StringAt> {
    string_at_from(text, offset, 0)
}

/// Like [`string_at`], ignoring quotes before `scan_from` (e.g. the HTML attribute around a
/// Twig expression).
pub fn string_at_from(text: &str, offset: usize, scan_from: usize) -> Option<StringAt> {
    let offset = offset.min(text.len());
    let line_start = text[..offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0)
        .max(scan_from.min(offset));
    let line_end = text[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' || c == b'"' {
            let open = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != c {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let closed = j < bytes.len();
            let content_end = j.min(bytes.len());
            let abs_open = line_start + open;
            let abs_start = abs_open + 1;
            let abs_end = line_start + content_end;
            if offset > abs_open && offset <= abs_end {
                return Some(StringAt {
                    quote: c as char,
                    open: abs_open,
                    start: abs_start,
                    end: abs_end,
                    prefix: text[abs_start..offset].to_string(),
                    value: text[abs_start..abs_end].to_string(),
                    closed,
                });
            }
            i = j + 1;
            continue;
        }
        if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            break;
        }
        if c == b'#' && bytes.get(i + 1) != Some(&b'[') && !line[..i].contains("{{") {
            // `#` comments in PHP and YAML, but not `#[Attribute]`.
            break;
        }
        i += 1;
    }
    None
}

/// The call whose argument list a string starting at `open` belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    /// Function or method name (`generateUrl`, `render`, `path`, `Autowire`).
    pub function: String,
    /// `->` or `::` method call (vs. a plain function).
    pub is_method: bool,
    /// 0-based argument index.
    pub arg: usize,
    /// Named argument (`service:` in `#[Autowire(service: '...')]`).
    pub named: Option<String>,
}

/// Walk back from the string's opening quote over the argument list to the call name.
pub fn call_context(text: &str, open: usize) -> Option<CallContext> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut arg = 0usize;
    let mut i = open;
    let mut named: Option<String> = None;
    let mut first = true;
    let floor = open.saturating_sub(2000);
    // Named argument right before the string: `service: '`.
    {
        let before = text[..open].trim_end();
        if let Some(head) = before.strip_suffix(':') {
            if !head.ends_with(':') {
                let name: String = head
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                if !name.is_empty() {
                    named = Some(name);
                }
            }
        }
    }
    while i > floor {
        i -= 1;
        let c = bytes[i];
        match c {
            b'\'' | b'"' if !first => {
                // Skip over an earlier string argument.
                let q = c;
                while i > floor {
                    i -= 1;
                    if bytes[i] == q && (i == 0 || bytes[i - 1] != b'\\') {
                        break;
                    }
                }
            }
            b')' | b']' | b'}' => depth += 1,
            b'[' | b'{' if depth > 0 => depth -= 1,
            b'(' if depth > 0 => depth -= 1,
            b'[' | b'{' => {
                // Inside an array literal argument (`['id' => ...]`): not a direct argument.
                return None;
            }
            b',' if depth == 0 => arg += 1,
            b';' if depth == 0 => return None,
            b'(' => {
                let head = &text[..i];
                let trimmed = head.trim_end();
                let name: String = trimmed
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                if name.is_empty() {
                    return None;
                }
                let rest = trimmed[..trimmed.len() - name.len()].trim_end();
                let is_method =
                    rest.ends_with("->") || rest.ends_with("::") || rest.ends_with("?->");
                let function = name.rsplit('\\').next().unwrap_or(&name).to_string();
                return Some(CallContext {
                    function,
                    is_method,
                    arg,
                    named,
                });
            }
            _ => {}
        }
        first = false;
    }
    None
}

/// Identifier-like word ending at `offset` (letters, digits, `_`, `.`, `:`, `\`).
pub fn word_before(text: &str, offset: usize, extra: &[char]) -> (usize, String) {
    let offset = offset.min(text.len());
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || extra.contains(c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(offset);
    (start, text[start..offset].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('|').unwrap();
        (src.replacen('|', "", 1), pos)
    }

    #[test]
    fn finds_string_and_call() {
        let (src, pos) = at("return $this->redirectToRoute('order_sh|ow', ['id' => 1]);");
        let s = string_at(&src, pos).unwrap();
        assert_eq!(s.prefix, "order_sh");
        assert_eq!(s.value, "order_show");
        let c = call_context(&src, s.open).unwrap();
        assert_eq!(c.function, "redirectToRoute");
        assert!(c.is_method);
        assert_eq!(c.arg, 0);
    }

    #[test]
    fn second_argument_and_named() {
        let (src, pos) = at("$qb->from(Order::class, 'o|')");
        let s = string_at(&src, pos).unwrap();
        assert_eq!(call_context(&src, s.open).unwrap().arg, 1);

        let (src, pos) = at("#[Autowire(service: 'app.ma|')]");
        let s = string_at(&src, pos).unwrap();
        let c = call_context(&src, s.open).unwrap();
        assert_eq!(c.function, "Autowire");
        assert_eq!(c.named.as_deref(), Some("service"));
    }

    #[test]
    fn unterminated_string() {
        let (src, pos) = at("{{ path('ord| }}");
        let s = string_at(&src, pos).unwrap();
        assert_eq!(s.prefix, "ord");
        assert_eq!(call_context(&src, s.open).unwrap().function, "path");
    }

    #[test]
    fn array_values_are_not_arguments() {
        let (src, pos) = at("$this->render('x', ['a' => 'b|'])");
        let s = string_at(&src, pos).unwrap();
        assert!(call_context(&src, s.open).is_none());
    }

    #[test]
    fn word_before_cursor() {
        assert_eq!(word_before("{{ x|upp", 8, &[]), (5, "upp".into()));
    }
}

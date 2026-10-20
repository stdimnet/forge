//! What the caret is on, found lexically so it works while the code is being typed
//! (`route('orders.|` has no closing quote yet): a string argument of a call, a property
//! access, a Blade component tag or a Blade directive.

/// A string literal that is an argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringArg {
    /// Called function or method: `route`, `view`, `where`, `@include`.
    pub func: String,
    /// What precedes `->`/`::` (`User::query()`, `$this->orders()`, `redirect()`), if any.
    pub receiver: Option<String>,
    /// `true` for `Foo::bar(`, `false` for `$x->bar(` and plain function calls.
    pub is_static: bool,
    /// 0-based argument position.
    pub arg_index: usize,
    /// Byte range of the string content (without quotes); `end` stops before the closing
    /// quote, or at the line end while it is still open.
    pub start: usize,
    pub end: usize,
    /// Content between `start` and the caret.
    pub prefix: String,
    /// The whole content.
    pub value: String,
}

/// `[OrderController::class, 'sh|']`: the method string of a controller action array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMethod {
    pub class: String,
    pub start: usize,
    pub end: usize,
    pub prefix: String,
    pub value: String,
}

/// `$order->sta|`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyAccess {
    /// The variable (`$order`) or `$this`.
    pub variable: String,
    /// Further property names between the variable and the caret (`$order->user->` → `[user]`).
    pub path: Vec<String>,
    pub start: usize,
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    String(StringArg),
    ActionMethod(ActionMethod),
    Property(PropertyAccess),
    /// `<x-ale|` or `<livewire:cou|`: the tag name from `start` (after `<`).
    ComponentTag {
        start: usize,
        end: usize,
        prefix: String,
        value: String,
    },
    /// `@for|` in Blade: from `start` (at `@`).
    Directive {
        start: usize,
        prefix: String,
    },
    /// `User::act|`: a static member of a class (local scopes of models).
    Static {
        class: String,
        start: usize,
        prefix: String,
    },
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Start of the current line.
fn line_start(src: &str, offset: usize) -> usize {
    src[..offset].rfind('\n').map_or(0, |i| i + 1)
}

/// Find the opening quote of the string containing `offset` on its line.
fn string_around(src: &str, offset: usize) -> Option<(usize, u8)> {
    let bytes = src.as_bytes();
    let mut i = line_start(src, offset);
    let mut open: Option<(usize, u8)> = None;
    while i < offset {
        let b = bytes[i];
        match open {
            Some((_, q)) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    open = None;
                }
            }
            None => {
                if b == b'\'' || b == b'"' {
                    open = Some((i, b));
                } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                    return None;
                }
            }
        }
        i += 1;
    }
    open
}

/// The end of a string's content: before the closing quote, or the line end.
fn string_end(src: &str, from: usize, quote: u8) -> usize {
    let bytes = src.as_bytes();
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'\n' {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i;
        }
        i += 1;
    }
    i.min(bytes.len())
}

/// From just before a string, walk back over earlier arguments to the call's `(`.
/// Returns (position of `(`, argument index).
fn enclosing_paren(src: &str, before: usize) -> Option<(usize, usize)> {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut commas = 0;
    let mut i = before;
    let limit = before.saturating_sub(2000);
    while i > limit {
        i -= 1;
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                if depth == 0 {
                    return (bytes[i] == b'(').then_some((i, commas));
                }
                depth -= 1;
            }
            b',' if depth == 0 => commas += 1,
            b';' if depth == 0 => return None,
            q @ (b'\'' | b'"') => {
                // Skip a string literal backwards.
                while i > limit {
                    i -= 1;
                    if bytes[i] == q && (i == 0 || bytes[i - 1] != b'\\') {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Identifier ending at `end` (exclusive), including `@` for Blade directives and `\`
/// for namespaced names.
fn ident_before(src: &str, end: usize) -> (usize, &str) {
    let bytes = src.as_bytes();
    let mut s = end;
    while s > 0 && (is_ident(bytes[s - 1]) || bytes[s - 1] == b'\\') {
        s -= 1;
    }
    if s > 0 && bytes[s - 1] == b'@' {
        s -= 1;
    }
    (s, &src[s..end])
}

/// The receiver expression ending at `end` (exclusive): a variable, a class name, or a
/// call chain like `User::query()->where('a', 1)`.
fn receiver_before(src: &str, end: usize) -> String {
    let bytes = src.as_bytes();
    let mut i = end;
    let limit = end.saturating_sub(1000);
    loop {
        while i > limit && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        if i > limit && bytes[i - 1] == b')' {
            // Skip balanced parentheses.
            let mut depth = 0;
            while i > limit {
                i -= 1;
                match bytes[i] {
                    b')' => depth += 1,
                    b'(' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        let (s, _) = ident_before(src, i);
        i = s;
        if i > limit && bytes[i - 1] == b'$' {
            i -= 1;
        }
        // Continue over `->`, `?->` and `::`.
        if i >= 2 && &src[i - 2..i] == "->" {
            i -= 2;
            if i >= 1 && bytes[i - 1] == b'?' {
                i -= 1;
            }
            continue;
        }
        if i >= 2 && &src[i - 2..i] == "::" {
            i -= 2;
            continue;
        }
        break;
    }
    src[i..end].trim().to_string()
}

pub fn context_at(src: &str, offset: usize) -> Option<Context> {
    let offset = offset.min(src.len());
    if !src.is_char_boundary(offset) {
        return None;
    }
    if let Some((quote_at, quote)) = string_around(src, offset) {
        let start = quote_at + 1;
        let end = string_end(src, offset, quote).max(offset);
        let prefix = src[start..offset].to_string();
        let value = src[start..end].to_string();
        let bytes = src.as_bytes();
        // Action array: [Controller::class, 'method']
        let mut j = quote_at;
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
            j -= 1;
        }
        if j > 0 && bytes[j - 1] == b',' {
            let before = src[..j - 1].trim_end();
            if let Some(class_part) = before.strip_suffix("::class") {
                let (s, class) = ident_before(class_part, class_part.len());
                if src[..s].trim_end().ends_with('[') && !class.is_empty() {
                    return Some(Context::ActionMethod(ActionMethod {
                        class: class.to_string(),
                        start,
                        end,
                        prefix,
                        value,
                    }));
                }
            }
        }
        let (paren, arg_index) = enclosing_paren(src, quote_at)?;
        let mut k = paren;
        while k > 0 && bytes[k - 1].is_ascii_whitespace() {
            k -= 1;
        }
        let (name_start, func) = ident_before(src, k);
        if func.is_empty() {
            return None;
        }
        let (receiver, is_static) = if name_start >= 2 && &src[name_start - 2..name_start] == "::" {
            (Some(receiver_before(src, name_start - 2)), true)
        } else if name_start >= 2 && &src[name_start - 2..name_start] == "->" {
            let mut e = name_start - 2;
            if e > 0 && bytes[e - 1] == b'?' {
                e -= 1;
            }
            (Some(receiver_before(src, e)), false)
        } else {
            (None, false)
        };
        return Some(Context::String(StringArg {
            func: func.trim_start_matches('\\').to_string(),
            receiver,
            is_static,
            arg_index,
            start,
            end,
            prefix,
            value,
        }));
    }
    let bytes = src.as_bytes();
    let (word_start, word) = ident_before(src, offset);
    // Blade directive.
    if word.starts_with('@') && !word.contains('\\') {
        let before = if word_start > 0 {
            bytes[word_start - 1]
        } else {
            b'\n'
        };
        if !is_ident(before) && before != b'@' {
            return Some(Context::Directive {
                start: word_start,
                prefix: word.to_string(),
            });
        }
    }
    // Component tag: `<x-alert`, `<x-forms.input`, `<livewire:counter`.
    {
        let mut s = offset;
        while s > 0 && (is_ident(bytes[s - 1]) || matches!(bytes[s - 1], b'-' | b'.' | b':')) {
            s -= 1;
        }
        if s > 0 && bytes[s - 1] == b'<' {
            let mut e = offset;
            while e < bytes.len() && (is_ident(bytes[e]) || matches!(bytes[e], b'-' | b'.' | b':'))
            {
                e += 1;
            }
            let name = &src[s..e];
            if name.starts_with("x-")
                || name.starts_with("livewire:")
                || "x-".starts_with(&src[s..offset]) && offset > s
            {
                return Some(Context::ComponentTag {
                    start: s,
                    end: e,
                    prefix: src[s..offset].to_string(),
                    value: name.to_string(),
                });
            }
        }
        // Closing tag `</x-alert>`.
        if s > 1 && &src[s - 2..s] == "</" {
            let mut e = offset;
            while e < bytes.len() && (is_ident(bytes[e]) || matches!(bytes[e], b'-' | b'.' | b':'))
            {
                e += 1;
            }
            let name = &src[s..e];
            if name.starts_with("x-") || name.starts_with("livewire:") {
                return Some(Context::ComponentTag {
                    start: s,
                    end: e,
                    prefix: src[s..offset].to_string(),
                    value: name.to_string(),
                });
            }
        }
    }
    // Property access: `$order->sta|` (also when the caret is inside the name).
    if word.starts_with('@') || word.contains('\\') {
        return None;
    }
    if word_start >= 2 && &src[word_start - 2..word_start] == "::" {
        let (_, class) = ident_before(src, word_start - 2);
        if class
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c == '\\')
        {
            return Some(Context::Static {
                class: class.to_string(),
                start: word_start,
                prefix: word.to_string(),
            });
        }
        return None;
    }
    let mut arrow = word_start;
    if arrow >= 2 && &src[arrow - 2..arrow] == "->" {
        arrow -= 2;
        if arrow > 0 && bytes[arrow - 1] == b'?' {
            arrow -= 1;
        }
        // Collect `$var->a->b->`.
        let mut path: Vec<String> = Vec::new();
        let mut i = arrow;
        loop {
            let (s, name) = ident_before(src, i);
            if s > 0 && bytes[s - 1] == b'$' {
                path.reverse();
                return Some(Context::Property(PropertyAccess {
                    variable: format!("${name}"),
                    path,
                    start: word_start,
                    prefix: word.to_string(),
                }));
            }
            if name.is_empty() || s < 2 {
                return None;
            }
            let mut a = s;
            if &src[a - 2..a] == "->" {
                a -= 2;
                if a > 0 && bytes[a - 1] == b'?' {
                    a -= 1;
                }
                path.push(name.to_string());
                i = a;
            } else {
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> Option<Context> {
        let off = src.find('|').unwrap();
        let s = src.replace('|', "");
        context_at(&s, off)
    }

    fn string(src: &str) -> StringArg {
        match at(src) {
            Some(Context::String(s)) => s,
            other => panic!("expected string context, got {other:?}"),
        }
    }

    #[test]
    fn unfinished_route_call() {
        let s = string("<?php\nreturn redirect()->route('orders.|");
        assert_eq!(s.func, "route");
        assert_eq!(s.receiver.as_deref(), Some("redirect()"));
        assert_eq!(s.prefix, "orders.");
        assert_eq!(s.arg_index, 0);
    }

    #[test]
    fn closed_string_and_second_argument() {
        let s = string("<?php $x = view('orders.sh|ow', ['order' => $o]);");
        assert_eq!(
            (s.func.as_str(), s.prefix.as_str(), s.value.as_str()),
            ("view", "orders.sh", "orders.show")
        );
        let s = string("<?php Route::view('/welcome', 'wel|');");
        assert_eq!(
            (s.func.as_str(), s.arg_index, s.is_static),
            ("view", 1, true)
        );
        assert_eq!(s.receiver.as_deref(), Some("Route"));
    }

    #[test]
    fn query_builder_receiver() {
        let s = string("<?php User::query()->where('active', true)->orderBy('na|')");
        assert_eq!(s.func, "orderBy");
        assert_eq!(
            s.receiver.as_deref(),
            Some("User::query()->where('active', true)")
        );
        let s = string("<?php $this->orders()->where('st|");
        assert_eq!(s.receiver.as_deref(), Some("$this->orders()"));
    }

    #[test]
    fn blade_contexts() {
        let s = string("<div>{{ __('messages.wel|') }}</div>");
        assert_eq!(s.func, "__");
        let s = string("@include('partials.he|')");
        assert_eq!(s.func, "@include");
        assert_eq!(
            at("<x-al|"),
            Some(Context::ComponentTag {
                start: 1,
                end: 5,
                prefix: "x-al".into(),
                value: "x-al".into()
            })
        );
        assert!(matches!(
            at("  @for|"),
            Some(Context::Directive { start: 2, .. })
        ));
        assert_eq!(at("mail@exam|"), None);
    }

    #[test]
    fn property_access() {
        match at("<?php $order->user->na|me") {
            Some(Context::Property(p)) => {
                assert_eq!(p.variable, "$order");
                assert_eq!(p.path, vec!["user"]);
                assert_eq!(p.prefix, "na");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn controller_action_method() {
        match at("<?php Route::get('/', [OrderController::class, 'sh|']);") {
            Some(Context::ActionMethod(a)) => assert_eq!(
                (a.class.as_str(), a.prefix.as_str()),
                ("OrderController", "sh")
            ),
            other => panic!("{other:?}"),
        }
    }
}

//! Vim key notation: `a`, `<Esc>`, `<C-r>`, `<CR>`, `<lt>`. Keys are kept as notation strings
//! (one string per key); plain characters are themselves.

use serde::{Deserialize, Serialize};

pub type Key = String;

/// A DOM key event as the UI forwards it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DomKey {
    pub key: String,
    pub code: String,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub shift_key: bool,
    pub meta_key: bool,
}

fn named(key: &str) -> Option<&'static str> {
    Some(match key {
        "Escape" => "Esc",
        "Enter" => "CR",
        "Backspace" => "BS",
        "Delete" => "Del",
        "Tab" => "Tab",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "Insert" => "Insert",
        _ => return None,
    })
}

/// Letter or punctuation of a physical key code (`KeyR` → `r`, `BracketLeft` → `[`), so
/// Ctrl combinations work with any keyboard layout.
fn code_char(code: &str) -> Option<char> {
    if let Some(l) = code.strip_prefix("Key") {
        return l.chars().next().map(|c| c.to_ascii_lowercase());
    }
    if let Some(d) = code.strip_prefix("Digit") {
        return d.chars().next();
    }
    Some(match code {
        "BracketLeft" => '[',
        "BracketRight" => ']',
        "Backslash" => '\\',
        "Slash" => '/',
        "Minus" => '-',
        "Equal" => '=',
        "Space" => ' ',
        _ => return None,
    })
}

const MODIFIER_KEYS: [&str; 7] = [
    "Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock", "OS",
];

/// Notation of a DOM key event; `None` for bare modifiers and dead keys.
pub fn key_of_event(e: &DomKey) -> Option<Key> {
    if MODIFIER_KEYS.contains(&e.key.as_str()) || e.key == "Dead" || e.key == "Unidentified" {
        return None;
    }
    if e.key.starts_with('F') && e.key.len() > 1 && e.key[1..].parse::<u8>().is_ok() {
        return Some(format!("<{}>", e.key));
    }
    let mut mods = String::new();
    if e.ctrl_key {
        mods.push_str("C-");
    }
    if e.alt_key {
        mods.push_str("A-");
    }
    if e.meta_key {
        mods.push_str("D-");
    }
    if let Some(n) = named(&e.key) {
        if e.shift_key && (mods.is_empty() || n != "CR") {
            mods.push_str("S-");
        }
        if mods == "C-" && n == "Esc" {
            return Some("<Esc>".into());
        }
        return Some(if mods.is_empty() {
            format!("<{n}>")
        } else {
            format!("<{mods}{n}>")
        });
    }
    let mut chars = e.key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if mods.is_empty() {
        return Some(c.to_string());
    }
    // Ctrl/Alt combinations use the physical key so layouts do not matter.
    let base = code_char(&e.code).unwrap_or(c.to_ascii_lowercase());
    if mods == "C-" && base == '[' {
        return Some("<Esc>".into());
    }
    let shift = if e.shift_key && base.is_ascii_alphabetic() {
        "S-"
    } else {
        ""
    };
    Some(format!("<{mods}{shift}{base}>"))
}

/// The plain character a key types, if it is one.
pub fn char_of(key: &str) -> Option<char> {
    let mut it = key.chars();
    let c = it.next()?;
    if it.next().is_none() {
        Some(c)
    } else {
        None
    }
}

fn canonical_special(inner: &str, leader: &str) -> Option<Vec<Key>> {
    let lower = inner.to_ascii_lowercase();
    let simple = match lower.as_str() {
        "esc" => Some("<Esc>"),
        "cr" | "enter" | "return" => Some("<CR>"),
        "bs" | "backspace" => Some("<BS>"),
        "del" | "delete" => Some("<Del>"),
        "tab" => Some("<Tab>"),
        "up" => Some("<Up>"),
        "down" => Some("<Down>"),
        "left" => Some("<Left>"),
        "right" => Some("<Right>"),
        "home" => Some("<Home>"),
        "end" => Some("<End>"),
        "pageup" => Some("<PageUp>"),
        "pagedown" => Some("<PageDown>"),
        "insert" => Some("<Insert>"),
        "space" => Some(" "),
        "lt" => Some("<"),
        "bar" => Some("|"),
        "bslash" => Some("\\"),
        "nop" => return Some(Vec::new()),
        "leader" => return Some(leader.chars().map(|c| c.to_string()).collect()),
        _ => None,
    };
    if let Some(s) = simple {
        return Some(vec![s.to_string()]);
    }
    if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() {
        return Some(vec![format!("<F{}>", &lower[1..])]);
    }
    // Modifier combinations: C-x, S-Tab, A-x, M-x, D-x.
    let mut mods = String::new();
    let mut rest = inner;
    loop {
        let b = rest.as_bytes();
        if b.len() > 2 && b[1] == b'-' {
            match b[0].to_ascii_uppercase() {
                b'C' => mods.push_str("C-"),
                b'A' | b'M' => mods.push_str("A-"),
                b'D' => mods.push_str("D-"),
                b'S' => mods.push_str("S-"),
                _ => return None,
            }
            rest = &rest[2..];
        } else {
            break;
        }
    }
    if mods.is_empty() {
        return None;
    }
    let base = match canonical_special(rest, leader) {
        Some(k) if k.len() == 1 && k[0].starts_with('<') => k[0]
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string(),
        _ => {
            let mut it = rest.chars();
            let c = it.next()?;
            if it.next().is_some() {
                return None;
            }
            if mods.contains("C-") {
                c.to_ascii_lowercase().to_string()
            } else {
                c.to_string()
            }
        }
    };
    // <C-[> is Escape; Shift on letters is part of the C- notation.
    if mods == "C-" && base == "[" {
        return Some(vec!["<Esc>".into()]);
    }
    let shift_first = mods.contains("S-") && base.chars().count() == 1;
    let mods = if shift_first {
        mods.replace("S-", "")
    } else {
        mods
    };
    let order = |m: &str| -> String {
        ["C-", "A-", "D-", "S-"]
            .iter()
            .filter(|x| m.contains(*x))
            .copied()
            .collect()
    };
    let mods = order(&mods);
    Some(vec![if shift_first {
        format!("<{mods}S-{base}>")
    } else {
        format!("<{mods}{base}>")
    }])
}

/// Split notation text ("dw", "<C-w>j", ":action Foo<CR>", "<leader>r") into keys.
pub fn parse_keys(text: &str, leader: &str) -> Vec<Key> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            // IdeaVim's `<Action>(ActionId)`.
            let rest: String = chars[i..].iter().take(9).collect();
            if rest.eq_ignore_ascii_case("<Action>(") {
                if let Some(len) = chars[i + 9..].iter().position(|c| *c == ')') {
                    let id: String = chars[i + 9..i + 9 + len].iter().collect();
                    out.push(format!("<Action>({id})"));
                    i += 9 + len + 1;
                    continue;
                }
            }
            if let Some(len) = chars[i + 1..].iter().position(|c| *c == '>') {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                if !inner.is_empty() && !inner.contains('<') && !inner.contains(' ') {
                    if let Some(keys) = canonical_special(&inner, leader) {
                        out.extend(keys);
                        i += len + 2;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i].to_string());
        i += 1;
    }
    out
}

/// Notation text of keys (the reverse of [`parse_keys`]), e.g. for macro registers.
pub fn keys_to_text(keys: &[Key]) -> String {
    keys.iter()
        .map(|k| {
            if k == "<" {
                "<lt>".to_string()
            } else {
                k.clone()
            }
        })
        .collect()
}

/// Display of pending keys in the status bar ("2d", "\"a", "^W").
pub fn display_keys(keys: &[Key]) -> String {
    keys.iter()
        .map(|k| {
            if let Some(c) = k.strip_prefix("<C-").and_then(|r| r.strip_suffix('>')) {
                if c.chars().count() == 1 {
                    return format!("^{}", c.to_uppercase());
                }
            }
            k.clone()
        })
        .collect()
}

/// `Ctrl+R` → `<C-r>` (the shortcut names of the handler settings).
pub fn shortcut_to_key(shortcut: &str) -> Option<Key> {
    let rest = shortcut.strip_prefix("Ctrl+")?;
    let mut it = rest.chars();
    let c = it.next()?;
    if it.next().is_some() {
        return None;
    }
    Some(if c == '[' {
        "<C-[>".into()
    } else {
        format!("<C-{}>", c.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: &str, code: &str, ctrl: bool, shift: bool) -> DomKey {
        DomKey {
            key: key.into(),
            code: code.into(),
            ctrl_key: ctrl,
            shift_key: shift,
            ..DomKey::default()
        }
    }

    #[test]
    fn converts_dom_events() {
        assert_eq!(key_of_event(&ev("a", "KeyA", false, false)).unwrap(), "a");
        assert_eq!(key_of_event(&ev("$", "Digit4", false, true)).unwrap(), "$");
        assert_eq!(
            key_of_event(&ev("Escape", "Escape", false, false)).unwrap(),
            "<Esc>"
        );
        assert_eq!(
            key_of_event(&ev("r", "KeyR", true, false)).unwrap(),
            "<C-r>"
        );
        assert_eq!(
            key_of_event(&ev("к", "KeyR", true, false)).unwrap(),
            "<C-r>"
        );
        assert_eq!(
            key_of_event(&ev("[", "BracketLeft", true, false)).unwrap(),
            "<Esc>"
        );
        assert_eq!(
            key_of_event(&ev("Tab", "Tab", false, true)).unwrap(),
            "<S-Tab>"
        );
        assert_eq!(key_of_event(&ev(" ", "Space", false, false)).unwrap(), " ");
        assert_eq!(key_of_event(&ev("Shift", "ShiftLeft", false, true)), None);
        assert_eq!(key_of_event(&ev("F5", "F5", false, false)).unwrap(), "<F5>");
    }

    #[test]
    fn parses_notation() {
        assert_eq!(parse_keys("d<C-W>j", "\\"), vec!["d", "<C-w>", "j"]);
        assert_eq!(parse_keys("<leader>r<CR>", ","), vec![",", "r", "<CR>"]);
        assert_eq!(parse_keys("<Space>a<lt>", "\\"), vec![" ", "a", "<"]);
        assert_eq!(parse_keys("a<b", "\\"), vec!["a", "<", "b"]);
        assert_eq!(parse_keys("<c-[>", "\\"), vec!["<Esc>"]);
        assert_eq!(
            parse_keys("<action>(Foo.Bar)x", "\\"),
            vec!["<Action>(Foo.Bar)", "x"]
        );
        assert_eq!(parse_keys("<S-Tab><esc>", "\\"), vec!["<S-Tab>", "<Esc>"]);
        assert_eq!(
            keys_to_text(&parse_keys("i<lt>a<Esc>", "\\")),
            "i<lt>a<Esc>"
        );
        assert_eq!(display_keys(&["2".into(), "<C-w>".into()]), "2^W");
        assert_eq!(shortcut_to_key("Ctrl+R").unwrap(), "<C-r>");
    }
}

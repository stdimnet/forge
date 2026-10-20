//! Keyboard shortcut model: parsing of PHPStorm-style strings, formatting for display
//! (Windows/Linux text or macOS symbols) and conversion from DOM keyboard events.
//!
//! ```text
//! "Ctrl+Shift+A"        one keystroke
//! "Ctrl+K Ctrl+C"       chord (two keystrokes; "Ctrl+K, Ctrl+C" is accepted too)
//! "Shift Shift"         double-tap of a modifier ("Double Shift" is accepted too)
//! "Cmd+," / "Meta+,"    macOS command key; "Mod+S" is Cmd on macOS and Ctrl elsewhere
//! ```

use serde::{Deserialize, Serialize};

/// Platform whose key names are used for parsing `Mod` and for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Mac,
    #[default]
    Other,
}

impl Platform {
    /// The platform this binary was built for.
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Meta => "Meta",
        }
    }

    fn mac_symbol(self) -> &'static str {
        match self {
            Self::Ctrl => "⌃",
            Self::Alt => "⌥",
            Self::Shift => "⇧",
            Self::Meta => "⌘",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStroke {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    /// Canonical key name ("A", "1", "F12", "Up", "Enter", "/", "NumPad+") or "" for a bare
    /// modifier.
    pub key: String,
}

impl KeyStroke {
    /// Canonical, platform-independent id ("Ctrl+Alt+Shift+Meta+Key").
    pub fn id(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(5);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.meta {
            parts.push("Meta");
        }
        if !self.key.is_empty() {
            parts.push(&self.key);
        }
        parts.join("+")
    }

    fn modifier_count(&self) -> usize {
        [self.ctrl, self.alt, self.shift, self.meta]
            .iter()
            .filter(|m| **m)
            .count()
    }

    fn set(&mut self, m: Modifier) {
        match m {
            Modifier::Ctrl => self.ctrl = true,
            Modifier::Alt => self.alt = true,
            Modifier::Shift => self.shift = true,
            Modifier::Meta => self.meta = true,
        }
    }

    /// The modifier when the stroke is exactly one modifier and no key.
    fn sole_modifier(&self) -> Option<Modifier> {
        if !self.key.is_empty() || self.modifier_count() != 1 {
            return None;
        }
        Some(if self.ctrl {
            Modifier::Ctrl
        } else if self.alt {
            Modifier::Alt
        } else if self.shift {
            Modifier::Shift
        } else {
            Modifier::Meta
        })
    }

    pub fn is_function_key(&self) -> bool {
        is_function_key(&self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shortcut {
    /// One keystroke or a two-stroke chord.
    Keys(Vec<KeyStroke>),
    /// Double tap of a modifier ("Double Shift").
    Double(Modifier),
}

impl Shortcut {
    /// Canonical id, used for lookups and conflict detection.
    pub fn id(&self) -> String {
        match self {
            Self::Double(m) => format!("Double {}", m.name()),
            Self::Keys(strokes) => strokes
                .iter()
                .map(KeyStroke::id)
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    pub fn strokes(&self) -> &[KeyStroke] {
        match self {
            Self::Keys(s) => s,
            Self::Double(_) => &[],
        }
    }

    /// Display text, PHPStorm style ("Ctrl+K, Ctrl+C" or "⇧⌘A").
    pub fn format(&self, platform: Platform) -> String {
        match self {
            Self::Double(m) => match platform {
                Platform::Mac => format!("Double{}", m.mac_symbol()),
                Platform::Other => format!("Double {}", m.name()),
            },
            Self::Keys(strokes) => strokes
                .iter()
                .map(|k| format_stroke(k, platform))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

fn modifier_alias(token: &str) -> Option<Option<Modifier>> {
    // `Some(None)` is "Mod" (platform dependent).
    Some(match token.to_lowercase().as_str() {
        "ctrl" | "control" | "⌃" => Some(Modifier::Ctrl),
        "alt" | "opt" | "option" | "⌥" => Some(Modifier::Alt),
        "shift" | "⇧" => Some(Modifier::Shift),
        "meta" | "cmd" | "command" | "⌘" | "win" => Some(Modifier::Meta),
        "mod" => None,
        _ => return None,
    })
}

fn key_alias(lower: &str) -> Option<&'static str> {
    Some(match lower {
        "esc" | "escape" => "Escape",
        "del" | "delete" => "Delete",
        "ins" | "insert" => "Insert",
        "return" | "enter" => "Enter",
        "backspace" | "back_space" => "Backspace",
        "tab" => "Tab",
        "space" => "Space",
        "home" => "Home",
        "end" => "End",
        "pageup" | "page_up" | "pgup" => "PageUp",
        "pagedown" | "page_down" | "pgdn" => "PageDown",
        "up" | "arrowup" => "Up",
        "down" | "arrowdown" => "Down",
        "left" | "arrowleft" => "Left",
        "right" | "arrowright" => "Right",
        "backquote" | "back_quote" => "`",
        "minus" => "-",
        "equals" | "equal" | "plus" => "=",
        "slash" => "/",
        "backslash" | "back_slash" => "\\",
        "period" => ".",
        "comma" => ",",
        "semicolon" => ";",
        "quote" => "'",
        "openbracket" | "open_bracket" => "[",
        "closebracket" | "close_bracket" => "]",
        "contextmenu" => "ContextMenu",
        "numpad+" | "add" => "NumPad+",
        "numpad-" | "subtract" => "NumPad-",
        "numpad*" | "multiply" => "NumPad*",
        "numpad/" | "divide" => "NumPad/",
        "numpad." => "NumPad.",
        _ => return None,
    })
}

/// "F1" … "F24".
fn is_function_key(key: &str) -> bool {
    let Some(n) = key.strip_prefix('F').or_else(|| key.strip_prefix('f')) else {
        return false;
    };
    !n.is_empty()
        && n.len() <= 2
        && !n.starts_with('0')
        && n.parse::<u8>().is_ok_and(|v| (1..=24).contains(&v))
}

/// Normalize a key token from a shortcut string to its canonical name.
pub fn normalize_key(token: &str) -> Option<String> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    if let Some(alias) = key_alias(&lower) {
        return Some(alias.to_string());
    }
    if is_function_key(t) {
        return Some(t.to_uppercase());
    }
    if let Some(d) = lower.strip_prefix("numpad") {
        if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("NumPad{d}"));
        }
    }
    if t.chars().count() == 1 {
        return Some(t.to_uppercase());
    }
    None
}

fn split_stroke_tokens(s: &str) -> Vec<String> {
    // "+" separates tokens, but a trailing "+" is the key itself ("Ctrl+NumPad+", "Ctrl++").
    let mut s = s;
    let mut tail: Option<&str> = None;
    let numpad_plus = s
        .len()
        .checked_sub(7)
        .and_then(|i| s.get(i..).map(|t| (i, t)))
        .filter(|(_, t)| t.eq_ignore_ascii_case("numpad+"));
    if let Some((i, _)) = numpad_plus {
        tail = Some("NumPad+");
        s = &s[..i];
    } else if s == "+" {
        tail = Some("+");
        s = "";
    } else if s.ends_with("++") {
        tail = Some("+");
        s = &s[..s.len() - 2];
    }
    let mut tokens: Vec<String> = s
        .split('+')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(t) = tail {
        tokens.push(t.to_string());
    }
    tokens
}

/// Parse one keystroke such as "Ctrl+Alt+Shift+T".
pub fn parse_stroke(text: &str, platform: Platform) -> Result<KeyStroke, String> {
    let mut stroke = KeyStroke::default();
    let tokens = split_stroke_tokens(text.trim());
    if tokens.is_empty() {
        return Err("empty keystroke".into());
    }
    let last = tokens.len() - 1;
    for (i, raw) in tokens.iter().enumerate() {
        if let Some(m) = modifier_alias(raw.trim()) {
            if i < last || tokens.len() == 1 || stroke.key.is_empty() {
                let m = m.unwrap_or(match platform {
                    Platform::Mac => Modifier::Meta,
                    Platform::Other => Modifier::Ctrl,
                });
                stroke.set(m);
                continue;
            }
        }
        if i != last {
            return Err(format!("unknown modifier \"{raw}\" in \"{text}\""));
        }
        stroke.key =
            normalize_key(raw).ok_or_else(|| format!("unknown key \"{raw}\" in \"{text}\""))?;
    }
    Ok(stroke)
}

/// Parse a full shortcut: single keystroke, two-stroke chord or modifier double-tap.
pub fn parse_shortcut(text: &str, platform: Platform) -> Result<Shortcut, String> {
    let trimmed = text.trim();
    let mut words = trimmed.split_whitespace();
    if let (Some(first), Some(second), None) = (words.next(), words.next(), words.next()) {
        if first.eq_ignore_ascii_case("double") {
            return parse_stroke(second, platform)?
                .sole_modifier()
                .map(Shortcut::Double)
                .ok_or_else(|| format!("\"{text}\": only modifiers can be double-tapped"));
        }
    }
    // Strokes are separated by whitespace; a trailing "," on a stroke ("Ctrl+K, Ctrl+C") is a
    // separator unless it is the key itself ("Cmd+,").
    let parts: Vec<&str> = trimmed
        .split_whitespace()
        .map(|p| {
            if p.chars().count() > 1 && p.ends_with(',') && !p.ends_with("+,") {
                &p[..p.len() - 1]
            } else {
                p
            }
        })
        .filter(|p| !p.is_empty() && *p != ",")
        .collect();
    if parts.is_empty() {
        return Err("empty shortcut".into());
    }
    if parts.len() > 2 {
        return Err(format!("\"{text}\": at most two keystrokes are supported"));
    }
    let strokes = parts
        .iter()
        .map(|p| parse_stroke(p, platform))
        .collect::<Result<Vec<_>, _>>()?;
    if strokes.len() == 2 {
        if let (Some(a), Some(b)) = (strokes[0].sole_modifier(), strokes[1].sole_modifier()) {
            if a == b {
                return Ok(Shortcut::Double(a));
            }
        }
    }
    if strokes.iter().any(|s| s.key.is_empty()) {
        return Err(format!("\"{text}\": a keystroke needs a key"));
    }
    Ok(Shortcut::Keys(strokes))
}

pub fn try_parse_shortcut(text: &str, platform: Platform) -> Option<Shortcut> {
    parse_shortcut(text, platform).ok()
}

/// Canonicalize a user-entered shortcut string; `None` when it does not parse.
pub fn canonical_shortcut(text: &str, platform: Platform) -> Option<String> {
    try_parse_shortcut(text, platform).map(|s| s.id())
}

fn mac_key_symbol(key: &str) -> Option<&'static str> {
    Some(match key {
        "Enter" => "⏎",
        "Backspace" => "⌫",
        "Delete" => "⌦",
        "Escape" => "⎋",
        "Tab" => "⇥",
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "PageUp" => "⇞",
        "PageDown" => "⇟",
        "Home" => "↖",
        "End" => "↘",
        "Space" => "Space",
        _ => return None,
    })
}

fn text_key_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "NumPad+" => "NumPad +",
        "NumPad-" => "NumPad -",
        "NumPad*" => "NumPad *",
        "NumPad/" => "NumPad /",
        "NumPad." => "NumPad .",
        _ => return None,
    })
}

/// Display text for a keystroke, PHPStorm style ("Ctrl+Shift+A" or "⇧⌘A").
pub fn format_stroke(s: &KeyStroke, platform: Platform) -> String {
    match platform {
        Platform::Mac => {
            let mut out = String::new();
            for (on, m) in [
                (s.ctrl, Modifier::Ctrl),
                (s.alt, Modifier::Alt),
                (s.shift, Modifier::Shift),
                (s.meta, Modifier::Meta),
            ] {
                if on {
                    out.push_str(m.mac_symbol());
                }
            }
            out.push_str(mac_key_symbol(&s.key).unwrap_or(&s.key));
            out
        }
        Platform::Other => {
            let mut stroke = s.clone();
            if let Some(name) = text_key_name(&s.key) {
                stroke.key = name.to_string();
            }
            stroke.id()
        }
    }
}

/// Format a shortcut string for display; unparsable strings are returned unchanged.
pub fn format_shortcut_text(text: &str, platform: Platform) -> String {
    match try_parse_shortcut(text, platform) {
        Some(s) => s.format(platform),
        None => text.to_string(),
    }
}

/// The subset of a DOM `KeyboardEvent` the keymap needs (field names as in the DOM).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyEvent {
    pub key: String,
    pub code: String,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub shift_key: bool,
    pub meta_key: bool,
    pub repeat: bool,
}

fn code_key(code: &str) -> Option<&'static str> {
    Some(match code {
        "Minus" => "-",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" | "IntlBackslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Space" => "Space",
        "Enter" | "NumpadEnter" => "Enter",
        "Escape" => "Escape",
        "Backspace" => "Backspace",
        "Tab" => "Tab",
        "Delete" => "Delete",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "ContextMenu" => "ContextMenu",
        "NumpadAdd" => "NumPad+",
        "NumpadSubtract" => "NumPad-",
        "NumpadMultiply" => "NumPad*",
        "NumpadDivide" => "NumPad/",
        "NumpadDecimal" => "NumPad.",
        _ => return None,
    })
}

const MODIFIER_KEYS: [&str; 8] = [
    "Shift", "Control", "Alt", "Meta", "AltGraph", "OS", "Hyper", "Super",
];

fn single_ascii(s: &str, pred: impl Fn(char) -> bool) -> Option<char> {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if pred(c) => Some(c),
        _ => None,
    }
}

/// Key name of an event. Letters are the character the layout produces, as in PHPStorm: on a
/// German keyboard the key labelled Z gives Ctrl+Z (undo), not the Ctrl+Y of the US key in
/// that position. Layouts without Latin letters (Cyrillic, Greek) and Alt/Option combinations
/// that produce special characters fall back to the physical key, so the shortcuts keep
/// working. Digits and punctuation use the physical key.
pub fn event_key(e: &KeyEvent) -> String {
    if MODIFIER_KEYS.contains(&e.key.as_str()) {
        return String::new();
    }
    if let Some(c) = single_ascii(&e.key, |c| c.is_ascii_alphabetic()) {
        return c.to_ascii_uppercase().to_string();
    }
    let c = e.code.as_str();
    if let Some(l) = c.strip_prefix("Key") {
        if single_ascii(l, |c| c.is_ascii_uppercase()).is_some() {
            return l.to_string();
        }
    }
    if let Some(d) = c.strip_prefix("Digit") {
        if single_ascii(d, |c| c.is_ascii_digit()).is_some() {
            return d.to_string();
        }
    }
    if let Some(d) = c.strip_prefix("Numpad") {
        if single_ascii(d, |c| c.is_ascii_digit()).is_some() {
            return format!("NumPad{d}");
        }
    }
    if let Some(n) = c.strip_prefix('F') {
        if (1..=2).contains(&n.len()) && n.chars().all(|c| c.is_ascii_digit()) {
            return c.to_string();
        }
    }
    if let Some(k) = code_key(c) {
        return k.to_string();
    }
    normalize_key(&e.key).unwrap_or_else(|| e.key.clone())
}

pub fn stroke_from_event(e: &KeyEvent) -> KeyStroke {
    KeyStroke {
        ctrl: e.ctrl_key,
        alt: e.alt_key,
        shift: e.shift_key,
        meta: e.meta_key,
        key: event_key(e),
    }
}

/// Which modifier a bare modifier key event is about (for double-tap detection).
pub fn modifier_of_event(e: &KeyEvent) -> Option<Modifier> {
    match e.key.as_str() {
        "Shift" => Some(Modifier::Shift),
        "Control" => Some(Modifier::Ctrl),
        "Alt" => Some(Modifier::Alt),
        "Meta" | "OS" => Some(Modifier::Meta),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: Platform = Platform::Other;

    fn sid(text: &str) -> String {
        parse_stroke(text, O).unwrap().id()
    }

    fn ev(code: &str, key: &str) -> KeyEvent {
        KeyEvent {
            code: code.into(),
            key: key.into(),
            ..Default::default()
        }
    }

    fn ctrl(mut e: KeyEvent) -> KeyEvent {
        e.ctrl_key = true;
        e
    }

    fn alt(mut e: KeyEvent) -> KeyEvent {
        e.alt_key = true;
        e
    }

    fn shift(mut e: KeyEvent) -> KeyEvent {
        e.shift_key = true;
        e
    }

    fn canon(text: &str) -> Option<String> {
        canonical_shortcut(text, O)
    }

    #[test]
    fn parses_modifiers_in_any_order_and_normalizes_the_key() {
        assert_eq!(sid("shift+ctrl+a"), "Ctrl+Shift+A");
        assert_eq!(sid("Ctrl+Alt+Shift+T"), "Ctrl+Alt+Shift+T");
        assert_eq!(sid("Alt+Insert"), "Alt+Insert");
        assert_eq!(sid("Ctrl+F12"), "Ctrl+F12");
        assert_eq!(sid("Alt+Shift+Up"), "Alt+Shift+Up");
    }

    #[test]
    fn understands_macos_names_and_symbols() {
        assert_eq!(sid("Cmd+Shift+A"), "Shift+Meta+A");
        assert_eq!(sid("⌘+,"), "Meta+,");
        assert_eq!(sid("Opt+Enter"), "Alt+Enter");
    }

    #[test]
    fn maps_mod_to_cmd_on_macos_and_ctrl_elsewhere() {
        assert_eq!(parse_stroke("Mod+S", Platform::Mac).unwrap().id(), "Meta+S");
        assert_eq!(parse_stroke("Mod+S", O).unwrap().id(), "Ctrl+S");
    }

    #[test]
    fn handles_punctuation_and_numpad_keys_including_plus() {
        assert_eq!(sid("Ctrl+/"), "Ctrl+/");
        assert_eq!(sid("Ctrl+-"), "Ctrl+-");
        assert_eq!(sid("Ctrl+="), "Ctrl+=");
        assert_eq!(sid("Ctrl+NumPad+"), "Ctrl+NumPad+");
        assert_eq!(sid("Ctrl+Shift+NumPad-"), "Ctrl+Shift+NumPad-");
        assert_eq!(sid("Alt+`"), "Alt+`");
        assert_eq!(sid("Ctrl+Shift+'"), "Ctrl+Shift+'");
        assert_eq!(sid("Ctrl++"), "Ctrl++");
    }

    #[test]
    fn accepts_key_aliases() {
        assert_eq!(sid("Shift+Esc"), "Shift+Escape");
        assert_eq!(sid("ctrl+back_space"), "Ctrl+Backspace");
        assert_eq!(sid("Alt+ArrowLeft"), "Alt+Left");
        assert_eq!(sid("NumPad5"), "NumPad5");
        assert_eq!(sid("f5"), "F5");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_stroke("Ctrl+Foo", O).is_err());
        assert!(parse_stroke("Hyper+A", O).is_err());
        assert!(parse_stroke("", O).is_err());
        assert!(parse_stroke("F25", O).is_err());
    }

    #[test]
    fn parses_double_taps() {
        let shift = Shortcut::Double(Modifier::Shift);
        assert_eq!(parse_shortcut("Shift Shift", O).unwrap(), shift);
        assert_eq!(parse_shortcut("Double Shift", O).unwrap(), shift);
        assert_eq!(
            parse_shortcut("Ctrl Ctrl", O).unwrap(),
            Shortcut::Double(Modifier::Ctrl)
        );
        assert!(parse_shortcut("Double A", O).is_err());
    }

    #[test]
    fn parses_chords_with_space_or_comma() {
        let a = parse_shortcut("Ctrl+K Ctrl+C", O).unwrap();
        assert!(matches!(a, Shortcut::Keys(_)));
        assert_eq!(a.id(), "Ctrl+K Ctrl+C");
        assert_eq!(
            parse_shortcut("Ctrl+K, Ctrl+C", O).unwrap().id(),
            "Ctrl+K Ctrl+C"
        );
    }

    #[test]
    fn rejects_three_stroke_and_keyless_shortcuts() {
        assert!(parse_shortcut("Ctrl+K Ctrl+C Ctrl+D", O).is_err());
        assert!(parse_shortcut("Ctrl+Shift", O).is_err());
        assert!(parse_shortcut("Shift Ctrl", O).is_err());
    }

    #[test]
    fn canonicalizes_equivalent_spellings() {
        assert_eq!(canon("shift+ctrl+a"), canon("Ctrl+Shift+A"));
        assert_eq!(canon("nonsense+"), None);
    }

    #[test]
    fn formats_windows_linux_text() {
        assert_eq!(format_shortcut_text("Ctrl+Shift+A", O), "Ctrl+Shift+A");
        assert_eq!(format_shortcut_text("Double Shift", O), "Double Shift");
        assert_eq!(format_shortcut_text("Ctrl+K Ctrl+C", O), "Ctrl+K, Ctrl+C");
        assert_eq!(format_shortcut_text("Ctrl+NumPad+", O), "Ctrl+NumPad +");
        assert_eq!(
            format_shortcut_text("not a shortcut!", O),
            "not a shortcut!"
        );
    }

    #[test]
    fn formats_macos_symbols_in_phpstorm_order() {
        let m = Platform::Mac;
        assert_eq!(format_shortcut_text("Cmd+Shift+A", m), "⇧⌘A");
        assert_eq!(format_shortcut_text("Ctrl+Alt+Shift+Cmd+Up", m), "⌃⌥⇧⌘↑");
        assert_eq!(format_shortcut_text("Alt+Enter", m), "⌥⏎");
        assert_eq!(format_shortcut_text("Double Shift", m), "Double⇧");
    }

    #[test]
    fn uses_physical_codes_so_alt_option_chars_do_not_matter() {
        assert_eq!(event_key(&alt(ev("KeyJ", "∆"))), "J");
        // Cyrillic layouts have no Latin letters: the physical key decides.
        assert_eq!(event_key(&ctrl(ev("KeyZ", "я"))), "Z");
        assert_eq!(event_key(&alt(ev("Digit1", "¡"))), "1");
        assert_eq!(event_key(&ev("Slash", "/")), "/");
        assert_eq!(event_key(&ev("NumpadAdd", "+")), "NumPad+");
        assert_eq!(event_key(&ev("Numpad7", "7")), "NumPad7");
        assert_eq!(event_key(&ev("ArrowUp", "ArrowUp")), "Up");
        assert_eq!(event_key(&ev("F12", "F12")), "F12");
        assert_eq!(event_key(&shift(ev("ShiftLeft", "Shift"))), "");
    }

    #[test]
    fn falls_back_to_key_when_code_is_unknown() {
        assert_eq!(event_key(&ev("", "Escape")), "Escape");
    }

    #[test]
    fn builds_strokes_from_events() {
        let s = stroke_from_event(&shift(ctrl(ev("KeyA", "A"))));
        assert_eq!(s.id(), "Ctrl+Shift+A");
    }

    #[test]
    fn keeps_comma_as_a_key() {
        assert_eq!(parse_shortcut("Cmd+,", O).unwrap().id(), "Meta+,");
        assert_eq!(
            parse_shortcut("Cmd+, Cmd+.", O).unwrap().id(),
            "Meta+, Meta+."
        );
    }

    #[test]
    fn uses_the_character_so_ctrl_z_is_undo_on_a_german_keyboard() {
        // QWERTZ: the key labelled Z sits where the US keyboard has Y.
        assert_eq!(event_key(&ctrl(ev("KeyY", "z"))), "Z");
        assert_eq!(event_key(&ctrl(ev("KeyZ", "y"))), "Y");
        assert_eq!(stroke_from_event(&ctrl(ev("KeyY", "z"))).id(), "Ctrl+Z");
        assert_eq!(stroke_from_event(&ctrl(ev("KeyZ", "y"))).id(), "Ctrl+Y");
    }

    #[test]
    fn follows_azerty_and_dvorak_too() {
        assert_eq!(event_key(&ctrl(ev("KeyQ", "a"))), "A");
        assert_eq!(event_key(&ctrl(ev("KeyW", "z"))), "Z");
        assert_eq!(event_key(&ctrl(ev("KeyS", "o"))), "O");
    }

    #[test]
    fn is_case_insensitive_for_shift_and_caps_lock() {
        assert_eq!(event_key(&shift(ctrl(ev("KeyY", "Z")))), "Z");
        assert_eq!(
            stroke_from_event(&shift(ctrl(ev("KeyY", "Z")))).id(),
            "Ctrl+Shift+Z"
        );
    }

    #[test]
    fn keeps_the_physical_key_for_digits_punctuation_and_special_alt_characters() {
        assert_eq!(event_key(&shift(ctrl(ev("Digit5", "%")))), "5");
        assert_eq!(event_key(&ev("Slash", "-")), "/");
        assert_eq!(event_key(&alt(ev("KeyJ", "∆"))), "J");
    }

    #[test]
    fn detects_modifier_events() {
        assert_eq!(
            modifier_of_event(&ev("ShiftLeft", "Shift")),
            Some(Modifier::Shift)
        );
        assert_eq!(
            modifier_of_event(&ev("ControlLeft", "Control")),
            Some(Modifier::Ctrl)
        );
        assert_eq!(modifier_of_event(&ev("OSLeft", "OS")), Some(Modifier::Meta));
        assert_eq!(modifier_of_event(&ev("KeyA", "a")), None);
    }
}

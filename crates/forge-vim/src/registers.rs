//! Registers: unnamed, numbered, small delete, named (append with A-Z), black hole, last
//! inserted text, last command line, last search and the system clipboard (`+`, `*`).

use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegKind {
    #[default]
    Char,
    Line,
    Block,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Register {
    pub text: String,
    pub kind: RegKind,
}

impl Register {
    pub fn new(text: impl Into<String>, kind: RegKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

#[derive(Debug, Default)]
pub struct Registers {
    named: HashMap<char, Register>,
    /// Register the unnamed register points to.
    unnamed: Option<char>,
    /// Mirror of the system clipboard (kept current by the UI).
    pub clipboard: Register,
    /// Clipboard writes for the UI to perform.
    pub clipboard_writes: Vec<String>,
}

pub fn is_valid(name: char) -> bool {
    name.is_ascii_alphanumeric() || "\"-_+*.:/%#".contains(name)
}

impl Registers {
    /// Store yanked (`yank`) or deleted text, following Vim's rules for `"0`, `"1`–`"9` and `"-`.
    pub fn store(&mut self, name: Option<char>, reg: Register, yank: bool, unnamed_clip: bool) {
        match name {
            Some('_') => {}
            Some(c @ ('+' | '*')) => {
                self.set_clipboard(reg.clone());
                self.unnamed = Some(c);
            }
            Some(c) if c.is_ascii_uppercase() => {
                let lower = c.to_ascii_lowercase();
                let entry = self.named.entry(lower).or_default();
                if reg.kind == RegKind::Line
                    && entry.kind != RegKind::Line
                    && !entry.text.is_empty()
                {
                    entry.text.push('\n');
                }
                entry.text.push_str(&reg.text);
                if reg.kind == RegKind::Line {
                    entry.kind = RegKind::Line;
                }
                self.unnamed = Some(lower);
            }
            Some(c) if c.is_ascii_lowercase() => {
                self.named.insert(c, reg);
                self.unnamed = Some(c);
            }
            _ => {
                if yank {
                    self.named.insert('0', reg.clone());
                    self.unnamed = Some('0');
                } else if reg.kind == RegKind::Line || reg.text.contains('\n') {
                    for i in (1..9).rev() {
                        let d = char::from_digit(i, 10).unwrap();
                        if let Some(r) = self.named.remove(&d) {
                            self.named.insert(char::from_digit(i + 1, 10).unwrap(), r);
                        }
                    }
                    self.named.insert('1', reg.clone());
                    self.unnamed = Some('1');
                } else {
                    self.named.insert('-', reg.clone());
                    self.unnamed = Some('-');
                }
                if unnamed_clip {
                    self.set_clipboard(reg);
                }
            }
        }
    }

    fn set_clipboard(&mut self, reg: Register) {
        self.clipboard_writes.push(reg.text.clone());
        self.clipboard = reg;
    }

    /// Text of a register for pasting.
    pub fn get(&self, name: Option<char>, unnamed_clip: bool) -> Option<Register> {
        match name {
            None | Some('"') => {
                if unnamed_clip {
                    return Some(self.clipboard.clone()).filter(|r| !r.text.is_empty());
                }
                let n = self.unnamed?;
                if n == '+' || n == '*' {
                    return Some(self.clipboard.clone());
                }
                self.named.get(&n).cloned()
            }
            Some('+' | '*') => Some(self.clipboard.clone()),
            Some(c) => self.named.get(&c.to_ascii_lowercase()).cloned(),
        }
    }

    /// Set a read-only register (`.`, `:`, `/`) or a macro register.
    pub fn set_special(&mut self, name: char, text: impl Into<String>) {
        self.named.insert(
            name.to_ascii_lowercase(),
            Register::new(text, RegKind::Char),
        );
    }

    pub fn set_clipboard_mirror(&mut self, text: &str) {
        if self.clipboard.text != text {
            let kind = if text.ends_with('\n') {
                RegKind::Line
            } else {
                RegKind::Char
            };
            self.clipboard = Register::new(text, kind);
        }
    }

    /// `:registers` listing: name and text.
    pub fn listing(&self) -> Vec<(char, String)> {
        let mut names: Vec<char> = self.named.keys().copied().collect();
        names.sort_by_key(|c| {
            let order = "\"0123456789abcdefghijklmnopqrstuvwxyz-.:/";
            order.find(*c).unwrap_or(usize::MAX)
        });
        let mut out = Vec::new();
        if let Some(r) = self.get(None, false) {
            out.push(('"', r.text));
        }
        for n in names {
            out.push((n, self.named[&n].text.clone()));
        }
        if !self.clipboard.text.is_empty() {
            out.push(('+', self.clipboard.text.clone()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_vim_register_rules() {
        let mut r = Registers::default();
        r.store(None, Register::new("one\n", RegKind::Line), false, false);
        r.store(None, Register::new("two\n", RegKind::Line), false, false);
        r.store(None, Register::new("w", RegKind::Char), false, false);
        assert_eq!(r.get(Some('1'), false).unwrap().text, "two\n");
        assert_eq!(r.get(Some('2'), false).unwrap().text, "one\n");
        assert_eq!(r.get(Some('-'), false).unwrap().text, "w");
        assert_eq!(r.get(None, false).unwrap().text, "w");
        r.store(None, Register::new("y", RegKind::Char), true, false);
        assert_eq!(r.get(Some('0'), false).unwrap().text, "y");
        r.store(Some('a'), Register::new("x", RegKind::Char), true, false);
        r.store(Some('A'), Register::new("z", RegKind::Char), true, false);
        assert_eq!(r.get(Some('a'), false).unwrap().text, "xz");
        r.store(Some('+'), Register::new("clip", RegKind::Char), true, false);
        assert_eq!(r.clipboard_writes, vec!["clip"]);
        assert_eq!(r.get(None, false).unwrap().text, "clip");
        r.store(
            Some('_'),
            Register::new("gone", RegKind::Char),
            false,
            false,
        );
        assert_eq!(r.get(None, false).unwrap().text, "clip");
    }
}

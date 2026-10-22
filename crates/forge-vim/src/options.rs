//! `:set` options.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub number: bool,
    pub relativenumber: bool,
    pub ignorecase: bool,
    pub smartcase: bool,
    pub hlsearch: bool,
    pub incsearch: bool,
    pub wrapscan: bool,
    pub scrolloff: usize,
    /// `None`: the editor's indentation.
    pub shiftwidth: Option<usize>,
    pub tabstop: Option<usize>,
    pub expandtab: Option<bool>,
    /// `unnamed` / `unnamedplus`: the unnamed register is the system clipboard.
    pub clipboard: String,
    pub timeout: bool,
    pub timeoutlen: u32,
    pub showmode: bool,
    pub showcmd: bool,
    /// IdeaVim's `ideajoin`: `J` uses the IDE's smart join.
    pub ideajoin: bool,
    pub visualbell: bool,
    pub history: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            number: true,
            relativenumber: false,
            ignorecase: false,
            smartcase: false,
            hlsearch: false,
            incsearch: false,
            wrapscan: true,
            scrolloff: 0,
            shiftwidth: None,
            tabstop: None,
            expandtab: None,
            clipboard: String::new(),
            timeout: true,
            timeoutlen: 1000,
            showmode: true,
            showcmd: true,
            ideajoin: false,
            visualbell: false,
            history: 50,
        }
    }
}

const ALIASES: [(&str, &str); 17] = [
    ("nu", "number"),
    ("rnu", "relativenumber"),
    ("ic", "ignorecase"),
    ("scs", "smartcase"),
    ("hls", "hlsearch"),
    ("is", "incsearch"),
    ("ws", "wrapscan"),
    ("so", "scrolloff"),
    ("sw", "shiftwidth"),
    ("ts", "tabstop"),
    ("et", "expandtab"),
    ("cb", "clipboard"),
    ("to", "timeout"),
    ("tm", "timeoutlen"),
    ("smd", "showmode"),
    ("sc", "showcmd"),
    ("hi", "history"),
];

/// Options IdeaVim users commonly set that change nothing here; accepted silently.
const IGNORED: [&str; 14] = [
    "surround",
    "commentary",
    "multiple-cursors",
    "easymotion",
    "NERDTree",
    "highlightedyank",
    "argtextobj",
    "exchange",
    "textobj-entire",
    "ReplaceWithRegister",
    "matchit",
    "idearefactormode",
    "ideamarks",
    "ideastatusicon",
];

fn full_name(name: &str) -> &str {
    ALIASES
        .iter()
        .find(|(a, _)| *a == name)
        .map(|(_, f)| *f)
        .unwrap_or(name)
}

impl Options {
    fn bool_mut(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "number" => &mut self.number,
            "relativenumber" => &mut self.relativenumber,
            "ignorecase" => &mut self.ignorecase,
            "smartcase" => &mut self.smartcase,
            "hlsearch" => &mut self.hlsearch,
            "incsearch" => &mut self.incsearch,
            "wrapscan" => &mut self.wrapscan,
            "timeout" => &mut self.timeout,
            "showmode" => &mut self.showmode,
            "showcmd" => &mut self.showcmd,
            "ideajoin" => &mut self.ideajoin,
            "visualbell" | "vb" => &mut self.visualbell,
            _ => return None,
        })
    }

    fn value_of(&self, name: &str) -> Option<String> {
        let mut copy = self.clone();
        if let Some(b) = copy.bool_mut(name) {
            return Some(if *b {
                name.to_string()
            } else {
                format!("no{name}")
            });
        }
        let opt = |v: Option<usize>| v.map(|n| n.to_string()).unwrap_or_else(|| "0".into());
        Some(match name {
            "scrolloff" => format!("{name}={}", self.scrolloff),
            "shiftwidth" => format!("{name}={}", opt(self.shiftwidth)),
            "tabstop" => format!("{name}={}", opt(self.tabstop)),
            "expandtab" => match self.expandtab {
                Some(false) => "noexpandtab".into(),
                _ => "expandtab".into(),
            },
            "clipboard" => format!("{name}={}", self.clipboard),
            "timeoutlen" => format!("{name}={}", self.timeoutlen),
            "history" => format!("{name}={}", self.history),
            _ => return None,
        })
    }

    fn set_value(&mut self, name: &str, value: &str, op: char) -> Result<(), String> {
        let num = || {
            value
                .parse::<usize>()
                .map_err(|_| format!("E521: Number required after =: {name}={value}"))
        };
        let combine = |old: usize, n: usize| match op {
            '+' => old + n,
            '-' => old.saturating_sub(n),
            '^' => old * n,
            _ => n,
        };
        match name {
            "scrolloff" => self.scrolloff = combine(self.scrolloff, num()?),
            "shiftwidth" => self.shiftwidth = Some(combine(self.shiftwidth.unwrap_or(0), num()?)),
            "tabstop" => self.tabstop = Some(combine(self.tabstop.unwrap_or(0), num()?)),
            "timeoutlen" => self.timeoutlen = combine(self.timeoutlen as usize, num()?) as u32,
            "history" => self.history = combine(self.history, num()?),
            "clipboard" => {
                let mut parts: Vec<String> = if op == '=' {
                    Vec::new()
                } else {
                    self.clipboard
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                };
                for v in value.split(',').filter(|s| !s.is_empty()) {
                    if op == '-' {
                        parts.retain(|p| p != v);
                    } else if !parts.iter().any(|p| p == v) {
                        parts.push(v.to_string());
                    }
                }
                self.clipboard = parts.join(",");
            }
            _ => return Err(format!("E518: Unknown option: {name}")),
        }
        Ok(())
    }

    /// Apply the arguments of `:set` ("number", "nohls", "sw=2", "ic?", "invnu", "cb+=unnamed").
    /// Returns the text to show (for `opt?`) or an error.
    pub fn apply_set(&mut self, args: &str) -> Result<Option<String>, String> {
        let mut shown = Vec::new();
        for arg in args.split_whitespace() {
            if arg == "all" {
                continue;
            }
            // name[op]=value
            if let Some(eq) = arg.find('=').or_else(|| arg.find(':')) {
                let (mut name, value) = (&arg[..eq], &arg[eq + 1..]);
                let mut op = '=';
                if let Some(last) = name.chars().last() {
                    if matches!(last, '+' | '-' | '^') {
                        op = last;
                        name = &name[..name.len() - 1];
                    }
                }
                let name = full_name(name);
                if IGNORED.contains(&name) {
                    continue;
                }
                self.set_value(name, value, op)?;
                continue;
            }
            if let Some(name) = arg.strip_suffix('?') {
                let name = full_name(name);
                shown.push(
                    self.value_of(name)
                        .ok_or_else(|| format!("E518: Unknown option: {name}"))?,
                );
                continue;
            }
            let (name, value) = if let Some(n) = arg.strip_suffix('!') {
                (full_name(n), None)
            } else if let Some(n) = arg.strip_prefix("inv") {
                (full_name(n), None)
            } else if let Some(n) = arg
                .strip_prefix("no")
                .filter(|n| self.clone().bool_mut(full_name(n)).is_some() || IGNORED.contains(n))
            {
                (full_name(n), Some(false))
            } else {
                (full_name(arg), Some(true))
            };
            if IGNORED.contains(&name) {
                continue;
            }
            if name == "expandtab" {
                self.expandtab = Some(value.unwrap_or(self.expandtab != Some(true)));
                continue;
            }
            match self.bool_mut(name) {
                Some(b) => *b = value.unwrap_or(!*b),
                None => match self.value_of(name) {
                    // `:set sw` shows a number option.
                    Some(v) => shown.push(v),
                    None => return Err(format!("E518: Unknown option: {arg}")),
                },
            }
        }
        Ok((!shown.is_empty()).then(|| shown.join("  ")))
    }

    pub fn clipboard_unnamed(&self) -> bool {
        self.clipboard
            .split(',')
            .any(|p| p == "unnamed" || p == "unnamedplus")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_options() {
        let mut o = Options::default();
        o.apply_set("nonu rnu ic scs hls is sw=2 cb+=unnamed")
            .unwrap();
        assert!(!o.number && o.relativenumber && o.ignorecase && o.smartcase);
        assert!(o.hlsearch && o.incsearch);
        assert_eq!(o.shiftwidth, Some(2));
        assert!(o.clipboard_unnamed());
        assert_eq!(o.apply_set("hls?").unwrap().unwrap(), "hlsearch");
        o.apply_set("hls!").unwrap();
        assert!(!o.hlsearch);
        o.apply_set("invnu").unwrap();
        assert!(o.number);
        assert!(o.apply_set("bogus").is_err());
        assert!(o.apply_set("surround nosurround").is_ok());
        assert_eq!(o.apply_set("sw").unwrap().unwrap(), "shiftwidth=2");
    }
}

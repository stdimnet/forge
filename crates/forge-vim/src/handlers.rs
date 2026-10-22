//! Shortcut conflicts between Vim and the IDE (IdeaVim's Settings | Editor | Vim): for each
//! Ctrl shortcut and mode, whether Vim or the IDE handles it. `sethandler` lines in
//! `~/.ideavimrc` override the settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Handler {
    Vim,
    Ide,
}

impl Handler {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vim" => Some(Self::Vim),
            "ide" => Some(Self::Ide),
            _ => None,
        }
    }
}

/// Modes a handler applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerMode {
    Normal,
    Insert,
    Visual,
}

/// One conflicting shortcut: what Vim does with it per mode (`None`: nothing, the IDE keeps it).
struct Def {
    shortcut: &'static str,
    normal: Option<&'static str>,
    insert: Option<&'static str>,
    visual: Option<&'static str>,
}

const fn d(
    shortcut: &'static str,
    normal: Option<&'static str>,
    insert: Option<&'static str>,
    visual: Option<&'static str>,
) -> Def {
    Def {
        shortcut,
        normal,
        insert,
        visual,
    }
}

const DEFS: [Def; 22] = [
    d(
        "Ctrl+A",
        Some("Increment number"),
        None,
        Some("Increment numbers"),
    ),
    d(
        "Ctrl+B",
        Some("Scroll page up"),
        None,
        Some("Scroll page up"),
    ),
    d(
        "Ctrl+C",
        Some("Cancel"),
        Some("Exit Insert mode"),
        Some("Exit Visual mode"),
    ),
    d(
        "Ctrl+D",
        Some("Scroll half page down"),
        Some("Unindent line"),
        Some("Scroll half page down"),
    ),
    d(
        "Ctrl+E",
        Some("Scroll line down"),
        Some("Insert character below"),
        Some("Scroll line down"),
    ),
    d(
        "Ctrl+F",
        Some("Scroll page down"),
        None,
        Some("Scroll page down"),
    ),
    d(
        "Ctrl+H",
        Some("Move left"),
        Some("Delete character before caret"),
        Some("Move left"),
    ),
    d("Ctrl+I", Some("Jump forward"), None, None),
    d(
        "Ctrl+J",
        Some("Move down"),
        Some("New line"),
        Some("Move down"),
    ),
    d("Ctrl+N", Some("Move down"), None, Some("Move down")),
    d(
        "Ctrl+O",
        Some("Jump back"),
        Some("Execute one command"),
        None,
    ),
    d("Ctrl+P", Some("Move up"), None, Some("Move up")),
    d("Ctrl+R", Some("Redo"), Some("Insert register"), None),
    d("Ctrl+T", None, Some("Indent line"), None),
    d(
        "Ctrl+U",
        Some("Scroll half page up"),
        Some("Delete to line start"),
        Some("Scroll half page up"),
    ),
    d(
        "Ctrl+V",
        Some("Visual block mode"),
        None,
        Some("Visual block mode"),
    ),
    d(
        "Ctrl+W",
        Some("Window commands"),
        Some("Delete word before caret"),
        Some("Window commands"),
    ),
    d(
        "Ctrl+X",
        Some("Decrement number"),
        None,
        Some("Decrement numbers"),
    ),
    d(
        "Ctrl+Y",
        Some("Scroll line up"),
        Some("Insert character above"),
        Some("Scroll line up"),
    ),
    d(
        "Ctrl+[",
        Some("Escape"),
        Some("Exit Insert mode"),
        Some("Exit Visual mode"),
    ),
    d("Ctrl+]", Some("Go to declaration"), None, None),
    d("Ctrl+6", Some("Alternate file"), None, None),
];

/// A row of the handler table (Settings | Editor | Vim).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerRow {
    pub shortcut: String,
    /// Vim notation of the key (`<C-r>`).
    pub key: String,
    pub normal: Handler,
    pub insert: Handler,
    pub visual: Handler,
    /// What Vim does with the key per mode; `None` where Vim has no meaning for it.
    pub normal_action: Option<String>,
    pub insert_action: Option<String>,
    pub visual_action: Option<String>,
    /// Whether a line in `~/.ideavimrc` decides this row.
    pub from_rc: bool,
}

/// A stored override (`vim.shortcutHandlers` setting).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HandlerOverride {
    pub shortcut: String,
    pub normal: String,
    pub insert: String,
    pub visual: String,
}

#[derive(Debug, Clone, Default)]
pub struct Handlers {
    settings: Vec<HandlerOverride>,
    /// `sethandler` from the rc file: key → (normal, insert, visual).
    rc: Vec<(String, [Option<Handler>; 3])>,
}

fn key_of(shortcut: &str) -> String {
    crate::keys::shortcut_to_key(shortcut).unwrap_or_default()
}

impl Handlers {
    pub fn set_overrides(&mut self, list: Vec<HandlerOverride>) {
        self.settings = list;
    }

    pub fn clear_rc(&mut self) {
        self.rc.clear();
    }

    /// `sethandler <C-V> n-v:vim i:ide` (or `a:ide`).
    pub fn sethandler(&mut self, args: &str) -> Result<(), String> {
        let mut parts = args.split_whitespace();
        let key_text = parts.next().ok_or("E471: Argument required")?;
        let keys = crate::keys::parse_keys(key_text, "\\");
        let key = match keys.as_slice() {
            [k] if k.starts_with("<C-") || k == "<Esc>" => {
                if key_text.eq_ignore_ascii_case("<C-[>") {
                    "<C-[>".to_string()
                } else {
                    k.clone()
                }
            }
            _ => return Err(format!("E475: Invalid argument: {key_text}")),
        };
        let mut modes = [None; 3];
        let mut any = false;
        for spec in parts {
            let (m, h) = spec
                .split_once(':')
                .ok_or_else(|| format!("E475: Invalid argument: {spec}"))?;
            let h = Handler::parse(h).ok_or_else(|| format!("E475: Invalid argument: {spec}"))?;
            for m in m.split('-') {
                let idx: &[usize] = match m {
                    "n" => &[0],
                    "i" => &[1],
                    "v" | "x" | "s" => &[2],
                    "a" => &[0, 1, 2],
                    _ => return Err(format!("E475: Invalid argument: {spec}")),
                };
                for i in idx {
                    modes[*i] = Some(h);
                }
            }
            any = true;
        }
        if !any {
            return Err("E471: Argument required".into());
        }
        self.rc.retain(|(k, _)| *k != key);
        self.rc.push((key, modes));
        Ok(())
    }

    pub fn table(&self) -> Vec<HandlerRow> {
        DEFS.iter()
            .map(|def| {
                let key = key_of(def.shortcut);
                let default = |a: Option<&str>| {
                    if a.is_some() {
                        Handler::Vim
                    } else {
                        Handler::Ide
                    }
                };
                let mut h = [
                    default(def.normal),
                    default(def.insert),
                    default(def.visual),
                ];
                if let Some(o) = self.settings.iter().find(|o| o.shortcut == def.shortcut) {
                    for (slot, v) in h.iter_mut().zip([&o.normal, &o.insert, &o.visual]) {
                        if let Some(x) = Handler::parse(v) {
                            *slot = x;
                        }
                    }
                }
                let rc = self.rc.iter().find(|(k, _)| *k == key);
                if let Some((_, modes)) = rc {
                    for (slot, m) in h.iter_mut().zip(modes) {
                        if let Some(m) = m {
                            *slot = *m;
                        }
                    }
                }
                HandlerRow {
                    shortcut: def.shortcut.to_string(),
                    key,
                    normal: h[0],
                    insert: h[1],
                    visual: h[2],
                    normal_action: def.normal.map(str::to_string),
                    insert_action: def.insert.map(str::to_string),
                    visual_action: def.visual.map(str::to_string),
                    from_rc: rc.is_some(),
                }
            })
            .collect()
    }

    /// Who handles `key` (Vim notation) in `mode`. Ctrl keys outside the table belong to the IDE.
    pub fn handler(&self, key: &str, mode: HandlerMode) -> Handler {
        let key = if key == "<Esc>" { "<C-[>" } else { key };
        let Some(def) = DEFS.iter().find(|d| key_of(d.shortcut) == key) else {
            return Handler::Ide;
        };
        let row = self
            .table()
            .into_iter()
            .find(|r| r.shortcut == def.shortcut)
            .expect("row");
        match mode {
            HandlerMode::Normal => row.normal,
            HandlerMode::Insert => row.insert,
            HandlerMode::Visual => row.visual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_what_vim_implements() {
        let h = Handlers::default();
        assert_eq!(h.handler("<C-r>", HandlerMode::Normal), Handler::Vim);
        assert_eq!(h.handler("<C-r>", HandlerMode::Visual), Handler::Ide);
        assert_eq!(h.handler("<C-v>", HandlerMode::Insert), Handler::Ide);
        assert_eq!(h.handler("<C-s>", HandlerMode::Normal), Handler::Ide);
        assert_eq!(h.table().len(), DEFS.len());
    }

    #[test]
    fn applies_settings_then_rc() {
        let mut h = Handlers::default();
        h.set_overrides(vec![HandlerOverride {
            shortcut: "Ctrl+W".into(),
            normal: "ide".into(),
            insert: "ide".into(),
            visual: String::new(),
        }]);
        assert_eq!(h.handler("<C-w>", HandlerMode::Normal), Handler::Ide);
        assert_eq!(h.handler("<C-w>", HandlerMode::Visual), Handler::Vim);
        h.sethandler("<C-W> n-v:vim i:ide").unwrap();
        assert_eq!(h.handler("<C-w>", HandlerMode::Normal), Handler::Vim);
        h.sethandler("<C-V> a:ide").unwrap();
        assert_eq!(h.handler("<C-v>", HandlerMode::Visual), Handler::Ide);
        assert!(
            h.table()
                .iter()
                .find(|r| r.shortcut == "Ctrl+V")
                .unwrap()
                .from_rc
        );
        assert!(h.sethandler("<C-V> q:ide").is_err());
        assert!(h.sethandler("x n:ide").is_err());
    }
}

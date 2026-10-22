//! The language families the editor actions know.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Php,
    /// JavaScript and TypeScript (with JSX).
    Js,
    Lua,
}

impl Lang {
    /// The family of a Forge file type id (`php`, `typescript`, `lua`, …).
    pub fn from_file_type(id: &str) -> Option<Lang> {
        match id {
            "php" | "php-template" => Some(Lang::Php),
            "javascript" | "jsx" | "typescript" | "tsx" | "js" | "ts" => Some(Lang::Js),
            "lua" => Some(Lang::Lua),
            _ => None,
        }
    }

    /// Group title in Settings | Editor | General | Postfix Completion.
    pub fn label(self) -> &'static str {
        match self {
            Lang::Php => "PHP",
            Lang::Js => "JavaScript and TypeScript",
            Lang::Lua => "Lua",
        }
    }

    /// Prefix of template ids (`php.if`, `js.log`, `lua.ipairs`).
    pub fn id(self) -> &'static str {
        match self {
            Lang::Php => "php",
            Lang::Js => "js",
            Lang::Lua => "lua",
        }
    }

    /// Characters that belong to identifiers besides letters, digits and `_`.
    pub(crate) fn is_ident(self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || (c == '$' && self != Lang::Lua)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_file_types() {
        assert_eq!(Lang::from_file_type("php"), Some(Lang::Php));
        assert_eq!(Lang::from_file_type("tsx"), Some(Lang::Js));
        assert_eq!(Lang::from_file_type("lua"), Some(Lang::Lua));
        assert_eq!(Lang::from_file_type("markdown"), None);
        assert!(Lang::Php.is_ident('$'));
        assert!(!Lang::Lua.is_ident('$'));
    }
}

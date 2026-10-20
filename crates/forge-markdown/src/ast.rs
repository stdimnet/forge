//! The Markdown AST sent to the UI (JSON tagged by `t`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Inline {
    Text {
        text: String,
    },
    Code {
        text: String,
        /// Documentation only: the code with URLs and class names marked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parts: Option<Vec<Inline>>,
    },
    Strong {
        children: Vec<Inline>,
    },
    Em {
        children: Vec<Inline>,
    },
    Del {
        children: Vec<Inline>,
    },
    /// A link; the UI decides whether `href` may be opened (only `http(s)`).
    Link {
        href: String,
        children: Vec<Inline>,
    },
    /// A bare URL in documentation text.
    Url {
        text: String,
    },
    /// A fully qualified PHP class name in documentation text (`\App\Models\User`).
    Fqcn {
        text: String,
    },
    Break,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    /// Task list items: `[x]` / `[ ]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Block {
    Heading {
        level: u8,
        inline: Vec<Inline>,
    },
    Paragraph {
        inline: Vec<Inline>,
    },
    /// Text of a tight list item (no paragraph spacing).
    Plain {
        inline: Vec<Inline>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
    Code {
        /// Lower-cased language of the fence (`php`), empty when none.
        lang: String,
        text: String,
        /// An unterminated fence (a message still streaming).
        open: bool,
    },
    Quote {
        blocks: Vec<Block>,
    },
    Hr,
}

impl Inline {
    /// Plain text of inline content.
    pub fn text_of(items: &[Inline]) -> String {
        let mut out = String::new();
        for i in items {
            match i {
                Inline::Text { text }
                | Inline::Code { text, .. }
                | Inline::Url { text }
                | Inline::Fqcn { text } => out.push_str(text),
                Inline::Strong { children }
                | Inline::Em { children }
                | Inline::Del { children }
                | Inline::Link { children, .. } => out.push_str(&Inline::text_of(children)),
                Inline::Break => out.push('\n'),
            }
        }
        out
    }
}

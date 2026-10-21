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
    /// An image (preview only; chat and documentation keep images as links). `src` is the
    /// URL or path as written; the UI resolves relative paths against the document.
    Image {
        src: String,
        alt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Size from an HTML `<img width height>`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<String>,
    },
    /// A footnote reference (`[^note]`), numbered in the order of first use (preview only).
    FootnoteRef {
        label: String,
        index: u32,
    },
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
        /// Anchor id like GitHub's (`#getting-started`), preview only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
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
        /// A GitHub alert (`> [!NOTE]`, preview only): `note`, `tip`, `important`,
        /// `warning` or `caution`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    Hr,
    /// A GitHub-flavored table (preview only). `align` per column: `left`, `center`,
    /// `right` or empty.
    Table {
        align: Vec<String>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
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
                Inline::Image { alt, .. } => out.push_str(alt),
                Inline::FootnoteRef { index, .. } => out.push_str(&index.to_string()),
            }
        }
        out
    }
}

//! Turns a chat message plus editor context (attached files, selection) into the prompt
//! text sent to the agent.

use serde::{Deserialize, Serialize};

/// A selection in an open editor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
}

/// Editor context attached to a message with `@`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MessageContext {
    /// Workspace-relative paths of attached files.
    pub files: Vec<String>,
    pub selection: Option<Selection>,
}

/// Compose the prompt. Files are referenced with `@path` (the agent reads them itself);
/// a selection is inlined because it may be unsaved.
pub fn compose_prompt(text: &str, ctx: &MessageContext) -> String {
    let mut out = text.trim_end().to_string();
    if ctx.files.is_empty() && ctx.selection.is_none() {
        return out;
    }
    out.push_str("\n\n");
    if !ctx.files.is_empty() {
        let refs: Vec<String> = ctx.files.iter().map(|f| format!("@{f}")).collect();
        out.push_str(&format!("Attached files: {}\n", refs.join(" ")));
    }
    if let Some(sel) = &ctx.selection {
        let fence = if sel.text.contains("```") {
            "````"
        } else {
            "```"
        };
        let lang = sel.path.rsplit('.').next().unwrap_or("");
        out.push_str(&format!(
            "Selected code in {} (lines {}-{}):\n{fence}{lang}\n{}\n{fence}\n",
            sel.path,
            sel.start_line,
            sel.end_line,
            sel.text.trim_end()
        ));
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(
            compose_prompt("hello\n", &MessageContext::default()),
            "hello"
        );
    }

    #[test]
    fn adds_files_and_selection() {
        let ctx = MessageContext {
            files: vec!["app/Models/Order.php".into()],
            selection: Some(Selection {
                path: "app/Models/User.php".into(),
                start_line: 3,
                end_line: 4,
                text: "class User\n{".into(),
            }),
        };
        assert_eq!(
            compose_prompt("Explain", &ctx),
            "Explain\n\nAttached files: @app/Models/Order.php\n\
             Selected code in app/Models/User.php (lines 3-4):\n```php\nclass User\n{\n```"
        );
    }
}

//! Deno: tasks of `deno.json` / `deno.jsonc` (run as `deno task <name>`), shown in Run
//! Anything and the Deno run configuration.

use serde::{Deserialize, Serialize};

use forge_jsonschema::tree::Node;
use forge_jsonschema::DocKind;

/// Config file names, in lookup order.
pub const CONFIG_FILES: &[&str] = &["deno.json", "deno.jsonc"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub name: String,
    pub command: String,
    /// 0-based line in the config file.
    pub line: u32,
}

/// The tasks of a Deno config (JSON with comments). Deno 2 object tasks
/// (`{ "command": …, "description": … }`) are read too.
pub fn tasks(text: &str) -> Vec<Task> {
    let Some(doc) = forge_jsonschema::parse(DocKind::Json, text)
        .into_iter()
        .next()
    else {
        return Vec::new();
    };
    doc.get("tasks")
        .and_then(Node::as_object)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            let v = t.value.as_ref()?;
            let command = v
                .as_str()
                .or_else(|| v.get("command").and_then(Node::as_str))?
                .to_string();
            Some(Task {
                name: t.key.clone(),
                command,
                line: text[..t.key_start].matches('\n').count() as u32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tasks_with_comments() {
        let text = "{\n  // tasks\n  \"tasks\": {\n    \"dev\": \"deno run --watch main.ts\",\n    \"test\": { \"command\": \"deno test -A\", \"description\": \"Tests\" }\n  },\n  \"imports\": {}\n}\n";
        let t = tasks(text);
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].name.as_str(), t[0].command.as_str(), t[0].line),
            ("dev", "deno run --watch main.ts", 3)
        );
        assert_eq!(t[1].command, "deno test -A");
        assert!(tasks("not json").is_empty());
    }
}

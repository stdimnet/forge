//! MCP lifecycle pieces that need no I/O: protocol version negotiation, the `initialize`
//! result, prompts and resource URIs.

use serde::Serialize;
use serde_json::{json, Value};

/// Protocol versions Forge speaks, newest first.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const LATEST_VERSION: &str = SUPPORTED_VERSIONS[0];

/// URI prefix of project files as resources: `forge://project/src/App.php`.
pub const RESOURCE_PREFIX: &str = "forge://project/";

/// The version to answer for a client's requested version: the same when supported, else the
/// latest (the client then decides whether it can continue).
pub fn negotiate_version(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| SUPPORTED_VERSIONS.iter().find(|v| **v == r).copied())
        .unwrap_or(LATEST_VERSION)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub title: String,
    pub version: String,
}

impl ServerInfo {
    pub fn forge(version: impl Into<String>) -> Self {
        Self {
            name: "forge".into(),
            title: "Forge IDE".into(),
            version: version.into(),
        }
    }
}

/// What the server tells clients in `initialize`.
pub const INSTRUCTIONS: &str = "Forge is the IDE the user is working in. Its tools work on the project open in Forge: paths are relative to the project root, lines and columns are 1-based. Prefer get_current_file to see what the user is looking at, search_in_files / find_symbol / go_to_definition / find_usages to explore, get_problems after edits, and replace_text_in_file or write_file to change code (the user sees the change in the editor and can undo it). Write tools, run configurations, terminal commands and database queries may wait for the user's approval in Forge.";

/// The `initialize` result.
pub fn initialize_result(version: &str, info: &ServerInfo) -> Value {
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false },
            "prompts": { "listChanged": false },
            "logging": {},
        },
        "serverInfo": info,
        "instructions": INSTRUCTIONS,
    })
}

/// The project-relative path of a `forge://project/…` URI.
pub fn resource_path(uri: &str) -> Option<&str> {
    uri.strip_prefix(RESOURCE_PREFIX).filter(|p| !p.is_empty())
}

pub fn resource_uri(path: &str) -> String {
    format!("{RESOURCE_PREFIX}{path}")
}

/// `resources/templates/list`: any project file by path.
pub fn resource_templates() -> Value {
    json!({
        "resourceTemplates": [{
            "uriTemplate": format!("{RESOURCE_PREFIX}{{path}}"),
            "name": "Project file",
            "description": "A file of the project open in Forge, by its path relative to the project root",
        }]
    })
}

struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// Optional argument and its description.
    argument: Option<(&'static str, &'static str)>,
    text: &'static str,
}

const PROMPTS: [Prompt; 3] = [
    Prompt {
        name: "review_changes",
        title: "Review Uncommitted Changes",
        description: "Review the uncommitted changes of the project open in Forge",
        argument: Some(("focus", "What to pay attention to (optional)")),
        text: "Review my uncommitted changes in Forge. Use git_status and git_diff to see them, read surrounding code with read_file where needed, and check get_problems for the changed files. Point out bugs, risky changes and missing tests, most important first.",
    },
    Prompt {
        name: "fix_problems",
        title: "Fix Problems in the Current File",
        description: "Fix the errors and warnings Forge shows for the file in the active editor",
        argument: None,
        text: "Use get_current_file to find the file I am looking at in Forge and get_problems for its errors and warnings. Fix them with replace_text_in_file, then check get_problems again.",
    },
    Prompt {
        name: "explain_selection",
        title: "Explain the Selection",
        description: "Explain the code selected in Forge's editor",
        argument: None,
        text: "Use get_current_file to read the code I selected in Forge and explain what it does. Use go_to_definition and find_usages for the symbols it uses where that helps.",
    },
];

/// `prompts/list`.
pub fn prompts_list() -> Value {
    let prompts: Vec<Value> = PROMPTS
        .iter()
        .map(|p| {
            let arguments: Vec<Value> = p
                .argument
                .iter()
                .map(|(name, description)| {
                    json!({ "name": name, "description": description, "required": false })
                })
                .collect();
            json!({ "name": p.name, "title": p.title, "description": p.description, "arguments": arguments })
        })
        .collect();
    json!({ "prompts": prompts })
}

/// `prompts/get`; `None` for an unknown prompt.
pub fn prompt_get(name: &str, arguments: &Value) -> Option<Value> {
    let p = PROMPTS.iter().find(|p| p.name == name)?;
    let mut text = p.text.to_string();
    if let Some((arg, _)) = p.argument {
        if let Some(v) = arguments
            .get(arg)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
        {
            text.push_str(&format!("\n\nFocus on: {}", v.trim()));
        }
    }
    Some(json!({
        "description": p.description,
        "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_versions() {
        assert_eq!(negotiate_version(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate_version(Some("2024-11-05")), "2024-11-05");
        assert_eq!(negotiate_version(Some("2099-01-01")), LATEST_VERSION);
        assert_eq!(negotiate_version(None), LATEST_VERSION);
    }

    #[test]
    fn initialize_announces_capabilities() {
        let r = initialize_result("2025-06-18", &ServerInfo::forge("1.2.3"));
        assert_eq!(r["protocolVersion"], "2025-06-18");
        assert!(r["capabilities"]["tools"].is_object());
        assert!(r["capabilities"]["resources"].is_object());
        assert_eq!(r["serverInfo"]["name"], "forge");
        assert_eq!(r["serverInfo"]["version"], "1.2.3");
    }

    #[test]
    fn prompts_and_resources() {
        assert_eq!(prompts_list()["prompts"].as_array().unwrap().len(), 3);
        let p = prompt_get("review_changes", &json!({ "focus": "security" })).unwrap();
        let text = p["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.ends_with("Focus on: security"));
        assert!(prompt_get("nope", &Value::Null).is_none());
        assert_eq!(
            resource_path("forge://project/src/a.php"),
            Some("src/a.php")
        );
        assert_eq!(resource_path("file:///etc/passwd"), None);
        assert_eq!(resource_uri("a/b.ts"), "forge://project/a/b.ts");
    }
}

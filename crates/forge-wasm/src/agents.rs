//! Agent CLIs (`forge_agent_cli`) for the browser-mode agent mock: the built-in agents'
//! profiles (what `agent_list` reports besides detection) and the stream parsers, so the
//! mock replays recorded CLI output through the same Rust code as the desktop app.

use forge_agent_cli::{profile::builtin_profiles, pure_parser};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

/// Profiles of the built-in agents (`AgentProfile[]`), in picker order.
#[wasm_bindgen]
pub fn agent_profiles() -> String {
    to_json(&builtin_profiles())
}

#[derive(Deserialize)]
struct ParseRequest {
    agent: String,
    lines: Vec<String>,
}

/// `{ agent, lines }` → the `AgentEvent[]` those raw stdout lines of the agent's CLI
/// produce.
#[wasm_bindgen]
pub fn agent_parse_lines(request_json: &str) -> String {
    let r = match parse::<ParseRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let Some(mut parser) = pure_parser(&r.agent) else {
        return to_json(&serde_json::json!({ "error": format!("no parser for {}", r.agent) }));
    };
    let events: Vec<_> = r.lines.iter().flat_map(|l| parser.parse_line(l)).collect();
    to_json(&events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gemini_lines_and_lists_profiles() {
        let out = agent_parse_lines(
            r#"{"agent":"gemini-cli","lines":["{\"type\":\"tool_use\",\"tool_name\":\"read_file\",\"tool_id\":\"t1\",\"parameters\":{\"absolute_path\":\"/p/a.php\"}}","noise"]}"#,
        );
        assert!(
            out.contains(r#""type":"toolUse""#) && out.contains(r#""name":"Read""#),
            "{out}"
        );
        assert!(agent_parse_lines(r#"{"agent":"nope","lines":[]}"#).contains("error"));
        assert!(agent_profiles().contains(r#""installHint":"npm install -g @google/gemini-cli""#));
    }
}

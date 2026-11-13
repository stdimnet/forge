//! The JSON reports of the audit tools, each turned into [`Findings`].

pub mod cargo;
pub mod composer;
pub mod depcheck;
pub mod go;
pub mod npm;
pub mod pip;

use serde_json::Value;

use crate::model::Findings;
use crate::tool::Tool;

/// Parse what `tool` printed on stdout.
pub fn parse(tool: Tool, stdout: &str) -> Result<Findings, String> {
    let found = match tool {
        Tool::Composer => composer::parse(stdout),
        Tool::Npm | Tool::Pnpm | Tool::YarnClassic | Tool::YarnBerry => npm::parse(stdout),
        Tool::PipAudit => pip::parse(stdout),
        Tool::CargoAudit => cargo::parse(stdout),
        Tool::Govulncheck => go::parse(stdout),
        Tool::DependencyCheck => depcheck::parse(stdout),
    }?;
    Ok(found.normalized())
}

pub(crate) fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
}

pub(crate) fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The JSON document after notices a tool may print first.
pub(crate) fn document(out: &str, tool: &str) -> Result<Value, String> {
    let start = out
        .find(['{', '['])
        .ok_or_else(|| format!("{tool} printed no JSON report"))?;
    serde_json::from_str(&out[start..]).map_err(|e| format!("cannot read the {tool} report: {e}"))
}

/// The first sentence of a description, for advisories without a title.
pub(crate) fn first_sentence(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("");
    let end = line
        .find(". ")
        .map(|i| i + 1)
        .unwrap_or(line.len())
        .min(160);
    let mut end = end.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].trim_end_matches('.').to_string()
}

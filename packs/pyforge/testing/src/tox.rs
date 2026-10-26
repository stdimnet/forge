//! tox environments from the project's configuration, read without running tox:
//! `tox.ini` (`[tox] envlist` / `env_list`, `[testenv:<name>]` sections), `setup.cfg`
//! (`[tox:tox]`), `tox.toml` (`env_list`, `[env.<name>]`) and `pyproject.toml`
//! (`[tool.tox] legacy_tox_ini`, or `env_list` with `[tool.tox.env.<name>]`). Generative
//! names (`py{311,312}-django{42,50}`) are expanded.

use serde::{Deserialize, Serialize};

/// A tox environment for the Tox tool window and the `tox` configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToxEnv {
    pub name: String,
    /// In the default `env_list` (`tox` without `-e` runs it).
    pub default: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The configuration file and 0-based line that declares it.
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Expand `py{311,312}-django{42,50}` (and ranges `py3{10-12}`) into names.
pub fn expand(name: &str) -> Vec<String> {
    let Some(open) = name.find('{') else {
        return vec![name.to_string()];
    };
    let Some(close) = name[open..].find('}').map(|c| c + open) else {
        return vec![name.to_string()];
    };
    let (head, body, tail) = (&name[..open], &name[open + 1..close], &name[close + 1..]);
    let mut alternatives: Vec<String> = Vec::new();
    for part in body.split(',').map(str::trim) {
        match part.split_once('-') {
            Some((a, b)) if a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok() => {
                let (a, b) = (a.parse::<u32>().unwrap_or(0), b.parse::<u32>().unwrap_or(0));
                if b >= a && b - a < 100 {
                    alternatives.extend((a..=b).map(|n| n.to_string()));
                }
            }
            _ => alternatives.push(part.to_string()),
        }
    }
    let mut out = Vec::new();
    for alt in alternatives {
        for rest in expand(tail) {
            out.push(format!("{head}{alt}{rest}"));
        }
    }
    out
}

/// Split an env list (commas or newlines, braces kept together) and expand it.
fn env_list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in value.chars() {
        match c {
            '{' => {
                depth += 1;
                cur.push(c);
            }
            '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' | '\n' if depth == 0 => items.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    items.push(cur);
    items
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .flat_map(expand)
        .collect()
}

/// A section of an INI file: its name, 0-based line and keys.
type Section = (String, u32, Vec<(String, String)>);

/// An INI file's sections, with continuation lines joined.
fn ini_sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            out.push((
                trimmed[1..trimmed.len() - 1].trim().to_string(),
                i as u32,
                Vec::new(),
            ));
            continue;
        }
        let Some((_, _, keys)) = out.last_mut() else {
            continue;
        };
        if line.starts_with([' ', '\t']) {
            if let Some((_, v)) = keys.last_mut() {
                v.push('\n');
                v.push_str(trimmed);
            }
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            keys.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

fn key<'a>(keys: &'a [(String, String)], names: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find(|(k, _)| names.contains(&k.as_str()))
        .map(|(_, v)| v.as_str())
}

/// Environments of an INI configuration (`tox.ini`, `setup.cfg`, `legacy_tox_ini`).
pub fn from_ini(text: &str, file: &str, main_section: &str, line_offset: u32) -> Vec<ToxEnv> {
    let sections = ini_sections(text);
    let mut envs: Vec<ToxEnv> = Vec::new();
    if let Some((_, line, keys)) = sections.iter().find(|(n, _, _)| n == main_section) {
        for name in env_list(key(keys, &["envlist", "env_list"]).unwrap_or_default()) {
            envs.push(ToxEnv {
                name,
                default: true,
                description: String::new(),
                file: file.to_string(),
                line: Some(line + line_offset),
            });
        }
    }
    let base_description = sections
        .iter()
        .find(|(n, _, _)| n == "testenv")
        .and_then(|(_, _, k)| key(k, &["description"]))
        .unwrap_or_default()
        .to_string();
    for (section, line, keys) in &sections {
        let Some(name) = section.strip_prefix("testenv:") else {
            continue;
        };
        let description = key(keys, &["description"]).unwrap_or_default().to_string();
        for n in expand(name.trim()) {
            match envs.iter_mut().find(|e| e.name == n) {
                Some(e) => {
                    e.line = Some(line + line_offset);
                    e.description = description.clone();
                }
                None => envs.push(ToxEnv {
                    name: n,
                    default: false,
                    description: description.clone(),
                    file: file.to_string(),
                    line: Some(line + line_offset),
                }),
            }
        }
    }
    for e in &mut envs {
        if e.description.is_empty() {
            e.description = base_description.clone();
        }
    }
    envs
}

/// The 0-based line of a TOML table header.
fn table_line(text: &str, header: &str) -> Option<u32> {
    text.lines()
        .position(|l| l.trim() == header)
        .map(|i| i as u32)
}

/// Environments of a TOML configuration (`tox.toml`, or `pyproject.toml`'s `[tool.tox]`).
pub fn from_toml(text: &str, file: &str) -> Vec<ToxEnv> {
    let Ok(doc) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let (table, prefix) = if file.ends_with("pyproject.toml") {
        match doc
            .get("tool")
            .and_then(|t| t.get("tox"))
            .and_then(|t| t.as_table())
        {
            Some(t) => (t.clone(), "tool.tox."),
            None => return Vec::new(),
        }
    } else {
        (doc, "")
    };
    if let Some(ini) = table.get("legacy_tox_ini").and_then(|v| v.as_str()) {
        let offset = text
            .lines()
            .position(|l| l.contains("legacy_tox_ini"))
            .map_or(0, |i| i as u32 + 1);
        return from_ini(ini.trim_start_matches('\n'), file, "tox", offset);
    }
    let mut envs: Vec<ToxEnv> = Vec::new();
    let list = table
        .get("env_list")
        .or_else(|| table.get("envlist"))
        .and_then(|v| v.as_array());
    for name in list.into_iter().flatten().filter_map(|v| v.as_str()) {
        for n in expand(name) {
            envs.push(ToxEnv {
                name: n,
                default: true,
                description: String::new(),
                file: file.to_string(),
                line: None,
            });
        }
    }
    if let Some(defined) = table.get("env").and_then(|v| v.as_table()) {
        for (name, body) in defined {
            let description = body
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or_default()
                .to_string();
            let line = table_line(text, &format!("[{prefix}env.{name}]"));
            match envs.iter_mut().find(|e| e.name == *name) {
                Some(e) => {
                    e.description = description;
                    e.line = line;
                }
                None => envs.push(ToxEnv {
                    name: name.clone(),
                    default: false,
                    description,
                    file: file.to_string(),
                    line,
                }),
            }
        }
    }
    envs
}

/// The environments of the project: the first configuration tox itself would use
/// (`tox.ini`, `setup.cfg`, `pyproject.toml`, `tox.toml`).
pub fn environments(read: impl Fn(&str) -> Option<String>) -> Vec<ToxEnv> {
    if let Some(t) = read("tox.ini") {
        return from_ini(&t, "tox.ini", "tox", 0);
    }
    if let Some(t) = read("setup.cfg").filter(|t| t.contains("[tox:tox]")) {
        return from_ini(&t, "setup.cfg", "tox:tox", 0);
    }
    if let Some(t) = read("pyproject.toml").filter(|t| t.contains("[tool.tox")) {
        return from_toml(&t, "pyproject.toml");
    }
    read("tox.toml")
        .map(|t| from_toml(&t, "tox.toml"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_generative_names() {
        assert_eq!(
            expand("py{311,312}-django{42,50}"),
            [
                "py311-django42",
                "py311-django50",
                "py312-django42",
                "py312-django50"
            ]
        );
        assert_eq!(expand("py3{10-12}"), ["py310", "py311", "py312"]);
        assert_eq!(expand("lint"), ["lint"]);
        assert_eq!(
            env_list("py{311, 312}, lint\n  docs"),
            ["py311", "py312", "lint", "docs"]
        );
    }

    #[test]
    fn reads_tox_ini() {
        let ini = "[tox]\nenvlist =\n    py{311,312}\n    lint\n\n[testenv]\ndescription = run the tests\ncommands = pytest {posargs}\n\n[testenv:lint]\ndescription = check the code\ncommands = ruff check .\n\n[testenv:docs]\ncommands = mkdocs build\n";
        let envs = from_ini(ini, "tox.ini", "tox", 0);
        let names: Vec<_> = envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["py311", "py312", "lint", "docs"]);
        assert!(envs[0].default && !envs[3].default);
        assert_eq!(envs[0].description, "run the tests");
        assert_eq!(envs[2].description, "check the code");
        assert_eq!(envs[2].line, Some(9));
        assert_eq!(envs[0].line, Some(0));
    }

    #[test]
    fn reads_toml_and_pyproject() {
        let toml = "env_list = [\"3.12\", \"lint\"]\n\n[env.lint]\ndescription = \"ruff\"\ncommands = [[\"ruff\", \"check\"]]\n\n[env.type]\ncommands = [[\"mypy\", \"src\"]]\n";
        let envs = from_toml(toml, "tox.toml");
        let names: Vec<_> = envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["3.12", "lint", "type"]);
        assert_eq!(envs[1].line, Some(2));
        assert_eq!(envs[1].description, "ruff");
        let pyproject = "[project]\nname = \"x\"\n\n[tool.tox]\nlegacy_tox_ini = \"\"\"\n[tox]\nenv_list = py312, lint\n[testenv:lint]\ncommands = ruff check .\n\"\"\"\n";
        let envs = environments(|f| (f == "pyproject.toml").then(|| pyproject.to_string()));
        assert_eq!(
            envs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["py312", "lint"]
        );
        assert_eq!(envs[1].line, Some(7));
        assert!(environments(|_| None).is_empty());
    }
}

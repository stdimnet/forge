//! `project.godot`: Godot's project file, an INI-like file of sections and `key=value`
//! lines whose values are Godot variants (`"strings"`, numbers, `PackedStringArray(…)`,
//! dictionaries that may span lines).

use serde::{Deserialize, Serialize};

pub const PROJECT_FILE: &str = "project.godot";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Autoload {
    pub name: String,
    /// `res://` path (without the `*` that marks singletons).
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotProject {
    pub name: String,
    /// `run/main_scene` (`res://…`; Godot 4.4+ may store a `uid://`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_scene: Option<String>,
    /// The engine version of `config/features` (`4.3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub features: Vec<String>,
    /// `config_version` (5 for Godot 4, 4 for Godot 3).
    #[serde(default)]
    pub config_version: u32,
    /// A C# (.NET) project: the `C#` feature or a `[dotnet]` section.
    pub csharp: bool,
    /// `[dotnet] project/assembly_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_name: Option<String>,
    pub autoloads: Vec<Autoload>,
}

/// The strings of a variant: `"a"`, `PackedStringArray("4.3", "C#")`.
pub fn strings_of(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        let mut escaped = false;
        for c in chars.by_ref() {
            if escaped {
                s.push(match c {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                break;
            } else {
                s.push(c);
            }
        }
        out.push(s);
    }
    out
}

fn balanced(v: &str) -> bool {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for c in v.chars() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
    }
    depth <= 0 && !in_str
}

/// The `(section, key, value)` entries of a Godot config file (`project.godot`, `.tscn`
/// headers aside), values joined across lines.
pub fn entries(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let l = line.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') && l.ends_with(']') {
            section = l[1..l.len() - 1].trim().to_string();
            continue;
        }
        let Some((key, value)) = l.split_once('=') else {
            continue;
        };
        let mut value = value.trim().to_string();
        while !balanced(&value) {
            match lines.next() {
                Some(next) => {
                    value.push('\n');
                    value.push_str(next);
                }
                None => break,
            }
        }
        out.push((section.clone(), key.trim().to_string(), value));
    }
    out
}

/// Parse `project.godot`.
pub fn parse_project_godot(text: &str) -> GodotProject {
    let mut p = GodotProject::default();
    for (section, key, value) in entries(text) {
        let first = || strings_of(&value).into_iter().next();
        match (section.as_str(), key.as_str()) {
            ("", "config_version") => p.config_version = value.trim().parse().unwrap_or(0),
            ("application", "config/name") => p.name = first().unwrap_or_default(),
            ("application", "run/main_scene") => p.main_scene = first().filter(|s| !s.is_empty()),
            ("application", "config/features") => p.features = strings_of(&value),
            ("dotnet", "project/assembly_name") => {
                p.csharp = true;
                p.assembly_name = first().filter(|s| !s.is_empty());
            }
            ("autoload", name) => {
                if let Some(path) = first() {
                    p.autoloads.push(super::project::Autoload {
                        name: name.to_string(),
                        path: path.trim_start_matches('*').to_string(),
                    });
                }
            }
            _ => {}
        }
        if section == "dotnet" {
            p.csharp = true;
        }
    }
    p.version = p
        .features
        .iter()
        .find(|f| f.chars().next().is_some_and(|c| c.is_ascii_digit()) && f.contains('.'))
        .cloned();
    if p.features.iter().any(|f| f == "C#" || f == "dotnet") {
        p.csharp = true;
    }
    p
}

/// The scenes among project files (`.tscn`, `.scn`), sorted.
pub fn scenes(files: &[String]) -> Vec<String> {
    let mut out: Vec<String> = files
        .iter()
        .filter(|f| {
            let l = f.to_ascii_lowercase();
            (l.ends_with(".tscn") || l.ends_with(".scn")) && !l.starts_with(".godot/")
        })
        .cloned()
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const DODGE: &str = r#"; Engine configuration file.
; It's best edited using the editor UI and not directly,
; since the parameters that go here are not all obvious.

config_version=5

[application]

config/name="Dodge the Creeps"
config/description="A simple game: \"dodge\" the creeps."
run/main_scene="res://main.tscn"
config/features=PackedStringArray("4.3", "C#", "Forward Plus")
config/icon="res://icon.svg"

[autoload]

Score="*res://autoload/score.gd"

[dotnet]

project/assembly_name="Dodge the Creeps"

[input]

move_left={
"deadzone": 0.5,
"events": [Object(InputEventKey,"resource_local_to_scene":false,"keycode":4194319)]
}
"#;

    #[test]
    fn parses_project_godot() {
        let p = parse_project_godot(DODGE);
        assert_eq!(p.name, "Dodge the Creeps");
        assert_eq!(p.main_scene.as_deref(), Some("res://main.tscn"));
        assert_eq!(p.version.as_deref(), Some("4.3"));
        assert_eq!(p.features, vec!["4.3", "C#", "Forward Plus"]);
        assert_eq!(p.config_version, 5);
        assert!(p.csharp);
        assert_eq!(p.assembly_name.as_deref(), Some("Dodge the Creeps"));
        assert_eq!(
            p.autoloads,
            vec![Autoload {
                name: "Score".into(),
                path: "res://autoload/score.gd".into()
            }]
        );
        // The multi-line input map does not leak keys.
        assert!(!entries(DODGE)
            .iter()
            .any(|(_, k, _)| k == "\"deadzone\": 0.5,"));

        let gd = parse_project_godot("config_version=5\n[application]\nconfig/name=\"Pong\"\nconfig/features=PackedStringArray(\"4.2\", \"GL Compatibility\")\n");
        assert!(!gd.csharp);
        assert_eq!(gd.main_scene, None);
        assert_eq!(strings_of(r#"x("a\"b", "c")"#), vec!["a\"b", "c"]);
        assert_eq!(
            scenes(&[
                "main.tscn".into(),
                "player.gd".into(),
                ".godot/x.tscn".into(),
                "hud.scn".into()
            ]),
            vec!["hud.scn", "main.tscn"]
        );
    }
}

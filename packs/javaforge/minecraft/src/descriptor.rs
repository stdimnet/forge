//! Plugin descriptors in the editor: the `main` class of `plugin.yml`, `paper-plugin.yml`,
//! `bungee.yml` and `velocity-plugin.json` is checked (a valid class name that exists in
//! the module's sources) and navigable with Ctrl+B / Ctrl+Click. Completion and the other
//! keys are validated by the bundled JSON schemas (`forge-jsonschema`).
//!
//! Offsets in and out are UTF-16 code units (CodeMirror's), lines are 0-based.

use serde::Serialize;

use crate::generate::{is_qualified_name, Descriptor};

/// The descriptor kind of a project file, by its file name.
pub fn kind(path: &str) -> Option<Descriptor> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    Descriptor::from_file_name(name)
}

/// The `main` value of a descriptor: its text and UTF-16 range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainEntry {
    pub value: String,
    pub from: usize,
    pub to: usize,
}

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

/// Find the `main` entry: a top-level `main:` key in YAML, a `"main"` member in JSON.
pub fn main_entry(kind: Descriptor, text: &str) -> Option<MainEntry> {
    let (start, end) = if kind == Descriptor::VelocityJson {
        json_main(text)?
    } else {
        yaml_main(text)?
    };
    Some(MainEntry {
        value: text[start..end].to_string(),
        from: utf16(text, start),
        to: utf16(text, end),
    })
}

/// Byte range of the value of the top-level `main:` key (quotes and comments excluded).
fn yaml_main(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let Some(rest) = line.strip_prefix("main:") else {
            continue;
        };
        let value_start = start + 5 + (rest.len() - rest.trim_start().len());
        let mut value = rest.trim_start();
        // Strip a trailing comment and line break.
        if let Some(i) = value.find(" #") {
            value = &value[..i];
        }
        let value = value.trim_end();
        let (quoted_start, inner) = match value.chars().next() {
            Some(q @ ('\'' | '"')) if value.len() >= 2 && value.ends_with(q) => {
                (value_start + 1, &value[1..value.len() - 1])
            }
            _ => (value_start, value),
        };
        return Some((quoted_start, quoted_start + inner.len()));
    }
    None
}

/// Byte range of the string value of the `"main"` member (between the quotes).
fn json_main(text: &str) -> Option<(usize, usize)> {
    let key = text.find("\"main\"")?;
    let after = &text[key + 6..];
    let colon = after.find(':')?;
    if !after[..colon].trim().is_empty() {
        return None;
    }
    let value = &after[colon + 1..];
    let open = value.find('"')?;
    if !value[..open].trim().is_empty() {
        return None;
    }
    let body = &value[open + 1..];
    let close = body.find(['"', '\n'])?;
    let start = key + 6 + colon + 1 + open + 1;
    Some((start, start + close))
}

/// The module folder of a descriptor (`plugins/core/src/main/resources/plugin.yml` →
/// `plugins/core/`), as a prefix to join with source paths; `None` outside
/// `src/main/resources`.
pub fn module_prefix(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let i = path.rfind("src/main/resources/")?;
    Some(path[..i].to_string())
}

/// Where the source of `class` (fully qualified) can be, relative to the project root.
pub fn class_candidates(prefix: &str, class: &str) -> Vec<String> {
    let outer = class.split('$').next().unwrap_or(class);
    let rel = outer.replace('.', "/");
    vec![
        format!("{prefix}src/main/java/{rel}.java"),
        format!("{prefix}src/main/kotlin/{rel}.kt"),
        format!("{prefix}src/main/java/{rel}.kt"),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    /// `error` or `warning`.
    pub severity: &'static str,
    pub message: String,
    pub code: &'static str,
}

/// Check the `main` class of a descriptor. `exists` says whether a project file exists.
pub fn inspect(path: &str, text: &str, exists: &dyn Fn(&str) -> bool) -> Vec<Problem> {
    let Some(kind) = kind(path) else {
        return Vec::new();
    };
    let Some(main) = main_entry(kind, text) else {
        return Vec::new();
    };
    let value = main.value.trim();
    // `${...}` is filled in by the build.
    if value.is_empty() || value.contains("${") {
        return Vec::new();
    }
    let problem = |message: String, code| Problem {
        from: main.from,
        to: main.to,
        severity: "error",
        message,
        code,
    };
    if !is_qualified_name(&value.replace('$', "_")) || !value.contains('.') {
        return vec![problem(
            format!("'{value}' is not a fully qualified class name"),
            "minecraft.mainClass.invalid",
        )];
    }
    if let Some(prefix) = module_prefix(path) {
        if !class_candidates(&prefix, value).iter().any(|c| exists(c)) {
            return vec![problem(
                format!("Cannot resolve class '{value}'"),
                "minecraft.mainClass.unresolved",
            )];
        }
    }
    Vec::new()
}

/// A navigation target (0-based line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub path: String,
    pub line: u32,
    pub name: String,
}

/// Ctrl+B on the `main` value: the class's source file at its declaration. `read` returns
/// a project file's text.
pub fn goto(
    path: &str,
    text: &str,
    pos: usize,
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<Target> {
    let main = main_entry(kind(path)?, text)?;
    if pos < main.from || pos > main.to {
        return None;
    }
    let value = main.value.trim();
    let prefix = module_prefix(path)?;
    let simple = value.rsplit(['.', '$']).next().unwrap_or(value).to_string();
    class_candidates(&prefix, value).into_iter().find_map(|c| {
        let source = read(&c)?;
        Some(Target {
            line: declaration_line(&source, &simple).unwrap_or(0),
            path: c,
            name: simple.clone(),
        })
    })
}

/// The 0-based line declaring `class` / `object` `name` in Java or Kotlin source.
pub fn declaration_line(source: &str, name: &str) -> Option<u32> {
    source
        .lines()
        .position(|l| {
            let mut words = l
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                .filter(|w| !w.is_empty());
            let mut prev = "";
            words.any(|w| {
                let hit = w == name && matches!(prev, "class" | "object" | "interface" | "record");
                prev = w;
                hit
            })
        })
        .map(|l| l as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const YML: &str = "name: Demo\nversion: '1.0'\nmain: 'com.example.demo.Demo' # the plugin\napi-version: '1.21'\n";

    #[test]
    fn finds_the_main_entry() {
        let m = main_entry(Descriptor::PluginYml, YML).unwrap();
        assert_eq!(m.value, "com.example.demo.Demo");
        assert_eq!(&YML[m.from..m.to], "com.example.demo.Demo");
        let plain = "name: X\nmain: a.b.C\n";
        let m = main_entry(Descriptor::PaperPluginYml, plain).unwrap();
        assert_eq!(m.value, "a.b.C");
        // Nested keys do not count.
        assert!(main_entry(Descriptor::PluginYml, "commands:\n  main: x\n").is_none());
        let json = "{\n  \"id\": \"demo\",\n  \"main\" : \"com.example.Demo\"\n}";
        let m = main_entry(Descriptor::VelocityJson, json).unwrap();
        assert_eq!(&json[m.from..m.to], "com.example.Demo");
    }

    #[test]
    fn offsets_are_utf16() {
        let text = "description: 'Grüße 👋'\nmain: a.b.C\n";
        let m = main_entry(Descriptor::PluginYml, text).unwrap();
        let units: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(String::from_utf16(&units[m.from..m.to]).unwrap(), "a.b.C");
    }

    #[test]
    fn kinds_and_candidates() {
        assert_eq!(
            kind("src/main/resources/plugin.yml"),
            Some(Descriptor::PluginYml)
        );
        assert_eq!(kind("a\\bungee.yml"), Some(Descriptor::BungeeYml));
        assert_eq!(kind("config.yml"), None);
        assert_eq!(
            module_prefix("core/src/main/resources/plugin.yml").as_deref(),
            Some("core/")
        );
        assert_eq!(module_prefix("plugin.yml"), None);
        assert_eq!(
            class_candidates("", "a.b.C$Inner")[0],
            "src/main/java/a/b/C.java"
        );
    }

    #[test]
    fn inspects_the_main_class() {
        let path = "src/main/resources/plugin.yml";
        let exists = |p: &str| p == "src/main/java/com/example/demo/Demo.java";
        assert!(inspect(path, YML, &exists).is_empty());
        let missing = inspect(path, &YML.replace("Demo'", "Missing'"), &exists);
        assert_eq!(missing.len(), 1);
        assert_eq!(
            missing[0].message,
            "Cannot resolve class 'com.example.demo.Missing'"
        );
        assert_eq!(missing[0].code, "minecraft.mainClass.unresolved");
        let invalid = inspect(path, "main: Demo\n", &exists);
        assert_eq!(invalid[0].code, "minecraft.mainClass.invalid");
        assert!(inspect(path, "main: ${mainClass}\n", &exists).is_empty());
        assert!(inspect("src/main/resources/config.yml", "main: x\n", &exists).is_empty());
        // Kotlin sources count too.
        let kt = |p: &str| p == "src/main/kotlin/com/example/demo/Demo.kt";
        assert!(inspect(path, YML, &kt).is_empty());
    }

    #[test]
    fn goes_to_the_main_class() {
        let path = "src/main/resources/plugin.yml";
        let source = "package com.example.demo;\n\nimport x;\n\npublic final class Demo extends JavaPlugin {\n}\n";
        let read =
            |p: &str| (p == "src/main/java/com/example/demo/Demo.java").then(|| source.to_string());
        let m = main_entry(Descriptor::PluginYml, YML).unwrap();
        let t = goto(path, YML, m.from + 3, &read).unwrap();
        assert_eq!(t.path, "src/main/java/com/example/demo/Demo.java");
        assert_eq!(t.line, 4);
        assert_eq!(t.name, "Demo");
        assert!(goto(path, YML, 2, &read).is_none());
        assert_eq!(
            declaration_line("class Demo : JavaPlugin()", "Demo"),
            Some(0)
        );
    }
}

//! Mod metadata in the editor: the classes of `fabric.mod.json` entrypoints and of mixin
//! configurations (`package` + `mixins` / `client` / `server`) are checked against the
//! module's sources and navigable with Ctrl+B; the mixin configurations listed in
//! `fabric.mod.json` open their file. Completion and validation of the keys come from the
//! bundled JSON schemas (`forge-jsonschema`); `neoforge.mods.toml` has its own completion in
//! [`crate::mods::toml`].
//!
//! Offsets in and out are UTF-16 code units (CodeMirror's), lines are 0-based.

use crate::descriptor::{declaration_line, Problem, Target};
use crate::generate::is_qualified_name;

/// The kind of a mod metadata file, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModFile {
    FabricModJson,
    MixinConfig,
}

pub fn kind(path: &str) -> Option<ModFile> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if name == "fabric.mod.json" {
        Some(ModFile::FabricModJson)
    } else if name.ends_with(".mixins.json") || name == "mixins.json" {
        Some(ModFile::MixinConfig)
    } else {
        None
    }
}

/// A step of a JSON path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}

/// A string value of a JSON document with its path and byte range (between the quotes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonString {
    pub path: Vec<Seg>,
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// Every string value of `text` (not keys). Tolerant: stops at the first syntax error and
/// returns what it found before.
pub fn json_strings(text: &str) -> Vec<JsonString> {
    let mut p = JsonScan {
        s: text.as_bytes(),
        text,
        i: 0,
        out: Vec::new(),
    };
    let mut path = Vec::new();
    let _ = p.value(&mut path);
    p.out
}

struct JsonScan<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
    out: Vec<JsonString>,
}

impl JsonScan<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() {
            match self.s[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                b'/' if self.s.get(self.i + 1) == Some(&b'/') => {
                    while self.i < self.s.len() && self.s[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// A string at `i` (on the quote): its decoded value and the byte range inside the quotes.
    fn string(&mut self) -> Option<(String, usize, usize)> {
        if self.s.get(self.i) != Some(&b'"') {
            return None;
        }
        let start = self.i + 1;
        let mut j = start;
        while j < self.s.len() {
            match self.s[j] {
                b'\\' => j += 2,
                b'"' => {
                    let raw = &self.text[start..j];
                    let value = serde_json::from_str::<String>(&self.text[start - 1..=j])
                        .unwrap_or_else(|_| raw.to_string());
                    self.i = j + 1;
                    return Some((value, start, j));
                }
                b'\n' => return None,
                _ => j += 1,
            }
        }
        None
    }

    fn value(&mut self, path: &mut Vec<Seg>) -> Option<()> {
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                loop {
                    self.ws();
                    match *self.s.get(self.i)? {
                        b'}' => {
                            self.i += 1;
                            return Some(());
                        }
                        b',' => {
                            self.i += 1;
                            continue;
                        }
                        _ => {}
                    }
                    let (key, _, _) = self.string()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    path.push(Seg::Key(key));
                    let r = self.value(path);
                    path.pop();
                    r?;
                }
            }
            b'[' => {
                self.i += 1;
                let mut index = 0;
                loop {
                    self.ws();
                    match *self.s.get(self.i)? {
                        b']' => {
                            self.i += 1;
                            return Some(());
                        }
                        b',' => {
                            self.i += 1;
                            continue;
                        }
                        _ => {}
                    }
                    path.push(Seg::Index(index));
                    let r = self.value(path);
                    path.pop();
                    r?;
                    index += 1;
                }
            }
            b'"' => {
                let (value, start, end) = self.string()?;
                self.out.push(JsonString {
                    path: path.clone(),
                    value,
                    start,
                    end,
                });
                Some(())
            }
            _ => {
                // Numbers, true, false, null.
                let start = self.i;
                while self.i < self.s.len()
                    && !matches!(
                        self.s[self.i],
                        b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t'
                    )
                {
                    self.i += 1;
                }
                (self.i > start).then_some(())
            }
        }
    }
}

fn key(seg: &Seg) -> Option<&str> {
    match seg {
        Seg::Key(k) => Some(k),
        Seg::Index(_) => None,
    }
}

/// A reference in a mod metadata file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A fully qualified class (an entrypoint or a mixin).
    Class(String),
    /// A mixin configuration file name (`modid.mixins.json`).
    MixinConfig(String),
}

/// A reference with its byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub reference: Reference,
    pub start: usize,
    pub end: usize,
}

/// The references of a mod metadata file.
pub fn references(kind: ModFile, text: &str) -> Vec<Located> {
    let strings = json_strings(text);
    let mut out = Vec::new();
    match kind {
        ModFile::FabricModJson => {
            for s in &strings {
                let p: Vec<Option<&str>> = s.path.iter().map(key).collect();
                let entrypoint = match p.as_slice() {
                    [Some("entrypoints"), Some(_), None] => true,
                    [Some("entrypoints"), Some(_), None, Some("value")] => true,
                    _ => false,
                };
                let mixin = matches!(
                    p.as_slice(),
                    [Some("mixins"), None] | [Some("mixins"), None, Some("config")]
                );
                if entrypoint {
                    // `pkg.Class` or `pkg.Class::field`.
                    let class = s.value.split("::").next().unwrap_or("").trim();
                    out.push(Located {
                        reference: Reference::Class(class.to_string()),
                        start: s.start,
                        end: s.start + class.len().min(s.end - s.start),
                    });
                } else if mixin {
                    out.push(Located {
                        reference: Reference::MixinConfig(s.value.clone()),
                        start: s.start,
                        end: s.end,
                    });
                }
            }
        }
        ModFile::MixinConfig => {
            let package = strings
                .iter()
                .find(|s| s.path == [Seg::Key("package".into())])
                .map(|s| s.value.trim().to_string())
                .unwrap_or_default();
            for s in &strings {
                if let [Seg::Key(k), Seg::Index(_)] = s.path.as_slice() {
                    if matches!(k.as_str(), "mixins" | "client" | "server") {
                        let class = if package.is_empty() {
                            s.value.clone()
                        } else {
                            format!("{package}.{}", s.value)
                        };
                        out.push(Located {
                            reference: Reference::Class(class),
                            start: s.start,
                            end: s.end,
                        });
                    }
                }
            }
        }
    }
    out
}

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

fn byte_of(text: &str, utf16_pos: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16_pos {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The module folder of a file in `src/<set>/resources` (`mods/core/src/main/resources/x` →
/// `mods/core/`).
pub fn module_prefix(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let i = path.rfind("/resources/")?;
    let before = &path[..i];
    // `<prefix>src/<set>`.
    let src = before.rfind("src/")?;
    if before[src + 4..].contains('/') {
        return None;
    }
    Some(path[..src].to_string())
}

const SOURCE_SETS: &[&str] = &["main", "client", "server"];

/// Where the source of `class` can be (Java or Kotlin, in the main, client or server source
/// set), relative to the project root.
pub fn class_candidates(prefix: &str, class: &str) -> Vec<String> {
    let outer = class.split('$').next().unwrap_or(class);
    let rel = outer.replace('.', "/");
    let mut out = Vec::new();
    for set in SOURCE_SETS {
        out.push(format!("{prefix}src/{set}/java/{rel}.java"));
        out.push(format!("{prefix}src/{set}/kotlin/{rel}.kt"));
        out.push(format!("{prefix}src/{set}/java/{rel}.kt"));
    }
    out
}

/// Where a mixin configuration named in `fabric.mod.json` can be.
pub fn resource_candidates(prefix: &str, name: &str) -> Vec<String> {
    SOURCE_SETS
        .iter()
        .map(|set| format!("{prefix}src/{set}/resources/{name}"))
        .collect()
}

/// Check the classes and mixin configurations a mod metadata file names. `exists` says
/// whether a project file exists.
pub fn inspect(path: &str, text: &str, exists: &dyn Fn(&str) -> bool) -> Vec<Problem> {
    let Some(kind) = kind(path) else {
        return Vec::new();
    };
    let prefix = module_prefix(path);
    let mut out = Vec::new();
    for r in references(kind, text) {
        let problem = |severity, message: String, code| Problem {
            from: utf16(text, r.start),
            to: utf16(text, r.end),
            severity,
            message,
            code,
        };
        match &r.reference {
            Reference::Class(class) => {
                if class.is_empty() || class.contains("${") {
                    continue;
                }
                if !is_qualified_name(&class.replace('$', "_")) {
                    out.push(problem(
                        "error",
                        format!("'{class}' is not a fully qualified class name"),
                        "minecraft.modClass.invalid",
                    ));
                } else if let Some(prefix) = &prefix {
                    if !class_candidates(prefix, class).iter().any(|c| exists(c)) {
                        out.push(problem(
                            "error",
                            format!("Cannot resolve class '{class}'"),
                            "minecraft.modClass.unresolved",
                        ));
                    }
                }
            }
            Reference::MixinConfig(name) => {
                if let Some(prefix) = &prefix {
                    if !name.is_empty()
                        && !resource_candidates(prefix, name).iter().any(|c| exists(c))
                    {
                        out.push(problem(
                            "warning",
                            format!("Cannot find the mixin configuration '{name}'"),
                            "minecraft.mixinConfig.unresolved",
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Ctrl+B on a reference: the class's source at its declaration, or the mixin
/// configuration. `read` returns a project file's text.
pub fn goto(
    path: &str,
    text: &str,
    pos: usize,
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<Target> {
    let kind = kind(path)?;
    let prefix = module_prefix(path)?;
    let at = byte_of(text, pos);
    let r = references(kind, text)
        .into_iter()
        .find(|r| r.start <= at && at <= r.end)?;
    match r.reference {
        Reference::Class(class) => {
            let simple = class.rsplit(['.', '$']).next().unwrap_or(&class).to_string();
            class_candidates(&prefix, &class).into_iter().find_map(|c| {
                let source = read(&c)?;
                Some(Target {
                    line: declaration_line(&source, &simple).unwrap_or(0),
                    path: c,
                    name: simple.clone(),
                })
            })
        }
        Reference::MixinConfig(name) => resource_candidates(&prefix, &name)
            .into_iter()
            .find(|c| read(c).is_some())
            .map(|c| Target {
                path: c,
                line: 0,
                name: name.clone(),
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FABRIC: &str = r#"{
  "schemaVersion": 1,
  "id": "demo",
  "entrypoints": {
    "main": ["com.example.demo.Demo"],
    "client": [{ "adapter": "kotlin", "value": "com.example.demo.client.DemoClient" }],
    "fabric-datagen": ["com.example.demo.Gen::INSTANCE"]
  },
  "mixins": ["demo.mixins.json", { "config": "demo.client.mixins.json", "environment": "client" }],
  "depends": { "minecraft": "~26.3" }
}"#;

    const MIXINS: &str = r#"{
  "required": true,
  "package": "com.example.demo.mixin",
  "mixins": ["ExampleMixin"],
  "client": ["client.ExampleClientMixin"],
  "injectors": { "defaultRequire": 1 }
}"#;

    #[test]
    fn scans_json_strings_with_paths() {
        let s = json_strings(r#"{"a": [1, "x\"y", {"b": "c"}], "d": true}"#);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].value, "x\"y");
        assert_eq!(s[0].path, vec![Seg::Key("a".into()), Seg::Index(1)]);
        assert_eq!(
            s[1].path,
            vec![Seg::Key("a".into()), Seg::Index(2), Seg::Key("b".into())]
        );
        // Broken documents keep what came before the error.
        assert_eq!(json_strings(r#"{"a": "x", "b": "#).len(), 1);
    }

    #[test]
    fn finds_references() {
        let refs = references(ModFile::FabricModJson, FABRIC);
        let names: Vec<_> = refs.iter().map(|r| r.reference.clone()).collect();
        assert_eq!(
            names,
            vec![
                Reference::Class("com.example.demo.Demo".into()),
                Reference::Class("com.example.demo.client.DemoClient".into()),
                Reference::Class("com.example.demo.Gen".into()),
                Reference::MixinConfig("demo.mixins.json".into()),
                Reference::MixinConfig("demo.client.mixins.json".into()),
            ]
        );
        assert_eq!(&FABRIC[refs[2].start..refs[2].end], "com.example.demo.Gen");
        let refs = references(ModFile::MixinConfig, MIXINS);
        assert_eq!(
            refs[1].reference,
            Reference::Class("com.example.demo.mixin.client.ExampleClientMixin".into())
        );
        assert_eq!(&MIXINS[refs[0].start..refs[0].end], "ExampleMixin");
    }

    #[test]
    fn module_prefixes() {
        assert_eq!(
            module_prefix("src/main/resources/fabric.mod.json").as_deref(),
            Some("")
        );
        assert_eq!(
            module_prefix("mods/core/src/client/resources/demo.client.mixins.json").as_deref(),
            Some("mods/core/")
        );
        assert_eq!(module_prefix("fabric.mod.json"), None);
        assert_eq!(kind("a/b.mixins.json"), Some(ModFile::MixinConfig));
        assert_eq!(kind("package.json"), None);
    }

    #[test]
    fn inspects_classes_and_configs() {
        let path = "src/main/resources/fabric.mod.json";
        let files = [
            "src/main/java/com/example/demo/Demo.java",
            "src/client/kotlin/com/example/demo/client/DemoClient.kt",
            "src/main/resources/demo.mixins.json",
        ];
        let exists = |p: &str| files.contains(&p);
        let problems = inspect(path, FABRIC, &exists);
        let codes: Vec<_> = problems.iter().map(|p| (p.code, p.message.as_str())).collect();
        assert_eq!(
            codes,
            vec![
                (
                    "minecraft.modClass.unresolved",
                    "Cannot resolve class 'com.example.demo.Gen'"
                ),
                (
                    "minecraft.mixinConfig.unresolved",
                    "Cannot find the mixin configuration 'demo.client.mixins.json'"
                ),
            ]
        );
        let bad = inspect(path, r#"{"entrypoints": {"main": ["not a class"]}}"#, &exists);
        assert_eq!(bad[0].code, "minecraft.modClass.invalid");
        let mixins = inspect("src/main/resources/demo.mixins.json", MIXINS, &|_| false);
        assert_eq!(mixins.len(), 2);
    }

    #[test]
    fn goes_to_classes_and_configs() {
        let path = "src/main/resources/fabric.mod.json";
        let read = |p: &str| match p {
            "src/main/java/com/example/demo/Demo.java" => {
                Some("package com.example.demo;\n\npublic class Demo implements ModInitializer {\n}\n".to_string())
            }
            "src/main/resources/demo.mixins.json" => Some("{}".to_string()),
            _ => None,
        };
        let pos = FABRIC.find("com.example.demo.Demo").unwrap() + 3;
        let t = goto(path, FABRIC, pos, &read).unwrap();
        assert_eq!(t.path, "src/main/java/com/example/demo/Demo.java");
        assert_eq!(t.line, 2);
        let pos = FABRIC.find("demo.mixins.json").unwrap();
        let t = goto(path, FABRIC, pos, &read).unwrap();
        assert_eq!(t.path, "src/main/resources/demo.mixins.json");
        assert!(goto(path, FABRIC, 3, &read).is_none());
    }
}

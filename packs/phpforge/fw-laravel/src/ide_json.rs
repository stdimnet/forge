//! `ide.json`: the completion file packages and projects ship for Laravel IDE plugins
//! (the format Laravel Idea documents publicly). Forge reads the project's `ide.json` and
//! those of installed packages (`vendor/<vendor>/<package>/ide.json`):
//!
//! - `completions`: values to complete (`staticStrings`, `directoryFiles`, or one of the
//!   project's own lists such as `routeName`, `viewName`, `configKey`, `translationKey`,
//!   `envKey`, `validationRule`) under `condition`s that name functions or methods (with
//!   classes) and 1-based parameter positions;
//! - `codeGenerations`: named file generators from stub templates with `${INPUT_CLASS}`
//!   style placeholders, offered in the Laravel tool window and in completion of their
//!   names in `ide.json` itself.
//!
//! The parser is written from the format description: unknown entries are ignored.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where completions apply.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub method_names: Vec<String>,
    /// Classes of the methods (without the leading `\`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_fqn: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_parent_fqn: Vec<String>,
    /// 1-based parameter positions; empty means the first one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeCompletion {
    /// `staticStrings`, `directoryFiles`, `routeName`, ...
    pub complete: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix_to_clear: Option<String>,
    pub conditions: Vec<Condition>,
    /// The `ide.json` it comes from.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationFile {
    /// Namespace below `App\` (`Data`).
    #[serde(default)]
    pub app_namespace: String,
    /// File name pattern (`${INPUT_CLASS|className|upperCamelCase}.php`).
    pub name: String,
    /// `stub` or `blade`.
    #[serde(default)]
    pub template_type: String,
    /// Project-relative template path.
    #[serde(default)]
    pub template_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeGeneration {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class_suffix: String,
    pub files: Vec<GenerationFile>,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeJson {
    /// The `ide.json` files that were read.
    pub files: Vec<String>,
    pub completions: Vec<IdeCompletion>,
    pub code_generations: Vec<CodeGeneration>,
    /// Files that could not be parsed, with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|x| match x {
                Value::String(s) => Some(s.clone()),
                Value::Object(o) => o
                    .get("name")
                    .or_else(|| o.get("value"))
                    .and_then(|n| n.as_str())
                    .map(String::from),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn condition(v: &Value) -> Condition {
    let list = |k: &str| v.get(k).map(strings).unwrap_or_default();
    let fqns = |k: &str| {
        list(k)
            .into_iter()
            .map(|c| c.trim_start_matches('\\').to_string())
            .collect()
    };
    Condition {
        function_names: list("functionNames"),
        method_names: list("methodNames"),
        class_fqn: fqns("classFqn"),
        class_parent_fqn: fqns("classParentFqn"),
        parameters: v
            .get("parameters")
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Parse one `ide.json`. `source` names the file in the results.
pub fn parse(text: &str, source: &str) -> Result<IdeJson, String> {
    let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out = IdeJson {
        files: vec![source.to_string()],
        ..Default::default()
    };
    for c in json
        .get("completions")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
    {
        let Some(complete) = c.get("complete").and_then(|x| x.as_str()) else {
            continue;
        };
        let options = c.get("options").cloned().unwrap_or(Value::Null);
        let conditions: Vec<Condition> = match c.get("condition") {
            Some(Value::Array(a)) => a.iter().map(condition).collect(),
            Some(o @ Value::Object(_)) => vec![condition(o)],
            _ => Vec::new(),
        };
        out.completions.push(IdeCompletion {
            complete: complete.to_string(),
            strings: options.get("strings").map(strings).unwrap_or_default(),
            directory: options
                .get("directory")
                .and_then(|d| d.as_str())
                .map(|d| d.trim_matches('/').to_string()),
            suffix_to_clear: options
                .get("suffixToClear")
                .and_then(|d| d.as_str())
                .map(String::from),
            conditions,
            source: source.to_string(),
        });
    }
    for g in json
        .get("codeGenerations")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
    {
        let Some(name) = g.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        let files = g
            .get("files")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| {
                let template = f.get("template");
                Some(GenerationFile {
                    app_namespace: f
                        .get("appNamespace")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: f.get("name")?.as_str()?.to_string(),
                    template_type: template
                        .and_then(|t| t.get("type"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("stub")
                        .to_string(),
                    template_path: template
                        .and_then(|t| t.get("path"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .trim_start_matches('/')
                        .to_string(),
                })
            })
            .collect();
        out.code_generations.push(CodeGeneration {
            id: g
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or(name)
                .to_string(),
            name: name.to_string(),
            class_suffix: g
                .get("classSuffix")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            files,
            source: source.to_string(),
        });
    }
    Ok(out)
}

/// The project's `ide.json` and those of its packages.
pub fn load(root: &Path) -> IdeJson {
    let mut paths = vec!["ide.json".to_string()];
    if let Ok(vendors) = std::fs::read_dir(root.join("vendor")) {
        let mut found = Vec::new();
        for v in vendors.flatten() {
            if let Ok(pkgs) = std::fs::read_dir(v.path()) {
                for p in pkgs.flatten() {
                    if p.path().join("ide.json").is_file() {
                        found.push(crate::project::rel_path(root, &p.path().join("ide.json")));
                    }
                }
            }
        }
        found.sort();
        paths.extend(found);
    }
    let mut out = IdeJson::default();
    for rel in paths {
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        match parse(&text, &rel) {
            Ok(one) => {
                out.files.extend(one.files);
                out.completions.extend(one.completions);
                out.code_generations.extend(one.code_generations);
            }
            Err(e) => out.errors.push(format!("{rel}: {e}")),
        }
    }
    out
}

/// A call around a string argument, as the completion sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite<'a> {
    pub function: &'a str,
    /// Resolved class of a static call or of `$this` (`None` for unknown receivers).
    pub class: Option<String>,
    /// Parent classes of `class`.
    pub parents: Vec<String>,
    pub is_method: bool,
    /// 0-based.
    pub arg_index: usize,
}

impl Condition {
    pub fn matches(&self, site: &CallSite) -> bool {
        let position = site.arg_index + 1;
        let positions_ok = if self.parameters.is_empty() {
            position == 1
        } else {
            self.parameters.contains(&position)
        };
        if !positions_ok {
            return false;
        }
        let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if !site.is_method {
            return self.function_names.iter().any(|f| eq(f, site.function));
        }
        if !self.method_names.iter().any(|m| eq(m, site.function)) {
            return false;
        }
        if self.class_fqn.is_empty() && self.class_parent_fqn.is_empty() {
            return true;
        }
        let Some(class) = &site.class else {
            // Unknown receiver: offer the values rather than hide them.
            return true;
        };
        self.class_fqn.iter().any(|c| eq(c, class))
            || self
                .class_parent_fqn
                .iter()
                .any(|p| eq(p, class) || site.parents.iter().any(|x| eq(x, p)))
    }
}

/// Completions that apply at `site`.
pub fn applicable<'a>(ide: &'a IdeJson, site: &CallSite) -> Vec<&'a IdeCompletion> {
    ide.completions
        .iter()
        .filter(|c| c.conditions.iter().any(|cond| cond.matches(site)))
        .collect()
}

/// Files below `directory` without the suffix (`icons/cart` for `icons/cart.svg`).
pub fn directory_values(root: &Path, c: &IdeCompletion) -> Vec<String> {
    let Some(dir) = &c.directory else {
        return Vec::new();
    };
    let base = root.join(dir);
    let mut out: Vec<String> = walkdir::WalkDir::new(&base)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let rel = e
                .path()
                .strip_prefix(&base)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some(match &c.suffix_to_clear {
                Some(s) => rel.strip_suffix(s.as_str()).unwrap_or(&rel).to_string(),
                None => rel,
            })
        })
        .collect();
    out.sort();
    out
}

fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn upper_first(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Apply `|className|upperCamelCase|…` modifiers to a value.
fn modify(value: &str, modifiers: &[&str]) -> String {
    let mut v = value.to_string();
    for m in modifiers {
        v = match *m {
            "className" => v.rsplit(['\\', '/']).next().unwrap_or(&v).to_string(),
            "namespace" => v
                .rsplit_once(['\\', '/'])
                .map(|(n, _)| n.replace('/', "\\"))
                .unwrap_or_default(),
            "upperCamelCase" | "studly" => words(&v).iter().map(|w| upper_first(w)).collect(),
            "camelCase" => {
                let w = words(&v);
                w.iter()
                    .enumerate()
                    .map(|(i, x)| if i == 0 { x.clone() } else { upper_first(x) })
                    .collect()
            }
            "snakeCase" => words(&v).join("_"),
            "kebabCase" => words(&v).join("-"),
            "lowerCase" => v.to_lowercase(),
            "upperCase" => v.to_uppercase(),
            _ => v,
        };
    }
    v
}

/// Replace `${NAME|modifier|…}` placeholders.
pub fn expand(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let Some(end) = rest[i..].find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &rest[i + 2..i + end];
        let mut parts = inner.split('|');
        let name = parts.next().unwrap_or("").trim();
        let mods: Vec<&str> = parts.map(str::trim).collect();
        match vars.iter().find(|(n, _)| *n == name) {
            Some((_, v)) => out.push_str(&modify(v, &mods)),
            None => out.push_str(&rest[i..i + end + 1]),
        }
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out
}

/// A file a code generation creates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    /// The file is already there (it is not overwritten).
    #[serde(default)]
    pub exists: bool,
}

/// The files of a code generation for the class name the user typed (`Orders/Invoice`).
/// `read` returns a project file; `class_path` maps `App\X` to its file (PSR-4).
pub fn generate(
    generation: &CodeGeneration,
    input: &str,
    read: &dyn Fn(&str) -> Option<String>,
    class_path: &dyn Fn(&str) -> String,
) -> Vec<GeneratedFile> {
    let input = input.trim().replace('/', "\\");
    let mut base = input.trim_matches('\\').to_string();
    if !generation.class_suffix.is_empty() && !base.ends_with(&generation.class_suffix) {
        base.push_str(&generation.class_suffix);
    }
    let mut out = Vec::new();
    for f in &generation.files {
        let (sub_ns, class) = match base.rsplit_once('\\') {
            Some((n, c)) => (n.to_string(), c.to_string()),
            None => (String::new(), base.clone()),
        };
        let mut namespace = String::from("App");
        for part in [f.app_namespace.as_str(), sub_ns.as_str()] {
            if !part.is_empty() {
                namespace.push('\\');
                namespace.push_str(part.trim_matches('\\'));
            }
        }
        let vars = [
            ("INPUT_CLASS", base.clone()),
            ("INPUT_FQN", format!("{namespace}\\{class}")),
            ("NAMESPACE", namespace.clone()),
            ("CLASS", class.clone()),
        ];
        let file_name = expand(&f.name, &vars);
        let file_name = file_name
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&file_name)
            .to_string();
        let stem = file_name
            .strip_suffix(".php")
            .unwrap_or(&file_name)
            .to_string();
        let vars = [
            ("INPUT_CLASS", base.clone()),
            ("INPUT_FQN", format!("{namespace}\\{stem}")),
            ("NAMESPACE", namespace.clone()),
            ("CLASS", stem.clone()),
        ];
        let template = read(&f.template_path).unwrap_or_default();
        let content = expand(&template, &vars);
        let path = if file_name.ends_with(".php") {
            class_path(&format!("{namespace}\\{stem}"))
        } else {
            file_name.clone()
        };
        out.push(GeneratedFile {
            exists: read(&path).is_some(),
            path,
            content,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDE: &str = r#"{
  "$schema": "https://laravel-ide.com/schema/laravel-ide-v2.json",
  "completions": [
    {
      "complete": "staticStrings",
      "options": { "strings": ["paid", "shipped", "cancelled"] },
      "condition": [
        { "functionNames": ["order_status"] },
        { "classFqn": ["\\App\\Models\\Order"], "methodNames": ["markAs"], "parameters": [1] }
      ]
    },
    {
      "complete": "directoryFiles",
      "options": { "directory": "/resources/svg", "suffixToClear": ".svg" },
      "condition": [{ "functionNames": ["svg"], "parameters": [1] }]
    },
    { "complete": "routeName", "condition": [{ "methodNames": ["linkTo"], "parameters": [2] }] },
    { "complete": "unknownThing" }
  ],
  "codeGenerations": [
    {
      "id": "acme:dto",
      "name": "Create DTO",
      "classSuffix": "Data",
      "files": [
        {
          "appNamespace": "Data",
          "name": "${INPUT_CLASS|className|upperCamelCase}.php",
          "template": { "type": "stub", "path": "/stubs/dto.stub" }
        }
      ]
    }
  ]
}"#;

    #[test]
    fn parses_completions_and_generations() {
        let ide = parse(IDE, "ide.json").unwrap();
        assert_eq!(ide.completions.len(), 4);
        assert_eq!(
            ide.completions[0].strings,
            vec!["paid", "shipped", "cancelled"]
        );
        assert_eq!(
            ide.completions[0].conditions[1].class_fqn,
            vec!["App\\Models\\Order"]
        );
        assert_eq!(
            ide.completions[1].directory.as_deref(),
            Some("resources/svg")
        );
        assert_eq!(
            ide.code_generations[0].files[0].template_path,
            "stubs/dto.stub"
        );
        assert!(parse("{", "x").is_err());
    }

    #[test]
    fn conditions_match_calls() {
        let ide = parse(IDE, "ide.json").unwrap();
        let site = |f, class: Option<&str>, method, arg| CallSite {
            function: f,
            class: class.map(String::from),
            parents: Vec::new(),
            is_method: method,
            arg_index: arg,
        };
        assert_eq!(
            applicable(&ide, &site("order_status", None, false, 0)).len(),
            1
        );
        assert!(applicable(&ide, &site("order_status", None, false, 1)).is_empty());
        assert_eq!(
            applicable(&ide, &site("markAs", Some("App\\Models\\Order"), true, 0)).len(),
            1
        );
        assert!(applicable(&ide, &site("markAs", Some("App\\Models\\User"), true, 0)).is_empty());
        assert_eq!(
            applicable(&ide, &site("linkTo", None, true, 1))[0].complete,
            "routeName"
        );
    }

    #[test]
    fn directory_files_and_generation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("resources/svg/icons")).unwrap();
        std::fs::write(dir.path().join("resources/svg/logo.svg"), "").unwrap();
        std::fs::write(dir.path().join("resources/svg/icons/cart.svg"), "").unwrap();
        let ide = parse(IDE, "ide.json").unwrap();
        assert_eq!(
            directory_values(dir.path(), &ide.completions[1]),
            vec!["icons/cart", "logo"]
        );

        let read = |p: &str| {
            (p == "stubs/dto.stub").then(|| {
                "<?php\n\nnamespace ${NAMESPACE};\n\nfinal class ${CLASS}\n{\n}\n".to_string()
            })
        };
        let class_path = |fqn: &str| {
            format!(
                "app/{}.php",
                fqn.trim_start_matches("App\\").replace('\\', "/")
            )
        };
        let files = generate(
            &ide.code_generations[0],
            "Orders/invoice",
            &read,
            &class_path,
        );
        assert_eq!(files[0].path, "app/Data/Orders/InvoiceData.php");
        assert!(files[0].content.contains("namespace App\\Data\\Orders;"));
        assert!(files[0].content.contains("final class InvoiceData"));
        assert_eq!(
            expand("${X|snakeCase}-${Y}", &[("X", "OrderLine".into())]),
            "order_line-${Y}"
        );
    }
}

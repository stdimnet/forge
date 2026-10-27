//! Launch profiles (`Properties/launchSettings.json`): what `dotnet run --launch-profile`
//! and Rider's "Launch Settings Profile" configurations start, and the environment the
//! debugger needs to start the same program.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Remove `//` and `/* */` comments and trailing commas (launchSettings.json and
/// global.json are read by .NET's tolerant JSON reader).
pub fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.trim_start_matches('\u{feff}').chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            ',' => {
                // A comma before `}` or `]` (after whitespace / comments) is dropped.
                let mut j = i + 1;
                loop {
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    if chars.get(j) == Some(&'/') && chars.get(j + 1) == Some(&'/') {
                        while j < chars.len() && chars[j] != '\n' {
                            j += 1;
                        }
                        continue;
                    }
                    break;
                }
                if !matches!(chars.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// One profile of `launchSettings.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfile {
    pub name: String,
    /// `Project`, `IISExpress`, `IIS`, `Executable`, `Docker`, `DockerCompose`, ...
    pub command_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command_line_args: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub working_directory: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub executable_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub application_url: String,
    #[serde(default)]
    pub launch_browser: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub launch_url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_variables: BTreeMap<String, String>,
    /// 0-based line of the profile in the file.
    #[serde(default)]
    pub line: u32,
}

impl LaunchProfile {
    /// Whether `dotnet run` starts it (`Project`); the others need IIS, Docker or another
    /// program and are listed but not run.
    pub fn is_runnable(&self) -> bool {
        self.command_name.eq_ignore_ascii_case("Project")
            || self.command_name.eq_ignore_ascii_case("Executable")
    }

    /// The URL the browser opens: `launchUrl` against the first application URL.
    pub fn browser_url(&self) -> Option<String> {
        if !self.launch_browser {
            return None;
        }
        let base = self
            .application_url
            .split(';')
            .map(str::trim)
            .find(|u| !u.is_empty())?;
        if self.launch_url.starts_with("http://") || self.launch_url.starts_with("https://") {
            return Some(self.launch_url.clone());
        }
        Some(if self.launch_url.is_empty() {
            base.to_string()
        } else {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                self.launch_url.trim_start_matches('/')
            )
        })
    }

    /// The environment `dotnet run` gives the program for this profile (the debugger
    /// starts the built program directly): the profile's variables, `ASPNETCORE_URLS` from
    /// `applicationUrl` (`DOTNET_URLS` for non-web projects is the same setting).
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut env = self.environment_variables.clone();
        if !self.application_url.is_empty() && !env.contains_key("ASPNETCORE_URLS") {
            env.insert("ASPNETCORE_URLS".into(), self.application_url.clone());
        }
        env
    }
}

fn string(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Parse `launchSettings.json`. Profiles keep the file's order.
pub fn parse_launch_settings(text: &str) -> Result<Vec<LaunchProfile>, String> {
    let clean = strip_jsonc(text);
    let v: Value = serde_json::from_str(&clean).map_err(|e| format!("launchSettings.json: {e}"))?;
    let Some(profiles) = v.get("profiles").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    // serde_json's map is sorted unless preserve_order is on: take the order from the text.
    let mut names: Vec<(usize, String)> = profiles
        .keys()
        .map(|k| {
            let needle = format!("\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""));
            let at = text.find("\"profiles\"").unwrap_or(0);
            (text[at..].find(&needle).map(|i| i + at).unwrap_or(usize::MAX), k.clone())
        })
        .collect();
    names.sort();
    let mut out = Vec::new();
    for (pos, name) in names {
        let p = &profiles[&name];
        let env = p
            .get("environmentVariables")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(LaunchProfile {
            command_name: match string(p, "commandName") {
                s if s.is_empty() => "Project".into(),
                s => s,
            },
            command_line_args: string(p, "commandLineArgs"),
            working_directory: string(p, "workingDirectory"),
            executable_path: string(p, "executablePath"),
            application_url: string(p, "applicationUrl"),
            launch_browser: p.get("launchBrowser").and_then(Value::as_bool).unwrap_or(false),
            launch_url: string(p, "launchUrl"),
            environment_variables: env,
            line: if pos == usize::MAX {
                0
            } else {
                text[..pos].matches('\n').count() as u32
            },
            name,
        });
    }
    Ok(out)
}

/// `<project dir>/Properties/launchSettings.json` (`My Project/launchSettings.json` for VB).
pub fn launch_settings_paths(project_dir: &str) -> Vec<String> {
    let base = if project_dir.is_empty() {
        String::new()
    } else {
        format!("{project_dir}/")
    };
    vec![
        format!("{base}Properties/launchSettings.json"),
        format!("{base}My Project/launchSettings.json"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn strips_comments_and_trailing_commas() {
        let t = strip_jsonc("{ // c\n \"a\": \"x//y\", /* b */ \"b\": [1, 2,], }");
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["a"], "x//y");
        assert_eq!(v["b"].as_array().unwrap().len(), 2);
        assert_eq!(strip_jsonc("{\"s\": \"q\\\"\",}"), "{\"s\": \"q\\\"\"}");
    }

    #[test]
    fn reads_profiles_in_file_order() {
        let p = parse_launch_settings(&fixture("bookstore/src/Bookstore.Api/Properties/launchSettings.json")).unwrap();
        let names: Vec<&str> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["http", "https", "IIS Express"]);
        assert!(p[0].is_runnable() && !p[2].is_runnable());
        assert_eq!(p[0].browser_url().as_deref(), Some("http://localhost:5180/swagger"));
        assert_eq!(p[1].browser_url(), None);
        assert_eq!(p[1].command_line_args, "--seed-data");
        let env = p[1].environment();
        assert_eq!(env["ASPNETCORE_URLS"], "https://localhost:7180;http://localhost:5180");
        assert_eq!(env["BOOKSTORE_CACHE"], "memory");
        assert_eq!(p[0].line, 8);
        assert!(parse_launch_settings("{}").unwrap().is_empty());
        assert!(parse_launch_settings("{").is_err());
        assert_eq!(launch_settings_paths("src/App")[0], "src/App/Properties/launchSettings.json");
    }
}

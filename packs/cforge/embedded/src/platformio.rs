//! PlatformIO projects: `platformio.ini` (sections, multi-line values, comments, `[env]`
//! defaults, `extends`, `${section.key}` interpolation, `default_envs`) and the commands of
//! the PlatformIO tool window's tasks (`pio run -e <env> -t <target>`, `pio test`,
//! `pio device monitor`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ToolCommand;

/// One `[env:<name>]` with its resolved options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PioEnv {
    pub name: String,
    pub platform: String,
    pub board: String,
    pub framework: Vec<String>,
    /// Built by default (`default_envs`, or every env when there is none).
    pub default: bool,
    /// The section's line in platformio.ini (1-based).
    pub line: u32,
    /// Every resolved option (`build_flags`, `upload_port`, `monitor_speed`, …).
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PioProject {
    pub envs: Vec<PioEnv>,
    pub default_envs: Vec<String>,
    /// `[platformio]` options (`src_dir`, `build_dir`, …).
    pub platformio: BTreeMap<String, String>,
    /// Problems (unknown `extends`, interpolation loops).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

struct Section {
    name: String,
    line: u32,
    values: Vec<(String, String)>,
}

/// Remove a `;` comment that follows a value (PlatformIO allows ` ; …` inline).
fn strip_inline_comment(v: &str) -> &str {
    match v.find(" ;") {
        Some(i) => v[..i].trim_end(),
        None => v,
    }
}

fn sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut last_key: Option<usize> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            out.push(Section {
                name: t[1..t.len() - 1].trim().to_string(),
                line: i as u32 + 1,
                values: Vec::new(),
            });
            last_key = None;
            continue;
        }
        let Some(section) = out.last_mut() else {
            continue;
        };
        let indented = line.starts_with([' ', '\t']);
        if indented {
            if let Some(k) = last_key {
                let v = &mut section.values[k].1;
                if !v.is_empty() {
                    v.push('\n');
                }
                v.push_str(strip_inline_comment(t));
                continue;
            }
        }
        if let Some((k, v)) = t.split_once('=') {
            section.values.push((
                k.trim().to_string(),
                strip_inline_comment(v.trim()).to_string(),
            ));
            last_key = Some(section.values.len() - 1);
        }
    }
    out
}

/// A list value: entries separated by commas or new lines.
pub fn list(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Expand `${section.key}` (and `${this.key}`) references.
fn interpolate(
    value: &str,
    this: &BTreeMap<String, String>,
    raw: &BTreeMap<String, BTreeMap<String, String>>,
    depth: u32,
    problems: &mut Vec<String>,
) -> String {
    if depth > 8 {
        problems.push(format!("Interpolation is too deep: {value}"));
        return value.to_string();
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let reference = &after[..end];
        let resolved = reference.split_once('.').and_then(|(section, key)| {
            match section {
                "this" => this.get(key).cloned(),
                // Environment variables are only known when PlatformIO runs.
                "sysenv" => None,
                s => raw.get(s).and_then(|m| m.get(key)).cloned(),
            }
        });
        match resolved {
            Some(v) => out.push_str(&interpolate(&v, this, raw, depth + 1, problems)),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parse `platformio.ini`.
pub fn parse(text: &str) -> PioProject {
    let sections = sections(text);
    let mut raw: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for s in &sections {
        let entry = raw.entry(s.name.clone()).or_default();
        for (k, v) in &s.values {
            entry.insert(k.clone(), v.clone());
        }
    }
    let mut project = PioProject {
        platformio: raw.get("platformio").cloned().unwrap_or_default(),
        ..PioProject::default()
    };
    project.default_envs = project
        .platformio
        .get("default_envs")
        .map(|v| list(v))
        .unwrap_or_default();
    let common = raw.get("env").cloned().unwrap_or_default();
    for s in sections.iter().filter(|s| s.name.starts_with("env:")) {
        let name = s.name["env:".len()..].trim().to_string();
        // [env] < extended sections (in order) < the env itself.
        let mut options = common.clone();
        let mut chain: Vec<String> = Vec::new();
        let mut next = raw.get(&s.name).and_then(|m| m.get("extends")).cloned();
        while let Some(ext) = next.take() {
            for parent in list(&ext).into_iter().rev() {
                if chain.contains(&parent) || parent == s.name {
                    project
                        .problems
                        .push(format!("[{}] extends itself through {parent}", s.name));
                    continue;
                }
                match raw.get(&parent) {
                    Some(p) => {
                        chain.push(parent.clone());
                        if let Some(e) = p.get("extends") {
                            next = Some(e.clone());
                        }
                    }
                    None => project
                        .problems
                        .push(format!("[{}] extends unknown section [{parent}]", s.name)),
                }
            }
        }
        for parent in chain.iter().rev() {
            if let Some(p) = raw.get(parent) {
                options.extend(p.clone());
            }
        }
        if let Some(own) = raw.get(&s.name) {
            options.extend(own.clone());
        }
        options.remove("extends");
        let snapshot = options.clone();
        let mut problems = Vec::new();
        for v in options.values_mut() {
            *v = interpolate(v, &snapshot, &raw, 0, &mut problems);
        }
        project.problems.extend(problems);
        project.envs.push(PioEnv {
            platform: options.get("platform").cloned().unwrap_or_default(),
            board: options.get("board").cloned().unwrap_or_default(),
            framework: options
                .get("framework")
                .map(|v| list(v))
                .unwrap_or_default(),
            default: project.default_envs.is_empty() || project.default_envs.contains(&name),
            line: s.line,
            name,
            options,
        });
    }
    project
}

/// A task of the PlatformIO tool window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PioTask {
    Build,
    Upload,
    Monitor,
    Test,
    Clean,
    /// `compile_commands.json` for clangd.
    Compiledb,
}

impl PioTask {
    pub const ALL: [PioTask; 6] = [
        PioTask::Build,
        PioTask::Upload,
        PioTask::Monitor,
        PioTask::Test,
        PioTask::Clean,
        PioTask::Compiledb,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PioTask::Build => "Build",
            PioTask::Upload => "Upload",
            PioTask::Monitor => "Monitor",
            PioTask::Test => "Test",
            PioTask::Clean => "Clean",
            PioTask::Compiledb => "Compilation Database",
        }
    }
}

/// The command of a task for one environment (`env` empty: the default environments).
pub fn task_command(pio: &str, task: PioTask, env: &str, project_dir: &str) -> ToolCommand {
    let mut args: Vec<String> = match task {
        PioTask::Monitor => vec!["device".into(), "monitor".into()],
        PioTask::Test => vec!["test".into()],
        _ => vec!["run".into()],
    };
    if !env.is_empty() {
        args.push("-e".into());
        args.push(env.into());
    }
    let target = match task {
        PioTask::Upload => Some("upload"),
        PioTask::Clean => Some("clean"),
        PioTask::Compiledb => Some("compiledb"),
        _ => None,
    };
    if let Some(t) = target {
        args.push("-t".into());
        args.push(t.into());
    }
    ToolCommand {
        program: pio.into(),
        args,
        cwd: project_dir.into(),
        env: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = include_str!("../tests/fixtures/platformio.ini");

    #[test]
    fn reads_envs_with_defaults_extends_and_interpolation() {
        let p = parse(INI);
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert_eq!(p.default_envs, ["nucleo_f401re", "esp32dev"]);
        assert_eq!(p.platformio.get("src_dir").map(String::as_str), Some("src"));
        let names: Vec<_> = p.envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["nucleo_f401re", "esp32dev", "esp32dev_debug", "native"]
        );
        let nucleo = &p.envs[0];
        assert_eq!(
            (nucleo.platform.as_str(), nucleo.board.as_str()),
            ("ststm32", "nucleo_f401re")
        );
        assert_eq!(nucleo.framework, ["arduino"]);
        assert!(nucleo.default);
        assert_eq!(nucleo.line, 24);
        assert_eq!(
            nucleo.options["build_flags"],
            "-Wall -DBLINK_MS=250 -DLED_PIN=PA5"
        );
        assert_eq!(nucleo.options["monitor_speed"], "115200");
        assert!(nucleo.options["lib_deps"].contains("ArduinoJson"));
        let esp = &p.envs[1];
        assert_eq!(esp.framework, ["arduino", "espidf"]);
        assert_eq!(esp.options["monitor_speed"], "921600");
        assert_eq!(
            esp.options["build_flags"],
            "-Wall -DBLINK_MS=250\n-DLED_PIN=2"
        );
        let debug = &p.envs[2];
        assert!(!debug.default);
        assert_eq!(debug.board, "esp32dev");
        assert_eq!(debug.options["upload_port"], "/dev/ttyUSB0");
        assert_eq!(debug.options["debug_tool"], "esp-prog");
        assert!(!debug.options.contains_key("extends"));
        let native = &p.envs[3];
        assert!(native.framework.is_empty());
        assert_eq!(native.options["test_framework"], "unity");
    }

    #[test]
    fn problems_and_all_default() {
        let p = parse("[env:a]\nextends = missing\n[env:b]\nextends = env:b\nboard = ${this.platform}\nplatform = x ; inline\n");
        assert_eq!(p.problems.len(), 2);
        assert!(p.envs.iter().all(|e| e.default));
        assert_eq!(p.envs[1].board, "x");
        assert!(parse("").envs.is_empty());
    }

    #[test]
    fn task_commands() {
        let line = |t, e| {
            let c = task_command("pio", t, e, "/p");
            format!("{} {}", c.program, c.args.join(" "))
        };
        assert_eq!(line(PioTask::Build, "esp32dev"), "pio run -e esp32dev");
        assert_eq!(
            line(PioTask::Upload, "esp32dev"),
            "pio run -e esp32dev -t upload"
        );
        assert_eq!(
            line(PioTask::Monitor, "esp32dev"),
            "pio device monitor -e esp32dev"
        );
        assert_eq!(line(PioTask::Test, "native"), "pio test -e native");
        assert_eq!(line(PioTask::Clean, ""), "pio run -t clean");
        assert_eq!(
            line(PioTask::Compiledb, "nucleo_f401re"),
            "pio run -e nucleo_f401re -t compiledb"
        );
        assert_eq!(task_command("pio", PioTask::Build, "", "/p").cwd, "/p");
        assert_eq!(PioTask::Compiledb.label(), "Compilation Database");
    }
}

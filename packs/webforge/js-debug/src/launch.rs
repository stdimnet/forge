//! Debug (Shift+F9) of WebForge's run configurations: the js-debug launch request of a
//! "Node.js", "npm", "JavaScript Debug", "Attach to Node.js/Chrome" or test runner
//! configuration (Jest, Vitest, Mocha, Playwright, `node --test`).

use std::collections::BTreeMap;

use forge_dap::{LaunchMode, LaunchRequest};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use webforge_js_testing::{JsTestRunner, Runner};

use crate::adapter::Target;
use crate::browsers::{pick, Browser};

/// A run configuration as the UI and `.forge/runConfigurations.toml` store it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JsConfiguration {
    pub name: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub options: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub interpreter: String,
    pub interpreter_options: String,
    pub arguments: String,
    pub working_directory: String,
}

impl JsConfiguration {
    fn option(&self, key: &str) -> &str {
        self.options.get(key).map(|s| s.trim()).unwrap_or("")
    }

    fn runtools(&self) -> Configuration {
        Configuration {
            name: self.name.clone(),
            type_id: self.type_id.clone(),
            working_directory: self.working_directory.clone(),
            interpreter_options: self.interpreter_options.clone(),
            arguments: self.arguments.clone(),
            options: self.options.clone(),
            before_launch: Vec::new(),
        }
    }
}

/// What building a request may use besides the configuration.
#[derive(Debug, Clone, Default)]
pub struct LaunchContext {
    /// The project's Node interpreter (`web.node.interpreter`, else the detected one).
    pub node: String,
    /// The browsers of this machine and the default choice (`web.jsDebug.browser`).
    pub browsers: Vec<Browser>,
    pub browser: String,
    /// WebForge's test runner specs (from the manifest).
    pub test_specs: Vec<TestFrameworkSpec>,
    /// A folder for report files test runners write while debugged.
    pub report_dir: String,
}

/// The configuration types Debug handles with js-debug.
pub const TYPES: &[&str] = &[
    "node",
    "npm",
    "javascript-debug",
    "js-attach",
    "jest",
    "vitest",
    "mocha",
    "playwright",
    "node-test",
];

fn object(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        if !v.is_null() {
            m.insert(k.to_string(), v);
        }
    }
    Value::Object(m)
}

fn base(cfg: &JsConfiguration, adapter: &str) -> LaunchRequest {
    LaunchRequest {
        adapter: adapter.into(),
        name: Some(cfg.name.clone()).filter(|n| !n.is_empty()),
        mode: LaunchMode::Launch,
        program: None,
        args: Vec::new(),
        arguments: None,
        cwd: Some(cfg.working_directory.trim().to_string()).filter(|c| !c.is_empty()),
        env: cfg.env.clone(),
        stop_on_entry: false,
        configuration: Value::Null,
    }
}

/// The launch request of a configuration.
pub fn request(cfg: &JsConfiguration, ctx: &LaunchContext) -> Result<LaunchRequest, String> {
    let node = Target::Node.adapter_type();
    let interpreter = match cfg.interpreter.trim() {
        "" => ctx.node.trim(),
        i => i,
    };
    match cfg.type_id.as_str() {
        "node" => {
            let file = cfg.option("file");
            if file.is_empty() {
                return Err("JavaScript file is not specified".into());
            }
            let mut r = base(cfg, node);
            r.program = Some(file.to_string());
            r.arguments = Some(cfg.arguments.clone()).filter(|a| !a.trim().is_empty());
            r.stop_on_entry = cfg.option("stopOnEntry") == "true";
            r.configuration = object(vec![
                (
                    "runtimeExecutable",
                    (!interpreter.is_empty())
                        .then(|| json!(interpreter))
                        .unwrap_or(Value::Null),
                ),
                ("runtimeArgs", json!(split_args(&cfg.interpreter_options))),
            ]);
            Ok(r)
        }
        "npm" => {
            let command = match cfg.option("command") {
                "" => "run",
                c => c,
            };
            let manager = match cfg.option("packageManager") {
                "" => "npm",
                m => m,
            };
            let mut runtime_args = vec![command.to_string()];
            if command == "run" {
                let script = cfg.option("script");
                if script.is_empty() {
                    return Err("Script is not specified".into());
                }
                runtime_args.push(script.to_string());
            }
            let extra = split_args(&cfg.arguments);
            if !extra.is_empty() {
                if manager == "npm" && command == "run" {
                    runtime_args.push("--".into());
                }
                runtime_args.extend(extra);
            }
            let mut r = base(cfg, node);
            r.name = Some(match cfg.option("script") {
                "" => cfg.name.clone(),
                s if cfg.name.is_empty() => s.to_string(),
                _ => cfg.name.clone(),
            });
            if r.cwd.is_none() {
                let package_dir = cfg
                    .option("packageJson")
                    .rsplit_once(['/', '\\'])
                    .map(|(d, _)| d.to_string())
                    .filter(|d| !d.is_empty());
                r.cwd = package_dir;
            }
            r.configuration = json!({
                "runtimeExecutable": manager,
                "runtimeArgs": runtime_args,
            });
            Ok(r)
        }
        "javascript-debug" => {
            let url = cfg.option("url");
            if url.is_empty() {
                return Err("URL is not specified".into());
            }
            let choice = match cfg.option("browser") {
                "" => ctx.browser.as_str(),
                b => b,
            };
            let browser = pick(&ctx.browsers, choice).ok_or_else(|| {
                if ctx.browsers.is_empty() {
                    "No Chrome, Edge or Chromium was found. Install one, or enter the browser's path in the configuration.".to_string()
                } else {
                    format!("The browser '{choice}' was not found")
                }
            })?;
            let mut r = base(cfg, browser.kind.adapter());
            r.program = Some(url.to_string());
            r.configuration = object(vec![
                ("runtimeExecutable", json!(browser.path)),
                (
                    "runtimeArgs",
                    json!(split_args(cfg.option("browserArguments"))),
                ),
                (
                    // A separate profile, so an open browser window does not swallow the
                    // debug launch.
                    "userDataDir",
                    json!(cfg.option("userDataDir") != "false"),
                ),
            ]);
            Ok(r)
        }
        "js-attach" => {
            let (adapter, default_port) = match cfg.option("debugger") {
                "chrome" => (Target::Chrome.adapter_type(), 9222),
                "edge" => (Target::Edge.adapter_type(), 9222),
                _ => (node, 9229),
            };
            let host = match cfg.option("host") {
                "" => "localhost",
                h => h,
            };
            let port = match cfg.option("port") {
                "" => default_port,
                p => p
                    .parse::<u16>()
                    .map_err(|_| format!("'{p}' is not a port number"))?,
            };
            let mut r = base(cfg, adapter);
            r.mode = LaunchMode::AttachPort {
                host: host.to_string(),
                port,
            };
            let remote_root = cfg.option("remoteRoot");
            if !remote_root.is_empty() {
                r.configuration = json!({ "remoteRoot": remote_root });
            }
            Ok(r)
        }
        id => {
            let Some(runner) = Runner::from_id(id) else {
                return Err(format!(
                    "'{id}' configurations cannot be debugged with js-debug"
                ));
            };
            let spec = ctx
                .test_specs
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "{} is not available: enable JavaScript Test Runners",
                        runner.id()
                    )
                })?;
            let wrapper = JsTestRunner::new(spec).expect("a runner spec");
            let settings = match (wrapper.spec.program_setting.as_deref(), interpreter) {
                (Some(key), node) if !node.is_empty() => {
                    [(key.to_string(), node.to_string())].into()
                }
                _ => BTreeMap::new(),
            };
            let program = wrapper.debug_program(
                &cfg.runtools(),
                &CommandContext {
                    report_dir: ctx.report_dir.clone(),
                    coverage_file: None,
                    settings,
                },
            )?;
            let mut r = base(cfg, node);
            r.program = Some(program.program);
            r.args = program.args;
            r.env.extend(program.env);
            r.configuration = json!({
                "runtimeExecutable": program.runtime,
                "runtimeArgs": program.runtime_args,
            });
            Ok(r)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsers::BrowserKind;

    fn cfg(type_id: &str, options: &[(&str, &str)]) -> JsConfiguration {
        JsConfiguration {
            name: "c".into(),
            type_id: type_id.into(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..JsConfiguration::default()
        }
    }

    fn ctx() -> LaunchContext {
        LaunchContext {
            node: "/opt/node/bin/node".into(),
            browsers: vec![Browser {
                kind: BrowserKind::Chromium,
                name: "Chromium".into(),
                path: "/usr/bin/chromium".into(),
            }],
            browser: "auto".into(),
            test_specs: Vec::new(),
            report_dir: "/tmp/r".into(),
        }
    }

    #[test]
    fn node_files_npm_scripts_and_attach() {
        let mut c = cfg("node", &[("file", "src/server.ts")]);
        c.interpreter_options = "--import tsx".into();
        c.arguments = "--port 8080".into();
        c.env.insert("PORT".into(), "8080".into());
        let r = request(&c, &ctx()).unwrap();
        assert_eq!(r.adapter, "pwa-node");
        assert_eq!(r.program.as_deref(), Some("src/server.ts"));
        assert_eq!(r.arguments.as_deref(), Some("--port 8080"));
        assert_eq!(r.env["PORT"], "8080");
        assert_eq!(
            r.configuration,
            json!({"runtimeExecutable": "/opt/node/bin/node", "runtimeArgs": ["--import", "tsx"]})
        );
        assert!(request(&cfg("node", &[]), &ctx()).is_err());

        let npm = cfg(
            "npm",
            &[
                ("packageJson", "web/package.json"),
                ("script", "dev"),
                ("packageManager", "pnpm"),
            ],
        );
        let r = request(&npm, &ctx()).unwrap();
        assert_eq!(r.program, None);
        assert_eq!(r.cwd.as_deref(), Some("web"));
        assert_eq!(
            r.configuration,
            json!({"runtimeExecutable": "pnpm", "runtimeArgs": ["run", "dev"]})
        );
        let mut with_args = cfg("npm", &[("script", "test")]);
        with_args.arguments = "--watch".into();
        let r = request(&with_args, &ctx()).unwrap();
        assert_eq!(
            r.configuration["runtimeArgs"],
            json!(["run", "test", "--", "--watch"])
        );

        let attach = cfg("js-attach", &[("port", "9230")]);
        let r = request(&attach, &ctx()).unwrap();
        assert_eq!(
            (r.adapter.as_str(), &r.mode),
            (
                "pwa-node",
                &LaunchMode::AttachPort {
                    host: "localhost".into(),
                    port: 9230
                }
            )
        );
        let chrome = request(&cfg("js-attach", &[("debugger", "chrome")]), &ctx()).unwrap();
        assert_eq!(
            (chrome.adapter.as_str(), &chrome.mode),
            (
                "pwa-chrome",
                &LaunchMode::AttachPort {
                    host: "localhost".into(),
                    port: 9222
                }
            )
        );
        assert!(request(&cfg("js-attach", &[("port", "x")]), &ctx()).is_err());
    }

    #[test]
    fn javascript_debug_launches_a_found_browser() {
        let c = cfg("javascript-debug", &[("url", "http://localhost:5173/")]);
        let r = request(&c, &ctx()).unwrap();
        assert_eq!(r.adapter, "pwa-chrome");
        assert_eq!(r.program.as_deref(), Some("http://localhost:5173/"));
        assert_eq!(r.configuration["runtimeExecutable"], "/usr/bin/chromium");
        assert_eq!(r.configuration["userDataDir"], true);
        let edge = cfg(
            "javascript-debug",
            &[("url", "http://x/"), ("browser", "C:\\Edge\\msedge.exe")],
        );
        assert_eq!(request(&edge, &ctx()).unwrap().adapter, "pwa-msedge");
        let none = LaunchContext {
            browsers: Vec::new(),
            ..ctx()
        };
        let err = request(&c, &none).unwrap_err();
        assert!(err.contains("No Chrome"), "{err}");
    }

    #[test]
    fn test_configurations_debug_their_runner() {
        let spec: TestFrameworkSpec = serde_json::from_value(json!({
            "id": "jest", "name": "Jest", "program": "node", "programSetting": "web.node.interpreter",
            "args": ["--json", "--outputFile=${reportDir}/jest.json"], "report": "jUnitFile",
            "nameSeparator": " ",
            "scopes": { "test": ["${file}", "--testNamePattern=^${testPattern}"] }
        }))
        .unwrap();
        let ctx = LaunchContext {
            test_specs: vec![spec],
            ..ctx()
        };
        let c = cfg(
            "jest",
            &[
                ("scope", "test"),
                ("file", "src/cart.test.ts"),
                ("test", "Cart adds"),
            ],
        );
        let r = request(&c, &ctx).unwrap();
        assert_eq!(r.adapter, "pwa-node");
        assert_eq!(r.program.as_deref(), Some("node_modules/jest/bin/jest.js"));
        assert_eq!(
            r.args,
            [
                "--runInBand",
                "--testTimeout=999999",
                "--json",
                "--outputFile=/tmp/r/jest.json",
                "src/cart.test.ts",
                "--testNamePattern=^Cart adds"
            ]
        );
        assert_eq!(r.configuration["runtimeExecutable"], "/opt/node/bin/node");
        assert!(request(&cfg("vitest", &[]), &ctx).is_err(), "not declared");
        assert!(request(&cfg("phpunit", &[]), &ctx).is_err());
    }
}

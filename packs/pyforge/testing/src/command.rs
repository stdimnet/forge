//! Command lines of the Python test runners, the profiler and the console, as the
//! arguments of the interpreter (`python <args>`); the app runs them through the selected
//! interpreter (local, Docker, Docker Compose, SSH, WSL) or a run target.
//!
//! - pytest: `python [options] [debug] -m pytest <scope> --junitxml <dir>/pytest.xml -o
//!   junit_family=xunit1 [--cov --cov-report=xml:<file>] [arguments]`, or a report log
//!   (`--report-log <dir>/pytest.jsonl`, the pytest-reportlog plugin).
//! - unittest: `python [options] [debug] -c <Forge's runner> <scope> [--coverage-xml
//!   <file>]`; the runner reports TeamCity messages on stdout (`helpers/forge_unittest.py`)
//!   and, for Run with Coverage, measures with coverage.py and writes a Cobertura report.
//! - Django tests: `python [options] [debug] -c <Forge's Django runner> --manage manage.py
//!   [labels]` (`manage.py test` with a runner that reports TeamCity messages), or pytest
//!   (pytest-django) with `DJANGO_SETTINGS_MODULE`.
//! - tox: `python -m tox` (or the `python.tox.path` program) `[-e envs] [arguments]`.
//! - Profile: `python [options] -m cProfile -o <file> (<script> | -m <module>) [parameters]`.
//! - Python Console: `python -u [options] -c <Forge's console>` (`helpers/forge_console.py`).
//!
//! Helper scripts travel inline (`-c` with the source base64-encoded), so they also work
//! in containers and on remote hosts without copying files.

use std::collections::BTreeMap;

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::config::{self, scope};

pub const UNITTEST_RUNNER: &str = include_str!("../helpers/forge_unittest.py");
pub const CONSOLE: &str = include_str!("../helpers/forge_console.py");
pub const DJANGO_RUNNER: &str = include_str!("../helpers/forge_django_tests.py");

/// How pytest reports its results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PytestReporter {
    /// JUnit XML (`--junitxml`, built into pytest).
    #[default]
    Junitxml,
    /// A report log (`--report-log`, needs pytest-reportlog).
    ReportLog,
}

impl PytestReporter {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "reportLog" => PytestReporter::ReportLog,
            _ => PytestReporter::Junitxml,
        }
    }
}

/// What building a command needs besides the configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// The run's working directory (absolute), to make report paths relative to it: they
    /// stay valid when an interpreter or run target maps the folder elsewhere.
    pub cwd: String,
    /// Folder for report files.
    pub report_dir: String,
    /// Run with Coverage: the Cobertura report to write.
    pub coverage_file: Option<String>,
    pub reporter: PytestReporter,
    /// `python.tox.path`: tox's program (empty: `python -m tox`).
    pub tox_program: String,
    /// Arguments placed after the interpreter options, before the runner: a debugger
    /// (`-m debugpy --listen …`).
    pub debug_args: Vec<String>,
}

/// A Python test run: the interpreter's arguments (or another program) and how its
/// results are read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonCommand {
    /// Another program than the interpreter (tox from `python.tox.path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    pub report: TestReport,
    pub flavor: JUnitFlavor,
    /// The runner writes report files into the report folder.
    #[serde(default)]
    pub report_files: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_file: Option<String>,
    /// Run with Coverage: the Cobertura report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_file: Option<String>,
}

/// Standard base64 (RFC 4648) for inline helper scripts.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// `-c <code>` that runs a helper script (named `name` in tracebacks).
pub fn inline_script(name: &str, source: &str) -> [String; 2] {
    [
        "-c".to_string(),
        format!(
            "import base64;exec(compile(base64.b64decode('{}').decode('utf-8'),'{name}','exec'))",
            base64(source.as_bytes())
        ),
    ]
}

/// `path` relative to `cwd` when it lies below it (forward slashes), else unchanged.
pub fn relative_to(cwd: &str, path: &str) -> String {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_string();
    let (c, p) = (norm(cwd), norm(path));
    if c.is_empty() {
        return p;
    }
    match p.strip_prefix(&format!("{c}/")) {
        Some(rest) => rest.to_string(),
        None if p == c => ".".into(),
        None => p,
    }
}

fn base_env() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("PYTHONUNBUFFERED".to_string(), "1".to_string()),
        ("PYTHONIOENCODING".to_string(), "utf-8".to_string()),
    ])
}

fn required<'a>(cfg: &'a Configuration, key: &str, what: &str) -> Result<&'a str, String> {
    match cfg.option(key).trim() {
        "" => Err(format!("'{}': choose the {what}", cfg.name)),
        v => Ok(v),
    }
}

/// Interpreter options and the debugger's arguments: what comes before the runner.
fn interpreter_head(cfg: &Configuration, ctx: &Context) -> Vec<String> {
    let mut args = split_args(&cfg.interpreter_options);
    args.extend(ctx.debug_args.iter().cloned());
    args
}

/// The pytest command of a configuration.
pub fn pytest(cfg: &Configuration, ctx: &Context) -> Result<PythonCommand, String> {
    let mut args = interpreter_head(cfg, ctx);
    args.extend(["-m".into(), "pytest".into()]);
    let mut default_file = None;
    match scope(cfg) {
        "all" => {}
        "directory" => args.push(required(cfg, "directory", "directory")?.to_string()),
        "file" => {
            let f = required(cfg, "file", "test file")?;
            default_file = Some(f.to_string());
            args.push(f.to_string());
        }
        "test" => {
            let f = required(cfg, "file", "test file")?;
            let t = required(cfg, "test", "test")?;
            default_file = Some(f.to_string());
            args.push(format!("{f}::{t}"));
        }
        "keyword" => {
            let k = required(cfg, "keyword", "keyword expression")?;
            for key in ["file", "directory"] {
                if !cfg.option(key).trim().is_empty() {
                    args.push(cfg.option(key).trim().to_string());
                }
            }
            args.extend(["-k".into(), k.to_string()]);
        }
        "nodes" => {
            let nodes: Vec<String> = cfg
                .option("nodes")
                .lines()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            if nodes.is_empty() {
                return Err(format!("'{}': no tests to run", cfg.name));
            }
            args.extend(nodes);
        }
        other => return Err(format!("unknown pytest scope '{other}'")),
    }
    let dir = relative_to(&ctx.cwd, &ctx.report_dir);
    let (report, flavor) = match ctx.reporter {
        PytestReporter::Junitxml => {
            args.extend([
                "--junitxml".into(),
                format!("{dir}/pytest.xml"),
                "-o".into(),
                "junit_family=xunit1".into(),
            ]);
            (TestReport::JUnitFile, JUnitFlavor::Pytest)
        }
        PytestReporter::ReportLog => {
            args.extend(["--report-log".into(), format!("{dir}/pytest.jsonl")]);
            (TestReport::JUnitFile, JUnitFlavor::Generic)
        }
    };
    let extra = split_args(&cfg.arguments);
    if !extra.iter().any(|a| a.starts_with("--color")) {
        args.push("--color=yes".into());
    }
    let coverage_file = ctx.coverage_file.as_ref().map(|f| {
        args.extend([
            "--cov".into(),
            format!("--cov-report=xml:{}", relative_to(&ctx.cwd, f)),
            "--cov-report=term".into(),
        ]);
        f.clone()
    });
    args.extend(extra);
    Ok(PythonCommand {
        program: None,
        args,
        env: base_env(),
        report,
        flavor,
        report_files: true,
        default_file,
        coverage_file,
    })
}

/// The unittest command of a configuration (Forge's runner).
pub fn unittest(cfg: &Configuration, ctx: &Context) -> Result<PythonCommand, String> {
    let mut args = interpreter_head(cfg, ctx);
    args.extend(inline_script("forge_unittest.py", UNITTEST_RUNNER));
    let pattern = match cfg.option("pattern").trim() {
        "" => "test*.py".to_string(),
        p => p.to_string(),
    };
    let mut default_file = None;
    match scope(cfg) {
        "all" => args.extend(["--discover".into(), ".".into(), "--pattern".into(), pattern]),
        "directory" => args.extend([
            "--discover".into(),
            required(cfg, "directory", "directory")?.to_string(),
            "--pattern".into(),
            pattern,
        ]),
        "file" | "test" => {
            let f = required(cfg, "file", "test file")?;
            default_file = Some(f.to_string());
            args.extend(["--file".into(), relative_to(&ctx.cwd, f)]);
            if scope(cfg) == "test" {
                args.extend(["--test".into(), required(cfg, "test", "test")?.to_string()]);
            }
        }
        "nodes" => {
            let ids: Vec<&str> = cfg
                .option("nodes")
                .lines()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .collect();
            if ids.is_empty() {
                return Err(format!("'{}': no tests to run", cfg.name));
            }
            for id in ids {
                args.extend(["--id".into(), id.to_string()]);
            }
        }
        other => return Err(format!("unknown unittest scope '{other}'")),
    }
    let coverage_file = ctx.coverage_file.as_ref().map(|f| {
        args.extend(["--coverage-xml".into(), relative_to(&ctx.cwd, f)]);
        f.clone()
    });
    args.extend(split_args(&cfg.arguments));
    Ok(PythonCommand {
        program: None,
        args,
        env: base_env(),
        report: TestReport::TeamCity,
        flavor: JUnitFlavor::Generic,
        report_files: false,
        default_file,
        coverage_file,
    })
}

/// The tox command of a configuration.
pub fn tox(cfg: &Configuration, ctx: &Context) -> Result<PythonCommand, String> {
    let (program, mut args) = match ctx.tox_program.trim() {
        "" => {
            let mut a = split_args(&cfg.interpreter_options);
            a.extend(["-m".into(), "tox".into()]);
            (None, a)
        }
        p => (Some(p.to_string()), Vec::new()),
    };
    let envs: Vec<&str> = cfg
        .option("environments")
        .split([',', '\n'])
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    if !envs.is_empty() {
        args.extend(["-e".into(), envs.join(",")]);
    }
    args.extend(split_args(&cfg.arguments));
    let mut env = base_env();
    // Plain output: the parser reads the environments' lines.
    env.insert("NO_COLOR".into(), "1".into());
    Ok(PythonCommand {
        program,
        args,
        env,
        report: TestReport::Tox,
        flavor: JUnitFlavor::Generic,
        report_files: false,
        default_file: None,
        coverage_file: None,
    })
}

/// Test labels of a Django configuration: Rerun Failed Tests' ids, else `target`.
fn django_labels(cfg: &Configuration) -> Vec<String> {
    let nodes: Vec<String> = cfg
        .option("nodes")
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect();
    if !nodes.is_empty() {
        return nodes;
    }
    split_args(cfg.option("target"))
}

/// The Django tests command: `manage.py test` through Forge's runner, or pytest-django.
pub fn django_tests(cfg: &Configuration, ctx: &Context) -> Result<PythonCommand, String> {
    let settings = cfg.option("settings").trim().to_string();
    let mut command = if cfg.option("runner") == "pytest" {
        let labels = django_labels(cfg);
        let mut py = cfg.clone();
        py.type_id = config::PYTEST.into();
        py.options.clear();
        if labels.is_empty() {
            py.options.insert("scope".into(), "all".into());
        } else {
            py.options.insert("scope".into(), "nodes".into());
            py.options.insert("nodes".into(), labels.join("\n"));
        }
        pytest(&py, ctx)?
    } else {
        let mut args = interpreter_head(cfg, ctx);
        let combined = format!(
            "FORGE_UNITTEST = '{}'\n{DJANGO_RUNNER}",
            base64(UNITTEST_RUNNER.as_bytes())
        );
        args.extend(inline_script("forge_django_tests.py", &combined));
        let manage = match cfg.option("manage").trim() {
            "" => "manage.py".to_string(),
            m => relative_to(&ctx.cwd, m),
        };
        args.extend(["--manage".into(), manage]);
        args.extend(django_labels(cfg));
        args.extend(split_args(&cfg.arguments));
        PythonCommand {
            program: None,
            args,
            env: base_env(),
            report: TestReport::TeamCity,
            flavor: JUnitFlavor::Generic,
            report_files: false,
            default_file: None,
            coverage_file: None,
        }
    };
    if !settings.is_empty() {
        command
            .env
            .insert("DJANGO_SETTINGS_MODULE".into(), settings);
    }
    Ok(command)
}

/// The command of any Python test configuration.
pub fn build(cfg: &Configuration, ctx: &Context) -> Result<PythonCommand, String> {
    match cfg.type_id.as_str() {
        config::PYTEST => pytest(cfg, ctx),
        config::UNITTEST => unittest(cfg, ctx),
        config::TOX => tox(cfg, ctx),
        config::DJANGO => django_tests(cfg, ctx),
        other => Err(format!("'{other}' is not a Python test configuration")),
    }
}

/// `-m debugpy --listen <host>:<port> --wait-for-client`: what a debug run puts before the
/// program (`--configure-subProcess true` follows child processes).
pub fn debugpy_args(host: &str, port: u16, sub_process: bool) -> Vec<String> {
    let mut args = vec![
        "-m".to_string(),
        "debugpy".to_string(),
        "--listen".to_string(),
        format!("{host}:{port}"),
        "--wait-for-client".to_string(),
    ];
    if !sub_process {
        args.extend(["--configure-subProcess".into(), "false".into()]);
    }
    args
}

/// `-m cProfile -o <file>`: what Profile puts before the program.
pub fn profile_args(file: &str, sort: &str) -> Vec<String> {
    let mut args = vec!["-m".into(), "cProfile".into(), "-o".into(), file.into()];
    if !sort.is_empty() {
        args.extend(["-s".into(), sort.into()]);
    }
    args
}

/// The console process: `python -u [options] -c <console> [--no-ipython]`.
pub fn console(options: &str, ipython: bool) -> Vec<String> {
    let mut args = vec!["-u".to_string()];
    args.extend(split_args(options));
    args.extend(inline_script("forge_console.py", CONSOLE));
    if !ipython {
        args.push("--no-ipython".into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            cwd: "/home/me/shop".into(),
            report_dir: "/tmp/forge-test-reports/run-1".into(),
            ..Context::default()
        }
    }

    #[test]
    fn pytest_scopes() {
        let cfg = config::pytest_test("tests/test_cart.py", "TestCart::test_add[one]", "t");
        let c = pytest(&cfg, &ctx()).unwrap();
        assert_eq!(
            c.args,
            [
                "-m",
                "pytest",
                "tests/test_cart.py::TestCart::test_add[one]",
                "--junitxml",
                "/tmp/forge-test-reports/run-1/pytest.xml",
                "-o",
                "junit_family=xunit1",
                "--color=yes"
            ]
        );
        assert_eq!(c.report, TestReport::JUnitFile);
        assert_eq!(c.flavor, JUnitFlavor::Pytest);
        assert_eq!(c.default_file.as_deref(), Some("tests/test_cart.py"));
        assert_eq!(c.env["PYTHONUNBUFFERED"], "1");

        let mut cfg = Configuration::new("k", "pytest")
            .with_option("scope", "keyword")
            .with_option("keyword", "cart and not slow")
            .with_option("directory", "tests");
        cfg.interpreter_options = "-X dev".into();
        cfg.arguments = "-x --color=no".into();
        let mut cx = ctx();
        cx.report_dir = "/home/me/shop/.forge/test-reports/run-2".into();
        cx.coverage_file =
            Some("/home/me/shop/.forge/test-reports/run-2-coverage/coverage.xml".into());
        cx.reporter = PytestReporter::ReportLog;
        cx.debug_args = debugpy_args("0.0.0.0", 5678, true);
        let c = pytest(&cfg, &cx).unwrap();
        assert_eq!(
            c.args,
            [
                "-X",
                "dev",
                "-m",
                "debugpy",
                "--listen",
                "0.0.0.0:5678",
                "--wait-for-client",
                "-m",
                "pytest",
                "tests",
                "-k",
                "cart and not slow",
                "--report-log",
                ".forge/test-reports/run-2/pytest.jsonl",
                "--cov",
                "--cov-report=xml:.forge/test-reports/run-2-coverage/coverage.xml",
                "--cov-report=term",
                "-x",
                "--color=no"
            ]
        );
        assert!(c.coverage_file.is_some());

        let rerun = Configuration::new("r", "pytest")
            .with_option("scope", "nodes")
            .with_option("nodes", "a.py::t1\n\na.py::C::t2\n");
        let c = pytest(&rerun, &ctx()).unwrap();
        assert_eq!(c.args[2..4], ["a.py::t1", "a.py::C::t2"]);
        assert!(pytest(
            &Configuration::new("x", "pytest").with_option("scope", "file"),
            &ctx()
        )
        .is_err());
        assert_eq!(
            pytest(&Configuration::new("all", "pytest"), &ctx())
                .unwrap()
                .args[..2],
            ["-m", "pytest"]
        );
    }

    #[test]
    fn unittest_runs_forges_runner() {
        let cfg =
            config::unittest_test("/home/me/shop/tests/test_cart.py", "TestCart.test_add", "u");
        let c = unittest(&cfg, &ctx()).unwrap();
        assert_eq!(c.args[0], "-c");
        assert!(c.args[1].starts_with("import base64;exec(compile(base64.b64decode('"));
        assert_eq!(
            c.args[2..],
            [
                "--file",
                "tests/test_cart.py",
                "--test",
                "TestCart.test_add"
            ]
        );
        assert_eq!(c.report, TestReport::TeamCity);
        let all = unittest(
            &Configuration::new("a", "unittest").with_option("pattern", "*_test.py"),
            &ctx(),
        )
        .unwrap();
        assert_eq!(all.args[2..], ["--discover", ".", "--pattern", "*_test.py"]);
        let ids = Configuration::new("r", "unittest")
            .with_option("scope", "nodes")
            .with_option("nodes", "tests.test_cart.TestCart.test_add");
        assert_eq!(
            unittest(&ids, &ctx()).unwrap().args[2..],
            ["--id", "tests.test_cart.TestCart.test_add"]
        );
        // Run with Coverage: coverage.py inside Forge's runner, a Cobertura report.
        let mut cx = ctx();
        cx.coverage_file =
            Some("/home/me/shop/.forge/test-reports/run-3-coverage/coverage.xml".into());
        let c = unittest(&ids, &cx).unwrap();
        assert_eq!(
            c.args[4..],
            [
                "--coverage-xml",
                ".forge/test-reports/run-3-coverage/coverage.xml"
            ]
        );
        assert!(c.coverage_file.is_some());
    }

    #[test]
    fn tox_environments() {
        let cfg = Configuration::new("tox", "tox").with_option("environments", "py312, lint");
        let c = tox(&cfg, &ctx()).unwrap();
        assert_eq!(c.program, None);
        assert_eq!(c.args, ["-m", "tox", "-e", "py312,lint"]);
        assert_eq!(c.report, TestReport::Tox);
        let mut cx = ctx();
        cx.tox_program = "/home/me/.local/bin/tox".into();
        let c = tox(&Configuration::new("tox", "tox"), &cx).unwrap();
        assert_eq!(c.program.as_deref(), Some("/home/me/.local/bin/tox"));
        assert!(c.args.is_empty());
        assert!(build(&Configuration::new("x", "python"), &cx).is_err());
    }

    #[test]
    fn helpers_and_paths() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
        assert_eq!(relative_to("/a/b", "/a/b/c/d.xml"), "c/d.xml");
        assert_eq!(relative_to("/a/b", "/tmp/x"), "/tmp/x");
        assert_eq!(relative_to("C:\\p", "C:\\p\\r.xml"), "r.xml");
        assert_eq!(
            profile_args("/c/p.pstats", ""),
            ["-m", "cProfile", "-o", "/c/p.pstats"]
        );
        assert_eq!(
            debugpy_args("127.0.0.1", 1, false)[5..],
            ["--configure-subProcess", "false"]
        );
        let c = console("-X dev", false);
        assert_eq!(c[..3], ["-u", "-X", "dev"]);
        assert_eq!(c.last().unwrap(), "--no-ipython");
    }

    #[test]
    fn django_tests() {
        let ctx = Context {
            cwd: "/p".into(),
            report_dir: "/p/.r".into(),
            ..Default::default()
        };
        let cfg = Configuration::new("Django tests", config::DJANGO)
            .with_option("target", "shop.tests blog")
            .with_option("settings", "mysite.settings")
            .with_option("manage", "/p/manage.py");
        let c = build(&cfg, &ctx).unwrap();
        assert_eq!(c.report, TestReport::TeamCity);
        assert_eq!(c.args[0], "-c");
        assert!(c.args[1].contains("forge_django_tests.py"));
        assert_eq!(c.args[2..], ["--manage", "manage.py", "shop.tests", "blog"]);
        assert_eq!(c.env["DJANGO_SETTINGS_MODULE"], "mysite.settings");
        let py = build(
            &cfg.clone()
                .with_option("runner", "pytest")
                .with_option("target", ""),
            &ctx,
        )
        .unwrap();
        assert_eq!(py.args[..2], ["-m", "pytest"]);
        assert_eq!(py.env["DJANGO_SETTINGS_MODULE"], "mysite.settings");
        let rerun = build(&cfg.with_option("nodes", "shop.tests.T.test_a"), &ctx).unwrap();
        assert_eq!(
            rerun.args[2..],
            ["--manage", "manage.py", "shop.tests.T.test_a"]
        );
    }
}

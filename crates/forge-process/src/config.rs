//! Run/Debug configurations.
//!
//! Shared configurations ("Store as project file") live in
//! `<project>/.forge/runConfigurations.toml`, so they can be committed; the others and the
//! selected configuration live in `<project>/.forge/runConfigurations.local.toml` (PHPStorm
//! keeps those in `workspace.xml`).
//!
//! A [`KindRegistry`] maps the configuration `type` to a [`RunConfigurationKind`] that builds
//! the command line. Plugins register their own kinds; built in are PHP Script, PHPUnit,
//! Artisan, Composer script, Shell script and npm script.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

use crate::args::split_args;
use crate::runner::CommandLine;

pub const PROJECT_FILE: &str = ".forge/runConfigurations.toml";
pub const LOCAL_FILE: &str = ".forge/runConfigurations.local.toml";

/// A task run before the configuration starts ("Before launch").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeLaunchTask {
    /// `runConfiguration` (another configuration by name), `shell` (a command line) or a
    /// plugin-defined kind.
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

/// One Run/Debug configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfiguration {
    pub name: String,
    /// Configuration type id, e.g. `phpunit` (see `runConfigurationTypes` contributions).
    #[serde(rename = "type")]
    pub type_id: String,
    /// Stored in the shared project file rather than the local one. Sent to the UI but not
    /// written to disk: the file a configuration is in says where it belongs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub store_as_project_file: bool,
    /// Working directory, relative to the project root or absolute; empty = project root.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub working_directory: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Interpreter executable (PHP, node, bash); empty = the project default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interpreter: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interpreter_options: String,
    /// Program / script arguments.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    /// Type-specific fields (file, class, method, script, ...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before_launch: Vec<BeforeLaunchTask>,
}

impl RunConfiguration {
    pub fn option(&self, key: &str) -> &str {
        self.options.get(key).map(String::as_str).unwrap_or("")
    }
}

/// All configurations of a project plus the one selected in the toolbar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfigurations {
    #[serde(default)]
    pub configurations: Vec<RunConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    /// Per-type defaults for new configurations ("Edit configuration templates…"); kept in
    /// the local file. Their `name` is ignored.
    #[serde(default)]
    pub templates: Vec<RunConfiguration>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected: Option<String>,
    #[serde(default, rename = "configuration")]
    configurations: Vec<RunConfiguration>,
    #[serde(default, rename = "template", skip_serializing_if = "Vec::is_empty")]
    templates: Vec<RunConfiguration>,
}

fn read_text(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ForgeError::io(path, e)),
    }
}

fn parse_file(name: &str, text: Option<&str>) -> Result<FileFormat> {
    match text {
        Some(t) => toml::from_str(t).map_err(|e| ForgeError::Other(format!("{name}: {e}"))),
        None => Ok(FileFormat::default()),
    }
}

/// The file's text, `None` when it has nothing to store.
fn file_text(file: &FileFormat) -> Result<Option<String>> {
    if file.configurations.is_empty() && file.selected.is_none() && file.templates.is_empty() {
        return Ok(None);
    }
    toml::to_string_pretty(file)
        .map(Some)
        .map_err(|e| ForgeError::Other(e.to_string()))
}

fn write_text(path: &Path, text: Option<&str>) -> Result<()> {
    let Some(text) = text else {
        // Do not leave empty files behind in the project.
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(ForgeError::io(path, e)),
            _ => Ok(()),
        };
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
    }
    std::fs::write(path, text).map_err(|e| ForgeError::io(path, e))
}

impl RunConfigurations {
    /// Read both files of the project at `root`. Missing files mean no configurations.
    pub fn load(root: &Path) -> Result<Self> {
        let shared = read_text(&root.join(PROJECT_FILE))?;
        let local = read_text(&root.join(LOCAL_FILE))?;
        Self::from_texts(shared.as_deref(), local.as_deref())
    }

    /// Parse the texts of the shared ([`PROJECT_FILE`]) and local ([`LOCAL_FILE`]) files
    /// (`None` = missing), e.g. read from a remote host.
    pub fn from_texts(shared: Option<&str>, local: Option<&str>) -> Result<Self> {
        let shared = parse_file(PROJECT_FILE, shared)?;
        let local = parse_file(LOCAL_FILE, local)?;
        let mut configurations: Vec<RunConfiguration> = shared
            .configurations
            .into_iter()
            .map(|c| RunConfiguration {
                store_as_project_file: true,
                ..c
            })
            .collect();
        configurations.extend(local.configurations);
        Ok(Self {
            configurations,
            selected: local.selected.or(shared.selected),
            templates: local.templates,
        })
    }

    /// Write the configurations back, each to the file its `store_as_project_file` picks.
    pub fn save(&self, root: &Path) -> Result<()> {
        let [shared, local] = self.to_texts()?;
        write_text(&root.join(PROJECT_FILE), shared.as_deref())?;
        write_text(&root.join(LOCAL_FILE), local.as_deref())
    }

    /// The texts of the shared and the local file; `None` means the file should not exist.
    pub fn to_texts(&self) -> Result<[Option<String>; 2]> {
        let mut seen = std::collections::HashSet::new();
        for c in &self.configurations {
            if c.name.trim().is_empty() {
                return Err(ForgeError::Other("a configuration has no name".into()));
            }
            if !seen.insert((&c.type_id, &c.name)) {
                return Err(ForgeError::Other(format!(
                    "duplicate configuration name `{}`",
                    c.name
                )));
            }
        }
        let (shared, local): (Vec<_>, Vec<_>) = self
            .configurations
            .iter()
            .cloned()
            .partition(|c| c.store_as_project_file);
        let shared = shared
            .into_iter()
            .map(|c| RunConfiguration {
                store_as_project_file: false,
                ..c
            })
            .collect();
        let shared = file_text(&FileFormat {
            selected: None,
            configurations: shared,
            templates: Vec::new(),
        })?;
        let local = file_text(&FileFormat {
            selected: self.selected.clone(),
            configurations: local,
            templates: self
                .templates
                .iter()
                .map(|t| RunConfiguration {
                    name: if t.name.is_empty() {
                        t.type_id.clone()
                    } else {
                        t.name.clone()
                    },
                    ..t.clone()
                })
                .collect(),
        })?;
        Ok([shared, local])
    }
}

/// What a kind needs to know to build a command line.
#[derive(Debug, Clone)]
pub struct BuildContext {
    pub root: PathBuf,
    /// Project default PHP interpreter (setting `php.interpreter`).
    pub php_interpreter: String,
}

impl BuildContext {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            php_interpreter: "php".into(),
        }
    }

    /// Resolve a path from a configuration field against the project root.
    pub fn resolve(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    fn working_dir(&self, cfg: &RunConfiguration, fallback: &str) -> PathBuf {
        let dir = if cfg.working_directory.trim().is_empty() {
            fallback
        } else {
            cfg.working_directory.trim()
        };
        if dir.is_empty() {
            self.root.clone()
        } else {
            self.resolve(dir)
        }
    }

    fn php(&self, cfg: &RunConfiguration) -> String {
        match cfg.interpreter.trim() {
            "" => match self.php_interpreter.trim() {
                "" => "php".into(),
                p => p.into(),
            },
            p => p.into(),
        }
    }
}

/// A configuration type that knows how to start its configurations.
pub trait RunConfigurationKind: Send + Sync {
    fn id(&self) -> &str;
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine>;
}

/// Registry of configuration kinds, keyed by type id.
pub struct KindRegistry {
    kinds: Vec<Box<dyn RunConfigurationKind>>,
}

impl Default for KindRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl KindRegistry {
    pub fn empty() -> Self {
        Self { kinds: Vec::new() }
    }

    /// Registry with the built-in kinds.
    pub fn builtin() -> Self {
        let mut r = Self::empty();
        r.register(Box::new(PhpScript));
        r.register(Box::new(PhpUnit));
        r.register(Box::new(Artisan));
        r.register(Box::new(ComposerScript));
        r.register(Box::new(ShellScript));
        r.register(Box::new(NpmScript));
        r
    }

    /// Add a kind; a kind with the same id replaces the earlier one.
    pub fn register(&mut self, kind: Box<dyn RunConfigurationKind>) {
        self.kinds.retain(|k| k.id() != kind.id());
        self.kinds.push(kind);
    }

    pub fn get(&self, id: &str) -> Option<&dyn RunConfigurationKind> {
        self.kinds.iter().find(|k| k.id() == id).map(|k| k.as_ref())
    }

    pub fn ids(&self) -> Vec<&str> {
        self.kinds.iter().map(|k| k.id()).collect()
    }

    /// Build the command line of `cfg`, including its environment variables.
    pub fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let kind = self.get(&cfg.type_id).ok_or_else(|| {
            ForgeError::Other(format!("unknown run configuration type `{}`", cfg.type_id))
        })?;
        let mut cmd = kind.build(cfg, ctx)?;
        for (k, v) in &cfg.env {
            cmd.env.insert(k.clone(), v.clone());
        }
        Ok(cmd)
    }
}

fn required<'a>(cfg: &'a RunConfiguration, key: &str, label: &str) -> Result<&'a str> {
    match cfg.option(key).trim() {
        "" => Err(ForgeError::Other(format!("{label} is not specified"))),
        v => Ok(v),
    }
}

fn platform_program(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.into()
    }
}

/// `php [interpreter options] <file> [arguments]`.
struct PhpScript;
impl RunConfigurationKind for PhpScript {
    fn id(&self) -> &str {
        "php-script"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let file = required(cfg, "file", "File")?;
        let mut cmd = CommandLine::new(ctx.php(cfg))
            .args(split_args(&cfg.interpreter_options))
            .arg(file)
            .args(split_args(&cfg.arguments));
        cmd.cwd = ctx.working_dir(cfg, "");
        Ok(cmd)
    }
}

/// PHPUnit with TeamCity output for the test tree. Options: `scope` (`directory`, `file`,
/// `class`, `method`, `config`), `directory`, `file`, `class`, `method`, `configFile`,
/// `phpunitPath` (default `vendor/bin/phpunit`), `testRunnerOptions`.
struct PhpUnit;
impl RunConfigurationKind for PhpUnit {
    fn id(&self) -> &str {
        "phpunit"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let phpunit = match cfg.option("phpunitPath").trim() {
            "" => "vendor/bin/phpunit",
            p => p,
        };
        let mut cmd = CommandLine::new(ctx.php(cfg))
            .args(split_args(&cfg.interpreter_options))
            .arg(phpunit)
            .arg("--teamcity");
        let config = cfg.option("configFile").trim();
        if !config.is_empty() {
            cmd = cmd.args(["--configuration", config]);
        } else if let Some(found) = ["phpunit.xml", "phpunit.xml.dist", "phpunit.dist.xml"]
            .into_iter()
            .find(|f| ctx.root.join(f).is_file())
        {
            cmd = cmd.args(["--configuration", found]);
        }
        cmd = cmd.args(split_args(cfg.option("testRunnerOptions")));
        match cfg.option("scope") {
            "directory" => cmd = cmd.arg(required(cfg, "directory", "Directory")?),
            "file" => cmd = cmd.arg(required(cfg, "file", "File")?),
            "class" => {
                let class = required(cfg, "class", "Class")?;
                let short = class.rsplit('\\').next().unwrap_or(class);
                cmd = cmd.args(["--filter", &format!("/{}(::.*)?$/", regex_escape(short))]);
                if let Some(file) = Some(cfg.option("file").trim()).filter(|f| !f.is_empty()) {
                    cmd = cmd.arg(file);
                }
            }
            "method" => {
                let method = required(cfg, "method", "Method")?;
                let class = cfg.option("class").trim();
                let short = class.rsplit('\\').next().unwrap_or(class);
                let filter = if short.is_empty() {
                    format!("/::{}( .*)?$/", regex_escape(method))
                } else {
                    format!("/{}::{}( .*)?$/", regex_escape(short), regex_escape(method))
                };
                cmd = cmd.args(["--filter", &filter]);
                if let Some(file) = Some(cfg.option("file").trim()).filter(|f| !f.is_empty()) {
                    cmd = cmd.arg(file);
                }
            }
            // "config" or empty: run what phpunit.xml defines.
            _ => {}
        }
        cmd = cmd.args(split_args(&cfg.arguments));
        cmd.cwd = ctx.working_dir(cfg, "");
        Ok(cmd)
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$/#".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `php artisan <command> [arguments] --ansi`. Option: `command`.
struct Artisan;
impl RunConfigurationKind for Artisan {
    fn id(&self) -> &str {
        "artisan"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let command = required(cfg, "command", "Command")?;
        let mut cmd = CommandLine::new(ctx.php(cfg))
            .args(split_args(&cfg.interpreter_options))
            .arg("artisan")
            .args(split_args(command))
            .args(split_args(&cfg.arguments))
            .arg("--ansi");
        cmd.cwd = ctx.working_dir(cfg, "");
        Ok(cmd)
    }
}

/// `composer run-script <script> --ansi [-- arguments]`. Options: `script`, `composerPath`.
struct ComposerScript;
impl RunConfigurationKind for ComposerScript {
    fn id(&self) -> &str {
        "composer"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let script = required(cfg, "script", "Script")?;
        let program = match cfg.option("composerPath").trim() {
            "" => platform_program("composer"),
            p => p.to_string(),
        };
        let mut cmd = CommandLine::new(program).args(["run-script", script, "--ansi"]);
        let extra = split_args(&cfg.arguments);
        if !extra.is_empty() {
            cmd = cmd.arg("--").args(extra);
        }
        cmd.cwd = ctx.working_dir(cfg, "");
        Ok(cmd)
    }
}

/// A script file or inline text. Options: `mode` (`file` | `text`), `scriptPath`,
/// `scriptText`; the interpreter defaults to `/bin/sh` (`cmd.exe` on Windows).
struct ShellScript;
impl RunConfigurationKind for ShellScript {
    fn id(&self) -> &str {
        "shell"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let interpreter = match cfg.interpreter.trim() {
            "" if cfg!(windows) => "cmd.exe".to_string(),
            "" => "/bin/sh".to_string(),
            i => i.to_string(),
        };
        let mut cmd = CommandLine::new(&interpreter).args(split_args(&cfg.interpreter_options));
        let is_cmd = interpreter.to_ascii_lowercase().ends_with("cmd.exe");
        if cfg.option("mode") == "text" {
            let text = required(cfg, "scriptText", "Script text")?;
            cmd = cmd.args([if is_cmd { "/C" } else { "-c" }, text]);
        } else {
            let path = required(cfg, "scriptPath", "Script path")?;
            if is_cmd {
                cmd = cmd.arg("/C");
            }
            cmd = cmd.arg(path).args(split_args(&cfg.arguments));
        }
        let script_dir = Path::new(cfg.option("scriptPath"))
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        cmd.cwd = ctx.working_dir(cfg, &script_dir);
        Ok(cmd)
    }
}

/// `npm run <script> [-- arguments]`. Options: `script`, `command` (default `run`),
/// `packageJson` (its directory is the default working directory), `packageManager`
/// (`npm`, `pnpm`, `yarn`).
struct NpmScript;
impl RunConfigurationKind for NpmScript {
    fn id(&self) -> &str {
        "npm"
    }
    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let command = match cfg.option("command").trim() {
            "" => "run",
            c => c,
        };
        let manager = match cfg.option("packageManager").trim() {
            "" => "npm",
            m => m,
        };
        let mut cmd = CommandLine::new(platform_program(manager)).arg(command);
        if command == "run" {
            cmd = cmd.arg(required(cfg, "script", "Script")?);
        }
        let extra = split_args(&cfg.arguments);
        if !extra.is_empty() {
            cmd = cmd.arg("--").args(extra);
        }
        let package_dir = Path::new(cfg.option("packageJson"))
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        cmd.cwd = ctx.working_dir(cfg, &package_dir);
        Ok(cmd)
    }
}

/// A kind declared by a plugin manifest: a command template such as
/// `["${interpreter}", "${interpreterOptions}", "${file}", "${arguments}"]`.
pub struct TemplateKind {
    pub id: String,
    pub command: Vec<String>,
    pub default_interpreter: Option<String>,
    /// Option keys that must be set (labels for the error message).
    pub required: Vec<(String, String)>,
}

impl RunConfigurationKind for TemplateKind {
    fn id(&self) -> &str {
        &self.id
    }

    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        for (key, label) in &self.required {
            required(cfg, key, label)?;
        }
        let interpreter = match cfg.interpreter.trim() {
            "" => self.default_interpreter.clone().unwrap_or_default(),
            i => i.to_string(),
        };
        let mut parts: Vec<String> = Vec::new();
        for entry in &self.command {
            match entry.strip_prefix("${").and_then(|e| e.strip_suffix('}')) {
                Some("interpreter") => parts.push(interpreter.clone()),
                Some("interpreterOptions") => parts.extend(split_args(&cfg.interpreter_options)),
                Some("arguments") => parts.extend(split_args(&cfg.arguments)),
                Some(key) => parts.push(cfg.option(key).to_string()),
                None => parts.push(entry.clone()),
            }
        }
        parts.retain(|p| !p.is_empty());
        if parts.is_empty() {
            return Err(ForgeError::Other(format!(
                "run configuration type `{}` has no command",
                self.id
            )));
        }
        let mut cmd = CommandLine::new(parts.remove(0)).args(parts);
        cmd.cwd = ctx.working_dir(cfg, "");
        Ok(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(type_id: &str, options: &[(&str, &str)]) -> RunConfiguration {
        RunConfiguration {
            name: "test".into(),
            type_id: type_id.into(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn ctx() -> BuildContext {
        BuildContext {
            root: PathBuf::from("/proj"),
            php_interpreter: "/usr/bin/php8.3".into(),
        }
    }

    #[test]
    fn php_script_uses_project_interpreter_unless_overridden() {
        let r = KindRegistry::builtin();
        let mut c = cfg("php-script", &[("file", "bin/run.php")]);
        c.arguments = "--force \"a b\"".into();
        c.interpreter_options = "-d memory_limit=-1".into();
        c.env.insert("APP_ENV".into(), "local".into());
        let cmd = r.build(&c, &ctx()).unwrap();
        assert_eq!(cmd.program, "/usr/bin/php8.3");
        assert_eq!(
            cmd.args,
            ["-d", "memory_limit=-1", "bin/run.php", "--force", "a b"]
        );
        assert_eq!(cmd.cwd, PathBuf::from("/proj"));
        assert_eq!(cmd.env["APP_ENV"], "local");
        c.interpreter = "php7".into();
        c.working_directory = "bin".into();
        let cmd = r.build(&c, &ctx()).unwrap();
        assert_eq!(cmd.program, "php7");
        assert_eq!(cmd.cwd, PathBuf::from("/proj/bin"));
    }

    #[test]
    fn missing_required_fields_are_reported() {
        let r = KindRegistry::builtin();
        let err = r.build(&cfg("php-script", &[]), &ctx()).unwrap_err();
        assert_eq!(err.to_string(), "File is not specified");
        assert!(r.build(&cfg("nope", &[]), &ctx()).is_err());
    }

    #[test]
    fn phpunit_scopes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("phpunit.xml"), "<phpunit/>").unwrap();
        let ctx = BuildContext::new(dir.path());
        let r = KindRegistry::builtin();
        let cmd = r
            .build(&cfg("phpunit", &[("scope", "config")]), &ctx)
            .unwrap();
        assert_eq!(cmd.program, "php");
        assert_eq!(
            cmd.args,
            [
                "vendor/bin/phpunit",
                "--teamcity",
                "--configuration",
                "phpunit.xml"
            ]
        );

        let cmd = r
            .build(
                &cfg(
                    "phpunit",
                    &[("scope", "directory"), ("directory", "tests/Unit")],
                ),
                &ctx,
            )
            .unwrap();
        assert_eq!(cmd.args.last().unwrap(), "tests/Unit");

        let c = cfg(
            "phpunit",
            &[
                ("scope", "method"),
                ("class", "Tests\\Unit\\UserTest"),
                ("method", "test_full_name"),
                ("file", "tests/Unit/UserTest.php"),
                ("configFile", "phpunit.ci.xml"),
            ],
        );
        let cmd = r.build(&c, &ctx).unwrap();
        assert_eq!(
            cmd.args,
            [
                "vendor/bin/phpunit",
                "--teamcity",
                "--configuration",
                "phpunit.ci.xml",
                "--filter",
                "/UserTest::test_full_name( .*)?$/",
                "tests/Unit/UserTest.php"
            ]
        );
        let cmd = r
            .build(
                &cfg("phpunit", &[("scope", "class"), ("class", "App\\FooTest")]),
                &ctx,
            )
            .unwrap();
        assert_eq!(cmd.args[cmd.args.len() - 1], "/FooTest(::.*)?$/");
    }

    #[test]
    fn artisan_composer_npm_and_shell() {
        let r = KindRegistry::builtin();
        let cmd = r
            .build(&cfg("artisan", &[("command", "migrate --seed")]), &ctx())
            .unwrap();
        assert_eq!(
            cmd.display(),
            "/usr/bin/php8.3 artisan migrate --seed --ansi"
        );

        let mut c = cfg("composer", &[("script", "test")]);
        c.arguments = "--stop-on-failure".into();
        let cmd = r.build(&c, &ctx()).unwrap();
        assert_eq!(
            cmd.args,
            ["run-script", "test", "--ansi", "--", "--stop-on-failure"]
        );

        let cmd = r
            .build(
                &cfg(
                    "npm",
                    &[("script", "dev"), ("packageJson", "frontend/package.json")],
                ),
                &ctx(),
            )
            .unwrap();
        assert_eq!(cmd.args, ["run", "dev"]);
        assert_eq!(cmd.cwd, PathBuf::from("/proj/frontend"));

        let cmd = r
            .build(
                &cfg("shell", &[("mode", "text"), ("scriptText", "echo hi")]),
                &ctx(),
            )
            .unwrap();
        assert_eq!(cmd.args.last().unwrap(), "echo hi");
        let cmd = r
            .build(
                &cfg("shell", &[("scriptPath", "scripts/deploy.sh")]),
                &ctx(),
            )
            .unwrap();
        assert!(cmd.args.contains(&"scripts/deploy.sh".to_string()));
        assert_eq!(cmd.cwd, PathBuf::from("/proj/scripts"));
    }

    #[test]
    fn plugins_can_register_kinds() {
        struct Lua;
        impl RunConfigurationKind for Lua {
            fn id(&self) -> &str {
                "lua-script"
            }
            fn build(&self, cfg: &RunConfiguration, _: &BuildContext) -> Result<CommandLine> {
                Ok(CommandLine::new("lua").arg(cfg.option("file")))
            }
        }
        let mut r = KindRegistry::builtin();
        r.register(Box::new(Lua));
        assert!(r.ids().contains(&"lua-script"));
        let cmd = r
            .build(&cfg("lua-script", &[("file", "main.lua")]), &ctx())
            .unwrap();
        assert_eq!(cmd.display(), "lua main.lua");
    }

    #[test]
    fn template_kinds_expand_placeholders() {
        let mut r = KindRegistry::builtin();
        r.register(Box::new(TemplateKind {
            id: "lua-script".into(),
            command: vec![
                "${interpreter}".into(),
                "${interpreterOptions}".into(),
                "${file}".into(),
                "${arguments}".into(),
            ],
            default_interpreter: Some("lua".into()),
            required: vec![("file".into(), "File".into())],
        }));
        let mut c = cfg("lua-script", &[("file", "main.lua")]);
        c.arguments = "a 'b c'".into();
        let cmd = r.build(&c, &ctx()).unwrap();
        assert_eq!(cmd.program, "lua");
        assert_eq!(cmd.args, ["main.lua", "a", "b c"]);
        c.interpreter = "luajit".into();
        assert_eq!(r.build(&c, &ctx()).unwrap().program, "luajit");
        let err = r.build(&cfg("lua-script", &[]), &ctx()).unwrap_err();
        assert_eq!(err.to_string(), "File is not specified");
    }

    #[test]
    fn persists_shared_and_local_configurations() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            RunConfigurations::load(root).unwrap(),
            RunConfigurations::default()
        );
        let mut shared = cfg("phpunit", &[("scope", "config")]);
        shared.name = "All tests".into();
        shared.store_as_project_file = true;
        shared.env.insert("APP_ENV".into(), "testing".into());
        shared.before_launch.push(BeforeLaunchTask {
            kind: "shell".into(),
            value: "composer dump-autoload".into(),
        });
        let mut local = cfg("artisan", &[("command", "serve")]);
        local.name = "serve".into();
        let all = RunConfigurations {
            configurations: vec![shared.clone(), local.clone()],
            selected: Some("serve".into()),
            templates: vec![{
                let mut t = cfg("phpunit", &[("scope", "directory"), ("directory", "tests")]);
                t.name = "phpunit".into();
                t
            }],
        };
        all.save(root).unwrap();
        let text = std::fs::read_to_string(root.join(PROJECT_FILE)).unwrap();
        assert!(text.contains("[[configuration]]"), "{text}");
        assert!(text.contains("name = \"All tests\""), "{text}");
        assert!(!text.contains("serve"), "{text}");
        assert!(!text.contains("storeAsProjectFile"), "{text}");
        let loaded = RunConfigurations::load(root).unwrap();
        assert_eq!(loaded, all);
        // The UI sees where each configuration is stored.
        let json = serde_json::to_value(&loaded).unwrap();
        assert_eq!(json["configurations"][0]["storeAsProjectFile"], true);
        assert!(json["configurations"][1]
            .get("storeAsProjectFile")
            .is_none());

        // Removing everything removes the files again.
        RunConfigurations::default().save(root).unwrap();
        assert!(!root.join(PROJECT_FILE).exists());
        assert!(!root.join(LOCAL_FILE).exists());
    }

    #[test]
    fn rejects_duplicate_and_empty_names() {
        let dir = tempfile::tempdir().unwrap();
        let a = cfg("phpunit", &[]);
        let dup = RunConfigurations {
            configurations: vec![a.clone(), a.clone()],
            selected: None,
            templates: Vec::new(),
        };
        assert!(dup.save(dir.path()).is_err());
        let mut empty = a;
        empty.name = " ".into();
        let bad = RunConfigurations {
            configurations: vec![empty],
            selected: None,
            templates: Vec::new(),
        };
        assert!(bad.save(dir.path()).is_err());
    }
}

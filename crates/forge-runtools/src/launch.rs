//! What starting a run configuration does, before anything runs: a compound configuration
//! starts its members together, and every configuration first runs its "Before launch"
//! tasks one after another (the UI aborts on the first failure).
//!
//! [`plan`] flattens this into a [`LaunchPlan`]: the configurations to start and, for each,
//! the ordered tasks. "Run Another Configuration" tasks bring their own before-launch tasks
//! along; missing configurations and cycles are errors.

use serde::{Deserialize, Serialize};

use crate::{Configuration, Task};

/// Type id of compound configurations.
pub const COMPOUND: &str = "compound";
/// Option of a compound with its members, one configuration name per line.
pub const MEMBERS_OPTION: &str = "configurations";
/// Option of a compound: "Allow multiple instances" (`"true"`).
pub const MULTIPLE_OPTION: &str = "allowMultipleInstances";

/// Before-launch task kinds, in the order of PHPStorm's "Add" menu, with their menu labels.
pub const TASK_KINDS: &[(&str, &str)] = &[
    ("externalTool", "Run External tool"),
    ("runConfiguration", "Run Another Configuration"),
    ("browser", "Launch Web Browser"),
    ("npm", "Run npm script"),
    ("composer", "Run Composer script"),
    ("artisan", "Run Artisan command"),
    ("shell", "Run Shell Command"),
];

pub fn is_compound(cfg: &Configuration) -> bool {
    cfg.type_id == COMPOUND
}

/// The member names of a compound, without blanks and duplicates.
pub fn members(cfg: &Configuration) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in cfg.option(MEMBERS_OPTION).lines().map(str::trim) {
        if !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

pub fn allows_multiple_instances(cfg: &Configuration) -> bool {
    cfg.option(MULTIPLE_OPTION) == "true"
}

/// What the plan needs besides the configurations.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanContext {
    /// Package manager of "Run npm script" tasks (`npm`, `pnpm`, `yarn`, `bun`).
    pub package_manager: String,
}

/// One step before a configuration starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTask {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    /// Shown in the Before launch list and in failure balloons.
    pub label: String,
    /// `runConfiguration`: the configuration to run and wait for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<String>,
    /// npm, Composer, Artisan and shell tasks: the temporary configuration that runs them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<Configuration>,
}

/// A configuration to start and its before-launch tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedRun {
    pub configuration: String,
    pub tasks: Vec<PlannedTask>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    /// One entry, or the members of a compound (started together).
    pub runs: Vec<PlannedRun>,
    /// The target is a compound.
    pub compound: bool,
    pub allow_multiple_instances: bool,
}

/// The label of a task, e.g. `Run 'UserTest'` or `Run npm script 'build'`.
pub fn task_label(task: &Task) -> String {
    let v = task.value.trim();
    match task.kind.as_str() {
        "runConfiguration" => format!("Run '{v}'"),
        "externalTool" => format!("External tool '{v}'"),
        "browser" => format!("Launch Web Browser: {v}"),
        "npm" => format!("Run npm script '{v}'"),
        "composer" => format!("Run Composer script '{v}'"),
        "artisan" => format!("Run Artisan command '{v}'"),
        "shell" => format!("Run '{v}'"),
        other => format!("{other}: {v}"),
    }
}

/// The temporary configuration that runs an npm, Composer, Artisan or shell task in the
/// working directory of the configuration it belongs to.
pub fn task_run(task: &Task, working_directory: &str, ctx: &PlanContext) -> Option<Configuration> {
    let v = task.value.trim();
    let cfg = match task.kind.as_str() {
        "npm" => {
            let manager = match ctx.package_manager.trim() {
                "" | "auto" => "npm",
                m => m,
            };
            Configuration::new(format!("{manager} run {v}"), "npm")
                .with_option("command", "run")
                .with_option("script", v)
                .with_option("packageManager", manager)
        }
        "composer" => Configuration::new(format!("composer {v}"), "composer-command")
            .with_option("command", v),
        "artisan" => {
            Configuration::new(format!("artisan {v}"), "artisan").with_option("command", v)
        }
        "shell" => Configuration::new(v, "shell")
            .with_option("mode", "text")
            .with_option("scriptText", v),
        _ => return None,
    };
    Some(Configuration {
        working_directory: working_directory.to_string(),
        ..cfg
    })
}

struct Planner<'a> {
    configs: &'a [Configuration],
    ctx: &'a PlanContext,
}

impl Planner<'_> {
    fn find(&self, name: &str) -> Result<&Configuration, String> {
        self.configs
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("Configuration '{name}' not found"))
    }

    fn enter(stack: &[String], name: &str) -> Result<Vec<String>, String> {
        if let Some(i) = stack.iter().position(|n| n == name) {
            let mut cycle: Vec<&str> = stack[i..].iter().map(String::as_str).collect();
            cycle.push(name);
            return Err(format!(
                "Configuration '{name}' depends on itself: {}",
                cycle.join(" → ")
            ));
        }
        let mut next = stack.to_vec();
        next.push(name.to_string());
        Ok(next)
    }

    /// The tasks of `cfg`, with "Run Another Configuration" expanded.
    fn tasks(&self, cfg: &Configuration, stack: &[String]) -> Result<Vec<PlannedTask>, String> {
        let mut out = Vec::new();
        for task in &cfg.before_launch {
            if task.value.trim().is_empty() {
                continue;
            }
            if task.kind == "runConfiguration" {
                self.run_another(task.value.trim(), stack, &mut out)?;
                continue;
            }
            out.push(PlannedTask {
                kind: task.kind.clone(),
                value: task.value.trim().to_string(),
                label: task_label(task),
                configuration: None,
                run: task_run(task, &cfg.working_directory, self.ctx),
            });
        }
        Ok(out)
    }

    /// Another configuration as a task: its own tasks, then it (a compound: each member).
    fn run_another(
        &self,
        name: &str,
        stack: &[String],
        out: &mut Vec<PlannedTask>,
    ) -> Result<(), String> {
        let target = self.find(name)?;
        let stack = Self::enter(stack, name)?;
        if is_compound(target) {
            for member in members(target) {
                self.run_another(&member, &stack, out)?;
            }
            return Ok(());
        }
        out.extend(self.tasks(target, &stack)?);
        out.push(PlannedTask {
            kind: "runConfiguration".into(),
            value: name.to_string(),
            label: task_label(&Task::new("runConfiguration", name)),
            configuration: Some(name.to_string()),
            run: None,
        });
        Ok(())
    }

    /// The configurations a target starts: itself, or a compound's members (recursively).
    fn runs(&self, name: &str, stack: &[String], out: &mut Vec<PlannedRun>) -> Result<(), String> {
        let cfg = self.find(name)?;
        let stack = Self::enter(stack, name)?;
        if is_compound(cfg) {
            for member in members(cfg) {
                self.runs(&member, &stack, out)?;
            }
            return Ok(());
        }
        if out.iter().any(|r| r.configuration == name) {
            return Ok(());
        }
        let tasks = self.tasks(cfg, &stack)?;
        out.push(PlannedRun {
            configuration: name.to_string(),
            tasks,
        });
        Ok(())
    }
}

/// Plan starting `target`, one of `configs` (saved and temporary ones).
pub fn plan(
    configs: &[Configuration],
    target: &str,
    ctx: &PlanContext,
) -> Result<LaunchPlan, String> {
    let planner = Planner { configs, ctx };
    let cfg = planner.find(target)?;
    let mut runs = Vec::new();
    planner.runs(target, &[], &mut runs)?;
    if runs.is_empty() {
        return Err(format!(
            "Compound configuration '{target}' has no configurations to run"
        ));
    }
    Ok(LaunchPlan {
        runs,
        compound: is_compound(cfg),
        allow_multiple_instances: allows_multiple_instances(cfg),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str, type_id: &str, tasks: &[(&str, &str)]) -> Configuration {
        Configuration {
            before_launch: tasks.iter().map(|(k, v)| Task::new(*k, *v)).collect(),
            ..Configuration::new(name, type_id)
        }
    }

    fn compound(name: &str, members: &[&str]) -> Configuration {
        Configuration::new(name, COMPOUND).with_option(MEMBERS_OPTION, members.join("\n"))
    }

    fn labels(run: &PlannedRun) -> Vec<&str> {
        run.tasks.iter().map(|t| t.label.as_str()).collect()
    }

    #[test]
    fn a_plain_configuration_runs_its_tasks_in_order() {
        let configs = [
            cfg(
                "serve",
                "artisan",
                &[
                    ("npm", "build"),
                    ("runConfiguration", "migrate"),
                    ("browser", "http://localhost:8000"),
                    ("shell", "  "),
                ],
            ),
            cfg("migrate", "artisan", &[("composer", "dump-autoload")]),
        ];
        let ctx = PlanContext {
            package_manager: "pnpm".into(),
        };
        let p = plan(&configs, "serve", &ctx).unwrap();
        assert!(!p.compound);
        assert_eq!(p.runs.len(), 1);
        assert_eq!(
            labels(&p.runs[0]),
            [
                "Run npm script 'build'",
                "Run Composer script 'dump-autoload'",
                "Run 'migrate'",
                "Launch Web Browser: http://localhost:8000",
            ]
        );
        let npm = p.runs[0].tasks[0].run.as_ref().unwrap();
        assert_eq!(npm.type_id, "npm");
        assert_eq!(npm.option("packageManager"), "pnpm");
        assert_eq!(npm.option("script"), "build");
        assert_eq!(p.runs[0].tasks[2].configuration.as_deref(), Some("migrate"));
        assert_eq!(p.runs[0].tasks[3].run, None);
    }

    #[test]
    fn compounds_start_their_members_each_with_its_tasks() {
        let configs = [
            compound("Full stack", &["api", "frontend", "", "api", "Workers"]),
            cfg("api", "artisan", &[("artisan", "migrate")]),
            cfg("frontend", "npm", &[]),
            compound("Workers", &["queue", "frontend"]),
            cfg("queue", "artisan", &[]),
        ];
        let mut c = configs.to_vec();
        c[0].options.insert(MULTIPLE_OPTION.into(), "true".into());
        let p = plan(&c, "Full stack", &PlanContext::default()).unwrap();
        assert!(p.compound && p.allow_multiple_instances);
        let names: Vec<&str> = p.runs.iter().map(|r| r.configuration.as_str()).collect();
        assert_eq!(names, ["api", "frontend", "queue"]);
        assert_eq!(labels(&p.runs[0]), ["Run Artisan command 'migrate'"]);
        assert_eq!(members(&c[0]), ["api", "frontend", "Workers"]);
    }

    #[test]
    fn missing_configurations_and_cycles_are_errors() {
        let configs = [
            cfg("a", "shell", &[("runConfiguration", "b")]),
            cfg("b", "shell", &[("runConfiguration", "a")]),
            cfg("c", "shell", &[("runConfiguration", "gone")]),
            compound("loop", &["loop"]),
            compound("empty", &[]),
            compound("d", &["c"]),
        ];
        let ctx = PlanContext::default();
        assert_eq!(
            plan(&configs, "a", &ctx).unwrap_err(),
            "Configuration 'a' depends on itself: a → b → a"
        );
        assert_eq!(
            plan(&configs, "c", &ctx).unwrap_err(),
            "Configuration 'gone' not found"
        );
        assert_eq!(
            plan(&configs, "d", &ctx).unwrap_err(),
            "Configuration 'gone' not found"
        );
        assert!(plan(&configs, "loop", &ctx)
            .unwrap_err()
            .contains("depends on itself"));
        assert!(plan(&configs, "empty", &ctx)
            .unwrap_err()
            .contains("no configurations"));
        assert_eq!(
            plan(&configs, "x", &ctx).unwrap_err(),
            "Configuration 'x' not found"
        );
    }

    #[test]
    fn running_a_compound_as_a_task_runs_each_member() {
        let configs = [
            cfg("deploy", "shell", &[("runConfiguration", "checks")]),
            compound("checks", &["lint", "test"]),
            cfg("lint", "shell", &[("externalTool", "Linters/PHPStan")]),
            cfg("test", "phpunit", &[]),
        ];
        let p = plan(&configs, "deploy", &PlanContext::default()).unwrap();
        assert_eq!(
            labels(&p.runs[0]),
            [
                "External tool 'Linters/PHPStan'",
                "Run 'lint'",
                "Run 'test'"
            ]
        );
    }

    #[test]
    fn task_runs_use_the_configuration_directory() {
        let t = Task::new("shell", "make assets");
        let run = task_run(&t, "frontend", &PlanContext::default()).unwrap();
        assert_eq!(run.working_directory, "frontend");
        assert_eq!(run.option("scriptText"), "make assets");
        let npm = task_run(&Task::new("npm", "dev"), "", &PlanContext::default()).unwrap();
        assert_eq!(npm.option("packageManager"), "npm");
        assert_eq!(
            task_run(&Task::new("browser", "x"), "", &PlanContext::default()),
            None
        );
        assert_eq!(task_label(&Task::new("custom", "v")), "custom: v");
    }
}

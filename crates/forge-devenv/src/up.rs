//! Create or start a dev container with the local Docker CLI ("Create Dev Container and
//! Mount Sources…"): pull or build the image (or `docker compose up`), create the container
//! with the sources mounted, run the lifecycle commands, and report every step and line of
//! output to the build log. Dropping the future stops the running process.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::devcontainer::{ContainerSource, LifecycleCommand, ResolvedConfig};
use crate::docker;
use crate::runner::{Runner, Stream};

/// A line of the build log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LogLine {
    /// A new step ("Building image…").
    Step {
        text: String,
    },
    /// The command line that runs next.
    Command {
        text: String,
    },
    /// Output of the running command.
    Output {
        stream: Stream,
        text: String,
    },
    Warning {
        text: String,
    },
}

#[derive(Debug, Clone)]
pub struct UpOptions {
    /// The Docker CLI (`docker`, or a configured path).
    pub docker: String,
    /// Remove an existing container (and build without cache) first.
    pub rebuild: bool,
    /// Where Forge keeps its compose override files (the user cache dir, never the project).
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpOutcome {
    pub container_id: String,
    /// A new container was created (not just started).
    pub created: bool,
}

/// The compose override file of a configuration.
pub fn override_path(cache_dir: &Path, id: &str) -> PathBuf {
    cache_dir.join(id).join("docker-compose.devcontainer.yml")
}

struct Ctx<'a> {
    runner: &'a dyn Runner,
    docker: &'a str,
    log: &'a (dyn Fn(LogLine) + Send + Sync),
}

impl Ctx<'_> {
    fn step(&self, text: impl Into<String>) {
        (self.log)(LogLine::Step { text: text.into() });
    }

    /// Run with streamed output; `Err` with `what` and the exit code when it fails.
    async fn stream(&self, args: &[String], what: &str) -> Result<(), String> {
        (self.log)(LogLine::Command {
            text: docker::display_command(self.docker, args),
        });
        let sink = |stream: Stream, text: &str| {
            (self.log)(LogLine::Output {
                stream,
                text: text.to_string(),
            })
        };
        let code = self
            .runner
            .stream(self.docker, args, &sink)
            .await
            .map_err(|e| e.to_string())?;
        match code {
            Some(0) => Ok(()),
            Some(c) => Err(format!("{what} failed (exit code {c})")),
            None => Err(format!("{what} was stopped")),
        }
    }

    async fn output(&self, args: &[String], what: &str) -> Result<String, String> {
        (self.log)(LogLine::Command {
            text: docker::display_command(self.docker, args),
        });
        let out = self
            .runner
            .output(self.docker, args, None)
            .await
            .map_err(|e| e.to_string())?;
        if out.success() {
            Ok(out.stdout_text())
        } else {
            let message = out.message();
            (self.log)(LogLine::Output {
                stream: Stream::Stderr,
                text: format!("{message}\n"),
            });
            Err(format!("{what} failed: {message}"))
        }
    }

    async fn lifecycle(
        &self,
        cfg: &ResolvedConfig,
        container: &str,
        name: &str,
        cmd: Option<&LifecycleCommand>,
    ) -> Result<(), String> {
        let Some(cmd) = cmd else { return Ok(()) };
        for (label, argv) in cmd.argvs() {
            self.step(match &label {
                Some(l) => format!("Running {name} ({l})…"),
                None => format!("Running {name}…"),
            });
            let args = docker::exec_args(
                container,
                cfg.remote_user.as_deref(),
                Some(&cfg.workspace_folder),
                &cfg.remote_env,
                false,
                &argv,
            );
            self.stream(&args, name).await?;
        }
        Ok(())
    }
}

/// Create (or start) the container of `cfg`. The caller checked that the project is
/// trusted: the image build and the lifecycle commands run the project's code.
pub async fn up(
    runner: &dyn Runner,
    cfg: &ResolvedConfig,
    opts: &UpOptions,
    log: &(dyn Fn(LogLine) + Send + Sync),
) -> Result<UpOutcome, String> {
    let cx = Ctx {
        runner,
        docker: &opts.docker,
        log,
    };
    for w in &cfg.warnings {
        log(LogLine::Warning { text: w.clone() });
    }
    let override_file = override_path(&opts.cache_dir, &cfg.id);
    let override_text = override_file.to_string_lossy().into_owned();

    let existing = docker::parse_find(&cx.output(&docker::find_args(&cfg.id), "docker ps").await?);
    let existing = match existing {
        Some((id, _)) if opts.rebuild => {
            cx.step("Removing the old container…");
            if matches!(cfg.source, ContainerSource::Compose { .. }) && override_file.is_file() {
                let args = docker::compose_down_args(cfg, &override_text).unwrap_or_default();
                cx.stream(&args, "docker compose down").await?;
            } else {
                cx.output(&docker::remove_args(&id), "docker rm").await?;
            }
            None
        }
        other => other,
    };

    if let Some((id, running)) = existing {
        if !running {
            cx.step(format!(
                "Starting container {}…",
                docker::container_name(cfg)
            ));
            cx.output(&docker::start_args(&id), "docker start").await?;
        }
        if !running {
            cx.lifecycle(
                cfg,
                &id,
                "postStartCommand",
                cfg.post_start_command.as_ref(),
            )
            .await?;
        }
        return Ok(UpOutcome {
            container_id: id,
            created: false,
        });
    }

    let container = match &cfg.source {
        ContainerSource::Compose { .. } => {
            cx.step("Starting Docker Compose services…");
            let text = docker::compose_override(cfg).unwrap_or_default();
            if let Some(dir) = override_file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&override_file, text).map_err(|e| e.to_string())?;
            let args =
                docker::compose_up_args(cfg, &override_text, opts.rebuild).unwrap_or_default();
            cx.stream(&args, "docker compose up").await?;
            docker::parse_find(&cx.output(&docker::find_args(&cfg.id), "docker ps").await?)
                .map(|(id, _)| id)
                .ok_or_else(|| {
                    "docker compose up did not create the service's container".to_string()
                })?
        }
        source => {
            if let ContainerSource::Image { image } = source {
                cx.step(format!("Pulling image {image}…"));
                let args = docker::pull_args(cfg).unwrap_or_default();
                if let Err(e) = cx.stream(&args, "docker pull").await {
                    // Offline with the image already here: `docker run` still works.
                    log(LogLine::Warning {
                        text: format!("{e}; trying the local image"),
                    });
                }
            } else {
                cx.step("Building image…");
                let args = docker::build_args(cfg, opts.rebuild).unwrap_or_default();
                cx.stream(&args, "docker build").await?;
            }
            cx.step(format!(
                "Creating container {}…",
                docker::container_name(cfg)
            ));
            let args = docker::run_args(cfg).unwrap_or_default();
            let out = cx.output(&args, "docker run").await?;
            out.lines()
                .rev()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(12).collect::<String>())
                .ok_or_else(|| "docker run printed no container id".to_string())?
        }
    };

    cx.lifecycle(
        cfg,
        &container,
        "onCreateCommand",
        cfg.on_create_command.as_ref(),
    )
    .await?;
    cx.lifecycle(
        cfg,
        &container,
        "postCreateCommand",
        cfg.post_create_command.as_ref(),
    )
    .await?;
    cx.lifecycle(
        cfg,
        &container,
        "postStartCommand",
        cfg.post_start_command.as_ref(),
    )
    .await?;
    cx.step("Dev container is ready.");
    Ok(UpOutcome {
        container_id: container,
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::FakeRunner;
    use crate::devcontainer::{load, LocalContext};
    use crate::runner::Output;
    use std::sync::Mutex;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn cfg(text: &str) -> ResolvedConfig {
        load(
            text,
            &LocalContext {
                folder: "/src/shop",
                config_path: "/src/shop/.devcontainer/devcontainer.json",
                env: &no_env,
            },
        )
        .unwrap()
    }

    fn opts(dir: &Path, rebuild: bool) -> UpOptions {
        UpOptions {
            docker: "docker".into(),
            rebuild,
            cache_dir: dir.to_path_buf(),
        }
    }

    type Lines = Mutex<Vec<LogLine>>;

    fn show(lines: &Lines) -> Vec<String> {
        lines
            .lock()
            .unwrap()
            .iter()
            .map(|l| match l {
                LogLine::Step { text } => format!("step: {text}"),
                LogLine::Command { text } => format!("$ {text}"),
                LogLine::Output { text, .. } => format!("> {}", text.trim()),
                LogLine::Warning { text } => format!("warning: {text}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn builds_creates_and_runs_lifecycle_commands() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new("");
        runner.answer(
            "run",
            Output::ok("Unable to find image locally\n0123456789abcdef0123\n"),
        );
        let c = cfg(
            r#"{ "build": { "dockerfile": "Dockerfile" }, "remoteUser": "vscode",
            "postCreateCommand": "composer install", "postStartCommand": ["php", "-v"],
            "features": { "ghcr.io/devcontainers/features/node:1": {} } }"#,
        );
        let lines = Lines::default();
        let out = up(runner.as_ref(), &c, &opts(dir.path(), false), &|l| {
            lines.lock().unwrap().push(l)
        })
        .await
        .unwrap();
        assert_eq!(
            out,
            UpOutcome {
                container_id: "0123456789ab".into(),
                created: true
            }
        );
        let log = show(&lines);
        assert!(log[0].starts_with("warning: Features"), "{log:?}");
        assert!(log.iter().any(|l| l == "step: Building image…"));
        assert!(log
            .iter()
            .any(|l| l.starts_with("$ docker build -f /src/shop/.devcontainer/Dockerfile")));
        assert!(log.iter().any(|l| l == "step: Running postCreateCommand…"));
        assert!(
            log.iter().any(|l| l.contains(
                "exec -u vscode -w /workspaces/shop 0123456789ab /bin/sh -c 'composer install'"
            )),
            "{log:?}"
        );
        assert!(
            log.iter().any(|l| l.ends_with("0123456789ab php -v")),
            "{log:?}"
        );
        assert_eq!(log.last().unwrap(), "step: Dev container is ready.");
    }

    #[tokio::test]
    async fn starts_an_existing_container_and_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(
            r#"{ "image": "php:8.3", "postCreateCommand": "make", "postStartCommand": "serve" }"#,
        );
        let runner = FakeRunner::new("");
        runner.answer("label=forge.devcontainer.id", Output::ok("abc\texited\n"));
        let lines = Lines::default();
        let out = up(runner.as_ref(), &c, &opts(dir.path(), false), &|l| {
            lines.lock().unwrap().push(l)
        })
        .await
        .unwrap();
        assert_eq!(
            out,
            UpOutcome {
                container_id: "abc".into(),
                created: false
            }
        );
        let log = show(&lines);
        assert!(log.iter().any(|l| l == "$ docker start abc"));
        assert!(log.iter().any(|l| l == "step: Running postStartCommand…"));
        assert!(!log.iter().any(|l| l.contains("postCreateCommand")));

        // Running already: nothing to do.
        let running = FakeRunner::new("");
        running.answer("label=forge.devcontainer.id", Output::ok("abc\trunning\n"));
        let out = up(running.as_ref(), &c, &opts(dir.path(), false), &|_| {})
            .await
            .unwrap();
        assert!(!out.created);
        assert_eq!(running.commands().len(), 1);

        // Rebuild: remove, pull, create.
        let rebuild = FakeRunner::new("");
        rebuild.answer("label=forge.devcontainer.id", Output::ok("abc\trunning\n"));
        rebuild.answer("--entrypoint", Output::ok("fff111\n"));
        let out = up(rebuild.as_ref(), &c, &opts(dir.path(), true), &|_| {})
            .await
            .unwrap();
        assert_eq!(out.container_id, "fff111");
        let cmds = rebuild.commands();
        assert_eq!(cmds[1], "docker rm -f abc");
        assert_eq!(cmds[2], "docker pull php:8.3");
    }

    #[tokio::test]
    async fn compose_writes_the_override_into_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(r#"{ "dockerComposeFile": "compose.yaml", "service": "app" }"#);
        let runner = FakeRunner::new("");
        // `docker ps` finds nothing first, the service's container after `up`.
        runner.answer("compose", Output::ok(""));
        let lines = Lines::default();
        let err = up(runner.as_ref(), &c, &opts(dir.path(), false), &|l| {
            lines.lock().unwrap().push(l)
        })
        .await
        .unwrap_err();
        assert!(err.contains("did not create"), "{err}");
        let file = override_path(dir.path(), &c.id);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("forge.devcontainer.id"));
        let up_args = runner.streamed.lock().unwrap()[0].clone();
        assert!(up_args.contains(&file.to_string_lossy().into_owned()));
    }

    #[tokio::test]
    async fn failing_steps_stop_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(r#"{ "build": { "dockerfile": "Dockerfile" } }"#);
        let runner = FakeRunner::new("");
        runner.answer("build", Output::failed(1, "failed to solve: COPY failed\n"));
        let lines = Lines::default();
        let err = up(runner.as_ref(), &c, &opts(dir.path(), false), &|l| {
            lines.lock().unwrap().push(l)
        })
        .await
        .unwrap_err();
        assert_eq!(err, "docker build failed (exit code 1)");
        assert!(show(&lines).iter().any(|l| l.contains("COPY failed")));
    }
}

//! The container sandbox: the commands and tests of a task run in a Docker or Podman
//! container instead of on the machine itself. The agent's file tools keep working on the
//! sandbox worktree; only processes move into the container.
//!
//! The container mounts the sandbox worktree read-write at [`WORKSPACE`] and nothing else,
//! has no network unless the task allows it (`--network none`), drops all capabilities,
//! cannot gain privileges, runs as the user who owns the sandbox (so its files stay
//! removable) and has the CPU and memory limits of the settings. It lives as long as its
//! task runs and is removed when the task finishes or is discarded.
//!
//! Everything here builds command lines ([`ShellCommand`]s for the engine's CLI; in Flatpak
//! `commands::run_process` starts them on the host through `flatpak-spawn --host`); running
//! them is the caller's business (on this machine or on a remote project's host).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::commands::ShellCommand;

/// Where the sandbox appears inside the container.
pub const WORKSPACE: &str = "/workspace";

/// Label carrying the task id, to find containers left behind.
pub const LABEL_TASK: &str = "forge.agent-task";

/// Keeps the container alive until it is removed (the Dev Containers CLI's loop, which ends
/// on SIGTERM).
pub const KEEP_ALIVE: &str =
    "echo Container started; trap \"exit 0\" 15; while sleep 1 & wait $!; do :; done";

/// Most processes inside the container (fork bombs end there).
pub const PIDS_LIMIT: u32 = 1024;

/// The container part of a task request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContainerOptions {
    /// Allow network access (every command then needs the user's approval).
    pub network: bool,
    /// Image for this task; `None`: the project's `devcontainer.json`, else the
    /// `ai.agentTasks.container.image` setting.
    pub image: Option<String>,
}

/// A task's container, ready to be turned into command lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    /// The engine's CLI: `docker`, `podman` or a path to one of them.
    pub engine: String,
    /// Container name ([`container_name`]).
    pub name: String,
    pub task_id: String,
    pub image: String,
    /// The sandbox worktree on the machine that runs the engine (the only mount).
    pub sandbox: String,
    pub network: bool,
    /// `--cpus`, e.g. `2` or `1.5`.
    pub cpus: Option<String>,
    /// `--memory`, e.g. `4g` or `512m`.
    pub memory: Option<String>,
    /// `uid:gid` of the sandbox's owner (Docker); Podman keeps the user's id instead.
    pub user: Option<String>,
    /// Environment of the container (`containerEnv` of `devcontainer.json`).
    pub env: BTreeMap<String, String>,
}

/// The container name of task `id`: `forge-agent-<id>` with characters Docker allows.
pub fn container_name(task_id: &str) -> String {
    let id: String = task_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("forge-agent-{id}")
}

/// A `--cpus` value: empty means no limit; else a positive number.
pub fn parse_cpus(value: &str) -> Result<Option<String>, String> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    match v.parse::<f64>() {
        Ok(n) if n > 0.0 && n.is_finite() => Ok(Some(v.to_string())),
        _ => Err(format!(
            "invalid CPU limit {v:?}: use a number like 2 or 1.5"
        )),
    }
}

/// A `--memory` value: empty means no limit; else a number with an optional unit
/// (`b`, `k`, `m`, `g`), at least 6 MB as Docker requires.
pub fn parse_memory(value: &str) -> Result<Option<String>, String> {
    let v = value.trim().to_ascii_lowercase();
    if v.is_empty() {
        return Ok(None);
    }
    let (num, unit) = match v.strip_suffix(['b', 'k', 'm', 'g']) {
        Some(n) => (n, &v[n.len()..]),
        None => (v.as_str(), "b"),
    };
    let bytes = num.parse::<u64>().ok().and_then(|n| {
        n.checked_mul(match unit {
            "k" => 1 << 10,
            "m" => 1 << 20,
            "g" => 1 << 30,
            _ => 1,
        })
    });
    match bytes {
        Some(b) if b >= 6 << 20 => Ok(Some(v)),
        Some(_) => Err(format!("memory limit {v:?} is below the minimum of 6m")),
        None => Err(format!(
            "invalid memory limit {v:?}: use a size like 4g or 512m"
        )),
    }
}

/// A host path with `/` separators and without a trailing one.
fn slashed(path: &str) -> String {
    let p = path.replace('\\', "/");
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        "/".into()
    } else {
        t.to_string()
    }
}

/// Map paths below `from` in `text` to the same paths below `to` (only whole path
/// prefixes: `/p/app` matches `/p/app/x` but not `/p/apps`).
pub fn map_paths(text: &str, from: &str, to: &str) -> String {
    let from = from.trim_end_matches(['/', '\\']);
    if from.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(from) {
        let after = &rest[at + from.len()..];
        let whole = after
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')));
        out.push_str(&rest[..at]);
        out.push_str(if whole { to } else { from });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Whether `engine` is Podman (rootless Podman maps the user with `--userns keep-id`).
pub fn is_podman(engine: &str) -> bool {
    let base = engine.rsplit(['/', '\\']).next().unwrap_or(engine);
    let base = base.to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base) == "podman"
}

fn s(v: &str) -> String {
    v.to_string()
}

impl ContainerSpec {
    fn command(&self, args: Vec<String>) -> ShellCommand {
        ShellCommand {
            program: self.engine.clone(),
            args,
            cwd: PathBuf::from(&self.sandbox),
            env: BTreeMap::new(),
        }
    }

    /// The path inside the container of a path in the sandbox; `None` outside it.
    pub fn container_path(&self, host: &str) -> Option<String> {
        let root = slashed(&self.sandbox);
        let p = slashed(host);
        if p == root {
            return Some(WORKSPACE.into());
        }
        let rest = p.strip_prefix(&root)?.strip_prefix('/')?;
        if rest.split('/').any(|seg| seg == "..") {
            return None;
        }
        Some(format!("{WORKSPACE}/{rest}"))
    }

    /// Paths of the sandbox in an argument, mapped into the container.
    fn map(&self, text: &str) -> String {
        let mut out = map_paths(text, &self.sandbox, WORKSPACE);
        let slashed = slashed(&self.sandbox);
        if slashed != self.sandbox {
            out = map_paths(&out, &slashed, WORKSPACE);
        }
        out
    }

    /// `docker run`: start the container detached, with the sandbox as its only mount.
    pub fn start(&self) -> Result<ShellCommand, String> {
        let image = self.image.trim();
        if image.is_empty() || image.starts_with('-') || image.contains(char::is_whitespace) {
            return Err(format!("invalid container image {image:?}"));
        }
        if self.sandbox.contains(',') {
            return Err(
                "The sandbox path contains a comma, which cannot be mounted into a container."
                    .into(),
            );
        }
        let mut a = vec![
            s("run"),
            s("--detach"),
            s("--rm"),
            s("--name"),
            self.name.clone(),
            s("--label"),
            format!("{LABEL_TASK}={}", self.task_id),
        ];
        if !self.network {
            a.extend([s("--network"), s("none")]);
        }
        a.extend([
            s("--cap-drop"),
            s("ALL"),
            s("--security-opt"),
            s("no-new-privileges"),
            s("--pids-limit"),
            PIDS_LIMIT.to_string(),
        ]);
        if let Some(c) = &self.cpus {
            a.extend([s("--cpus"), c.clone()]);
        }
        if let Some(m) = &self.memory {
            a.extend([s("--memory"), m.clone()]);
        }
        if is_podman(&self.engine) {
            a.extend([s("--userns"), s("keep-id")]);
        } else if let Some(u) = &self.user {
            a.extend([s("--user"), u.clone()]);
        }
        a.extend([
            s("--mount"),
            format!("type=bind,source={},target={WORKSPACE}", self.sandbox),
            s("--workdir"),
            s(WORKSPACE),
            // The user may have no home folder in the image.
            s("--env"),
            s("HOME=/tmp"),
        ]);
        for (k, v) in &self.env {
            a.extend([s("--env"), format!("{k}={}", self.map(v))]);
        }
        a.extend([
            s("--entrypoint"),
            s("/bin/sh"),
            image.to_string(),
            s("-c"),
            s(KEEP_ALIVE),
        ]);
        Ok(self.command(a))
    }

    /// `docker exec`: run `cmd` (a command for the sandbox on the host) in the container,
    /// with its working folder and the sandbox paths in its arguments mapped inside.
    pub fn exec(&self, cmd: &ShellCommand) -> Result<ShellCommand, String> {
        let cwd = self
            .container_path(&cmd.cwd.to_string_lossy())
            .ok_or_else(|| {
                format!(
                    "{} is outside the sandbox and not in the container",
                    cmd.cwd.display()
                )
            })?;
        let mut a = vec![s("exec"), s("--workdir"), cwd];
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        env.insert(s("CI"), s("1"));
        env.insert(s("GIT_TERMINAL_PROMPT"), s("0"));
        for (k, v) in &cmd.env {
            env.insert(k.clone(), self.map(v));
        }
        for (k, v) in env {
            a.extend([s("--env"), format!("{k}={v}")]);
        }
        a.push(self.name.clone());
        a.push(self.map(&cmd.program));
        a.extend(cmd.args.iter().map(|x| self.map(x)));
        Ok(self.command(a))
    }

    /// `docker rm --force`: stop and remove the container.
    pub fn remove(&self) -> ShellCommand {
        remove_command(&self.engine, &self.name, &self.sandbox)
    }
}

/// `docker rm --force <name>`, run in `cwd`.
pub fn remove_command(engine: &str, name: &str, cwd: &str) -> ShellCommand {
    ShellCommand {
        program: engine.to_string(),
        args: vec![s("rm"), s("--force"), name.to_string()],
        cwd: PathBuf::from(cwd),
        env: BTreeMap::new(),
    }
}

/// Whether the output of `docker rm` says there was nothing to remove.
pub fn already_gone(output: &str) -> bool {
    let o = output.to_ascii_lowercase();
    o.contains("no such container") || o.contains("no container with name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ContainerSpec {
        ContainerSpec {
            engine: "docker".into(),
            name: container_name("task-1a"),
            task_id: "task-1a".into(),
            image: "php:8.3-cli".into(),
            sandbox: "/home/u/.cache/forge/agent-tasks/shop-1/task-1a".into(),
            network: false,
            cpus: Some("2".into()),
            memory: Some("4g".into()),
            user: Some("1000:1000".into()),
            env: BTreeMap::from([("APP_ENV".into(), "testing".into())]),
        }
    }

    #[test]
    fn starts_with_only_the_sandbox_mounted_and_no_network() {
        let c = spec().start().unwrap();
        assert_eq!(c.program, "docker");
        assert_eq!(
            c.cwd,
            PathBuf::from("/home/u/.cache/forge/agent-tasks/shop-1/task-1a")
        );
        assert_eq!(
            c.display(),
            "docker run --detach --rm --name forge-agent-task-1a --label forge.agent-task=task-1a \
--network none --cap-drop ALL --security-opt no-new-privileges --pids-limit 1024 --cpus 2 \
--memory 4g --user 1000:1000 \
--mount type=bind,source=/home/u/.cache/forge/agent-tasks/shop-1/task-1a,target=/workspace \
--workdir /workspace --env HOME=/tmp --env APP_ENV=testing --entrypoint /bin/sh php:8.3-cli \
-c \"echo Container started; trap \\\"exit 0\\\" 15; while sleep 1 & wait $!; do :; done\""
        );
        // Exactly one mount, no volumes, no privileges.
        let args = c.args.join(" ");
        assert_eq!(args.matches("--mount").count(), 1);
        assert!(!args.contains("--volume") && !args.contains(" -v "));
        assert!(!args.contains("--privileged"));

        let open = ContainerSpec {
            network: true,
            cpus: None,
            memory: None,
            engine: "/usr/bin/podman".into(),
            ..spec()
        }
        .start()
        .unwrap();
        let args = open.args.join(" ");
        assert!(!args.contains("--network"));
        assert!(!args.contains("--cpus") && !args.contains("--memory"));
        assert!(args.contains("--userns keep-id") && !args.contains("--user "));
    }

    #[test]
    fn refuses_bad_images_and_unmountable_paths() {
        for image in ["", "  ", "--privileged", "php 8"] {
            let s = ContainerSpec {
                image: image.into(),
                ..spec()
            };
            assert!(s.start().is_err(), "{image:?}");
        }
        let s = ContainerSpec {
            sandbox: "/tmp/a,b".into(),
            ..spec()
        };
        assert!(s.start().unwrap_err().contains("comma"));
    }

    #[test]
    fn exec_maps_the_working_folder_and_paths_inside() {
        let s = spec();
        let cmd = ShellCommand {
            program: "/home/u/.cache/forge/agent-tasks/shop-1/task-1a/vendor/bin/phpunit".into(),
            args: vec![
                "--configuration=/home/u/.cache/forge/agent-tasks/shop-1/task-1a/phpunit.xml"
                    .into(),
                "--teamcity".into(),
                "/home/u/.cache/forge/agent-tasks/shop-1/task-1ab".into(),
            ],
            cwd: "/home/u/.cache/forge/agent-tasks/shop-1/task-1a/app".into(),
            env: BTreeMap::from([("XDEBUG_MODE".into(), "off".into())]),
        };
        let e = s.exec(&cmd).unwrap();
        assert_eq!(e.program, "docker");
        assert_eq!(
            e.args,
            [
                "exec",
                "--workdir",
                "/workspace/app",
                "--env",
                "CI=1",
                "--env",
                "GIT_TERMINAL_PROMPT=0",
                "--env",
                "XDEBUG_MODE=off",
                "forge-agent-task-1a",
                "/workspace/vendor/bin/phpunit",
                "--configuration=/workspace/phpunit.xml",
                "--teamcity",
                // Not below the sandbox: a different folder with a longer name.
                "/home/u/.cache/forge/agent-tasks/shop-1/task-1ab",
            ]
        );
        let outside = ShellCommand {
            cwd: "/home/u/project".into(),
            ..cmd.clone()
        };
        assert!(s
            .exec(&outside)
            .unwrap_err()
            .contains("outside the sandbox"));
        let escape = ShellCommand {
            cwd: "/home/u/.cache/forge/agent-tasks/shop-1/task-1a/../x".into(),
            ..cmd
        };
        assert!(s.exec(&escape).is_err());
        assert_eq!(
            s.remove().display(),
            "docker rm --force forge-agent-task-1a"
        );
        assert!(already_gone(
            "Error response from daemon: No such container: forge-agent-x"
        ));
    }

    #[test]
    fn windows_sandboxes_map_too() {
        let s = ContainerSpec {
            sandbox: r"C:\Users\u\AppData\Local\forge\agent-tasks\p\t1".into(),
            ..spec()
        };
        assert_eq!(
            s.container_path(r"C:\Users\u\AppData\Local\forge\agent-tasks\p\t1\src")
                .as_deref(),
            Some("/workspace/src")
        );
    }

    #[test]
    fn names_and_limits() {
        assert_eq!(container_name("task 1/x"), "forge-agent-task-1-x");
        assert_eq!(parse_cpus(" ").unwrap(), None);
        assert_eq!(parse_cpus("1.5").unwrap().as_deref(), Some("1.5"));
        assert!(parse_cpus("0").is_err() && parse_cpus("two").is_err());
        assert_eq!(parse_memory("").unwrap(), None);
        assert_eq!(parse_memory("4G").unwrap().as_deref(), Some("4g"));
        assert_eq!(parse_memory("512m").unwrap().as_deref(), Some("512m"));
        assert!(parse_memory("1m").unwrap_err().contains("minimum"));
        assert!(parse_memory("lots").is_err());
        assert!(is_podman("C:\\bin\\Podman.exe") && !is_podman("docker"));
        assert_eq!(map_paths("/p/apps:/p/app", "/p/app", "/c"), "/p/apps:/c");
    }
}

//! Run targets ("Run on", like IntelliJ's run targets): where a run, test or debug run
//! executes, and the command line that starts it there from this machine.
//!
//! | Target | Command line |
//! | --- | --- |
//! | Local machine | `<program> <args>` |
//! | Docker image | `docker [--context c] run --rm -i [-v <local>:<remote>]… [-p p:p]… [-w <dir>] [-e K=V]… [options] <image> <program> <args>` |
//! | Docker Compose service | `docker compose -f <file>… run --rm -T [-p p:p]… [-w <dir>] [-e K=V]… <service> …` or `exec -T` |
//! | SSH | `ssh -o BatchMode=yes [-p port] [-i key] [-R p:127.0.0.1:p] [-L p:127.0.0.1:p] user@host "cd '<dir>' && env 'K=V' <program> <args>"` |
//! | WSL | `wsl.exe -d <distribution> [-u user] --cd <dir> --exec env K=V <program> <args>` |
//! | Dev Container | `docker exec -i [-u user] -w <dir> -e K=V <container> <program> <args>` |
//!
//! Paths in the arguments and the working directory are mapped to the target with the
//! target's [`PathMapping`]s (automatic ones plus the user's); the Run console maps paths in
//! the output back and debuggers use the same pairs ([`DebugTarget`]). Ports the program
//! serves are published (`-p`) or forwarded (`-L`); a debug run whose debugger listens in
//! the IDE gets `host.docker.internal` in containers and a reverse tunnel (`-R`) over SSH.
//! Dev Container projects (`forge-devenv`) use the same `docker exec` command line.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mapping::{
    compose_mappings, join, normalize, wsl_path, Mapper, PathMapping, DOCKER_PROJECT_ROOT,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    #[default]
    Local,
    DockerImage,
    ComposeService,
    Ssh,
    Wsl,
    DevContainer,
}

impl TargetKind {
    pub fn label(self) -> &'static str {
        match self {
            TargetKind::Local => "Local machine",
            TargetKind::DockerImage => "Docker image",
            TargetKind::ComposeService => "Docker Compose service",
            TargetKind::Ssh => "SSH",
            TargetKind::Wsl => "WSL",
            TargetKind::DevContainer => "Dev Container",
        }
    }

    /// Docker based targets (the IDE is `host.docker.internal` there).
    pub fn is_container(self) -> bool {
        matches!(
            self,
            TargetKind::DockerImage | TargetKind::ComposeService | TargetKind::DevContainer
        )
    }
}

/// Docker Compose: start a new container for the run, or exec in the running one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lifecycle {
    #[default]
    Run,
    Exec,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A run configuration's "Run on" (`runOn` in the configuration). Fields that a kind does
/// not use stay empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunTarget {
    pub kind: TargetKind,
    /// Docker image (`node:22`, `python:3.13`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    /// Extra `docker run` options (`--network host`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run_options: String,
    /// Docker context (empty: the default one).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub docker_context: String,
    /// Compose files, relative to the project root.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub compose_files: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub service: String,
    #[serde(skip_serializing_if = "is_default")]
    pub lifecycle: Lifecycle,
    /// SSH host, port, user and identity file.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub host: String,
    #[serde(skip_serializing_if = "is_default")]
    pub port: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key_file: String,
    /// WSL distribution (empty: the default one).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub distribution: String,
    /// Dev container name or id.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub container: String,
    /// Where the project is on the target (the "project path on target"): Docker images
    /// mount it there (default `/opt/project`), SSH hosts and dev containers have it there.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project_path: String,
    /// The user's path mappings; they win over the automatic ones.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<PathMapping>,
    /// Environment the target adds to every run.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl RunTarget {
    pub fn local() -> Self {
        Self::default()
    }

    pub fn is_local(&self) -> bool {
        self.kind == TargetKind::Local
    }

    /// "Docker image node:22", "SSH deploy@shop.example.com", …
    pub fn display(&self) -> String {
        let detail = match self.kind {
            TargetKind::Local => String::new(),
            TargetKind::DockerImage => self.image.clone(),
            TargetKind::ComposeService => self.service.clone(),
            TargetKind::Ssh if !self.user.is_empty() => format!("{}@{}", self.user, self.host),
            TargetKind::Ssh => self.host.clone(),
            TargetKind::Wsl => self.distribution.clone(),
            TargetKind::DevContainer => self.container.clone(),
        };
        if detail.trim().is_empty() {
            self.kind.label().to_string()
        } else {
            format!("{} {}", self.kind.label(), detail.trim())
        }
    }

    /// What is missing to run on the target.
    pub fn validate(&self) -> Result<(), String> {
        let empty = |s: &str| s.trim().is_empty();
        match self.kind {
            TargetKind::DockerImage if empty(&self.image) => {
                Err("Image name is not specified".into())
            }
            TargetKind::ComposeService if empty(&self.service) => {
                Err("Service is not specified".into())
            }
            TargetKind::Ssh if empty(&self.host) => {
                Err("SSH configuration is not specified".into())
            }
            TargetKind::DevContainer if empty(&self.container) => {
                Err("Dev container is not specified".into())
            }
            _ => Ok(()),
        }
    }
}

/// The mappings that follow from the target: the Docker image's mount, the compose
/// service's volumes (`compose`: the compose file's directory and the service's volumes),
/// WSL's drive paths, the project path on an SSH host or in a dev container.
pub fn automatic(
    target: &RunTarget,
    root: &str,
    compose: Option<(&str, &[String])>,
) -> Vec<PathMapping> {
    let project_path = target.project_path.trim();
    let at = |remote: &str| {
        vec![PathMapping {
            local: normalize(root),
            remote: remote.to_string(),
        }]
    };
    match target.kind {
        TargetKind::Local => Vec::new(),
        TargetKind::DockerImage => at(if project_path.is_empty() {
            DOCKER_PROJECT_ROOT
        } else {
            project_path
        }),
        TargetKind::ComposeService => compose
            .map(|(dir, volumes)| compose_mappings(root, dir, volumes))
            .unwrap_or_default(),
        TargetKind::Wsl => wsl_path(root).map(|remote| at(&remote)).unwrap_or_default(),
        TargetKind::Ssh | TargetKind::DevContainer => {
            if project_path.is_empty() {
                Vec::new()
            } else {
                at(project_path)
            }
        }
    }
}

/// The user's mappings followed by the automatic ones (the user's win for the same local
/// path, see [`Mapper::new`]).
pub fn effective(target: &RunTarget, automatic: Vec<PathMapping>) -> Vec<PathMapping> {
    let mut all = target.mappings.clone();
    all.extend(automatic);
    all
}

/// How a debugger and the debugged program reach each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DebugDirection {
    /// The program connects to the IDE, which listens on `port` (Xdebug, a DAP server in
    /// "connect" mode): containers reach it as `host.docker.internal`, SSH gets `-R`.
    ConnectsToIde,
    /// The IDE connects to the program, which listens on `port` (debugpy `--listen`,
    /// Delve, `node --inspect`): the port is published / forwarded like a served one.
    IdeConnects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugPort {
    pub port: u16,
    pub direction: DebugDirection,
}

/// What to run, as if it ran locally in the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Invocation {
    /// The program where the target runs it.
    pub program: String,
    /// Arguments right after the program that are not mapped (interpreter options).
    pub program_args: Vec<String>,
    /// Arguments; local paths in them are mapped to the target.
    pub args: Vec<String>,
    /// Absolute local working directory (empty = the project root).
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    /// Ports the program serves that must be reachable from this machine.
    pub publish: Vec<u16>,
    /// A debug run's connection.
    pub debug: Option<DebugPort>,
}

/// Where the project is and how to reach the target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TargetContext {
    /// Absolute local project root.
    pub root: String,
    /// The effective path mappings ([`effective`]).
    pub mappings: Vec<PathMapping>,
    /// Docker CLI (setting `docker.cliPath`); empty = `docker`.
    pub docker: String,
}

/// The process to start on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wrapped {
    pub program: String,
    pub args: Vec<String>,
    /// Absolute local working directory.
    pub cwd: String,
    pub env: BTreeMap<String, String>,
}

/// What a debugger needs to debug a run on a target (exposed to the debugger layers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTarget {
    /// The target ("Docker image node:22").
    pub target: String,
    /// Host name under which the program reaches the IDE (`host.docker.internal`,
    /// `127.0.0.1` through the SSH tunnel, `localhost`).
    pub ide_host: String,
    /// Host name under which the IDE reaches a port the program listens on.
    pub target_host: String,
    /// Local ↔ target path pairs, for breakpoints and stack frames.
    pub mappings: Vec<PathMapping>,
}

/// The debugger's view of a target with its effective mappings.
pub fn debug_target(target: &RunTarget, mappings: &[PathMapping]) -> DebugTarget {
    let ide_host = match target.kind {
        k if k.is_container() => "host.docker.internal",
        TargetKind::Ssh => "127.0.0.1",
        _ => "localhost",
    };
    DebugTarget {
        target: target.display(),
        ide_host: ide_host.into(),
        // Published and forwarded ports are local ones.
        target_host: "127.0.0.1".into(),
        mappings: mappings.to_vec(),
    }
}

/// POSIX shell quoting for SSH command strings.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn or<'a>(s: &'a str, default: &'a str) -> &'a str {
    match s.trim() {
        "" => default,
        s => s,
    }
}

/// The command line that runs `inv` on `target`.
pub fn wrap(target: &RunTarget, inv: &Invocation, ctx: &TargetContext) -> Result<Wrapped, String> {
    target.validate()?;
    let mut env = target.env.clone();
    env.extend(inv.env.clone());
    let cwd = if inv.cwd.trim().is_empty() {
        ctx.root.clone()
    } else {
        inv.cwd.clone()
    };
    let mut inner: Vec<String> = vec![inv.program.clone()];
    inner.extend(inv.program_args.iter().cloned());

    if target.kind == TargetKind::Local {
        let mut args = inner.split_off(1);
        args.extend(inv.args.iter().cloned());
        return Ok(Wrapped {
            program: inner.remove(0),
            args,
            cwd,
            env,
        });
    }

    let mapper = Mapper::new(&ctx.root, &ctx.mappings);
    inner.extend(inv.args.iter().map(|a| mapper.map_arg(a)));
    let remote_cwd = mapper.to_remote(&cwd);
    let connects = inv
        .debug
        .filter(|d| d.direction == DebugDirection::ConnectsToIde)
        .map(|d| d.port);
    let mut publish = inv.publish.clone();
    if let Some(d) = inv
        .debug
        .filter(|d| d.direction == DebugDirection::IdeConnects)
    {
        if !publish.contains(&d.port) {
            publish.push(d.port);
        }
    }
    let docker = or(&ctx.docker, "docker").to_string();
    let context_args = || -> Vec<String> {
        match target.docker_context.trim() {
            "" | "Docker" | "default" => Vec::new(),
            c => vec!["--context".into(), c.into()],
        }
    };
    let env_args = |flag: &str| -> Vec<String> {
        env.iter()
            .flat_map(|(k, v)| [flag.to_string(), format!("{k}={v}")])
            .collect()
    };

    let (program, args) = match target.kind {
        TargetKind::Local => unreachable!("handled above"),
        TargetKind::DockerImage => {
            let mut args = context_args();
            args.extend(["run", "--rm", "-i"].map(String::from));
            if connects.is_some() {
                args.push("--add-host=host.docker.internal:host-gateway".into());
            }
            for (local, remote) in mapper.pairs() {
                args.push("-v".into());
                args.push(format!("{local}:{remote}"));
            }
            for p in &publish {
                args.push("-p".into());
                args.push(format!("{p}:{p}"));
            }
            if let Some(dir) = &remote_cwd {
                args.push("-w".into());
                args.push(dir.clone());
            }
            args.extend(env_args("-e"));
            args.extend(forge_runtools::args::split_args(&target.run_options));
            args.push(target.image.trim().to_string());
            args.extend(inner);
            (docker, args)
        }
        TargetKind::ComposeService => {
            let mut args = context_args();
            args.push("compose".into());
            for f in &target.compose_files {
                args.push("-f".into());
                args.push(join(&ctx.root, f));
            }
            match target.lifecycle {
                Lifecycle::Run => {
                    args.extend(["run", "--rm", "-T"].map(String::from));
                    for p in &publish {
                        args.push("-p".into());
                        args.push(format!("{p}:{p}"));
                    }
                }
                // A running container keeps the ports it was started with.
                Lifecycle::Exec => args.extend(["exec", "-T"].map(String::from)),
            }
            if let Some(dir) = &remote_cwd {
                args.push("-w".into());
                args.push(dir.clone());
            }
            args.extend(env_args("-e"));
            args.push(target.service.trim().to_string());
            args.extend(inner);
            (docker, args)
        }
        TargetKind::Ssh => {
            let mut args: Vec<String> = vec!["-o".into(), "BatchMode=yes".into()];
            if target.port != 0 && target.port != 22 {
                args.push("-p".into());
                args.push(target.port.to_string());
            }
            if !target.key_file.trim().is_empty() {
                args.push("-i".into());
                args.push(target.key_file.trim().to_string());
            }
            if let Some(port) = connects.filter(|p| *p != 0) {
                args.push("-R".into());
                args.push(format!("{port}:127.0.0.1:{port}"));
            }
            for p in &publish {
                args.push("-L".into());
                args.push(format!("{p}:127.0.0.1:{p}"));
            }
            args.push(if target.user.trim().is_empty() {
                target.host.trim().to_string()
            } else {
                format!("{}@{}", target.user.trim(), target.host.trim())
            });
            let mut remote = String::new();
            if let Some(dir) = &remote_cwd {
                remote.push_str(&format!("cd {} && ", sh_quote(dir)));
            }
            if !env.is_empty() {
                remote.push_str("env ");
                for (k, v) in &env {
                    remote.push_str(&sh_quote(&format!("{k}={v}")));
                    remote.push(' ');
                }
            }
            remote.push_str(
                &inner
                    .iter()
                    .map(|a| sh_quote(a))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            args.push(remote);
            ("ssh".to_string(), args)
        }
        TargetKind::Wsl => {
            let mut args: Vec<String> = Vec::new();
            if !target.distribution.trim().is_empty() {
                args.push("-d".into());
                args.push(target.distribution.trim().to_string());
            }
            if !target.user.trim().is_empty() {
                args.push("-u".into());
                args.push(target.user.trim().to_string());
            }
            if let Some(dir) = &remote_cwd {
                args.push("--cd".into());
                args.push(dir.clone());
            }
            args.push("--exec".into());
            if !env.is_empty() {
                args.push("env".into());
                args.extend(env.iter().map(|(k, v)| format!("{k}={v}")));
            }
            args.extend(inner);
            ("wsl.exe".to_string(), args)
        }
        TargetKind::DevContainer => {
            let mut args = context_args();
            args.extend(docker_exec_args(
                target.container.trim(),
                Some(target.user.trim()),
                remote_cwd.as_deref(),
                &env,
                true,
                &inner,
            ));
            (docker, args)
        }
    };
    // The environment travels inside the command line; the local process gets none.
    Ok(Wrapped {
        program,
        args,
        cwd: ctx.root.clone(),
        env: BTreeMap::new(),
    })
}

/// `docker exec` running `argv` in a container. `interactive` keeps stdin open (`-i`).
/// Dev Container projects (`forge-devenv`) and Dev Container targets share it.
pub fn docker_exec_args(
    container: &str,
    user: Option<&str>,
    workdir: Option<&str>,
    env: &BTreeMap<String, String>,
    interactive: bool,
    argv: &[String],
) -> Vec<String> {
    let mut a = vec!["exec".to_string()];
    if interactive {
        a.push("-i".into());
    }
    if let Some(u) = user.filter(|u| !u.is_empty()) {
        a.push("-u".into());
        a.push(u.to_string());
    }
    if let Some(w) = workdir.filter(|w| !w.is_empty()) {
        a.push("-w".into());
        a.push(w.to_string());
    }
    for (k, v) in env {
        // `${containerEnv:…}` cannot be expanded from outside; leave such entries out.
        if !v.contains("${containerEnv:") {
            a.push("-e".into());
            a.push(format!("{k}={v}"));
        }
    }
    a.push(container.to_string());
    a.extend(argv.iter().cloned());
    a
}

/// [`automatic`] plus the user's mappings, as a [`TargetContext`].
pub fn context(
    target: &RunTarget,
    root: &str,
    compose: Option<(&str, &[String])>,
    docker: &str,
) -> TargetContext {
    TargetContext {
        root: root.to_string(),
        mappings: effective(target, automatic(target, root, compose)),
        docker: docker.to_string(),
    }
}

/// Starts a wrapped command; the backend starts real processes, tests use fakes.
pub trait Launcher {
    fn launch(&self, cmd: &Wrapped) -> Result<i32, String>;
}

/// Wrap and start `inv` on `target` with `launcher`; returns the exit code.
pub fn run_on(
    target: &RunTarget,
    inv: &Invocation,
    ctx: &TargetContext,
    launcher: &dyn Launcher,
) -> Result<i32, String> {
    launcher.launch(&wrap(target, inv, ctx)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records what it was asked to start, exits with 0.
    #[derive(Default)]
    struct FakeLauncher(RefCell<Vec<Wrapped>>);

    impl Launcher for FakeLauncher {
        fn launch(&self, cmd: &Wrapped) -> Result<i32, String> {
            self.0.borrow_mut().push(cmd.clone());
            Ok(0)
        }
    }

    fn inv(program: &str, args: &[&str]) -> Invocation {
        Invocation {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: "/home/me/shop".into(),
            ..Invocation::default()
        }
    }

    fn ctx(target: &RunTarget) -> TargetContext {
        context(target, "/home/me/shop", None, "")
    }

    #[test]
    fn local_runs_directly() {
        let t = RunTarget::local();
        let mut i = inv("node", &["test/cart.test.mjs"]);
        i.program_args = vec!["--test".into()];
        i.env.insert("CI".into(), "1".into());
        let w = wrap(&t, &i, &ctx(&t)).unwrap();
        assert_eq!(
            (w.program.as_str(), w.args.as_slice()),
            (
                "node",
                &["--test".to_string(), "test/cart.test.mjs".to_string()][..]
            )
        );
        assert_eq!(w.env["CI"], "1");
    }

    #[test]
    fn docker_images_mount_the_project_and_map_paths() {
        let t = RunTarget {
            kind: TargetKind::DockerImage,
            image: "node:22".into(),
            run_options: "--network host".into(),
            env: BTreeMap::from([("NODE_ENV".into(), "test".into())]),
            ..RunTarget::default()
        };
        let mut i = inv("npx", &["vitest", "run", "/home/me/shop/src/cart.test.ts"]);
        i.cwd = "/home/me/shop/src".into();
        let launcher = FakeLauncher::default();
        assert_eq!(run_on(&t, &i, &ctx(&t), &launcher), Ok(0));
        let w = launcher.0.borrow()[0].clone();
        assert_eq!(w.program, "docker");
        assert_eq!(
            w.args,
            [
                "run",
                "--rm",
                "-i",
                "-v",
                "/home/me/shop:/opt/project",
                "-w",
                "/opt/project/src",
                "-e",
                "NODE_ENV=test",
                "--network",
                "host",
                "node:22",
                "npx",
                "vitest",
                "run",
                "/opt/project/src/cart.test.ts"
            ]
        );
        assert!(w.env.is_empty());
        assert_eq!(w.cwd, "/home/me/shop");
        let mut custom = t.clone();
        custom.project_path = "/app".into();
        let w = wrap(&custom, &i, &ctx(&custom)).unwrap();
        assert!(w.args.contains(&"/home/me/shop:/app".to_string()));
        assert!(wrap(
            &RunTarget {
                kind: TargetKind::DockerImage,
                ..RunTarget::default()
            },
            &i,
            &ctx(&t)
        )
        .is_err());
    }

    #[test]
    fn compose_services_use_their_volumes() {
        let t = RunTarget {
            kind: TargetKind::ComposeService,
            compose_files: vec!["compose.yaml".into()],
            service: "app".into(),
            lifecycle: Lifecycle::Exec,
            ..RunTarget::default()
        };
        let volumes = vec![".:/srv/app".to_string()];
        let c = context(
            &t,
            "/home/me/shop",
            Some(("/home/me/shop", &volumes)),
            "/usr/bin/docker",
        );
        let w = wrap(&t, &inv("pytest", &["/home/me/shop/tests"]), &c).unwrap();
        assert_eq!(w.program, "/usr/bin/docker");
        assert_eq!(
            w.args,
            [
                "compose",
                "-f",
                "/home/me/shop/compose.yaml",
                "exec",
                "-T",
                "-w",
                "/srv/app",
                "app",
                "pytest",
                "/srv/app/tests"
            ]
        );
    }

    #[test]
    fn ssh_quotes_the_remote_command_and_forwards_ports() {
        let t = RunTarget {
            kind: TargetKind::Ssh,
            host: "ci.example.com".into(),
            port: 2222,
            user: "deploy".into(),
            key_file: "~/.ssh/id_ed25519".into(),
            project_path: "/srv/shop".into(),
            ..RunTarget::default()
        };
        let mut i = inv("go", &["test", "-json", "./..."]);
        i.debug = Some(DebugPort {
            port: 2345,
            direction: DebugDirection::IdeConnects,
        });
        i.env.insert("GOFLAGS".into(), "-count=1 -v".into());
        let w = wrap(&t, &i, &ctx(&t)).unwrap();
        assert_eq!(w.program, "ssh");
        assert_eq!(
            w.args,
            [
                "-o",
                "BatchMode=yes",
                "-p",
                "2222",
                "-i",
                "~/.ssh/id_ed25519",
                "-L",
                "2345:127.0.0.1:2345",
                "deploy@ci.example.com",
                "cd /srv/shop && env 'GOFLAGS=-count=1 -v' go test -json ./..."
            ]
        );
        i.debug = Some(DebugPort {
            port: 9003,
            direction: DebugDirection::ConnectsToIde,
        });
        let w = wrap(&t, &i, &ctx(&t)).unwrap();
        assert!(w
            .args
            .windows(2)
            .any(|a| a == ["-R", "9003:127.0.0.1:9003"]));
        let d = debug_target(&t, &ctx(&t).mappings);
        assert_eq!(d.ide_host, "127.0.0.1");
        assert_eq!(d.mappings[0].remote, "/srv/shop");
        assert_eq!(d.target, "SSH deploy@ci.example.com");
    }

    #[test]
    fn wsl_maps_windows_drives() {
        let t = RunTarget {
            kind: TargetKind::Wsl,
            distribution: "Ubuntu".into(),
            ..RunTarget::default()
        };
        let c = context(&t, "C:\\work\\shop", None, "");
        let i = Invocation {
            program: "cargo".into(),
            args: vec!["test".into(), "C:\\work\\shop\\tests\\cart.rs".into()],
            cwd: "C:\\work\\shop".into(),
            ..Invocation::default()
        };
        let w = wrap(&t, &i, &c).unwrap();
        assert_eq!(w.program, "wsl.exe");
        assert_eq!(
            w.args,
            [
                "-d",
                "Ubuntu",
                "--cd",
                "/mnt/c/work/shop",
                "--exec",
                "cargo",
                "test",
                "/mnt/c/work/shop/tests/cart.rs"
            ]
        );
    }

    #[test]
    fn dev_containers_exec_in_the_running_container() {
        let t = RunTarget {
            kind: TargetKind::DevContainer,
            container: "shop-dev".into(),
            user: "vscode".into(),
            project_path: "/workspaces/shop".into(),
            ..RunTarget::default()
        };
        let mut i = inv("busted", &["spec/cart_spec.lua"]);
        i.env.insert("LUA_PATH".into(), "src/?.lua".into());
        i.debug = Some(DebugPort {
            port: 9966,
            direction: DebugDirection::ConnectsToIde,
        });
        let w = wrap(&t, &i, &ctx(&t)).unwrap();
        assert_eq!(w.program, "docker");
        assert_eq!(
            w.args,
            [
                "exec",
                "-i",
                "-u",
                "vscode",
                "-w",
                "/workspaces/shop",
                "-e",
                "LUA_PATH=src/?.lua",
                "shop-dev",
                "busted",
                "spec/cart_spec.lua"
            ]
        );
        assert_eq!(debug_target(&t, &[]).ide_host, "host.docker.internal");
        assert_eq!(t.display(), "Dev Container shop-dev");
    }

    #[test]
    fn served_ports_are_published() {
        let t = RunTarget {
            kind: TargetKind::DockerImage,
            image: "python:3.13".into(),
            ..RunTarget::default()
        };
        let mut i = inv("python", &["-m", "http.server", "8000"]);
        i.publish = vec![8000];
        i.debug = Some(DebugPort {
            port: 5678,
            direction: DebugDirection::IdeConnects,
        });
        let w = wrap(&t, &i, &ctx(&t)).unwrap();
        assert!(w.args.windows(2).any(|a| a == ["-p", "8000:8000"]));
        assert!(w.args.windows(2).any(|a| a == ["-p", "5678:5678"]));
        assert!(!w.args.iter().any(|a| a.starts_with("--add-host")));
    }

    #[test]
    fn targets_serialize_compactly() {
        let t = RunTarget {
            kind: TargetKind::DockerImage,
            image: "node:22".into(),
            ..RunTarget::default()
        };
        assert_eq!(
            serde_json::to_string(&t).unwrap(),
            r#"{"kind":"dockerImage","image":"node:22"}"#
        );
        let back: RunTarget = serde_json::from_str(r#"{"kind":"wsl"}"#).unwrap();
        assert_eq!(back.kind, TargetKind::Wsl);
    }
}

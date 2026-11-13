//! Command lines that run PHP (or another program such as Composer) through an
//! interpreter:
//!
//! - local: `<php> -d … <args>`
//! - Docker: `docker run --rm -i -v <project>:/opt/project -w <dir> -e K=V <image> php …`
//! - Docker Compose: `docker compose -f <file> run --rm -T -w <dir> -e K=V <service> php …`
//!   or `… exec -T …` ("Connect to existing container")
//! - SSH: `ssh -o BatchMode=yes -p <port> user@host "cd '<dir>' && env 'K=V' 'php' …"`
//! - WSL: `wsl.exe -d <distribution> --cd <dir> --exec env K=V php …`
//!
//! Ports a program serves ([`Invocation::publish`]) are published by containers (`-p`) and
//! forwarded over SSH (`-L`); WSL 2 forwards localhost on its own.
//!
//! Paths in the arguments and the working directory are mapped to the remote side. A
//! debug run (an `XDEBUG_SESSION` / `XDEBUG_CONFIG` variable) also gets what Xdebug needs
//! to reach the IDE: `host.docker.internal` in containers, a reverse tunnel of the debug
//! port over SSH, the interpreter's name as `PHP_IDE_CONFIG` server (so the debugger uses
//! its mappings) and the "Debugger extension" when Xdebug is not loaded by php.ini.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use forge_runtargets::{DebugDirection, DebugPort, RunTarget, TargetContext, TargetKind};

use crate::mapping::PathMapping;
use crate::model::{InterpreterKind, Lifecycle, PhpInterpreter};

/// The program to start where the interpreter runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "name")]
pub enum Program {
    /// The interpreter's PHP executable.
    #[default]
    Php,
    /// Another program in the same environment (`composer`, `vendor/bin/phpunit`).
    Other(String),
}

/// What to run, as if PHP ran locally in the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Invocation {
    pub program: Program,
    pub args: Vec<String>,
    /// Absolute local working directory (empty = the project root).
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    /// Ports the program listens on that must be reachable from this machine (the PHP
    /// built-in web server): published by containers, forwarded over SSH.
    pub publish: Vec<u16>,
}

/// A saved SSH configuration, as far as the command line needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_file: Option<String>,
}

/// Where the project is and how to reach the interpreter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WrapContext {
    /// Absolute local project root.
    pub root: String,
    /// The effective path mappings ([`crate::mapping::effective`]).
    pub mappings: Vec<PathMapping>,
    /// Docker CLI (setting `docker.cliPath`); empty = `docker`.
    pub docker: String,
    /// The interpreter's SSH configuration.
    pub ssh: Option<SshTarget>,
}

/// The process to start on this machine (the generic run target command line).
pub use forge_runtargets::Wrapped;

/// Whether the environment starts an Xdebug session.
pub fn is_debug(env: &BTreeMap<String, String>) -> bool {
    env.contains_key("XDEBUG_SESSION") || env.contains_key("XDEBUG_CONFIG")
}

/// The `client_port` of `XDEBUG_CONFIG`.
pub fn debug_port(env: &BTreeMap<String, String>) -> Option<u16> {
    env.get("XDEBUG_CONFIG")?
        .split_whitespace()
        .find_map(|p| p.strip_prefix("client_port="))
        .and_then(|p| p.parse().ok())
}

/// `XDEBUG_CONFIG` with `client_host` pointing at `host`.
pub fn with_client_host(config: &str, host: &str) -> String {
    let mut parts: Vec<String> = config
        .split_whitespace()
        .filter(|p| !p.starts_with("client_host="))
        .map(String::from)
        .collect();
    parts.insert(0, format!("client_host={host}"));
    parts.join(" ")
}

/// POSIX shell quoting for SSH command strings.
pub use forge_runtargets::target::sh_quote;

/// The run target an interpreter stands for: where PHP runs and how to reach it.
pub fn run_target(interp: &PhpInterpreter, ssh: Option<&SshTarget>) -> RunTarget {
    let kind = match interp.kind {
        InterpreterKind::Local => TargetKind::Local,
        InterpreterKind::Docker => TargetKind::DockerImage,
        InterpreterKind::Compose => TargetKind::ComposeService,
        InterpreterKind::Ssh => TargetKind::Ssh,
        InterpreterKind::Wsl => TargetKind::Wsl,
    };
    let ssh = ssh.filter(|t| !t.host.trim().is_empty());
    RunTarget {
        kind,
        image: interp.image.trim().to_string(),
        docker_context: interp.docker_server.trim().to_string(),
        compose_files: interp.compose_files.clone(),
        service: interp.service.trim().to_string(),
        lifecycle: match interp.lifecycle {
            Lifecycle::Run => forge_runtargets::Lifecycle::Run,
            Lifecycle::Exec => forge_runtargets::Lifecycle::Exec,
        },
        host: ssh.map(|t| t.host.trim().to_string()).unwrap_or_default(),
        port: ssh.map_or(0, |t| t.port),
        user: ssh.map(|t| t.user.trim().to_string()).unwrap_or_default(),
        key_file: ssh
            .and_then(|t| t.key_file.as_deref())
            .map(|k| k.trim().to_string())
            .unwrap_or_default(),
        distribution: interp.distribution.trim().to_string(),
        mappings: interp.mappings.clone(),
        ..RunTarget::default()
    }
}

/// The command line that runs `inv` with `interp`: PHP's own options and Xdebug's
/// settings here, the target's command line from `forge-runtargets`.
pub fn wrap(
    interp: &PhpInterpreter,
    inv: &Invocation,
    ctx: &WrapContext,
) -> Result<Wrapped, String> {
    let debug = is_debug(&inv.env);
    let mut env = inv.env.clone();
    let cwd = if inv.cwd.trim().is_empty() {
        ctx.root.clone()
    } else {
        inv.cwd.clone()
    };

    // The program and its leading arguments where PHP runs.
    let (program, program_args) = match &inv.program {
        Program::Php => {
            let mut options = Vec::new();
            for o in &interp.options {
                if !o.name.trim().is_empty() {
                    options.push(format!("-d{}={}", o.name.trim(), o.value.trim()));
                }
            }
            if debug && interp.xdebug.trim().is_empty() && !interp.debugger_extension.is_empty() {
                options.push(format!(
                    "-dzend_extension={}",
                    interp.debugger_extension.trim()
                ));
            }
            (interp.php().to_string(), options)
        }
        Program::Other(p) => (p.clone(), Vec::new()),
    };

    if interp.kind != InterpreterKind::Local && debug {
        if !env.contains_key("PHP_IDE_CONFIG") {
            env.insert(
                "PHP_IDE_CONFIG".into(),
                format!("serverName={}", interp.debug_server_name()),
            );
        }
        if matches!(
            interp.kind,
            InterpreterKind::Docker | InterpreterKind::Compose
        ) {
            if let Some(c) = env.get_mut("XDEBUG_CONFIG") {
                *c = with_client_host(c, "host.docker.internal");
            }
        }
    }
    let target = run_target(interp, ctx.ssh.as_ref());
    let generic = forge_runtargets::Invocation {
        program,
        program_args,
        args: inv.args.clone(),
        cwd,
        // Port 0: Xdebug connects back without a known port (no SSH tunnel).
        debug: debug.then(|| DebugPort {
            port: debug_port(&env).unwrap_or(0),
            direction: DebugDirection::ConnectsToIde,
        }),
        env,
        publish: inv.publish.clone(),
    };
    forge_runtargets::wrap(
        &target,
        &generic,
        &TargetContext {
            root: ctx.root.clone(),
            mappings: ctx.mappings.clone(),
            docker: ctx.docker.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{automatic, effective};
    use crate::model::ConfigOption;

    fn inv(args: &[&str]) -> Invocation {
        Invocation {
            program: Program::Php,
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: "/home/me/shop".into(),
            env: BTreeMap::new(),
            publish: Vec::new(),
        }
    }

    fn ctx(interp: &PhpInterpreter, compose: Option<(&str, &[String])>) -> WrapContext {
        WrapContext {
            root: "/home/me/shop".into(),
            mappings: effective(interp, automatic(interp, "/home/me/shop", compose)),
            docker: String::new(),
            ssh: None,
        }
    }

    fn debug_env() -> BTreeMap<String, String> {
        [
            ("XDEBUG_SESSION", "FORGE"),
            (
                "XDEBUG_CONFIG",
                "client_host=127.0.0.1 client_port=9003 idekey=FORGE",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn local_interpreters_run_directly_with_options() {
        let php = PhpInterpreter {
            path: "/usr/bin/php8.3".into(),
            options: vec![ConfigOption {
                name: "memory_limit".into(),
                value: "-1".into(),
            }],
            debugger_extension: "/usr/lib/php/xdebug.so".into(),
            ..Default::default()
        };
        let mut i = inv(&["bin/run.php", "a b"]);
        let w = wrap(&php, &i, &ctx(&php, None)).unwrap();
        assert_eq!(w.program, "/usr/bin/php8.3");
        assert_eq!(w.args, ["-dmemory_limit=-1", "bin/run.php", "a b"]);
        assert_eq!(w.cwd, "/home/me/shop");
        // Debugging loads the debugger extension when Xdebug is not loaded.
        i.env = debug_env();
        let w = wrap(&php, &i, &ctx(&php, None)).unwrap();
        assert_eq!(w.args[1], "-dzend_extension=/usr/lib/php/xdebug.so");
        assert_eq!(w.env["XDEBUG_SESSION"], "FORGE");
        assert!(!w.env.contains_key("PHP_IDE_CONFIG"));
    }

    #[test]
    fn docker_runs_a_new_container_with_the_project_mounted() {
        let php = PhpInterpreter {
            name: "php:8.3-cli".into(),
            kind: InterpreterKind::Docker,
            image: "php:8.3-cli".into(),
            ..Default::default()
        };
        let mut i = inv(&["/home/me/shop/bin/run.php", "--x"]);
        i.cwd = "/home/me/shop/bin".into();
        let w = wrap(&php, &i, &ctx(&php, None)).unwrap();
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
                "/opt/project/bin",
                "php:8.3-cli",
                "php",
                "/opt/project/bin/run.php",
                "--x"
            ]
        );
        assert!(w.env.is_empty());

        i.env = debug_env();
        let mut with_context = php.clone();
        with_context.docker_server = "colima".into();
        let w = wrap(&with_context, &i, &ctx(&php, None)).unwrap();
        assert_eq!(w.args[..2], ["--context", "colima"]);
        assert!(w
            .args
            .contains(&"--add-host=host.docker.internal:host-gateway".to_string()));
        assert!(w.args.contains(
            &"XDEBUG_CONFIG=client_host=host.docker.internal client_port=9003 idekey=FORGE"
                .to_string()
        ));
        assert!(w
            .args
            .contains(&"PHP_IDE_CONFIG=serverName=php:8.3-cli".to_string()));
        let no_image = PhpInterpreter {
            kind: InterpreterKind::Docker,
            ..Default::default()
        };
        assert!(wrap(&no_image, &i, &ctx(&no_image, None)).is_err());
    }

    #[test]
    fn compose_runs_or_execs_in_the_service() {
        let mut php = PhpInterpreter {
            name: "app".into(),
            kind: InterpreterKind::Compose,
            compose_files: vec!["docker-compose.yml".into()],
            service: "app".into(),
            path: "/usr/local/bin/php".into(),
            ..Default::default()
        };
        let volumes = vec![".:/var/www/html".to_string()];
        let c = ctx(&php, Some(("/home/me/shop", &volumes)));
        let w = wrap(&php, &inv(&["artisan", "migrate"]), &c).unwrap();
        assert_eq!(
            w.args,
            [
                "compose",
                "-f",
                "/home/me/shop/docker-compose.yml",
                "run",
                "--rm",
                "-T",
                "-w",
                "/var/www/html",
                "app",
                "/usr/local/bin/php",
                "artisan",
                "migrate"
            ]
        );
        php.lifecycle = Lifecycle::Exec;
        let mut composer = inv(&["install"]);
        composer.program = Program::Other("composer".into());
        let w = wrap(&php, &composer, &c).unwrap();
        assert_eq!(
            w.args[3..],
            [
                "exec",
                "-T",
                "-w",
                "/var/www/html",
                "app",
                "composer",
                "install"
            ]
        );
    }

    #[test]
    fn ssh_runs_a_quoted_remote_command() {
        let php = PhpInterpreter {
            name: "prod".into(),
            kind: InterpreterKind::Ssh,
            path: "/usr/bin/php8.2".into(),
            mappings: vec![PathMapping {
                local: "".into(),
                remote: "/srv/shop".into(),
            }],
            ..Default::default()
        };
        let mut c = ctx(&php, None);
        assert!(wrap(&php, &inv(&[]), &c).is_err());
        c.ssh = Some(SshTarget {
            host: "shop.example.com".into(),
            port: 2222,
            user: "deploy".into(),
            key_file: Some("~/.ssh/id_ed25519".into()),
        });
        let mut i = inv(&["bin/run.php", "it's"]);
        i.env = debug_env();
        let w = wrap(&php, &i, &c).unwrap();
        assert_eq!(w.program, "ssh");
        assert_eq!(
            w.args[..9],
            [
                "-o",
                "BatchMode=yes",
                "-p",
                "2222",
                "-i",
                "~/.ssh/id_ed25519",
                "-R",
                "9003:127.0.0.1:9003",
                "deploy@shop.example.com"
            ]
        );
        assert_eq!(
            w.args[9],
            "cd /srv/shop && env PHP_IDE_CONFIG=serverName=prod \
             'XDEBUG_CONFIG=client_host=127.0.0.1 client_port=9003 idekey=FORGE' \
             XDEBUG_SESSION=FORGE /usr/bin/php8.2 bin/run.php 'it'\\''s'"
        );
    }

    #[test]
    fn wsl_maps_windows_paths() {
        let php = PhpInterpreter {
            kind: InterpreterKind::Wsl,
            distribution: "Ubuntu".into(),
            ..Default::default()
        };
        let c = WrapContext {
            root: "C:/work/shop".into(),
            mappings: effective(&php, automatic(&php, "C:\\work\\shop", None)),
            ..Default::default()
        };
        let i = Invocation {
            args: vec!["C:\\work\\shop\\bin\\run.php".into()],
            cwd: "C:\\work\\shop".into(),
            ..Default::default()
        };
        let w = wrap(&php, &i, &c).unwrap();
        assert_eq!(w.program, "wsl.exe");
        assert_eq!(
            w.args,
            [
                "-d",
                "Ubuntu",
                "--cd",
                "/mnt/c/work/shop",
                "--exec",
                "php",
                "/mnt/c/work/shop/bin/run.php"
            ]
        );
    }

    #[test]
    fn served_ports_are_published_and_forwarded() {
        let mut i = inv(&["-S", "0.0.0.0:8080", "-t", "/home/me/shop/public"]);
        i.publish = vec![8080];
        let docker = PhpInterpreter {
            kind: InterpreterKind::Docker,
            image: "php:8.3-cli".into(),
            ..Default::default()
        };
        let w = wrap(&docker, &i, &ctx(&docker, None)).unwrap();
        assert!(w.args.windows(2).any(|a| a == ["-p", "8080:8080"]));
        assert!(w.args.contains(&"/opt/project/public".to_string()));
        let compose = PhpInterpreter {
            kind: InterpreterKind::Compose,
            service: "app".into(),
            ..Default::default()
        };
        let w = wrap(&compose, &i, &ctx(&compose, None)).unwrap();
        assert!(w.args.windows(2).any(|a| a == ["-p", "8080:8080"]));
        let ssh = PhpInterpreter {
            kind: InterpreterKind::Ssh,
            ..Default::default()
        };
        let mut c = ctx(&ssh, None);
        c.ssh = Some(SshTarget {
            host: "h".into(),
            ..Default::default()
        });
        let w = wrap(&ssh, &i, &c).unwrap();
        assert!(w
            .args
            .windows(2)
            .any(|a| a == ["-L", "8080:127.0.0.1:8080"]));
    }

    #[test]
    fn quotes_for_the_shell() {
        assert_eq!(sh_quote("abc/d.php"), "abc/d.php");
        assert_eq!(sh_quote(""), "''");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(debug_port(&debug_env()), Some(9003));
    }
}

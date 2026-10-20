//! Running an install command (`npm install …`, `pipx install …`) with streamed output.
//! Commands come from the catalog only and run without a shell.
//!
//! npm servers are never installed globally: the catalog's `npm install -g <packages>` is
//! run as `npm install --prefix <Forge's langservers dir> <packages>`, so nothing outside
//! Forge's own directory changes and the servers are found again by [`Detector`].

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Notify};

use crate::catalog::{display_args, InstallOption};
use crate::detect::Detector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Stdout,
    Stderr,
    /// Lines written by Forge itself (the command line, the result).
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallOutcome {
    Succeeded,
    Failed { code: Option<i32> },
    Cancelled,
}

/// `npm install -g <packages>` as an install into `dir`:
/// `npm install --prefix <dir> --no-fund --no-audit <packages>`. `None` for anything that is
/// not a global npm install, which then runs unchanged.
pub fn managed_command(option: &InstallOption, dir: &Path) -> Option<Vec<String>> {
    if option.tool != "npm" {
        return None;
    }
    let (program, rest) = option.command.split_first()?;
    let (verb, rest) = rest.split_first()?;
    let global = |a: &String| a == "-g" || a == "--global";
    if program != "npm"
        || !matches!(verb.as_str(), "install" | "i" | "add")
        || !rest.iter().any(global)
    {
        return None;
    }
    let mut out = vec![
        program.clone(),
        "install".into(),
        "--prefix".into(),
        dir.display().to_string(),
        "--no-fund".into(),
        "--no-audit".into(),
    ];
    out.extend(rest.iter().filter(|a| !global(a)).cloned());
    Some(out)
}

/// The command that actually runs: the managed form of a global npm install when the
/// detector knows Forge's directory, otherwise the catalog command.
pub fn effective_command(option: &InstallOption, detector: &Detector) -> Vec<String> {
    detector
        .managed_dir()
        .and_then(|dir| managed_command(option, dir))
        .unwrap_or_else(|| option.command.clone())
}

/// [`effective_command`] as one line, for the install dialog and its output.
pub fn display_command(option: &InstallOption, detector: &Detector) -> String {
    if option.command.is_empty() {
        return option.display();
    }
    display_args(&effective_command(option, detector))
}

/// The program and arguments to run: the program is resolved like a language server
/// (npm, pipx and go are often outside a desktop session's `PATH`). The managed directory
/// of an npm install is created here.
pub fn resolve_command(
    option: &InstallOption,
    detector: &Detector,
) -> Result<(PathBuf, Vec<String>), String> {
    if !option.runnable() {
        return Err(format!(
            "{} commands are not run by Forge; run it in a terminal",
            option.tool
        ));
    }
    let command = effective_command(option, detector);
    let (program, args) = command
        .split_first()
        .ok_or_else(|| "empty install command".to_string())?;
    let path = detector.find(program).ok_or_else(|| {
        format!("{program} was not found. Install it first or run the command in a terminal.")
    })?;
    if command != option.command {
        if let Some(dir) = detector.managed_dir() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
    }
    Ok((path, args.to_vec()))
}

/// Run `program args…`, sending every output line to `on_line`. `cancel` stops it.
pub async fn run(
    program: PathBuf,
    args: Vec<String>,
    extra_path: Vec<PathBuf>,
    cancel: Arc<Notify>,
    mut on_line: impl FnMut(Stream, String),
) -> InstallOutcome {
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // The tool's own directory first, so npm finds node next to it.
    let mut path_dirs: Vec<PathBuf> = program
        .parent()
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_default();
    path_dirs.extend(extra_path);
    if let Some(current) = std::env::var_os("PATH") {
        path_dirs.extend(std::env::split_paths(&current));
    }
    if let Ok(joined) = std::env::join_paths(path_dirs) {
        cmd.env("PATH", joined);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            on_line(
                Stream::Info,
                format!("Could not start {}: {e}", program.display()),
            );
            return InstallOutcome::Failed { code: None };
        }
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<(Stream, String)>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((Stream::Stdout, l));
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((Stream::Stderr, l));
            }
        });
    }
    drop(tx);
    let status = loop {
        tokio::select! {
            Some((stream, line)) = rx.recv() => on_line(stream, line),
            status = child.wait() => break status,
            _ = cancel.notified() => {
                let _ = child.kill().await;
                while let Ok((stream, line)) = rx.try_recv() {
                    on_line(stream, line);
                }
                return InstallOutcome::Cancelled;
            }
        }
    };
    // Drain what the readers still hold.
    while let Some((stream, line)) = rx.recv().await {
        on_line(stream, line);
    }
    match status {
        Ok(s) if s.success() => InstallOutcome::Succeeded,
        Ok(s) => InstallOutcome::Failed { code: s.code() },
        Err(e) => {
            on_line(Stream::Info, format!("{e}"));
            InstallOutcome::Failed { code: None }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::detect::DetectEnv;

    #[tokio::test]
    async fn streams_output_and_reports_the_exit_code() {
        let mut lines = Vec::new();
        let outcome = run(
            PathBuf::from("/bin/sh"),
            vec![
                "-c".into(),
                "echo added 1 package; echo warn >&2; exit 3".into(),
            ],
            vec![],
            Arc::new(Notify::new()),
            |s, l| lines.push((s, l)),
        )
        .await;
        assert_eq!(outcome, InstallOutcome::Failed { code: Some(3) });
        assert!(lines.contains(&(Stream::Stdout, "added 1 package".into())));
        assert!(lines.contains(&(Stream::Stderr, "warn".into())));

        let ok = run(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "true".into()],
            vec![],
            Arc::new(Notify::new()),
            |_, _| {},
        )
        .await;
        assert_eq!(ok, InstallOutcome::Succeeded);
    }

    #[tokio::test]
    async fn can_be_cancelled() {
        let cancel = Arc::new(Notify::new());
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            c.notify_one();
        });
        let started = std::time::Instant::now();
        let outcome = run(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "sleep 20".into()],
            vec![],
            cancel,
            |_, _| {},
        )
        .await;
        assert_eq!(outcome, InstallOutcome::Cancelled);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    fn npm_option(args: &[&str]) -> InstallOption {
        InstallOption {
            tool: "npm".into(),
            command: args.iter().map(|a| a.to_string()).collect(),
            url: None,
            platforms: vec![],
        }
    }

    #[test]
    fn global_npm_installs_become_installs_into_the_managed_directory() {
        let dir = Path::new("/data/forge/langservers");
        let rewritten = managed_command(
            &npm_option(&[
                "npm",
                "install",
                "-g",
                "typescript-language-server",
                "typescript",
            ]),
            dir,
        )
        .unwrap();
        assert_eq!(
            rewritten,
            [
                "npm",
                "install",
                "--prefix",
                "/data/forge/langservers",
                "--no-fund",
                "--no-audit",
                "typescript-language-server",
                "typescript"
            ]
        );
        // The flag may come anywhere and in its long form.
        let long = managed_command(&npm_option(&["npm", "i", "pyright", "--global"]), dir).unwrap();
        assert_eq!(long.last().map(String::as_str), Some("pyright"));
        assert!(!long.iter().any(|a| a == "--global" || a == "-g"));
        // Local installs, other programs and other tools are left alone.
        assert_eq!(
            managed_command(&npm_option(&["npm", "install", "x"]), dir),
            None
        );
        assert_eq!(
            managed_command(&npm_option(&["npm", "view", "-g", "x"]), dir),
            None
        );
        let brew = InstallOption {
            tool: "brew".into(),
            ..npm_option(&["brew", "install", "-g", "x"])
        };
        assert_eq!(managed_command(&brew, dir), None);
        assert_eq!(managed_command(&npm_option(&[]), dir), None);
    }

    #[test]
    fn display_and_resolution_use_the_managed_form_and_create_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        crate::detect::tests::fake_bin(&tmp.path().join("bin/npm"));
        let managed = tmp.path().join("data dir/forge/langservers");
        let d = Detector::new(DetectEnv {
            path: Some(tmp.path().join("bin").into_os_string()),
            managed_dir: Some(managed.clone()),
            ..Default::default()
        });
        let option = npm_option(&["npm", "install", "-g", "yaml-language-server"]);
        let line = display_command(&option, &d);
        assert_eq!(
            line,
            format!(
                "npm install --prefix \"{}\" --no-fund --no-audit yaml-language-server",
                managed.display()
            )
        );
        assert!(!managed.exists(), "displaying changes nothing on disk");

        let (program, args) = resolve_command(&option, &d).unwrap();
        assert_eq!(program, tmp.path().join("bin/npm"));
        assert_eq!(args[..2], ["install", "--prefix"]);
        assert_eq!(args[2], managed.display().to_string());
        assert!(managed.is_dir(), "the managed directory is created");

        // Without a managed directory the catalog command runs as it is.
        let plain = Detector::new(DetectEnv {
            path: Some(tmp.path().join("bin").into_os_string()),
            ..Default::default()
        });
        assert_eq!(
            display_command(&option, &plain),
            "npm install -g yaml-language-server"
        );
        let brew = InstallOption {
            tool: "brew".into(),
            command: vec!["brew".into(), "install".into(), "x".into()],
            url: None,
            platforms: vec![],
        };
        assert_eq!(display_command(&brew, &d), "brew install x");
    }

    #[test]
    fn only_runnable_options_with_an_installed_tool_resolve() {
        let tmp = tempfile::tempdir().unwrap();
        crate::detect::tests::fake_bin(&tmp.path().join("npm"));
        let d = Detector::new(DetectEnv {
            path: Some(tmp.path().as_os_str().to_owned()),
            ..Default::default()
        });
        let npm = InstallOption {
            tool: "npm".into(),
            command: vec!["npm".into(), "install".into(), "-g".into(), "x".into()],
            url: None,
            platforms: vec![],
        };
        let (program, args) = resolve_command(&npm, &d).unwrap();
        assert_eq!(program, tmp.path().join("npm"));
        assert_eq!(args, vec!["install", "-g", "x"]);

        let pipx = InstallOption {
            tool: "pipx".into(),
            command: vec!["pipx".into()],
            ..npm.clone()
        };
        assert!(resolve_command(&pipx, &d)
            .unwrap_err()
            .contains("pipx was not found"));
        let pacman = InstallOption {
            tool: "pacman".into(),
            command: vec!["sudo".into(), "pacman".into()],
            ..npm
        };
        assert!(resolve_command(&pacman, &d)
            .unwrap_err()
            .contains("terminal"));
    }
}

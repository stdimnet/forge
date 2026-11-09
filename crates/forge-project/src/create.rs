//! Running a [`Plan`]: official CLIs with their output streamed line by line, built-in
//! templates and `git init`. Programs run without a shell and with stdin closed, so
//! interactive prompts cannot hang the creation.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use serde::Serialize;

use crate::generators::{plan, CreateRequest, Plan, Step};
use crate::templates::{render, write_template};

/// Progress of a project creation, emitted as `project://create`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CreateEvent {
    /// A step starts (`index` is 0-based).
    #[serde(rename_all = "camelCase")]
    Step {
        index: usize,
        total: usize,
        label: String,
    },
    /// One line of tool output.
    #[serde(rename_all = "camelCase")]
    Output { line: String, stderr: bool },
    /// The project is ready at `root`.
    #[serde(rename_all = "camelCase")]
    Done { root: String, name: String },
    #[serde(rename_all = "camelCase")]
    Failed { error: String },
}

/// Full path of an executable found on `PATH` (honoring `PATHEXT` on Windows); in Flatpak
/// on the host's `PATH`.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let path = forge_hostspawn::path_var()?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .chain(std::iter::once(String::new()))
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        forge_hostspawn::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        forge_hostspawn::is_file(p)
    }
}

pub fn has_tool(name: &str) -> bool {
    find_executable(name).is_some()
}

/// Plan a request against the tools installed on this machine.
pub fn plan_for_system(req: &CreateRequest) -> Result<Plan> {
    plan(req, &has_tool)
}

/// Create the project. Emits progress through `emit`, ending with `Done` or `Failed`.
/// Setting `cancel` stops the running program.
pub fn create(
    req: &CreateRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(CreateEvent),
) -> Result<PathBuf> {
    let result = run_plan(req, cancel, emit);
    match &result {
        Ok(root) => emit(CreateEvent::Done {
            root: root.display().to_string(),
            name: req.name.trim().to_string(),
        }),
        Err(e) => emit(CreateEvent::Failed {
            error: e.to_string(),
        }),
    }
    result
}

fn run_plan(
    req: &CreateRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(CreateEvent),
) -> Result<PathBuf> {
    let plan = plan_for_system(req)?;
    if let Some(error) = plan.error {
        return Err(ForgeError::Other(error));
    }
    if !plan.missing_tools.is_empty() {
        return Err(ForgeError::Other(format!(
            "Required tools are not installed: {}",
            plan.missing_tools.join(", ")
        )));
    }
    let location = plan.target.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(location).map_err(|e| ForgeError::io(location, e))?;
    let name = req.name.trim();
    let total = plan.steps.len();
    for (index, step) in plan.steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ForgeError::Other("Cancelled".into()));
        }
        emit(CreateEvent::Step {
            index,
            total,
            label: plan.preview[index].clone(),
        });
        match step {
            Step::Run { program, args, cwd } => run_program(program, args, cwd, cancel, emit)?,
            Step::Template {
                template,
                options,
                sample,
            } => {
                let rendered = render(template, name, options, *sample)?;
                for rel in write_template(&plan.target, &rendered)? {
                    emit(CreateEvent::Output {
                        line: format!("Created {}", rel.to_string_lossy().replace('\\', "/")),
                        stderr: false,
                    });
                }
            }
            Step::GitInit => run_program("git", &["init".to_string()], &plan.target, cancel, emit)?,
        }
    }
    if !plan.target.is_dir() {
        return Err(ForgeError::Other(format!(
            "The generator finished but {} was not created",
            plan.target.display()
        )));
    }
    Ok(plan.target)
}

fn run_program(
    program: &str,
    args: &[String],
    cwd: &Path,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(CreateEvent),
) -> Result<()> {
    let exe = find_executable(program).unwrap_or_else(|| PathBuf::from(program));
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .current_dir(cwd)
        // Never wait for answers: accept npx installs, no composer/npm prompts, no colors.
        .env("npm_config_yes", "true")
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    // In Flatpak composer / npm / laravel run on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| ForgeError::Other(format!("cannot run {program}: {e}")))?;

    let (tx, rx) = mpsc::channel::<(String, bool)>();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(i, pipe)| {
        let pipe = pipe?;
        let tx = tx.clone();
        Some(std::thread::spawn(move || {
            for line in BufReader::new(pipe).split(b'\n').map_while(|l| l.ok()) {
                let text = String::from_utf8_lossy(&line).to_string();
                // Progress bars redraw with `\r`; keep the last state of the line.
                let text = text
                    .rsplit('\r')
                    .find(|s| !s.trim().is_empty())
                    .unwrap_or("");
                if tx.send((strip_ansi(text), i == 1)).is_err() {
                    break;
                }
            }
        }))
    })
    .collect();
    drop(tx);

    let mut cancelled = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((line, stderr)) => emit(CreateEvent::Output { line, stderr }),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !cancelled && cancel.load(Ordering::Relaxed) {
            cancelled = true;
            let _ = child.kill();
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child
        .wait()
        .map_err(|e| ForgeError::Other(format!("{program} failed: {e}")))?;
    if cancelled {
        return Err(ForgeError::Other("Cancelled".into()));
    }
    if !status.success() {
        return Err(ForgeError::Other(format!(
            "{program} exited with {}",
            status
                .code()
                .map(|c| format!("code {c}"))
                .unwrap_or_else(|| "a signal".into())
        )));
    }
    Ok(())
}

/// Remove ANSI escape sequences (colors, cursor movement) from a line of tool output.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                // CSI: parameters, then one final byte in @..~
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: until BEL or ESC \
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(
        dir: &Path,
        generator: &str,
        options: &[(&str, serde_json::Value)],
    ) -> CreateRequest {
        CreateRequest {
            generator: generator.into(),
            location: dir.display().to_string(),
            name: "hello-world".into(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    #[test]
    fn creates_template_project_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut events = Vec::new();
        let root = create(
            &request(dir.path(), "empty-php", &[]),
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        )
        .unwrap();
        assert_eq!(root, dir.path().join("hello-world"));
        assert!(root.join("src/Greeter.php").is_file());
        assert!(matches!(
            events[0],
            CreateEvent::Step {
                index: 0,
                total: 1,
                ..
            }
        ));
        assert!(events.contains(&CreateEvent::Output {
            line: "Created composer.json".into(),
            stderr: false
        }));
        assert!(matches!(events.last(), Some(CreateEvent::Done { .. })));
        let detected = crate::detect(&root);
        assert_eq!(detected.primary.unwrap().version.as_deref(), Some("8.3"));
    }

    #[test]
    fn refuses_non_empty_target() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("hello-world")).unwrap();
        std::fs::write(dir.path().join("hello-world/keep.txt"), "").unwrap();
        let mut events = Vec::new();
        let err = create(
            &request(dir.path(), "empty", &[]),
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        )
        .unwrap_err();
        assert!(err.to_string().contains("not empty"));
        assert!(matches!(events.last(), Some(CreateEvent::Failed { .. })));
    }

    #[test]
    fn git_init_when_git_is_available() {
        if !has_tool("git") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = create(
            &request(dir.path(), "love-game", &[("git", json!(true))]),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert!(root.join(".git").is_dir());
        assert!(root.join("conf.lua").is_file());
        assert_eq!(crate::detect(&root).primary.unwrap().id, "love");
    }

    #[test]
    fn streams_program_output() {
        if !has_tool("sh") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut lines = Vec::new();
        run_program(
            "sh",
            &[
                "-c".into(),
                "echo one; echo two >&2; printf 'a\\rb\\n'".into(),
            ],
            dir.path(),
            &AtomicBool::new(false),
            &mut |e| {
                if let CreateEvent::Output { line, stderr } = e {
                    lines.push((line, stderr));
                }
            },
        )
        .unwrap();
        lines.sort();
        assert_eq!(
            lines,
            vec![
                ("b".into(), false),
                ("one".into(), false),
                ("two".into(), true)
            ]
        );
        let err = run_program(
            "sh",
            &["-c".into(), "exit 3".into()],
            dir.path(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("code 3"));
    }

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(
            strip_ansi("\u{1b}[32mInstalling\u{1b}[0m laravel"),
            "Installing laravel"
        );
        assert_eq!(
            strip_ansi("\u{1b}]8;;http://x\u{7}link\u{1b}]8;;\u{7}"),
            "link"
        );
    }
}

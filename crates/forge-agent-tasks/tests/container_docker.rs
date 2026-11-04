//! The container sandbox against a real Docker daemon: only the sandbox is mounted, files
//! written in the container belong to the user, there is no network and the container is
//! gone after `remove`. Skipped (passes with a note) when Docker is not available. The
//! image is `FORGE_TEST_IMAGE` (default `alpine:3`) and must be present or pullable.

use std::collections::BTreeMap;
use std::process::Command;

use forge_agent_tasks::commands::ShellCommand;
use forge_agent_tasks::container::{container_name, ContainerSpec};

fn run(cmd: &ShellCommand) -> (bool, String) {
    match Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .output()
    {
        Ok(o) => (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        ),
        Err(e) => (false, e.to_string()),
    }
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn id(flag: &str) -> String {
    let o = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn runs_commands_in_a_container_with_only_the_sandbox() {
    if !docker_available() {
        eprintln!("skipped: Docker is not available");
        return;
    }
    let image = std::env::var("FORGE_TEST_IMAGE").unwrap_or_else(|_| "alpine:3".into());
    let sandbox = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(sandbox.path().join("src")).unwrap();
    std::fs::write(sandbox.path().join("src/a.txt"), "hello\n").unwrap();
    let task = format!("it-{}", std::process::id());
    let spec = ContainerSpec {
        engine: "docker".into(),
        name: container_name(&task),
        task_id: task,
        image,
        sandbox: sandbox.path().to_string_lossy().into_owned(),
        network: false,
        cpus: Some("1".into()),
        memory: Some("256m".into()),
        user: Some(format!("{}:{}", id("-u"), id("-g"))),
        env: BTreeMap::from([("FORGE_IT".into(), "1".into())]),
    };
    let (ok, out) = run(&spec.start().unwrap());
    if !ok && (out.contains("pull") || out.contains("Unable to find image")) {
        eprintln!("skipped: the test image is not available: {out}");
        return;
    }
    assert!(ok, "docker run: {out}");
    let exec = |program: &str, args: &[&str], cwd: &std::path::Path| {
        run(&spec
            .exec(&ShellCommand {
                program: program.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
                cwd: cwd.to_path_buf(),
                env: BTreeMap::new(),
            })
            .unwrap())
    };
    let result = std::panic::catch_unwind(|| {
        // The working folder and paths are mapped into /workspace.
        let (ok, out) = exec("cat", &["a.txt"], &sandbox.path().join("src"));
        assert!(ok && out == "hello\n", "{out}");
        let (ok, out) = exec("pwd", &[], &sandbox.path().join("src"));
        assert!(ok && out.trim() == "/workspace/src", "{out}");
        // Writes land in the sandbox, owned by the user.
        let (ok, out) = exec("sh", &["-c", "echo made > made.txt"], sandbox.path());
        assert!(ok, "{out}");
        assert_eq!(
            std::fs::read_to_string(sandbox.path().join("made.txt")).unwrap(),
            "made\n"
        );
        // Only the sandbox is mounted.
        let (_, mounts) = exec("cat", &["/proc/mounts"], sandbox.path());
        assert_eq!(mounts.matches("/workspace").count(), 1, "{mounts}");
        // No network: only the loopback interface.
        let (_, ifaces) = exec("ls", &["/sys/class/net"], sandbox.path());
        assert_eq!(ifaces.split_whitespace().collect::<Vec<_>>(), ["lo"]);
    });
    let (ok, out) = run(&spec.remove());
    assert!(ok, "docker rm: {out}");
    let (exists, _) = run(&ShellCommand {
        program: "docker".into(),
        args: vec!["container".into(), "inspect".into(), spec.name.clone()],
        cwd: sandbox.path().to_path_buf(),
        env: BTreeMap::new(),
    });
    assert!(!exists, "the container is gone");
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

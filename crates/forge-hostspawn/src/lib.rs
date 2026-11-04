//! Running host tools from inside a Flatpak sandbox.
//!
//! In the Flatpak build, Forge runs in a sandbox whose `/usr` is the GNOME runtime: PHP,
//! Composer, Node, language servers, Git, Docker and the user's shell are installed on the
//! host, not in the sandbox. Every process that targets such a tool is therefore started as
//! `flatpak-spawn --host --watch-bus … <program> <args>` (the manifest grants
//! `--talk-name=org.freedesktop.Flatpak` for this). Outside Flatpak everything here is a no-op.
//!
//! - [`in_flatpak`] detects the sandbox (`/.flatpak-info` or `FLATPAK_ID`).
//! - [`set_enabled`] follows the setting `system.flatpak.runOnHost` ("Run tools on the host").
//! - [`host_command`] rewrites a configured [`std::process::Command`]; [`rewrite`] is the pure
//!   core over a [`Spec`]. Call them *before* setting stdio, process groups or Tokio options,
//!   which a rewrite cannot carry over.
//! - [`pty_spec`] wraps a terminal shell so that it gets the pseudo terminal as its
//!   controlling terminal on the host (job control, Ctrl+C, resize).
//! - [`path_var`], [`which`], [`is_file`] and [`sandbox_view`] look programs up on the host:
//!   the host's `PATH`, and host system folders through `/run/host` (`--filesystem=host-os:ro`).
//!
//! Programs that are part of Forge itself (the built-in PHP language server, project
//! windows) keep running inside the sandbox: their callers simply do not rewrite them.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The program that starts a host process from inside the sandbox.
pub const FLATPAK_SPAWN: &str = "flatpak-spawn";

/// The setting "Run tools on the host (Flatpak)".
pub const SETTING_KEY: &str = "system.flatpak.runOnHost";

/// Where the host's operating system files appear in the sandbox with `host-os` access.
pub const HOST_ROOT: &str = "/run/host";

/// Host folders that the sandbox replaces with its own (the runtime) and that `host-os`
/// access exposes under [`HOST_ROOT`].
const HOST_OS_DIRS: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc"];

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether a process with these traits runs in a Flatpak sandbox: Flatpak creates
/// `/.flatpak-info` in every sandbox and sets `FLATPAK_ID` for the app.
pub fn detect(flatpak_info_exists: bool, flatpak_id: Option<&str>) -> bool {
    flatpak_info_exists || flatpak_id.is_some_and(|id| !id.trim().is_empty())
}

/// Whether Forge runs inside a Flatpak sandbox (detected once).
pub fn in_flatpak() -> bool {
    static FLATPAK: OnceLock<bool> = OnceLock::new();
    *FLATPAK.get_or_init(|| {
        detect(
            Path::new("/.flatpak-info").exists(),
            std::env::var("FLATPAK_ID").ok().as_deref(),
        )
    })
}

/// The Flatpak application id (`dev.forge.ide`), when there is one.
pub fn flatpak_id() -> Option<String> {
    std::env::var("FLATPAK_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
}

/// Follow the setting [`SETTING_KEY`]. Off makes Forge run tools inside the sandbox (only
/// useful with a custom runtime that ships them).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
}

/// The value last given to [`set_enabled`] (default on).
pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Whether host tools are started through `flatpak-spawn --host` right now.
pub fn active() -> bool {
    in_flatpak() && enabled()
}

/// What the About dialog and Settings show about the sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub in_flatpak: bool,
    pub flatpak_id: Option<String>,
    /// The setting [`SETTING_KEY`].
    pub enabled: bool,
    /// Host tools run through `flatpak-spawn --host`.
    pub active: bool,
    /// The host's operating system files are visible under [`HOST_ROOT`].
    pub host_os_visible: bool,
}

pub fn status() -> Status {
    Status {
        in_flatpak: in_flatpak(),
        flatpak_id: flatpak_id(),
        enabled: enabled(),
        active: active(),
        host_os_visible: in_flatpak() && Path::new(HOST_ROOT).join("usr").is_dir(),
    }
}

/// A command line as data: what a [`Command`] was configured with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec {
    pub program: OsString,
    pub args: Vec<OsString>,
    /// Variables set (`Some`) or removed (`None`) on top of the inherited environment.
    pub env: Vec<(OsString, Option<OsString>)>,
    pub cwd: Option<PathBuf>,
}

impl Spec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }

    /// The program, arguments, environment changes and working directory of `cmd`.
    pub fn from_command(cmd: &Command) -> Self {
        Self {
            program: cmd.get_program().to_os_string(),
            args: cmd.get_args().map(OsStr::to_os_string).collect(),
            env: cmd
                .get_envs()
                .map(|(k, v)| (k.to_os_string(), v.map(OsStr::to_os_string)))
                .collect(),
            cwd: cmd.get_current_dir().map(Path::to_path_buf),
        }
    }

    /// A fresh [`Command`] with this spec (no stdio or other options set).
    pub fn into_command(self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        cmd
    }
}

/// The host path for a path seen in the sandbox: `/run/host/usr/bin/php` is `/usr/bin/php`
/// on the host; everything else (the home folder, `/media`, `/mnt`) is the same on both.
pub fn host_path(path: &Path) -> PathBuf {
    match path.strip_prefix(HOST_ROOT) {
        Ok(rest) if !rest.as_os_str().is_empty() => Path::new("/").join(rest),
        _ => path.to_path_buf(),
    }
}

/// A `PATH` value for the host: sandbox-only entries (`/app/…`) are dropped and
/// `/run/host/…` entries become host paths.
pub fn host_path_list(value: &OsStr) -> OsString {
    let dirs: Vec<PathBuf> = std::env::split_paths(value)
        .filter(|d| !d.starts_with("/app"))
        .map(|d| host_path(&d))
        .collect();
    std::env::join_paths(dirs).unwrap_or_else(|_| value.to_os_string())
}

/// `spec` as a `flatpak-spawn --host` command line (pure). The environment changes and the
/// working directory travel as `--env=`, `--unset-env=` and `--directory=`, because the host
/// process does not inherit the sandbox's environment; `--watch-bus` ends the host process
/// when Forge (and with it `flatpak-spawn`) goes away. Stdin, stdout and stderr are passed
/// through by `flatpak-spawn`, and its signals (Stop, Ctrl+C) are forwarded to the host
/// process.
pub fn rewrite(spec: &Spec) -> Spec {
    let mut args: Vec<OsString> = vec!["--host".into(), "--watch-bus".into()];
    if let Some(dir) = &spec.cwd {
        let mut a = OsString::from("--directory=");
        a.push(host_path(dir));
        args.push(a);
    }
    // The last change of a variable wins, as with `Command`.
    let mut env: BTreeMap<&OsStr, Option<&OsStr>> = BTreeMap::new();
    for (k, v) in &spec.env {
        env.insert(k, v.as_deref());
    }
    for (k, v) in env {
        match v {
            Some(v) => {
                let mut a = OsString::from("--env=");
                a.push(k);
                a.push("=");
                if k == "PATH" {
                    a.push(host_path_list(v));
                } else {
                    a.push(v);
                }
                args.push(a);
            }
            None => {
                let mut a = OsString::from("--unset-env=");
                a.push(k);
                args.push(a);
            }
        }
    }
    // `flatpak-spawn` stops parsing options at the program, so the program's own arguments
    // are passed on unchanged (no `--` needed, and older versions would pass it on).
    args.push(host_path(Path::new(&spec.program)).into_os_string());
    args.extend(spec.args.iter().cloned());
    Spec {
        program: FLATPAK_SPAWN.into(),
        args,
        env: Vec::new(),
        cwd: None,
    }
}

/// `cmd` unchanged outside Flatpak; inside it the `flatpak-spawn --host` command that runs
/// it on the host. Call it after program, arguments, environment and working directory are
/// set and before stdio and other options, which are not carried over.
pub fn host_command(cmd: Command) -> Command {
    if active() {
        rewrite(&Spec::from_command(&cmd)).into_command()
    } else {
        cmd
    }
}

/// [`host_command`] for a [`Spec`].
pub fn host_spec(spec: Spec) -> Spec {
    if active() {
        rewrite(&spec)
    } else {
        spec
    }
}

/// Runs a terminal shell (`"$@"`) in a new session with the pseudo terminal on stdin as its
/// controlling terminal, like `host-spawn` does: `flatpak-spawn` passes the terminal through
/// as stdin/stdout/stderr, but the host process would otherwise have no controlling terminal
/// (no job control; Ctrl+C and resizes would not reach it). Hang-up and termination signals
/// that `flatpak-spawn` forwards (closing the terminal tab) go to the shell's process group.
/// Without `setsid` on the host the shell simply runs without job control.
pub const PTY_WRAPPER: &str = r#"command -v setsid >/dev/null 2>&1 || exec "$@"
exec 3<&0
setsid -c "$@" <&3 3<&- &
pid=$!
exec 3<&-
trap 'kill -HUP -"$pid" 2>/dev/null' HUP
trap 'kill -TERM -"$pid" 2>/dev/null' TERM
status=0
while :; do
  wait "$pid"
  status=$?
  kill -0 "$pid" 2>/dev/null || break
done
exit "$status""#;

/// The command line of a terminal shell for the host: `sh -c` [`PTY_WRAPPER`] around
/// `spec`, then [`rewrite`]. The sandbox side must not make the terminal its controlling
/// terminal (`CommandBuilder::set_controlling_tty(false)`), so the host shell can.
pub fn pty_spec(spec: &Spec) -> Spec {
    let mut args: Vec<OsString> = vec![
        "-c".into(),
        PTY_WRAPPER.into(),
        "forge-terminal".into(),
        host_path(Path::new(&spec.program)).into_os_string(),
    ];
    args.extend(spec.args.iter().cloned());
    rewrite(&Spec {
        program: "/bin/sh".into(),
        args,
        env: spec.env.clone(),
        cwd: spec.cwd.clone(),
    })
}

/// Where a host path can be read from inside the sandbox: `/usr/bin/php` is
/// `/run/host/usr/bin/php` when the host's system folders are visible; other paths are
/// shared as they are. Outside Flatpak (or with the setting off) the path itself.
pub fn sandbox_view(path: &Path) -> PathBuf {
    if !active() {
        return path.to_path_buf();
    }
    sandbox_view_in(path, Path::new(HOST_ROOT))
}

/// Testable core of [`sandbox_view`].
pub fn sandbox_view_in(path: &Path, host_root: &Path) -> PathBuf {
    if path.starts_with(host_root) {
        return path.to_path_buf();
    }
    let system = HOST_OS_DIRS.iter().any(|d| path.starts_with(d));
    if system && host_root.join("usr").is_dir() {
        let rel = path.strip_prefix("/").unwrap_or(path);
        return host_root.join(rel);
    }
    path.to_path_buf()
}

/// Whether `path` is a file on the host.
pub fn is_file(path: &Path) -> bool {
    sandbox_view(path).is_file()
}

/// Whether `path` is a folder on the host.
pub fn is_dir(path: &Path) -> bool {
    sandbox_view(path).is_dir()
}

/// The host's environment as a login shell sees it (`sh -lc env`), read once through
/// `flatpak-spawn --host`. Empty when that fails or outside Flatpak.
pub fn host_env() -> &'static BTreeMap<String, String> {
    static ENV: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        if !active() {
            return BTreeMap::new();
        }
        let spec = rewrite(&Spec {
            program: "/bin/sh".into(),
            args: vec!["-lc".into(), format!("echo {ENV_MARKER}; exec env -0").into()],
            ..Spec::default()
        });
        let out = spec
            .into_command()
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() => parse_env(&String::from_utf8_lossy(&o.stdout)),
            _ => BTreeMap::new(),
        }
    })
}

const ENV_MARKER: &str = "__FORGE_HOST_ENV__";

/// Parse `env -0` output after [`ENV_MARKER`] (anything a profile script printed before it
/// is skipped).
pub fn parse_env(out: &str) -> BTreeMap<String, String> {
    let body = match out.rfind(ENV_MARKER) {
        Some(i) => out[i + ENV_MARKER.len()..].trim_start_matches(['\n', '\r']),
        None => out,
    };
    body.split('\0')
        .filter_map(|entry| {
            let (k, v) = entry.split_once('=')?;
            let valid = !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            valid.then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// The `PATH` that tools are looked up in: the host's in Flatpak (falling back to the usual
/// system folders), else this process's.
pub fn path_var() -> Option<OsString> {
    if active() {
        let path = host_env()
            .get("PATH")
            .cloned()
            .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
        return Some(path.into());
    }
    std::env::var_os("PATH")
}

/// A host environment variable (`HOME`, `JAVA_HOME`, …): from the host in Flatpak, else
/// from this process.
pub fn env_var(key: &str) -> Option<String> {
    if active() {
        return host_env().get(key).cloned();
    }
    std::env::var(key).ok()
}

/// Resolve a program name on the host's `PATH` (a path is checked as it is). The result is a
/// host path, ready to be started through [`host_command`].
pub fn which(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 || p.is_absolute() {
        return is_file(p).then(|| p.to_path_buf());
    }
    let path = path_var()?;
    let exts: Vec<String> = if cfg!(windows) && p.extension().is_none() {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter().find_map(|ext| {
            let candidate = dir.join(format!("{program}{ext}"));
            is_file(&candidate).then_some(candidate)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[OsString]) -> Vec<String> {
        v.iter().map(|s| s.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn detects_flatpak_from_info_file_or_app_id() {
        assert!(detect(true, None));
        assert!(detect(false, Some("dev.forge.ide")));
        assert!(!detect(false, Some("  ")));
        assert!(!detect(false, None));
    }

    #[test]
    fn rewrites_program_args_env_and_cwd() {
        let spec = Spec {
            program: "php".into(),
            args: vec!["artisan".into(), "--env=testing".into(), "a b".into()],
            env: vec![
                ("XDEBUG_MODE".into(), Some("debug".into())),
                ("TMUX".into(), None),
                ("XDEBUG_MODE".into(), Some("coverage".into())),
            ],
            cwd: Some("/home/me/app".into()),
        };
        let r = rewrite(&spec);
        assert_eq!(r.program, OsString::from("flatpak-spawn"));
        assert_eq!(
            strs(&r.args),
            [
                "--host",
                "--watch-bus",
                "--directory=/home/me/app",
                "--unset-env=TMUX",
                "--env=XDEBUG_MODE=coverage",
                "php",
                "artisan",
                "--env=testing",
                "a b",
            ]
        );
        assert!(r.env.is_empty());
        assert_eq!(r.cwd, None);
    }

    #[test]
    fn rewrite_translates_sandbox_paths_to_host_paths() {
        let spec = Spec {
            program: "/run/host/usr/bin/php8.3".into(),
            env: vec![(
                "PATH".into(),
                Some("/app/bin:/run/host/usr/local/bin:/home/me/.local/bin:/usr/bin".into()),
            )],
            cwd: Some("/home/me/p".into()),
            ..Spec::default()
        };
        let r = rewrite(&spec);
        assert_eq!(
            strs(&r.args),
            [
                "--host",
                "--watch-bus",
                "--directory=/home/me/p",
                "--env=PATH=/usr/local/bin:/home/me/.local/bin:/usr/bin",
                "/usr/bin/php8.3",
            ]
        );
    }

    #[test]
    fn without_cwd_or_env_only_the_command_follows() {
        let r = rewrite(&Spec::new("git").arg("status"));
        assert_eq!(strs(&r.args), ["--host", "--watch-bus", "git", "status"]);
    }

    #[test]
    fn spec_round_trips_through_a_command() {
        let mut cmd = Command::new("node");
        cmd.arg("server.js")
            .env("A", "1")
            .env_remove("B")
            .current_dir("/tmp");
        let spec = Spec::from_command(&cmd);
        assert_eq!(spec.program, OsString::from("node"));
        assert_eq!(strs(&spec.args), ["server.js"]);
        assert!(spec
            .env
            .contains(&(OsString::from("A"), Some(OsString::from("1")))));
        assert!(spec.env.contains(&(OsString::from("B"), None)));
        assert_eq!(spec.cwd.as_deref(), Some(Path::new("/tmp")));
        let back = Spec::from_command(&spec.clone().into_command());
        assert_eq!(back.program, spec.program);
        assert_eq!(back.cwd, spec.cwd);
    }

    #[test]
    fn pty_spec_wraps_the_shell_for_a_controlling_terminal() {
        let spec = Spec {
            program: "/usr/bin/zsh".into(),
            args: vec!["-l".into()],
            env: vec![("TERM".into(), Some("xterm-256color".into()))],
            cwd: Some("/home/me".into()),
        };
        let r = pty_spec(&spec);
        let args = strs(&r.args);
        assert_eq!(r.program, OsString::from("flatpak-spawn"));
        assert_eq!(
            &args[..5],
            [
                "--host",
                "--watch-bus",
                "--directory=/home/me",
                "--env=TERM=xterm-256color",
                "/bin/sh"
            ]
        );
        assert_eq!(args[5], "-c");
        assert_eq!(args[6], PTY_WRAPPER);
        assert_eq!(&args[7..], ["forge-terminal", "/usr/bin/zsh", "-l"]);
    }

    #[test]
    fn host_paths_and_path_lists() {
        assert_eq!(
            host_path(Path::new("/run/host/usr/bin/git")),
            Path::new("/usr/bin/git")
        );
        assert_eq!(host_path(Path::new("/run/host")), Path::new("/run/host"));
        assert_eq!(
            host_path(Path::new("/home/me/bin/php")),
            Path::new("/home/me/bin/php")
        );
        assert_eq!(
            host_path_list(OsStr::new("/app/bin:/usr/bin:/run/host/bin")),
            OsString::from("/usr/bin:/bin")
        );
    }

    #[test]
    fn sandbox_view_maps_system_folders_when_host_os_is_visible() {
        let root = tempfile::tempdir().unwrap();
        // Not visible yet: paths stay as they are.
        assert_eq!(
            sandbox_view_in(Path::new("/usr/bin/php"), root.path()),
            Path::new("/usr/bin/php")
        );
        std::fs::create_dir_all(root.path().join("usr/bin")).unwrap();
        assert_eq!(
            sandbox_view_in(Path::new("/usr/bin/php"), root.path()),
            root.path().join("usr/bin/php")
        );
        assert_eq!(
            sandbox_view_in(Path::new("/etc/os-release"), root.path()),
            root.path().join("etc/os-release")
        );
        // Shared folders and already mapped paths are left alone.
        assert_eq!(
            sandbox_view_in(Path::new("/home/me/.phpenv"), root.path()),
            Path::new("/home/me/.phpenv")
        );
        assert_eq!(
            sandbox_view_in(Path::new("/usrlocal"), root.path()),
            Path::new("/usrlocal")
        );
        let mapped = root.path().join("usr/bin/node");
        assert_eq!(sandbox_view_in(&mapped, root.path()), mapped);
    }

    #[test]
    fn parses_env_output_after_the_marker() {
        let out = format!("Welcome!\n{ENV_MARKER}\nPATH=/usr/bin:/bin\0HOME=/home/me\0MULTI=a\nb\0=bad\0");
        let env = parse_env(&out);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/me"));
        assert_eq!(env.get("MULTI").map(String::as_str), Some("a\nb"));
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn outside_flatpak_everything_is_a_no_op() {
        if in_flatpak() {
            return;
        }
        assert!(!active());
        let mut cmd = Command::new("git");
        cmd.arg("status").current_dir("/tmp");
        let out = host_command(cmd);
        assert_eq!(out.get_program(), "git");
        assert_eq!(out.get_current_dir(), Some(Path::new("/tmp")));
        assert_eq!(
            sandbox_view(Path::new("/usr/bin/php")),
            Path::new("/usr/bin/php")
        );
        assert_eq!(path_var(), std::env::var_os("PATH"));
        assert_eq!(which("sh"), {
            std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|d| d.join("sh"))
                .find(|p| p.is_file())
        });
    }
}

//! Picking the user's default shell.

use std::path::Path;

/// The shell a new terminal starts: `$SHELL` on Unix (falling back to bash, then sh); on
/// Windows PowerShell 7 (`pwsh`) when installed, then Windows PowerShell, then `%ComSpec%`.
/// In Flatpak the host's `$SHELL` and files count, since the terminal runs on the host.
pub fn default_shell() -> String {
    default_shell_from(forge_hostspawn::env_var, |p| find_in_path(p).is_some())
}

/// Testable core of [`default_shell`].
pub fn default_shell_from(
    env: impl Fn(&str) -> Option<String>,
    exists: impl Fn(&str) -> bool,
) -> String {
    if cfg!(windows) {
        if exists("pwsh.exe") {
            return "pwsh.exe".into();
        }
        if exists("powershell.exe") {
            return "powershell.exe".into();
        }
        return env("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    }
    if let Some(shell) = env("SHELL").filter(|s| !s.trim().is_empty()) {
        return shell;
    }
    for candidate in ["/bin/bash", "/usr/bin/bash", "/bin/zsh"] {
        if exists(candidate) {
            return candidate.into();
        }
    }
    "/bin/sh".into()
}

/// Arguments that start `shell` as an interactive login shell where that is customary.
pub fn shell_args(shell: &str) -> Vec<String> {
    let file = shell.rsplit(['/', '\\']).next().unwrap_or(shell);
    let name = file
        .strip_suffix(".exe")
        .unwrap_or(file)
        .to_ascii_lowercase();
    match name.as_str() {
        "bash" | "zsh" | "fish" => vec!["-l".into()],
        "pwsh" | "powershell" => vec!["-NoLogo".into()],
        _ => Vec::new(),
    }
}

/// Resolve a program name or path to an existing file, searching `PATH` for bare names (the
/// host's `PATH` and files in Flatpak, see [`forge_hostspawn::which`]).
pub fn find_in_path(program: &str) -> Option<std::path::PathBuf> {
    if forge_hostspawn::active() {
        return forge_hostspawn::which(program);
    }
    let p = Path::new(program);
    if p.components().count() > 1 || p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|c| c.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn prefers_shell_env_on_unix() {
        let env = |k: &str| (k == "SHELL").then(|| "/usr/bin/fish".to_string());
        assert_eq!(default_shell_from(env, |_| true), "/usr/bin/fish");
        assert_eq!(
            default_shell_from(|_| None, |p| p == "/bin/zsh"),
            "/bin/zsh"
        );
        assert_eq!(default_shell_from(|_| None, |_| false), "/bin/sh");
    }

    #[cfg(windows)]
    #[test]
    fn prefers_pwsh_on_windows() {
        assert_eq!(
            default_shell_from(|_| None, |p| p == "pwsh.exe"),
            "pwsh.exe"
        );
        let env = |k: &str| (k == "ComSpec").then(|| r"C:\Windows\cmd.exe".to_string());
        assert_eq!(default_shell_from(env, |_| false), r"C:\Windows\cmd.exe");
    }

    #[test]
    fn login_args_for_known_shells() {
        assert_eq!(shell_args("/bin/zsh"), vec!["-l"]);
        assert_eq!(
            shell_args(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            vec!["-NoLogo"]
        );
        assert!(shell_args("/bin/sh").is_empty());
    }
}

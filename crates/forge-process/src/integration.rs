//! Shell integration (Settings | Tools | Terminal): bash and zsh report their working
//! directory (`OSC 7`) and command boundaries (`OSC 133` A = prompt, B = command line,
//! C = command started, D;<exit code> = command finished) to the terminal, like in
//! PHPStorm and VS Code. The user's own startup files still run: bash starts with an init
//! file that sources them, zsh with a `ZDOTDIR` whose files source the user's.

use std::path::{Path, PathBuf};

/// How to start a shell with integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integration {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

const BASH: &str = r#"# Forge shell integration for bash (OSC 7 working directory, OSC 133 command marks).
# Started with --init-file instead of the usual startup files, which are sourced here.
if [ -n "$FORGE_LOGIN_SHELL" ]; then
  unset FORGE_LOGIN_SHELL
  [ -r /etc/profile ] && . /etc/profile
  for __forge_f in ~/.bash_profile ~/.bash_login ~/.profile; do
    if [ -r "$__forge_f" ]; then . "$__forge_f"; break; fi
  done
  unset __forge_f
else
  [ -r ~/.bashrc ] && . ~/.bashrc
fi
__forge_osc() { printf '\033]%s\007' "$1"; }
__forge_ran=
__forge_armed=
# The first command after the prompt (not the prompt commands themselves) starts a command.
__forge_preexec() {
  [ -n "$COMP_LINE" ] && return
  [ -z "$__forge_armed" ] && return
  case "$BASH_COMMAND" in __forge_*) return ;; esac
  __forge_armed=
  __forge_ran=1
  __forge_osc "133;C"
}
__forge_prompt() {
  local status=$?
  __forge_armed=
  if [ -n "$__forge_ran" ]; then __forge_osc "133;D;$status"; fi
  __forge_ran=
  __forge_osc "7;file://${HOSTNAME}${PWD}"
  __forge_osc "133;A"
  return $status
}
__forge_arm() { __forge_armed=1; }
PROMPT_COMMAND="__forge_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND};__forge_arm"
PS1="${PS1}\[\033]133;B\007\]"
trap '__forge_preexec' DEBUG
"#;

/// `.zshenv`, `.zprofile` and `.zlogin` of the integration `ZDOTDIR`: source the user's file.
fn zsh_shim(name: &str) -> String {
    format!(
        "# Forge shell integration: the user's {name}.\n\
         __forge_zd=$ZDOTDIR\n\
         ZDOTDIR=${{FORGE_USER_ZDOTDIR:-$HOME}}\n\
         [ -r \"$ZDOTDIR/{name}\" ] && . \"$ZDOTDIR/{name}\"\n\
         FORGE_USER_ZDOTDIR=$ZDOTDIR\n\
         ZDOTDIR=$__forge_zd\n\
         unset __forge_zd\n"
    )
}

const ZSHRC: &str = r#"# Forge shell integration for zsh (OSC 7 working directory, OSC 133 command marks).
ZDOTDIR=${FORGE_USER_ZDOTDIR:-$HOME}
[ -r "$ZDOTDIR/.zshrc" ] && . "$ZDOTDIR/.zshrc"
__forge_osc() { printf '\033]%s\007' "$1"; }
__forge_ran=
__forge_precmd() {
  local status=$?
  if [ -n "$__forge_ran" ]; then __forge_osc "133;D;$status"; fi
  __forge_ran=
  __forge_osc "7;file://${HOST}${PWD}"
  __forge_osc "133;A"
}
__forge_preexec() { __forge_ran=1; __forge_osc "133;C"; }
autoload -Uz add-zsh-hook
add-zsh-hook precmd __forge_precmd
add-zsh-hook preexec __forge_preexec
PROMPT="${PROMPT}%{$(printf '\033]133;B\007')%}"
"#;

fn shell_name(shell: &str) -> String {
    let file = shell.rsplit(['/', '\\']).next().unwrap_or(shell);
    file.strip_suffix(".exe")
        .unwrap_or(file)
        .to_ascii_lowercase()
}

/// Write `content` unless the file already has it.
fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Prepare the integration of `shell` (started with its default arguments `default_args`)
/// in `dir`. `None` for shells without integration (fish, sh, PowerShell, cmd).
/// `user_zdotdir` is the user's `ZDOTDIR` (or home) for zsh.
pub fn prepare(
    shell: &str,
    default_args: &[String],
    dir: &Path,
    user_zdotdir: Option<PathBuf>,
) -> std::io::Result<Option<Integration>> {
    match shell_name(shell).as_str() {
        "bash" => {
            let rc = dir.join("bash").join("forge.bash");
            write_if_changed(&rc, BASH)?;
            let login = default_args.iter().any(|a| a == "-l" || a == "--login");
            Ok(Some(Integration {
                args: vec!["--init-file".into(), rc.display().to_string()],
                env: if login {
                    vec![("FORGE_LOGIN_SHELL".into(), "1".into())]
                } else {
                    Vec::new()
                },
            }))
        }
        "zsh" => {
            let zdotdir = dir.join("zsh");
            for name in [".zshenv", ".zprofile", ".zlogin"] {
                write_if_changed(&zdotdir.join(name), &zsh_shim(name))?;
            }
            write_if_changed(&zdotdir.join(".zshrc"), ZSHRC)?;
            let mut env = vec![("ZDOTDIR".into(), zdotdir.display().to_string())];
            if let Some(user) = user_zdotdir {
                env.push(("FORGE_USER_ZDOTDIR".into(), user.display().to_string()));
            }
            Ok(Some(Integration {
                args: default_args.to_vec(),
                env,
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_starts_with_an_init_file_that_sources_the_users_files() {
        let dir = tempfile::tempdir().unwrap();
        let i = prepare("/bin/bash", &["-l".into()], dir.path(), None)
            .unwrap()
            .unwrap();
        assert_eq!(i.args[0], "--init-file");
        let rc = std::fs::read_to_string(&i.args[1]).unwrap();
        assert!(
            rc.contains(". ~/.bashrc") && rc.contains("133;D;$status") && rc.contains("7;file://")
        );
        assert_eq!(
            i.env,
            vec![("FORGE_LOGIN_SHELL".to_string(), "1".to_string())]
        );
        // Unchanged files are not written again.
        let before = std::fs::metadata(&i.args[1]).unwrap().modified().unwrap();
        prepare("bash", &[], dir.path(), None).unwrap();
        assert_eq!(
            std::fs::metadata(&i.args[1]).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn zsh_uses_a_zdotdir_that_sources_the_users_files() {
        let dir = tempfile::tempdir().unwrap();
        let i = prepare("zsh", &["-l".into()], dir.path(), Some("/home/me".into()))
            .unwrap()
            .unwrap();
        assert_eq!(i.args, vec!["-l"]);
        let zdotdir = &i.env[0].1;
        assert!(std::fs::read_to_string(Path::new(zdotdir).join(".zshrc"))
            .unwrap()
            .contains("add-zsh-hook precmd __forge_precmd"));
        assert!(std::fs::read_to_string(Path::new(zdotdir).join(".zshenv"))
            .unwrap()
            .contains("$ZDOTDIR/.zshenv"));
        assert_eq!(i.env[1], ("FORGE_USER_ZDOTDIR".into(), "/home/me".into()));
    }

    #[test]
    fn other_shells_have_no_integration() {
        let dir = tempfile::tempdir().unwrap();
        for shell in ["fish", "/bin/sh", "pwsh.exe", "cmd.exe"] {
            assert_eq!(prepare(shell, &[], dir.path(), None).unwrap(), None);
        }
    }
}

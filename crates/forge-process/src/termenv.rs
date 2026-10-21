//! The environment a terminal session starts with.
//!
//! TUI programs such as Claude Code, vim or htop pick their features from the environment:
//! `TERM` and `COLORTERM` for colors, `TERM_PROGRAM` for terminal specific behaviour and the
//! locale for UTF-8 box drawing and emoji. Variables that describe *another* terminal (for
//! example when Forge itself was started from kitty or inside tmux) are removed, otherwise
//! programs would send that terminal's private escape sequences to Forge.

/// Variables set for every terminal session, in addition to the inherited environment.
pub fn terminal_env(version: &str) -> Vec<(String, String)> {
    vec![
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        ("TERM_PROGRAM".into(), "Forge".into()),
        ("TERM_PROGRAM_VERSION".into(), version.into()),
        ("FORGE_TERMINAL".into(), "1".into()),
    ]
}

/// Inherited variables that belong to the terminal or multiplexer Forge was started from.
pub const FOREIGN_TERMINAL_VARS: &[&str] = &[
    "KITTY_WINDOW_ID",
    "KITTY_PID",
    "KITTY_PUBLIC_KEY",
    "KITTY_INSTALLATION_DIR",
    "KITTY_LISTEN_ON",
    "ITERM_SESSION_ID",
    "ITERM_PROFILE",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    "TERM_SESSION_ID",
    "WT_SESSION",
    "WT_PROFILE_ID",
    "VTE_VERSION",
    "KONSOLE_VERSION",
    "KONSOLE_DBUS_SESSION",
    "KONSOLE_DBUS_SERVICE",
    "KONSOLE_DBUS_WINDOW",
    "WEZTERM_EXECUTABLE",
    "WEZTERM_PANE",
    "WEZTERM_UNIX_SOCKET",
    "ALACRITTY_WINDOW_ID",
    "ALACRITTY_SOCKET",
    "ALACRITTY_LOG",
    "GHOSTTY_RESOURCES_DIR",
    "GHOSTTY_BIN_DIR",
    "GHOSTTY_SHELL_FEATURES",
    "TERMINAL_EMULATOR",
    "TMUX",
    "TMUX_PANE",
    "STY",
    "WINDOWID",
    "VSCODE_INJECTION",
    "VSCODE_GIT_IPC_HANDLE",
];

/// A UTF-8 locale to set as `LANG` when the inherited environment has none, e.g. when the
/// desktop app is started from the macOS Dock or a minimal desktop session. `None` when the
/// environment already selects a UTF-8 locale, or any locale through `LC_ALL`.
pub fn utf8_locale_fallback(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    let set = |k: &str| env(k).filter(|v| !v.trim().is_empty());
    if set("LC_ALL").is_some() {
        // LC_ALL overrides LANG, and setting it is an explicit choice.
        return None;
    }
    match set("LC_CTYPE").or_else(|| set("LANG")) {
        Some(v) if is_utf8(&v) => None,
        Some(v) if v != "C" && v != "POSIX" => {
            // e.g. "de_DE" or "de_DE.ISO-8859-1": keep the language, switch the encoding.
            let base = v.split(['.', '@']).next().unwrap_or("en_US");
            Some(format!("{base}.UTF-8"))
        }
        _ => Some(if cfg!(target_os = "macos") {
            "en_US.UTF-8".into()
        } else {
            "C.UTF-8".into()
        }),
    }
}

fn is_utf8(locale: &str) -> bool {
    let l = locale.to_ascii_lowercase();
    l.contains("utf-8") || l.contains("utf8")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn advertises_truecolor_and_forge() {
        let env = terminal_env("1.2.3");
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(get("COLORTERM"), Some("truecolor"));
        assert_eq!(get("TERM_PROGRAM"), Some("Forge"));
        assert_eq!(get("TERM_PROGRAM_VERSION"), Some("1.2.3"));
    }

    #[test]
    fn removes_variables_of_other_terminals() {
        for var in ["KITTY_WINDOW_ID", "TMUX", "WT_SESSION", "TERMINAL_EMULATOR"] {
            assert!(FOREIGN_TERMINAL_VARS.contains(&var), "{var}");
        }
        assert!(!FOREIGN_TERMINAL_VARS.contains(&"PATH"));
    }

    #[test]
    fn keeps_existing_utf8_locales() {
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LANG", "de_DE.UTF-8")])),
            None
        );
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LC_CTYPE", "en_GB.utf8")])),
            None
        );
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LC_ALL", "en_US.UTF-8")])),
            None
        );
    }

    #[test]
    fn falls_back_to_a_utf8_locale() {
        let default = if cfg!(target_os = "macos") {
            "en_US.UTF-8"
        } else {
            "C.UTF-8"
        };
        assert_eq!(utf8_locale_fallback(env_of(&[])).as_deref(), Some(default));
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LANG", "")])).as_deref(),
            Some(default)
        );
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LANG", "de_DE.ISO-8859-1")])).as_deref(),
            Some("de_DE.UTF-8")
        );
        // An explicit LC_ALL is the user's choice.
        assert_eq!(utf8_locale_fallback(env_of(&[("LC_ALL", "C")])), None);
        assert_eq!(
            utf8_locale_fallback(env_of(&[("LC_ALL", "de_DE.ISO-8859-1")])),
            None
        );
    }
}

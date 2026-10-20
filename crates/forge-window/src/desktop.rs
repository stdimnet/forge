//! Operating system and desktop environment detection.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
    Other,
}

impl Os {
    /// The OS Forge was compiled for.
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )) {
            Os::Linux
        } else {
            Os::Other
        }
    }
}

/// Desktop environment. Only the ones with their own title bar look are told apart; the rest
/// are `Other` (generic Linux).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Desktop {
    Windows,
    Macos,
    /// GNOME and GNOME-based shells using Adwaita/libadwaita title bars (Ubuntu, Pop!_OS,
    /// Budgie, Pantheon, Unity).
    Gnome,
    /// KDE Plasma (Breeze) and LXQt, which commonly runs KWin.
    Kde,
    Other,
}

/// Detect the desktop from the OS and the environment (`env` returns a variable's value).
///
/// Linux looks at `XDG_CURRENT_DESKTOP` (a `:`-separated list such as `ubuntu:GNOME`), then
/// `XDG_SESSION_DESKTOP` and `DESKTOP_SESSION`, then `KDE_FULL_SESSION` and
/// `GNOME_DESKTOP_SESSION_ID`.
pub fn detect_desktop(os: Os, env: impl Fn(&str) -> Option<String>) -> Desktop {
    match os {
        Os::Windows => return Desktop::Windows,
        Os::Macos => return Desktop::Macos,
        Os::Linux | Os::Other => {}
    }
    for var in [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ] {
        let Some(value) = env(var) else { continue };
        for part in value.split(':') {
            if let Some(d) = classify(part) {
                return d;
            }
        }
    }
    if env("KDE_FULL_SESSION").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        return Desktop::Kde;
    }
    if env("GNOME_DESKTOP_SESSION_ID").is_some() {
        return Desktop::Gnome;
    }
    Desktop::Other
}

/// One desktop name such as `GNOME`, `KDE`, `plasmawayland`, `ubuntu` or `X-Cinnamon`.
fn classify(name: &str) -> Option<Desktop> {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_prefix("x-").unwrap_or(&n);
    if n.is_empty() {
        return None;
    }
    if n == "kde" || n.starts_with("plasma") || n == "lxqt" || n.starts_with("kde-") {
        return Some(Desktop::Kde);
    }
    const GNOME_LIKE: [&str; 7] = [
        "gnome", "ubuntu", "pop", "budgie", "pantheon", "unity", "zorin",
    ];
    if GNOME_LIKE
        .iter()
        .any(|g| n == *g || n.starts_with(&format!("{g}-")))
    {
        return Some(Desktop::Gnome);
    }
    // Known desktops without an own title bar style: generic Linux.
    const OTHER: [&str; 9] = [
        "xfce",
        "mate",
        "cinnamon",
        "lxde",
        "deepin",
        "enlightenment",
        "sway",
        "hyprland",
        "i3",
    ];
    if OTHER.iter().any(|o| n == *o || n.starts_with(o)) {
        return Some(Desktop::Other);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn windows_and_macos_ignore_the_environment() {
        let e = env(&[("XDG_CURRENT_DESKTOP", "KDE")]);
        assert_eq!(detect_desktop(Os::Windows, &e), Desktop::Windows);
        assert_eq!(detect_desktop(Os::Macos, &e), Desktop::Macos);
    }

    #[test]
    fn reads_xdg_current_desktop_lists() {
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "GNOME")])),
            Desktop::Gnome
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")])),
            Desktop::Gnome
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "KDE")])),
            Desktop::Kde
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "pop:GNOME")])),
            Desktop::Gnome
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "Budgie:GNOME")])),
            Desktop::Gnome
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "X-Cinnamon")])),
            Desktop::Other
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "XFCE")])),
            Desktop::Other
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_CURRENT_DESKTOP", "LXQt")])),
            Desktop::Kde
        );
    }

    #[test]
    fn falls_back_to_the_session_name() {
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("DESKTOP_SESSION", "plasmawayland")])),
            Desktop::Kde
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("DESKTOP_SESSION", "plasma")])),
            Desktop::Kde
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("DESKTOP_SESSION", "gnome-xorg")])),
            Desktop::Gnome
        );
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("XDG_SESSION_DESKTOP", "ubuntu")])),
            Desktop::Gnome
        );
        // An unknown current desktop does not hide a known session.
        assert_eq!(
            detect_desktop(
                Os::Linux,
                env(&[
                    ("XDG_CURRENT_DESKTOP", "foo"),
                    ("DESKTOP_SESSION", "plasma")
                ])
            ),
            Desktop::Kde
        );
    }

    #[test]
    fn falls_back_to_legacy_variables() {
        assert_eq!(
            detect_desktop(Os::Linux, env(&[("KDE_FULL_SESSION", "true")])),
            Desktop::Kde
        );
        assert_eq!(
            detect_desktop(
                Os::Linux,
                env(&[("GNOME_DESKTOP_SESSION_ID", "this-is-deprecated")])
            ),
            Desktop::Gnome
        );
        assert_eq!(detect_desktop(Os::Linux, env(&[])), Desktop::Other);
        assert_eq!(detect_desktop(Os::Other, env(&[])), Desktop::Other);
    }
}

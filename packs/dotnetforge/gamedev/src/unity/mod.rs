//! Unity, like Rider's Unity support:
//!
//! - [`project`]: the project's editor version (`ProjectSettings/ProjectVersion.txt`), its
//!   packages (`Packages/manifest.json`), the IDE package that writes the solution, and the
//!   running editor (`Library/EditorInstance.json`, `Library/ForgeBridge.json`),
//! - [`editors`]: installed editors (Unity Hub folders per OS, its secondary install path
//!   and manually located editors) and the one matching the project,
//! - [`bridge`] / [`package`]: the Forge editor bridge (play / pause / stop, refresh,
//!   console logs, opening files from Unity),
//! - [`commands`]: command lines (open the editor, regenerate project files in batch mode),
//! - [`test_runner`]: the Unity Test Framework in batch mode with NUnit 3 results,
//! - [`processes`]: running editors and players and their Mono debugger ports.

pub mod bridge;
pub mod commands;
pub mod editors;
pub mod package;
pub mod processes;
pub mod project;
pub mod test_runner;

pub use editors::{find_editors, match_editor, EditorEnv, EditorMatch, UnityEditor};
pub use project::{read_project, UnityProject};

/// Unity versions (`2022.3.10f1`, `6000.0.23f1`, `2023.1.0b5`) as comparable keys:
/// major, minor, patch, release stage (`a` < `b` < `f` < `p`) and its number.
pub fn version_key(v: &str) -> Option<(u32, u32, u32, u8, u32)> {
    let mut parts = v.trim().splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let rest = parts.next().unwrap_or("0");
    let split = rest
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let patch = rest[..split].parse().ok()?;
    let tail = &rest[split..];
    let (stage, num) = match tail.chars().next() {
        None => (b'f', 0),
        Some(c) => {
            let stage = match c.to_ascii_lowercase() {
                'a' => b'a',
                'b' => b'b',
                'f' => b'f',
                'p' => b'p',
                'c' => b'c',
                _ => return None,
            };
            let digits: String = tail[1..].chars().take_while(char::is_ascii_digit).collect();
            (stage, digits.parse().unwrap_or(0))
        }
    };
    Some((major, minor, patch, stage, num))
}

#[cfg(test)]
mod version_tests {
    use super::version_key;

    #[test]
    fn versions_compare_like_unity() {
        let k = |v| version_key(v).unwrap();
        assert!(k("2022.3.10f1") > k("2022.3.9f1"));
        assert!(k("2023.1.0b5") > k("2023.1.0a20"));
        assert!(k("2023.1.0f1") > k("2023.1.0b5"));
        assert!(k("6000.0.23f1") > k("2022.3.50f1"));
        assert_eq!(k("2021.3"), (2021, 3, 0, b'f', 0));
        assert!(version_key("latest").is_none());
    }
}

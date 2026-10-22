//! Reveal a file in the system file manager (Select In… › Explorer / Finder / File Manager,
//! PHPStorm's `RevealIn`). The commands are only built here; the Tauri shell runs them.

use serde::Serialize;

use crate::desktop::Os;

/// A program to run with its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevealCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// The file manager's name in menus, like PHPStorm ("Explorer", "Finder", "File Manager").
pub fn file_manager_name(os: Os) -> &'static str {
    match os {
        Os::Windows => "Explorer",
        Os::Macos => "Finder",
        Os::Linux | Os::Other => "File Manager",
    }
}

/// Percent-encode an absolute path for a `file://` URI.
fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parent(path: &str) -> &str {
    match path.trim_end_matches(['/', '\\']).rfind(['/', '\\']) {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => path,
    }
}

/// The commands that show `path` (absolute) selected in the file manager, to try in order
/// until one starts: Explorer's `/select,`, `open -R` on macOS, and on Linux the freedesktop
/// `FileManager1.ShowItems` D-Bus call (Nautilus, Dolphin, Nemo, Thunar, …) with
/// `xdg-open` on the containing folder as a fallback.
pub fn reveal_commands(os: Os, path: &str) -> Vec<RevealCommand> {
    let cmd = |program: &str, args: &[&str]| RevealCommand {
        program: program.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    };
    match os {
        Os::Windows => vec![cmd("explorer", &[&format!("/select,{path}")])],
        Os::Macos => vec![cmd("open", &["-R", path])],
        Os::Linux | Os::Other => vec![
            cmd(
                "dbus-send",
                &[
                    "--session",
                    "--print-reply",
                    "--dest=org.freedesktop.FileManager1",
                    "--type=method_call",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &format!("array:string:{}", file_uri(path)),
                    "string:",
                ],
            ),
            cmd("xdg-open", &[parent(path)]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_per_os() {
        let win = reveal_commands(Os::Windows, r"C:\shop\app\User.php");
        assert_eq!(win[0].program, "explorer");
        assert_eq!(win[0].args, [r"/select,C:\shop\app\User.php"]);
        let mac = reveal_commands(Os::Macos, "/Users/me/shop/a b.php");
        assert_eq!(mac[0].args, ["-R", "/Users/me/shop/a b.php"]);
        let linux = reveal_commands(Os::Linux, "/home/me/shop/a b#.php");
        assert_eq!(linux.len(), 2);
        assert!(linux[0]
            .args
            .contains(&"array:string:file:///home/me/shop/a%20b%23.php".to_string()));
        assert_eq!(linux[1].args, ["/home/me/shop"]);
        assert_eq!(parent("/a"), "/");
        assert_eq!(file_manager_name(Os::Macos), "Finder");
        assert_eq!(file_manager_name(Os::Linux), "File Manager");
    }
}

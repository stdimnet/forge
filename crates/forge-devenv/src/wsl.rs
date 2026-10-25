//! Windows Subsystem for Linux: the distributions `wsl.exe -l -v` lists (its output is
//! UTF-16), `wsl.exe` command lines, and paths between the Windows view (`C:\…`,
//! `\\wsl$\<distro>\…`, `\\wsl.localhost\<distro>\…`) and the Linux view (`/mnt/c/…`,
//! `/home/…`). Pure code: it compiles and is tested on every platform; only Windows runs it.

use serde::{Deserialize, Serialize};

/// The WSL launcher.
pub const PROGRAM: &str = "wsl.exe";

/// A distribution of `wsl.exe -l -v`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distro {
    pub name: String,
    /// `Running`, `Stopped`, `Installing`, … (as printed, possibly localized).
    pub state: String,
    /// WSL version (1 or 2).
    pub version: u8,
    /// The default distribution (`*`).
    pub default: bool,
}

impl Distro {
    /// Docker Desktop's internal distributions are not for projects.
    pub fn is_internal(&self) -> bool {
        self.name.starts_with("docker-desktop")
    }
}

/// Decode `wsl.exe` output: UTF-16LE (with or without BOM, what `wsl.exe` prints to a
/// pipe), or UTF-8 when `WSL_UTF8=1` is set.
pub fn decode_output(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let utf16 = bytes.starts_with(&[0xFF, 0xFE])
        || (bytes.len() >= 2 && {
            // ASCII text in UTF-16LE: every second byte is zero.
            let pairs = bytes.len() / 2;
            let zeros = bytes.chunks_exact(2).filter(|c| c[1] == 0).count();
            pairs > 0 && zeros * 2 > pairs
        });
    if utf16 {
        let body = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units).replace('\0', "")
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// `wsl.exe -l -v`.
pub fn list_args() -> Vec<String> {
    vec!["-l".into(), "-v".into()]
}

/// Parse the (decoded) output of `wsl.exe -l -v`. The header and messages such as "no
/// installed distributions" are skipped, whatever their language.
pub fn parse_list(text: &str) -> Vec<Distro> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r').trim_start();
            let (default, rest) = match line.strip_prefix('*') {
                Some(r) => (true, r.trim_start()),
                None => (false, line),
            };
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if tokens.len() < 3 {
                return None;
            }
            let version: u8 = tokens[tokens.len() - 1].parse().ok()?;
            // Names have no spaces; localized states may ("Wird ausgeführt").
            let name = tokens[0].to_string();
            let state = tokens[1..tokens.len() - 1].join(" ");
            Some(Distro {
                name,
                state,
                version,
                default,
            })
        })
        .collect()
}

/// Parse raw `wsl.exe -l -v` bytes.
pub fn parse_list_bytes(bytes: &[u8]) -> Vec<Distro> {
    parse_list(&decode_output(bytes))
}

/// `wsl.exe -d <distro> [-u <user>] --exec <argv…>` (no shell in between).
pub fn exec_args(distro: &str, user: Option<&str>, argv: &[String]) -> Vec<String> {
    let mut a = vec!["-d".to_string(), distro.to_string()];
    if let Some(u) = user.filter(|u| !u.is_empty()) {
        a.push("-u".into());
        a.push(u.to_string());
    }
    a.push("--exec".into());
    a.extend(argv.iter().cloned());
    a
}

/// `wsl.exe -d <distro> --cd <dir>`: the distribution's login shell (Terminal › WSL).
pub fn shell_args(distro: &str, cwd: Option<&str>) -> Vec<String> {
    let mut a = vec!["-d".to_string(), distro.to_string()];
    if let Some(c) = cwd.filter(|c| !c.is_empty()) {
        a.push("--cd".into());
        a.push(c.to_string());
    }
    a
}

/// A path as the Linux side of WSL sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxPath {
    /// The distribution whose file system holds the path (`None` for Windows drives,
    /// which every distribution mounts under `/mnt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distro: Option<String>,
    pub path: String,
}

fn split_segments(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|s| !s.is_empty()).collect()
}

/// The distribution and Linux path of a `\\wsl$\<distro>\…` or `\\wsl.localhost\<distro>\…`
/// path (either slash, any case of the host name).
pub fn parse_unc(path: &str) -> Option<(String, String)> {
    let rest = path
        .strip_prefix("\\\\")
        .or_else(|| path.strip_prefix("//"))?;
    let rest = rest.strip_prefix("?\\UNC\\").unwrap_or(rest);
    let segments = split_segments(rest);
    let host = segments.first()?.to_ascii_lowercase();
    if host != "wsl$" && host != "wsl.localhost" {
        return None;
    }
    let distro = segments.get(1)?.to_string();
    let linux = format!("/{}", segments[2..].join("/"));
    Some((distro, linux))
}

/// A Windows path in the Linux view: `C:\Users\me` → `/mnt/c/Users/me`,
/// `\\wsl$\Ubuntu\home\me` → `/home/me` of `Ubuntu`. Other paths have no Linux view.
pub fn to_linux(path: &str) -> Option<LinuxPath> {
    if let Some((distro, path)) = parse_unc(path) {
        return Some(LinuxPath {
            distro: Some(distro),
            path,
        });
    }
    let path = path.strip_prefix("\\\\?\\").unwrap_or(path);
    let b = path.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        if b.len() > 2 && b[2] != b'\\' && b[2] != b'/' {
            return None; // `C:foo` is relative to the drive's current folder
        }
        let drive = (b[0] as char).to_ascii_lowercase();
        let rest = split_segments(&path[2..]).join("/");
        return Some(LinuxPath {
            distro: None,
            path: if rest.is_empty() {
                format!("/mnt/{drive}")
            } else {
                format!("/mnt/{drive}/{rest}")
            },
        });
    }
    None
}

/// `\\wsl.localhost\<distro>\<path>` for a Linux path of a distribution.
pub fn unc(distro: &str, linux: &str) -> String {
    let rest = split_segments(linux).join("\\");
    format!("\\\\wsl.localhost\\{distro}\\{rest}")
}

/// A Linux path of `distro` in the Windows view: `/mnt/c/Users/me` → `C:\Users\me`,
/// `/home/me` → `\\wsl.localhost\<distro>\home\me`. Relative paths are returned unchanged.
pub fn to_windows(distro: &str, linux: &str) -> String {
    if !linux.starts_with('/') {
        return linux.to_string();
    }
    let segments = split_segments(linux);
    if segments.len() >= 2 && segments[0] == "mnt" {
        let d = segments[1];
        if d.len() == 1 && d.as_bytes()[0].is_ascii_alphabetic() {
            let rest = segments[2..].join("\\");
            return format!("{}:\\{rest}", d.to_ascii_uppercase());
        }
    }
    unc(distro, linux)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `wsl.exe -l -v` as it arrives on a pipe: UTF-16LE with CRLF.
    fn utf16(text: &str, bom: bool) -> Vec<u8> {
        let mut out = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
        for u in text.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out
    }

    const LIST: &str = "  NAME                   STATE           VERSION\r\n* Ubuntu-24.04           Running         2\r\n  Debian                 Stopped         2\r\n  docker-desktop         Stopped         2\r\n  Legacy                 Stopped         1\r\n";

    #[test]
    fn parses_utf16_list_output() {
        for bom in [false, true] {
            let list = parse_list_bytes(&utf16(LIST, bom));
            assert_eq!(list.len(), 4, "bom={bom}");
            assert_eq!(
                list[0],
                Distro {
                    name: "Ubuntu-24.04".into(),
                    state: "Running".into(),
                    version: 2,
                    default: true
                }
            );
            assert!(!list[1].default);
            assert!(list[2].is_internal());
            assert_eq!(list[3].version, 1);
        }
    }

    #[test]
    fn parses_utf8_and_localized_output() {
        // WSL_UTF8=1, German header.
        let text = "  NAME      STATUS          VERSION\n* Ubuntu    Wird ausgeführt 2\n";
        let list = parse_list_bytes(text.as_bytes());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Ubuntu");
        assert_eq!(list[0].state, "Wird ausgeführt");
        // Messages without a table give no distributions.
        let none = utf16(
            "Windows Subsystem for Linux has no installed distributions.\r\nUse 'wsl.exe --list --online' to list available distributions.\r\n",
            false,
        );
        assert!(parse_list_bytes(&none).is_empty());
        assert!(parse_list_bytes(&[]).is_empty());
    }

    #[test]
    fn decodes_non_ascii_names() {
        let list = parse_list_bytes(&utf16("* Übuntu Running 2\r\n", true));
        assert_eq!(list[0].name, "Übuntu");
    }

    #[test]
    fn command_lines() {
        assert_eq!(list_args(), vec!["-l", "-v"]);
        assert_eq!(
            exec_args(
                "Ubuntu",
                Some("me"),
                &["sh".into(), "-c".into(), "id".into()]
            ),
            vec!["-d", "Ubuntu", "-u", "me", "--exec", "sh", "-c", "id"]
        );
        assert_eq!(
            exec_args("Ubuntu", None, &[]),
            vec!["-d", "Ubuntu", "--exec"]
        );
        assert_eq!(
            shell_args("Ubuntu", Some("/home/me/shop")),
            vec!["-d", "Ubuntu", "--cd", "/home/me/shop"]
        );
        assert_eq!(shell_args("Ubuntu", None), vec!["-d", "Ubuntu"]);
    }

    #[test]
    fn maps_windows_paths_to_linux() {
        assert_eq!(
            to_linux(r"C:\Users\me\shop"),
            Some(LinuxPath {
                distro: None,
                path: "/mnt/c/Users/me/shop".into()
            })
        );
        assert_eq!(to_linux("D:/").unwrap().path, "/mnt/d");
        assert_eq!(to_linux(r"\\?\C:\x").unwrap().path, "/mnt/c/x");
        assert_eq!(
            to_linux(r"\\wsl$\Ubuntu\home\me\shop"),
            Some(LinuxPath {
                distro: Some("Ubuntu".into()),
                path: "/home/me/shop".into()
            })
        );
        assert_eq!(
            to_linux("//WSL.localhost/Debian/var/www/"),
            Some(LinuxPath {
                distro: Some("Debian".into()),
                path: "/var/www".into()
            })
        );
        assert_eq!(
            parse_unc(r"\\wsl.localhost\Ubuntu"),
            Some(("Ubuntu".into(), "/".into()))
        );
        assert_eq!(parse_unc(r"\\server\share\x"), None);
        assert_eq!(parse_unc(r"\\wsl$"), None);
        assert_eq!(to_linux("C:relative"), None);
        assert_eq!(to_linux("/home/me"), None);
        assert_eq!(to_linux("relative/path"), None);
    }

    #[test]
    fn maps_linux_paths_to_windows() {
        assert_eq!(to_windows("Ubuntu", "/mnt/c/Users/me"), r"C:\Users\me");
        assert_eq!(to_windows("Ubuntu", "/mnt/d"), r"D:\");
        assert_eq!(
            to_windows("Ubuntu", "/home/me/shop"),
            r"\\wsl.localhost\Ubuntu\home\me\shop"
        );
        assert_eq!(
            to_windows("Ubuntu", "/mnt/wsl/x"),
            r"\\wsl.localhost\Ubuntu\mnt\wsl\x"
        );
        assert_eq!(to_windows("Ubuntu", "/"), r"\\wsl.localhost\Ubuntu\");
        assert_eq!(to_windows("Ubuntu", "rel/x"), "rel/x");
        // Round trips.
        for p in ["/home/me/shop", "/mnt/c/src/app"] {
            let w = to_windows("Ubuntu", p);
            assert_eq!(to_linux(&w).unwrap().path, p);
        }
    }
}

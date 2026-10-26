//! Local processes for "Attach to Process…": `/proc` on Linux, `ps` on macOS and other
//! Unix systems, `tasklist` on Windows. No third-party crate is needed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    /// Executable name (`php-fpm`, `demo`).
    pub name: String,
    /// Full command line, if readable.
    pub command_line: String,
    /// Owner (user name or uid), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// Running processes, sorted by name then pid. Forge's own process is left out.
pub fn list_processes() -> Vec<ProcessInfo> {
    let mut list = platform_list();
    let own = std::process::id();
    list.retain(|p| p.pid != own && p.pid != 0);
    list.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.pid.cmp(&b.pid))
    });
    list
}

/// Keep processes whose name, command line or pid contains `filter` (case-insensitive).
pub fn filter_processes(list: &[ProcessInfo], filter: &str) -> Vec<ProcessInfo> {
    let f = filter.trim().to_lowercase();
    list.iter()
        .filter(|p| {
            f.is_empty()
                || p.name.to_lowercase().contains(&f)
                || p.command_line.to_lowercase().contains(&f)
                || p.pid.to_string().contains(&f)
        })
        .cloned()
        .collect()
}

#[cfg(target_os = "linux")]
fn platform_list() -> Vec<ProcessInfo> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return ps_list();
    };
    let users = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    dir.flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let base = e.path();
            let comm = std::fs::read_to_string(base.join("comm")).unwrap_or_default();
            let cmdline = std::fs::read(base.join("cmdline")).unwrap_or_default();
            let status = std::fs::read_to_string(base.join("status")).unwrap_or_default();
            parse_proc_entry(pid, &comm, &cmdline, &status, &users)
        })
        .collect()
}

#[cfg(all(unix, not(target_os = "linux")))]
fn platform_list() -> Vec<ProcessInfo> {
    ps_list()
}

#[cfg(unix)]
fn ps_list() -> Vec<ProcessInfo> {
    std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,user=,comm=,args="])
        .output()
        .map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(windows)]
fn platform_list() -> Vec<ProcessInfo> {
    std::process::Command::new("tasklist")
        .args(["/fo", "csv", "/nh", "/v"])
        .output()
        .map(|o| parse_tasklist(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(not(any(unix, windows)))]
fn platform_list() -> Vec<ProcessInfo> {
    Vec::new()
}

/// One `/proc/<pid>` entry. Kernel threads (empty command line) are skipped.
pub fn parse_proc_entry(
    pid: u32,
    comm: &str,
    cmdline: &[u8],
    status: &str,
    passwd: &str,
) -> Option<ProcessInfo> {
    if cmdline.is_empty() {
        return None;
    }
    let args: Vec<String> = cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let name = comm.trim();
    let name = if name.is_empty() {
        args.first()
            .map(|a| a.rsplit('/').next().unwrap_or(a).to_string())
            .unwrap_or_default()
    } else {
        name.to_string()
    };
    let uid = status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|v| v.split_whitespace().next())
        .map(str::to_string);
    let user = uid.map(|uid| {
        passwd
            .lines()
            .find_map(|l| {
                let mut parts = l.split(':');
                let name = parts.next()?;
                let _pw = parts.next()?;
                (parts.next()? == uid).then(|| name.to_string())
            })
            .unwrap_or(uid)
    });
    Some(ProcessInfo {
        pid,
        name,
        command_line: args.join(" "),
        user,
    })
}

/// Output of `ps -axww -o pid=,user=,comm=,args=`.
pub fn parse_ps(text: &str) -> Vec<ProcessInfo> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let pid: u32 = parts.next()?.parse().ok()?;
            let user = parts.next()?.to_string();
            let comm = parts.next()?.to_string();
            let args: Vec<&str> = parts.collect();
            Some(ProcessInfo {
                pid,
                name: comm.rsplit('/').next().unwrap_or(&comm).to_string(),
                command_line: if args.is_empty() {
                    comm.clone()
                } else {
                    args.join(" ")
                },
                user: Some(user),
            })
        })
        .collect()
}

/// Split one CSV line with quoted fields (`tasklist /fo csv`).
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Output of `tasklist /fo csv /nh /v`: image, pid, session, session#, memory, status,
/// user, cpu time, window title.
pub fn parse_tasklist(text: &str) -> Vec<ProcessInfo> {
    text.lines()
        .filter_map(|l| {
            let f = csv_fields(l.trim());
            let name = f.first()?.clone();
            let pid: u32 = f.get(1)?.trim().parse().ok()?;
            let user = f.get(6).filter(|u| !u.is_empty() && *u != "N/A").cloned();
            Some(ProcessInfo {
                pid,
                command_line: name.clone(),
                name,
                user,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_entries_resolve_users_and_skip_kernel_threads() {
        let passwd = "root:x:0:0::/root:/bin/sh\nanna:x:1000:1000::/home/anna:/bin/zsh\n";
        let status = "Name:\tdemo\nUid:\t1000\t1000\t1000\t1000\n";
        let p = parse_proc_entry(42, "demo\n", b"./demo\0--fast\0", status, passwd).unwrap();
        assert_eq!(p.name, "demo");
        assert_eq!(p.command_line, "./demo --fast");
        assert_eq!(p.user.as_deref(), Some("anna"));
        assert!(parse_proc_entry(2, "kthreadd", b"", "", passwd).is_none());
    }

    #[test]
    fn parses_ps_and_tasklist() {
        let ps =
            "  101 anna  /usr/bin/php-fpm php-fpm: pool www\n 7 root launchd /sbin/launchd\nbad\n";
        let list = parse_ps(ps);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "php-fpm");
        assert_eq!(list[0].command_line, "php-fpm: pool www");
        let tl = "\"demo.exe\",\"4242\",\"Console\",\"1\",\"1,024 K\",\"Running\",\"PC\\anna\",\"0:00:01\",\"N/A\"\n";
        let list = parse_tasklist(tl);
        assert_eq!(list[0].pid, 4242);
        assert_eq!(list[0].user.as_deref(), Some("PC\\anna"));
    }

    #[test]
    fn filters_and_lists_this_machine() {
        let list = list_processes();
        assert!(list.iter().all(|p| p.pid != std::process::id()));
        let all = vec![
            ProcessInfo {
                pid: 10,
                name: "php-fpm".into(),
                command_line: "php-fpm: pool www".into(),
                user: None,
            },
            ProcessInfo {
                pid: 11,
                name: "demo".into(),
                command_line: "./demo".into(),
                user: None,
            },
        ];
        assert_eq!(filter_processes(&all, "POOL").len(), 1);
        assert_eq!(filter_processes(&all, "11")[0].name, "demo");
        assert_eq!(filter_processes(&all, "").len(), 2);
    }
}

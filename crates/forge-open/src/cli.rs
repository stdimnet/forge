//! Command-line launcher: `forge [project-dir]`, `forge path/to/file:12[:3]`,
//! `forge --line 12 --column 3 file` (JetBrains style), `forge file 12 3` (launch-editor)
//! and `forge forge://open?...`.

use std::path::{Path, PathBuf};

use crate::request::{parse_forge_url, split_position_suffix, LinkAction, OpenRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A directory to open as the project.
    Project(PathBuf),
    /// A file typed on the command line (trusted: no confirmation needed).
    File(OpenRequest),
    /// A `forge://` link handed over by the OS (untrusted: may come from any web page).
    Link(LinkAction),
}

/// Interpret launcher arguments (without the program name). Relative paths are resolved
/// against `cwd`; arguments that name nothing on disk are skipped with a warning.
pub fn parse_launch_args<I, S>(args: I, cwd: &Path) -> Vec<LaunchTarget>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Vec::new();
    let mut line = None;
    let mut column = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        match arg {
            "--line" | "-l" => {
                line = args.next().and_then(|v| v.as_ref().parse().ok());
                continue;
            }
            "--column" | "-c" => {
                column = args.next().and_then(|v| v.as_ref().parse().ok());
                continue;
            }
            _ if arg.starts_with('-') => continue,
            // `forge file 12 3`: numbers right after a file set its line, then its column
            // (how `launch-editor` calls custom editors).
            _ if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) => {
                if let Some(LaunchTarget::File(r)) = out.last_mut() {
                    let n = arg.parse().ok().filter(|n: &u32| *n > 0);
                    if r.line.is_none() {
                        r.line = n;
                    } else if r.column.is_none() {
                        r.column = n;
                    }
                    continue;
                }
            }
            _ => {}
        }
        if arg.starts_with("forge:") {
            match parse_forge_url(arg) {
                Ok(action) => out.push(LaunchTarget::Link(action)),
                Err(e) => tracing::warn!("ignoring link {arg}: {e}"),
            }
            continue;
        }
        let abs = |p: &str| {
            let p = Path::new(p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            }
        };
        let whole = abs(arg);
        let target = if whole.is_dir() {
            Some(LaunchTarget::Project(whole))
        } else if whole.is_file() {
            Some(LaunchTarget::File(
                OpenRequest::new(whole.display().to_string()).at(line, column),
            ))
        } else {
            let (path, l, c) = split_position_suffix(arg);
            let file = abs(path);
            file.is_file().then(|| {
                LaunchTarget::File(
                    OpenRequest::new(file.display().to_string()).at(l.or(line), c.or(column)),
                )
            })
        };
        match target {
            Some(LaunchTarget::File(r)) => match r.validate() {
                Ok(r) => out.push(LaunchTarget::File(r)),
                Err(e) => tracing::warn!("ignoring {arg}: {e}"),
            },
            Some(t) => out.push(t),
            None => tracing::warn!("ignoring {arg}: no such file or directory"),
        }
        line = None;
        column = None;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        fs::create_dir_all(base.join("app/src")).unwrap();
        fs::write(base.join("app/src/App.tsx"), "x").unwrap();
        (dir, base)
    }

    fn file(path: &Path, line: Option<u32>, column: Option<u32>) -> LaunchTarget {
        LaunchTarget::File(OpenRequest::new(path.display().to_string()).at(line, column))
    }

    #[test]
    fn projects_and_files() {
        let (_d, base) = fixture();
        let targets = parse_launch_args(["app", "app/src/App.tsx:12:3", "missing.ts"], &base);
        assert_eq!(
            targets,
            vec![
                LaunchTarget::Project(base.join("app")),
                file(&base.join("app/src/App.tsx"), Some(12), Some(3)),
            ]
        );
    }

    #[test]
    fn jetbrains_style_flags() {
        let (_d, base) = fixture();
        let abs = base.join("app/src/App.tsx");
        let targets = parse_launch_args(
            [
                "--line",
                "7",
                "--column",
                "2",
                abs.to_str().unwrap(),
                "--wait",
            ],
            Path::new("/"),
        );
        assert_eq!(targets, vec![file(&abs, Some(7), Some(2))]);
    }

    #[test]
    fn positional_line_and_column() {
        let (_d, base) = fixture();
        let targets = parse_launch_args(["app/src/App.tsx", "14", "5"], &base);
        assert_eq!(
            targets,
            vec![file(&base.join("app/src/App.tsx"), Some(14), Some(5))]
        );
        // `0` from tools that do not know the column means "unknown".
        let targets = parse_launch_args(["app/src/App.tsx", "14", "0"], &base);
        assert_eq!(
            targets,
            vec![file(&base.join("app/src/App.tsx"), Some(14), None)]
        );
    }

    #[test]
    fn links_are_kept_separate() {
        let targets = parse_launch_args(["forge://open?file=/x/y.php&line=3"], Path::new("/"));
        assert!(matches!(
            &targets[..],
            [LaunchTarget::Link(LinkAction::Open(r))] if r.file == "/x/y.php" && r.line == Some(3)
        ));
        assert!(parse_launch_args(["forge://nope"], Path::new("/")).is_empty());
    }
}

//! How Forge starts processes on Windows, as pure functions that compile and are tested on
//! every platform. [`crate::host_command`] and [`crate::local_command`] apply them to a
//! [`std::process::Command`] on Windows:
//!
//! - **Program lookup** ([`resolve_program`]): `Command::new` only tries `<name>.exe`, so
//!   `composer` (`composer.bat`), `npm` (`npm.cmd`), `gradle` (`gradle.bat`) or `mvn`
//!   (`mvn.cmd`) would not be found. Bare names and extension-less paths are looked up with
//!   `PATH` and `PATHEXT`, like `cmd.exe` does. The standard library then starts `.bat` and
//!   `.cmd` files through `cmd.exe` with its own safe quoting.
//! - **`cmd.exe` command lines** ([`cmd_invocation`]): `cmd /C <line>` does not parse its
//!   arguments the way C programs do, so the MSVC-style quoting of `Command::args` (`\"` for a
//!   quote) breaks lines like `git commit -m "fix bug"`. Everything after `/C` (or `/K`) is
//!   joined into one line and passed verbatim as `/D /V:OFF /S /C "<line>"` (`raw_arg`).
//! - **Building lines** ([`cmd_line`], [`cmd_arg`], [`cmd_escape`]): arguments that go into such
//!   a line are quoted so that `cmd.exe` neither splits them nor runs operators in them, and
//!   `%` is written so that `%VAR%` is never expanded (also inside quotes).
//! - **No console windows** ([`CREATE_NO_WINDOW`]): Forge is a GUI-subsystem app, so every
//!   console child would open a console window of its own.

/// `CREATE_NO_WINDOW` process creation flag: a console program gets no console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `PATHEXT` when the variable is not set.
pub const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// The extensions of a `PATHEXT` value, lower case with their dot (`[".com", ".exe", …]`).
pub fn pathext(value: Option<&str>) -> Vec<String> {
    let value = value
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(DEFAULT_PATHEXT);
    value
        .split(';')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| e.len() > 1 && e.starts_with('.'))
        .collect()
}

/// The folders of a Windows `PATH` value (`;`-separated, entries may be quoted).
pub fn split_path_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(|d| d.trim().trim_matches('"').trim())
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `program` names a path (`bin\tool`, `C:\x\tool`, `./tool`) rather than a bare name.
pub fn is_path_like(program: &str) -> bool {
    program.contains(['/', '\\', ':'])
}

/// The file name part of a Windows path.
fn file_name(program: &str) -> &str {
    program.rsplit(['/', '\\']).next().unwrap_or(program)
}

/// Whether the file name of `program` ends in one of `exts` (case-insensitive).
pub fn has_executable_ext(program: &str, exts: &[String]) -> bool {
    let name = file_name(program).to_ascii_lowercase();
    exts.iter()
        .any(|e| name.len() > e.len() && name.ends_with(e.as_str()))
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with(['\\', '/']) {
        format!("{dir}{name}")
    } else {
        format!("{dir}\\{name}")
    }
}

/// The executable that Windows runs for `program`, when `Command::new` would not find it on
/// its own: a bare name without one of the `PATHEXT` extensions is looked up in every folder
/// of `path` with every extension (`composer` → `C:\…\composer.bat`), a path without such an
/// extension is tried with each of them (`vendor\bin\phpunit` → `vendor\bin\phpunit.bat`).
/// `None` when nothing needs to change (the name already has an executable extension) or
/// nothing was found (the spawn then fails with the usual "program not found").
pub fn resolve_program(
    program: &str,
    path: Option<&str>,
    pathext_value: Option<&str>,
    is_file: impl Fn(&str) -> bool,
) -> Option<String> {
    if program.trim().is_empty() {
        return None;
    }
    let exts = pathext(pathext_value);
    if has_executable_ext(program, &exts) {
        return None;
    }
    if is_path_like(program) {
        return exts
            .iter()
            .map(|e| format!("{program}{e}"))
            .find(|c| is_file(c));
    }
    split_path_list(path.unwrap_or("")).iter().find_map(|dir| {
        exts.iter()
            .map(|e| join(dir, &format!("{program}{e}")))
            .find(|c| is_file(c))
    })
}

/// Whether `program` is the Windows command interpreter (`cmd`, `cmd.exe`,
/// `C:\Windows\System32\CMD.EXE`).
pub fn is_cmd(program: &str) -> bool {
    let name = file_name(program).to_ascii_lowercase();
    name == "cmd" || name == "cmd.exe"
}

/// What `%` becomes in a `cmd.exe` line so that `%VAR%` is never expanded: `%cd:~,%` is an
/// empty substring of the always defined `cd` variable, which separates the `%` from the
/// variable name (the same trick the Rust standard library uses for batch file arguments).
const PERCENT: &str = "%%cd:~,%";

/// `text` for an unquoted place in a `cmd.exe` line (such as `echo`), shown literally: the
/// operators `& | < > ( )`, `^`, `!` and quotes are escaped with `^`, and `%` cannot start a
/// variable.
pub fn cmd_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            '%' => out.push_str(PERCENT),
            '&' | '|' | '<' | '>' | '(' | ')' | '^' | '!' | '"' => {
                out.push('^');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// Characters that need neither quoting for the program nor for `cmd.exe`.
fn is_plain(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./\\:=+@#$*?{}~".contains(c))
}

/// One argument (or the program) for a `cmd.exe` line, passed on literally: in double quotes,
/// so `cmd.exe` operators, spaces and `;`/`,` lose their meaning; inner quotes doubled (which
/// both `cmd.exe` and the MSVC argument parser read as one literal quote), backslashes before
/// a quote doubled, and `%` written so that it cannot expand a variable. Plain arguments
/// (`-q`, `C:\p\a.kt`) stay unquoted.
pub fn cmd_arg(arg: &str) -> String {
    if is_plain(arg) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => {
                backslashes += 1;
                out.push(c);
                continue;
            }
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push_str("\"\"");
            }
            '%' => out.push_str(PERCENT),
            c => out.push(c),
        }
        backslashes = 0;
    }
    // A trailing backslash must not escape the closing quote.
    out.extend(std::iter::repeat_n('\\', backslashes));
    out.push('"');
    out
}

/// A command line for `cmd /C` that runs `argv[0]` with the other elements as its arguments,
/// each passed on literally.
pub fn cmd_line<S: AsRef<str>>(argv: &[S]) -> String {
    let mut parts = Vec::with_capacity(argv.len());
    if let Some((program, args)) = argv.split_first() {
        parts.push(cmd_arg(program.as_ref()));
        parts.extend(args.iter().map(|a| cmd_arg(a.as_ref())));
    }
    parts.join(" ")
}

/// The verbatim argument string for `cmd.exe` started with `args` (`["/C", "git commit -m
/// \"fix bug\""]`, `["/d", "/s", "/c", line]`): options before `/C` or `/K` are kept, `/D`
/// (no AutoRun commands) and `/V:OFF` are added unless given, and the rest, joined with
/// spaces, becomes the line after `/S /C`, in quotes that `/S` strips again. `None` when there
/// is no `/C` or `/K` (an interactive `cmd.exe`), which needs no special handling.
pub fn cmd_invocation<S: AsRef<str>>(args: &[S]) -> Option<String> {
    let at = args.iter().position(|a| {
        let a = a.as_ref();
        a.eq_ignore_ascii_case("/c") || a.eq_ignore_ascii_case("/k")
    })?;
    let before: Vec<&str> = args[..at]
        .iter()
        .map(AsRef::as_ref)
        .filter(|a| !a.eq_ignore_ascii_case("/s"))
        .collect();
    let mut out: Vec<String> = Vec::new();
    if !before.iter().any(|a| a.eq_ignore_ascii_case("/d")) {
        out.push("/D".into());
    }
    if !before
        .iter()
        .any(|a| a.len() >= 2 && a[..2].eq_ignore_ascii_case("/v"))
    {
        out.push("/V:OFF".into());
    }
    out.extend(before.iter().map(|a| a.to_string()));
    out.push("/S".into());
    out.push(args[at].as_ref().to_ascii_uppercase());
    let line: Vec<&str> = args[at + 1..].iter().map(AsRef::as_ref).collect();
    out.push(format!("\"{}\"", line.join(" ")));
    Some(out.join(" "))
}

/// What has to change about a command before Windows starts it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The executable to start instead of the configured program ([`resolve_program`]).
    pub program: Option<String>,
    /// The verbatim argument string that replaces all arguments ([`cmd_invocation`]).
    pub raw_args: Option<String>,
}

impl Plan {
    /// Nothing to change: the command can be started as it is.
    pub fn is_noop(&self) -> bool {
        self.program.is_none() && self.raw_args.is_none()
    }
}

/// The [`Plan`] for `program` with `args` (`None` when an argument is not valid Unicode; such
/// arguments are left alone). `path` is the `PATH` the child sees, `pathext_value` the
/// `PATHEXT` variable.
pub fn plan(
    program: &str,
    args: Option<&[String]>,
    path: Option<&str>,
    pathext_value: Option<&str>,
    is_file: impl Fn(&str) -> bool,
) -> Plan {
    let resolved = resolve_program(program, path, pathext_value, is_file);
    let effective = resolved.as_deref().unwrap_or(program);
    let raw_args = if is_cmd(effective) {
        args.and_then(cmd_invocation)
    } else {
        None
    };
    Plan {
        program: resolved,
        raw_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const PATH: &str = r#"C:\Windows\system32;"C:\Program Files\nodejs";C:\tools\bin\;"#;

    fn files(list: &[&str]) -> impl Fn(&str) -> bool {
        // Like NTFS: case-insensitive, `/` and `\` are the same separator.
        let norm = |p: &str| p.to_ascii_lowercase().replace('/', "\\");
        let set: HashSet<String> = list.iter().map(|f| norm(f)).collect();
        move |p: &str| set.contains(&norm(p))
    }

    #[test]
    fn pathext_defaults_and_normalizes() {
        assert_eq!(pathext(None), [".com", ".exe", ".bat", ".cmd"]);
        assert_eq!(pathext(Some("  ")), [".com", ".exe", ".bat", ".cmd"]);
        assert_eq!(
            pathext(Some(".EXE; .Ps1;;bad;.CMD")),
            [".exe", ".ps1", ".cmd"]
        );
    }

    #[test]
    fn splits_windows_path_lists() {
        assert_eq!(
            split_path_list(PATH),
            [
                r"C:\Windows\system32",
                r"C:\Program Files\nodejs",
                r"C:\tools\bin\"
            ]
        );
    }

    #[test]
    fn finds_batch_files_and_cmd_shims_through_pathext() {
        let fs = files(&[
            r"C:\tools\bin\composer.bat",
            r"C:\Program Files\nodejs\npm.cmd",
            r"C:\Program Files\nodejs\node.exe",
            r"C:\tools\bin\bun.exe",
            r"C:\Windows\system32\cmd.exe",
        ]);
        let r = |p: &str| resolve_program(p, Some(PATH), None, &fs);
        assert_eq!(r("composer").as_deref(), Some(r"C:\tools\bin\composer.bat"));
        assert_eq!(
            r("npm").as_deref(),
            Some(r"C:\Program Files\nodejs\npm.cmd")
        );
        // Bun ships bun.exe: no `.cmd` is guessed.
        assert_eq!(r("bun").as_deref(), Some(r"C:\tools\bin\bun.exe"));
        assert_eq!(r("cmd").as_deref(), Some(r"C:\Windows\system32\cmd.exe"));
        // Names with an executable extension are found by Windows itself.
        assert_eq!(r("node.exe"), None);
        assert_eq!(r("composer.BAT"), None);
        assert_eq!(r("missing"), None);
        assert_eq!(r(""), None);
    }

    #[test]
    fn earlier_folders_and_pathext_order_win() {
        let fs = files(&[
            r"C:\Windows\system32\gradle.cmd",
            r"C:\tools\bin\gradle.exe",
            r"C:\Program Files\nodejs\gradle.bat",
            r"C:\Program Files\nodejs\gradle.exe",
        ]);
        assert_eq!(
            resolve_program("gradle", Some(PATH), None, &fs).as_deref(),
            Some(r"C:\Windows\system32\gradle.cmd")
        );
        assert_eq!(
            resolve_program(
                "gradle",
                Some(r"C:\Program Files\nodejs"),
                Some(".BAT;.EXE"),
                &fs
            )
            .as_deref(),
            Some(r"C:\Program Files\nodejs\gradle.bat")
        );
    }

    #[test]
    fn paths_without_an_extension_get_one() {
        let fs = files(&[r"C:\p\vendor\bin\phpunit.bat", r"C:\p\gradlew.bat"]);
        let r = |p: &str| resolve_program(p, Some(PATH), None, &fs);
        assert_eq!(
            r(r"C:\p\vendor\bin\phpunit").as_deref(),
            Some(r"C:\p\vendor\bin\phpunit.bat")
        );
        assert_eq!(r("C:/p/gradlew").as_deref(), Some(r"C:/p/gradlew.bat"));
        assert_eq!(r(r"C:\p\gradlew.bat"), None);
        assert_eq!(r(r"C:\p\other"), None);
    }

    #[test]
    fn recognizes_cmd_exe() {
        for p in ["cmd", "CMD.EXE", r"C:\Windows\System32\cmd.exe", "c:/x/Cmd"] {
            assert!(is_cmd(p), "{p}");
        }
        for p in ["cmdx", "bash.exe", r"C:\cmd\sh.exe", "pwsh"] {
            assert!(!is_cmd(p), "{p}");
        }
    }

    #[test]
    fn cmd_escape_neutralizes_operators_and_variables() {
        assert_eq!(cmd_escape("> gradlew a&b|c"), "^> gradlew a^&b^|c");
        assert_eq!(cmd_escape("%PATH%"), "%%cd:~,%PATH%%cd:~,%");
        assert_eq!(cmd_escape(r#"(1)!^"x""#), r#"^(1^)^!^^^"x^""#);
    }

    #[test]
    fn cmd_arguments_keep_spaces_quotes_and_percent_signs() {
        assert_eq!(cmd_arg("-q"), "-q");
        assert_eq!(cmd_arg(r"C:\p\Main.kt"), r"C:\p\Main.kt");
        assert_eq!(
            cmd_arg(r"C:\Users\Jane Doe\app.jar"),
            r#""C:\Users\Jane Doe\app.jar""#
        );
        // `%USERNAME%` is not expanded, inside quotes either (#215).
        assert_eq!(cmd_arg("100%USERNAME%"), r#""100%%cd:~,%USERNAME%%cd:~,%""#);
        assert_eq!(cmd_arg(r#"say "hi""#), r#""say ""hi""""#);
        assert_eq!(cmd_arg(r#"a\"b"#), r#""a\\""b""#);
        assert_eq!(cmd_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
        assert_eq!(cmd_arg(""), r#""""#);
        assert_eq!(cmd_arg("a&b"), r#""a&b""#);
    }

    #[test]
    fn cmd_lines_quote_programs_with_spaces() {
        assert_eq!(
            cmd_line(&[r"C:\Users\Jane Doe\sdk\gradlew.bat", "assemble", "-Pa=b c"]),
            r#""C:\Users\Jane Doe\sdk\gradlew.bat" assemble "-Pa=b c""#
        );
        assert_eq!(cmd_line::<&str>(&[]), "");
    }

    #[test]
    fn cmd_invocations_pass_the_line_verbatim() {
        assert_eq!(
            cmd_invocation(&["/C", r#"git commit -m "fix bug""#]).as_deref(),
            Some(r#"/D /V:OFF /S /C "git commit -m "fix bug"""#)
        );
        // Android's own `/d /s` are not repeated.
        assert_eq!(
            cmd_invocation(&["/d", "/s", "/c", "echo a && b"]).as_deref(),
            Some(r#"/V:OFF /d /S /C "echo a && b""#)
        );
        // User options before /C are kept; several arguments after it form one line.
        assert_eq!(
            cmd_invocation(&["/Q", "/v:on", "/k", "dir", "/b"]).as_deref(),
            Some(r#"/D /Q /v:on /S /K "dir /b""#)
        );
        assert_eq!(cmd_invocation(&["/Q"]), None);
        assert_eq!(cmd_invocation::<&str>(&[]), None);
    }

    #[test]
    fn plans_lookup_and_cmd_lines_together() {
        let fs = files(&[r"C:\Windows\system32\cmd.exe", r"C:\tools\bin\composer.bat"]);
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let cmd = plan(
            "cmd",
            Some(&args(&["/C", "echo %OS%"])),
            Some(PATH),
            None,
            &fs,
        );
        assert_eq!(cmd.program.as_deref(), Some(r"C:\Windows\system32\cmd.exe"));
        assert_eq!(
            cmd.raw_args.as_deref(),
            Some(r#"/D /V:OFF /S /C "echo %OS%""#)
        );
        let composer = plan("composer", Some(&args(&["install"])), Some(PATH), None, &fs);
        assert_eq!(
            composer,
            Plan {
                program: Some(r"C:\tools\bin\composer.bat".into()),
                raw_args: None
            }
        );
        assert!(plan("php.exe", Some(&args(&["-v"])), Some(PATH), None, &fs).is_noop());
        // An interactive cmd.exe and non-Unicode arguments stay as they are.
        assert!(plan("cmd.exe", Some(&[]), Some(PATH), None, &fs).is_noop());
        assert!(plan("cmd.exe", None, Some(PATH), None, &fs).is_noop());
    }

    #[test]
    fn plans_are_idempotent() {
        let fs = files(&[r"C:\Windows\system32\cmd.exe"]);
        let first = plan(
            "cmd",
            Some(&["/C".to_string(), "dir".to_string()]),
            Some(PATH),
            None,
            &fs,
        );
        // A command that already went through the plan (one verbatim argument) is left alone.
        let again = plan(
            first.program.as_deref().unwrap(),
            Some(&[first.raw_args.clone().unwrap()]),
            Some(PATH),
            None,
            &fs,
        );
        assert!(again.is_noop(), "{again:?}");
    }
}

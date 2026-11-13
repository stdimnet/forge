//! CForge's native code analyzers, in the style of CLion. Pure Rust (no I/O), shared by the
//! desktop backend and the WebAssembly build of browser mode.
//!
//! - [`sanitizer`]: AddressSanitizer, LeakSanitizer, ThreadSanitizer, MemorySanitizer and
//!   UndefinedBehaviorSanitizer reports in run output.
//! - [`valgrind`]: Valgrind Memcheck's XML output, with the leak summary.
//! - [`perf`]: `perf record` / `perf script` command lines and `perf script` output as a
//!   profile of the shared Profiler (`goforge-pprof`'s views).
//! - [`frames`]: the stack frames of both, mapped onto the project.
//! - [`xml`]: a small XML tree (Valgrind, CMSIS-SVD).

pub mod frames;
pub mod perf;
pub mod sanitizer;
pub mod valgrind;
pub mod xml;

pub use frames::{project_path, Stack, StackFrame};
pub use sanitizer::{Sanitizer, SanitizerReport};
pub use valgrind::{LeakSummary, Leaked, ValgrindError, ValgrindReport};

/// The `--tool=memcheck` arguments of "Run with Valgrind Memcheck": XML into `xml_file`,
/// the user's options, then the program (arguments for `valgrind`).
pub fn valgrind_args(xml_file: &str, options: &str, program: &str, args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![
        "--tool=memcheck".into(),
        "--xml=yes".into(),
        format!("--xml-file={xml_file}"),
    ];
    let user: Vec<String> = split_words(options);
    // Memcheck's leak check is what the results tree is for; the user may turn it down.
    if !user.iter().any(|a| a.starts_with("--leak-check")) {
        out.push("--leak-check=full".into());
    }
    out.extend(user.into_iter().filter(|a| {
        !a.starts_with("--xml") && !a.starts_with("--tool") && !a.starts_with("--log-")
    }));
    out.push(program.into());
    out.extend(args.iter().cloned());
    out
}

/// Split options on whitespace, keeping quoted groups together.
fn split_words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (_, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn valgrind_command_line() {
        let a = super::valgrind_args(
            "/tmp/v.xml",
            "--track-origins=yes --xml=no",
            "/b/app",
            &["x".into()],
        );
        assert_eq!(
            a.join(" "),
            "--tool=memcheck --xml=yes --xml-file=/tmp/v.xml --leak-check=full --track-origins=yes /b/app x"
        );
        let a = super::valgrind_args("v.xml", "--leak-check=summary", "app", &[]);
        assert!(a.contains(&"--leak-check=summary".to_string()));
        assert!(!a.contains(&"--leak-check=full".to_string()));
    }
}

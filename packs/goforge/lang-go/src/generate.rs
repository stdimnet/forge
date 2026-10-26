//! `//go:generate` directives: the gutter ▶ of each one and the `go generate` command that
//! runs exactly that directive (`-run` matches a directive's full source text).

use serde::{Deserialize, Serialize};

use crate::env::quote_meta;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateDirective {
    /// 0-based line.
    pub line: u32,
    /// The command after `//go:generate `.
    pub command: String,
}

const PREFIX: &str = "//go:generate";

/// The directives of a Go file (a directive starts its line, as `go generate` requires).
pub fn directives(text: &str) -> Vec<GenerateDirective> {
    text.lines()
        .enumerate()
        .filter_map(|(n, l)| {
            let rest = l.strip_prefix(PREFIX)?;
            if !rest.starts_with([' ', '\t']) {
                return None;
            }
            let command = rest.trim().to_string();
            (!command.is_empty()).then_some(GenerateDirective {
                line: n as u32,
                command,
            })
        })
        .collect()
}

/// `go generate` arguments for a file (relative to the command's working directory): all
/// its directives, or only the one on `line`.
pub fn generate_args(file: &str, text: &str, line: Option<u32>) -> Result<Vec<String>, String> {
    let mut args = vec!["generate".to_string(), "-x".to_string()];
    if let Some(line) = line {
        let raw = text
            .lines()
            .nth(line as usize)
            .filter(|l| l.starts_with(PREFIX))
            .ok_or_else(|| format!("No //go:generate directive on line {}", line + 1))?;
        args.push("-run".into());
        args.push(format!("^{}$", quote_meta(raw.trim_end())));
    }
    args.push(file.to_string());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "package store\n\n//go:generate stringer -type=Kind\n// go:generate not-a-directive\n//go:generate\n\t//go:generate indented\n//go:generate go run ./gen.go -out=x.go\n";

    #[test]
    fn finds_directives_at_line_start() {
        let d = directives(SRC);
        assert_eq!(
            d,
            [
                GenerateDirective {
                    line: 2,
                    command: "stringer -type=Kind".into()
                },
                GenerateDirective {
                    line: 6,
                    command: "go run ./gen.go -out=x.go".into()
                },
            ]
        );
    }

    #[test]
    fn runs_one_directive() {
        assert_eq!(
            generate_args("./store/kind.go", SRC, Some(6)).unwrap(),
            [
                "generate",
                "-x",
                "-run",
                "^//go:generate go run \\./gen\\.go -out=x\\.go$",
                "./store/kind.go"
            ]
        );
        assert_eq!(generate_args("k.go", SRC, None).unwrap(), ["generate", "-x", "k.go"]);
        assert!(generate_args("k.go", SRC, Some(0)).is_err());
    }
}

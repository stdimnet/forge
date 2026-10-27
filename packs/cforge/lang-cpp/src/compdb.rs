//! Compilation databases (`compile_commands.json`, written by CMake with
//! `CMAKE_EXPORT_COMPILE_COMMANDS`, by Bear for Make projects or by Meson): how each file is
//! compiled. A folder with one opens as a project without CMake; clangd reads it, and Forge
//! shows what it contains (files, compilers, standards) and which compilers clangd may ask
//! for their system headers (`--query-driver`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FILE_NAME: &str = "compile_commands.json";

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileCommand {
    /// The working directory of the compiler (absolute).
    pub directory: String,
    /// The source file, absolute.
    pub file: String,
    /// The compiler and its arguments.
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

impl CompileCommand {
    pub fn compiler(&self) -> &str {
        self.arguments.first().map(String::as_str).unwrap_or("")
    }

    /// `-std=c++20` / `/std:c++20` → `c++20`.
    pub fn standard(&self) -> Option<&str> {
        self.arguments.iter().find_map(|a| {
            a.strip_prefix("-std=")
                .or_else(|| a.strip_prefix("/std:"))
                .or_else(|| a.strip_prefix("-std:"))
        })
    }

    /// Include folders (`-I`, `-isystem`, `/I`), absolute.
    pub fn include_dirs(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = self.arguments.iter();
        while let Some(a) = it.next() {
            let dir = if a == "-I" || a == "-isystem" || a == "/I" || a == "-iquote" {
                it.next().cloned()
            } else if let Some(d) = a.strip_prefix("-I").or_else(|| a.strip_prefix("/I")) {
                Some(d.to_string())
            } else {
                a.strip_prefix("-isystem").map(str::to_string)
            };
            if let Some(d) = dir.filter(|d| !d.is_empty()) {
                out.push(absolute(&self.directory, &d));
            }
        }
        out
    }
}

fn absolute(dir: &str, path: &str) -> String {
    if path.starts_with('/') || path.get(1..2) == Some(":") || path.starts_with('\\') {
        path.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches(['/', '\\']), path)
    }
}

/// Split a `command` string like the compilation database specification says: shell
/// quoting with `"` and `\`.
fn split_command(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            '\\' if matches!(chars.peek(), Some('"') | Some('\\')) => {
                cur.push(chars.next().unwrap_or('\\'));
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Parse a compilation database.
pub fn parse(json: &str) -> Result<Vec<CompileCommand>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad {FILE_NAME}: {e}"))?;
    let list = v
        .as_array()
        .ok_or_else(|| format!("{FILE_NAME} is not a list of entries"))?;
    let mut out = Vec::with_capacity(list.len());
    for e in list {
        let s = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
        let (Some(directory), Some(file)) = (s("directory"), s("file")) else {
            continue;
        };
        let arguments = match e.get("arguments").and_then(Value::as_array) {
            Some(a) => a
                .iter()
                .filter_map(|a| a.as_str().map(str::to_string))
                .collect(),
            None => s("command").map(|c| split_command(&c)).unwrap_or_default(),
        };
        out.push(CompileCommand {
            file: absolute(&directory, &file),
            directory,
            arguments,
            output: s("output"),
        });
    }
    Ok(out)
}

/// What the Toolchains / CMake views show about a database.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub files: usize,
    /// Files inside the project.
    pub project_files: usize,
    pub compilers: Vec<String>,
    pub standards: Vec<String>,
}

pub fn summary(entries: &[CompileCommand], root: &str) -> Summary {
    let root = root.trim_end_matches(['/', '\\']);
    let compilers: BTreeSet<String> = entries.iter().map(|e| e.compiler().to_string()).collect();
    let standards: BTreeSet<String> = entries
        .iter()
        .filter_map(|e| e.standard().map(str::to_string))
        .collect();
    Summary {
        files: entries.len(),
        project_files: entries.iter().filter(|e| e.file.starts_with(root)).count(),
        compilers: compilers.into_iter().filter(|c| !c.is_empty()).collect(),
        standards: standards.into_iter().collect(),
    }
}

/// clangd's `--query-driver` globs: the absolute compilers of the database (clangd only
/// asks trusted drivers for their system include folders).
pub fn query_drivers(entries: &[CompileCommand]) -> Vec<String> {
    let set: BTreeSet<String> = entries
        .iter()
        .map(|e| e.compiler().to_string())
        .filter(|c| c.starts_with('/') || c.get(1..2) == Some(":"))
        .collect();
    set.into_iter().collect()
}

/// The entry of a project file (absolute path compare, `\` and `/` alike).
pub fn entry_for<'a>(entries: &'a [CompileCommand], abs: &str) -> Option<&'a CompileCommand> {
    let norm = |p: &str| p.replace('\\', "/");
    let want = norm(abs);
    entries.iter().find(|e| norm(&e.file) == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: &str = r#"[
      {
        "directory": "/home/dev/shapes/build/debug",
        "command": "/usr/bin/c++  -I/home/dev/shapes/src -isystem /opt/gtest/include -g -std=gnu++20 -o CMakeFiles/shapes.dir/src/shapes.cpp.o -c /home/dev/shapes/src/shapes.cpp",
        "file": "/home/dev/shapes/src/shapes.cpp",
        "output": "CMakeFiles/shapes.dir/src/shapes.cpp.o"
      },
      {
        "directory": "/home/dev/shapes",
        "arguments": ["clang++", "-I", "src", "-DNAME=\"shapes app\"", "-std=c++17", "-c", "src/main.cpp"],
        "file": "src/main.cpp"
      },
      { "directory": "/x" }
    ]"#;

    #[test]
    fn reads_command_and_arguments_entries() {
        let db = parse(DB).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(db[0].compiler(), "/usr/bin/c++");
        assert_eq!(db[0].standard(), Some("gnu++20"));
        assert_eq!(
            db[0].include_dirs(),
            ["/home/dev/shapes/src", "/opt/gtest/include"]
        );
        assert_eq!(db[1].file, "/home/dev/shapes/src/main.cpp");
        assert_eq!(db[1].include_dirs(), ["/home/dev/shapes/src"]);
        assert_eq!(db[1].arguments[3], "-DNAME=\"shapes app\"");
        let s = summary(&db, "/home/dev/shapes/");
        assert_eq!(s.files, 2);
        assert_eq!(s.project_files, 2);
        assert_eq!(s.compilers, ["/usr/bin/c++", "clang++"]);
        assert_eq!(s.standards, ["c++17", "gnu++20"]);
        assert_eq!(query_drivers(&db), ["/usr/bin/c++"]);
        assert!(entry_for(&db, "/home/dev/shapes/src/main.cpp").is_some());
        assert!(parse("{}").is_err());
    }

    #[test]
    fn splits_quoted_commands() {
        assert_eq!(
            split_command(r#"cc -DX="a b" "-I/my dir" -c x.c"#),
            ["cc", "-DX=a b", "-I/my dir", "-c", "x.c"]
        );
        assert_eq!(split_command(r#"cc -DS=\"q\""#), ["cc", "-DS=\"q\""]);
    }
}

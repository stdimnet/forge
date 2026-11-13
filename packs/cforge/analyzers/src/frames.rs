//! Stack frames shared by sanitizer reports and Valgrind errors, and the mapping of their
//! files onto the project (clickable frames, the editor's highlights).

use serde::{Deserialize, Serialize};

/// One frame of a stack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackFrame {
    /// `#0` → 0.
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The function (demangled as the tool printed it), `??` when unknown.
    pub function: String,
    /// The source file as reported (absolute, or relative to the compiler's working dir).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based; 0 when unknown.
    pub line: u32,
    pub column: u32,
    /// The binary or library (`libc.so.6`, `shapes_app+0x2384`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// The project file (relative), once resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl StackFrame {
    /// Whether the frame is in the project (resolved to a project file).
    pub fn in_project(&self) -> bool {
        self.path.is_some()
    }
}

/// A titled stack ("READ of size 4 …", "freed by thread T0 here", "Address … is 0 bytes after
/// a block of size 16 alloc'd").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stack {
    pub title: String,
    pub frames: Vec<StackFrame>,
}

/// Files of the toolchain, the C library and the sanitizer runtimes are never project files.
fn is_system_file(file: &str) -> bool {
    let f = file.replace('\\', "/");
    f.starts_with("/usr/")
        || f.starts_with("/lib")
        || f.starts_with("/opt/")
        || f.contains("/sysdeps/")
        || f.contains("../csu/")
        || f.contains("libsanitizer/")
        || f.contains("compiler-rt/")
        || f.starts_with("<")
}

/// Map a reported file onto the project: an absolute path below `root` (or a mapped one), or
/// a path relative to the project or the build folder that `exists`.
pub fn project_path(
    file: &str,
    root: &str,
    build_dir: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let file = file.trim().replace('\\', "/");
    if file.is_empty() || is_system_file(&file) {
        return None;
    }
    let root = root.trim_end_matches('/').replace('\\', "/");
    let absolute = file.starts_with('/') || file.get(1..2) == Some(":");
    if absolute {
        let rel = file.strip_prefix(&format!("{root}/"))?;
        return Some(normalize(rel));
    }
    let rel = normalize(&file);
    if exists(&rel) {
        return Some(rel);
    }
    let build = build_dir.trim_matches('/');
    if !build.is_empty() {
        let joined = normalize(&format!("{build}/{file}"));
        if !joined.starts_with("..") && exists(&joined) {
            return Some(joined);
        }
    }
    None
}

/// `a/./b/../c` → `a/c`.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|p| *p != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

/// Resolve every frame of `stacks` and return the first project frame of the first stack
/// that has one (the error's location).
pub fn resolve_stacks(
    stacks: &mut [Stack],
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Option<StackFrame> {
    for s in stacks.iter_mut() {
        for f in s.frames.iter_mut() {
            f.path = f.file.as_deref().and_then(resolve);
        }
    }
    stacks
        .iter()
        .flat_map(|s| s.frames.iter())
        .find(|f| f.in_project())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_files_onto_the_project() {
        let exists = |p: &str| p == "src/shapes.cpp";
        let root = "/home/anna/Projects/shapes";
        assert_eq!(
            project_path(
                "/home/anna/Projects/shapes/src/./shapes.cpp",
                root,
                "",
                &exists
            ),
            Some("src/shapes.cpp".into())
        );
        assert_eq!(
            project_path("src/shapes.cpp", root, "", &exists),
            Some("src/shapes.cpp".into())
        );
        assert_eq!(
            project_path("../src/shapes.cpp", root, "cmake-build-debug", &exists),
            Some("src/shapes.cpp".into())
        );
        assert_eq!(
            project_path("/usr/include/c++/13/bits/invoke.h", root, "", &exists),
            None
        );
        assert_eq!(
            project_path("../sysdeps/nptl/libc_start_call_main.h", root, "", &exists),
            None
        );
        assert_eq!(project_path("/elsewhere/x.cpp", root, "", &exists), None);
    }
}

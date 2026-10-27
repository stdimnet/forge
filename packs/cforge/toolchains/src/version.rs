//! Version outputs of compilers and build tools: `gcc -v` / `clang -v` (written to stderr),
//! the banner `cl.exe` prints without arguments, and the first line of `--version` of CMake,
//! Ninja, Make, GDB, LLDB and clang-format.

use serde::{Deserialize, Serialize};

/// The compiler family, which decides flags, error formats and the default debugger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompilerFamily {
    Gcc,
    Clang,
    AppleClang,
    Msvc,
    #[default]
    Unknown,
}

impl CompilerFamily {
    pub fn label(self) -> &'static str {
        match self {
            CompilerFamily::Gcc => "GCC",
            CompilerFamily::Clang => "Clang",
            CompilerFamily::AppleClang => "Apple Clang",
            CompilerFamily::Msvc => "MSVC",
            CompilerFamily::Unknown => "Compiler",
        }
    }

    /// CMake's `CMAKE_<LANG>_COMPILER_ID`.
    pub fn cmake_id(self) -> &'static str {
        match self {
            CompilerFamily::Gcc => "GNU",
            CompilerFamily::Clang => "Clang",
            CompilerFamily::AppleClang => "AppleClang",
            CompilerFamily::Msvc => "MSVC",
            CompilerFamily::Unknown => "",
        }
    }
}

/// What a compiler says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerInfo {
    pub family: CompilerFamily,
    /// `13.3.0`, `18.1.3`, `19.41.34123`.
    pub version: String,
    /// Target triple (`x86_64-pc-linux-gnu`, `arm-none-eabi`) or MSVC's architecture (`x64`).
    pub target: String,
    /// `posix`, `win32`, `single`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_model: Option<String>,
    /// Clang's `InstalledDir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_dir: Option<String>,
    /// Who built it: `Ubuntu`, `MSYS2 project`, `Arm GNU Toolchain 13.3.Rel1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

impl CompilerInfo {
    /// `GCC 13.3.0`, `Clang 18.1.3 (arm64-apple-darwin24.1.0)`.
    pub fn label(&self) -> String {
        if self.version.is_empty() {
            self.family.label().to_string()
        } else {
            format!("{} {}", self.family.label(), self.version)
        }
    }

    /// Whether it builds for Windows with the GNU runtime (MinGW-w64).
    pub fn is_mingw(&self) -> bool {
        self.family != CompilerFamily::Msvc
            && (self.target.contains("mingw") || self.target.contains("windows-gnu"))
    }

    /// The major version number.
    pub fn major(&self) -> Option<u32> {
        major(&self.version)
    }
}

/// The leading number of a version (`15` of `15.1`).
pub fn major(version: &str) -> Option<u32> {
    version
        .split(['.', '-', ' '])
        .next()
        .and_then(|m| m.parse().ok())
}

/// The numeric parts of a version, for ordering (`14.41.34120` → `[14, 41, 34120]`).
pub fn version_key(version: &str) -> Vec<u32> {
    version
        .split(['.', '-'])
        .map_while(|p| p.parse::<u32>().ok())
        .collect()
}

/// Whether `version` is `min` or newer (numeric parts compared in order).
pub fn at_least(version: &str, min: &str) -> bool {
    let (a, b) = (version_key(version), version_key(min));
    if a.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    true
}

fn is_version(token: &str) -> bool {
    token.starts_with(|c: char| c.is_ascii_digit()) && token.contains('.')
}

/// The text inside the last balanced parentheses of a line (`(Ubuntu 13.3.0-6ubuntu2)`).
fn parenthesized(line: &str) -> Option<&str> {
    let start = line.find('(')?;
    let end = line.rfind(')')?;
    (end > start).then(|| line[start + 1..end].trim())
}

/// Parse `gcc -v`, `clang -v` (or `--version`) and MSVC's `cl` banner.
pub fn parse_compiler(text: &str) -> Option<CompilerInfo> {
    let mut info = CompilerInfo::default();
    let mut found = false;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Target:") {
            info.target = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Thread model:") {
            info.thread_model = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("InstalledDir:") {
            info.installed_dir = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("gcc version ") {
            found = true;
            info.family = CompilerFamily::Gcc;
            info.version = rest.split_whitespace().next().unwrap_or("").to_string();
            info.vendor = parenthesized(rest).map(vendor_of);
        } else if line.contains("clang version ") && !found {
            found = true;
            let (before, after) = line.split_once("clang version ").unwrap_or(("", line));
            info.family = if before.contains("Apple") {
                CompilerFamily::AppleClang
            } else {
                CompilerFamily::Clang
            };
            info.version = after.split_whitespace().next().unwrap_or("").to_string();
            let vendor = before.trim();
            if !vendor.is_empty() && vendor != "Apple" {
                info.vendor = Some(vendor.to_string());
            }
        } else if line.contains("C/C++ Optimizing Compiler Version") {
            found = true;
            info.family = CompilerFamily::Msvc;
            let after = line
                .split_once("Version ")
                .map(|(_, a)| a)
                .unwrap_or_default();
            let mut words = after.split_whitespace();
            info.version = words.next().unwrap_or("").to_string();
            if words.next() == Some("for") {
                info.target = words.next().unwrap_or("").to_string();
            }
            info.vendor = Some("Microsoft".into());
        }
    }
    found.then_some(info)
}

/// `Ubuntu 13.3.0-6ubuntu2~24.04` → `Ubuntu`; `Rev2, Built by MSYS2 project` → `MSYS2
/// project`; `Arm GNU Toolchain 13.3.Rel1 (Build arm-13.24)` → `Arm GNU Toolchain 13.3.Rel1`.
fn vendor_of(text: &str) -> String {
    if let Some((_, by)) = text.split_once("Built by ") {
        return by.trim().to_string();
    }
    let text = text.split(" (").next().unwrap_or(text);
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.as_slice() {
        [first, rest @ ..] if rest.iter().all(|w| w.starts_with(|c: char| c.is_ascii_digit())) => {
            first.to_string()
        }
        _ => text.trim().to_string(),
    }
}

/// The version on the first line of a `--version` output: CMake (`cmake version 3.28.3`),
/// Ninja (`1.11.1`), Make (`GNU Make 4.3`), GDB (`GNU gdb (Ubuntu 15.1-1ubuntu1) 15.1`), LLDB
/// (`lldb version 18.1.3`), clang-format (`Ubuntu clang-format version 18.1.3`).
pub fn parse_tool_version(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    if let Some((_, after)) = line.split_once("version ") {
        let v = after.split_whitespace().next()?;
        return is_version(v).then(|| v.trim_end_matches(',').to_string());
    }
    // Skip parenthesized vendor parts (GDB) and take the last version-looking word.
    let mut depth = 0i32;
    let mut last = None;
    for word in line.split_whitespace() {
        let opens = word.matches('(').count() as i32;
        let closes = word.matches(')').count() as i32;
        if depth == 0 && opens == 0 && is_version(word) {
            last = Some(word);
        }
        depth = (depth + opens - closes).max(0);
    }
    last.map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn gcc_v_from_ubuntu() {
        let gcc = parse_compiler(&fixture("gcc-v.txt")).unwrap();
        assert_eq!(gcc.family, CompilerFamily::Gcc);
        assert_eq!(gcc.version, "13.3.0");
        assert_eq!(gcc.target, "x86_64-linux-gnu");
        assert_eq!(gcc.thread_model.as_deref(), Some("posix"));
        assert_eq!(gcc.vendor.as_deref(), Some("Ubuntu"));
        assert_eq!(gcc.label(), "GCC 13.3.0");
        assert!(!gcc.is_mingw());
    }

    #[test]
    fn clang_and_apple_clang() {
        let clang = parse_compiler(&fixture("clang-v.txt")).unwrap();
        assert_eq!(clang.family, CompilerFamily::Clang);
        assert_eq!(clang.version, "18.1.3");
        assert_eq!(clang.target, "x86_64-pc-linux-gnu");
        assert_eq!(clang.installed_dir.as_deref(), Some("/usr/bin"));
        assert_eq!(clang.vendor.as_deref(), Some("Ubuntu"));
        assert_eq!(clang.major(), Some(18));
        let apple = parse_compiler(&fixture("apple-clang-v.txt")).unwrap();
        assert_eq!(apple.family, CompilerFamily::AppleClang);
        assert_eq!(apple.version, "16.0.0");
        assert_eq!(apple.target, "arm64-apple-darwin24.1.0");
        assert_eq!(apple.vendor, None);
        assert_eq!(apple.family.cmake_id(), "AppleClang");
    }

    #[test]
    fn mingw_cross_and_msvc() {
        let mingw = parse_compiler(&fixture("mingw-gcc-v.txt")).unwrap();
        assert_eq!(mingw.version, "14.2.0");
        assert!(mingw.is_mingw());
        assert_eq!(mingw.vendor.as_deref(), Some("MSYS2 project"));
        let arm = parse_compiler(&fixture("arm-none-eabi-gcc-v.txt")).unwrap();
        assert_eq!(arm.target, "arm-none-eabi");
        assert_eq!(arm.version, "13.3.1");
        assert_eq!(arm.thread_model.as_deref(), Some("single"));
        assert_eq!(arm.vendor.as_deref(), Some("Arm GNU Toolchain 13.3.Rel1"));
        let cl = parse_compiler(&fixture("msvc-cl-banner.txt")).unwrap();
        assert_eq!(cl.family, CompilerFamily::Msvc);
        assert_eq!(cl.version, "19.41.34123");
        assert_eq!(cl.target, "x64");
        assert!(parse_compiler("bash: gcc: command not found").is_none());
    }

    #[test]
    fn tool_versions() {
        let v = |f: &str| parse_tool_version(&fixture(f));
        assert_eq!(v("cmake-version.txt").as_deref(), Some("3.28.3"));
        assert_eq!(v("ninja-version.txt").as_deref(), Some("1.11.1"));
        assert_eq!(v("make-version.txt").as_deref(), Some("4.3"));
        assert_eq!(v("gdb-version.txt").as_deref(), Some("15.1"));
        assert_eq!(v("lldb-version.txt").as_deref(), Some("18.1.3"));
        assert_eq!(v("clang-format-version.txt").as_deref(), Some("18.1.3"));
        assert_eq!(parse_tool_version("GNU Make"), None);
    }

    #[test]
    fn version_comparison() {
        assert!(at_least("15.1", "14"));
        assert!(at_least("14.0.0", "14"));
        assert!(!at_least("13.2", "14"));
        assert!(at_least("3.28.3", "3.20"));
        assert!(!at_least("", "1"));
        assert_eq!(major("19.1.2"), Some(19));
    }
}

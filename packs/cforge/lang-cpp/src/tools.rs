//! Command lines of the language tools: clangd (pointed at the compilation database of the
//! active CMake profile, with clang-tidy diagnostics) and clang-format (Reformat Code and
//! format on save, reading the project's `.clang-format`), and the debugger's launch
//! settings for C++ programs with STL pretty printers.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// clangd's options (Settings | Languages & Frameworks | C/C++ | Clangd).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClangdOptions {
    /// The folder with `compile_commands.json` (the active profile's build folder).
    pub compile_commands_dir: Option<String>,
    /// clang-tidy checks in the diagnostics (`--clang-tidy`; `.clang-tidy` files decide which).
    pub clang_tidy: bool,
    /// `iwyu` or `never`.
    pub header_insertion: String,
    pub background_index: bool,
    /// `detailed` or `bundled`.
    pub completion_style: String,
    /// Compilers clangd may run to learn their system headers (cross and MSVC toolchains).
    pub query_drivers: Vec<String>,
    /// More arguments, one string.
    pub extra_args: String,
}

impl Default for ClangdOptions {
    fn default() -> Self {
        Self {
            compile_commands_dir: None,
            clang_tidy: true,
            header_insertion: "iwyu".into(),
            background_index: true,
            completion_style: "detailed".into(),
            query_drivers: Vec::new(),
            extra_args: String::new(),
        }
    }
}

/// clangd's arguments.
pub fn clangd_args(o: &ClangdOptions) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(dir) = o.compile_commands_dir.as_ref().filter(|d| !d.is_empty()) {
        args.push(format!("--compile-commands-dir={dir}"));
    }
    if o.background_index {
        args.push("--background-index".into());
    }
    if o.clang_tidy {
        args.push("--clang-tidy".into());
    }
    let insertion = if o.header_insertion == "never" { "never" } else { "iwyu" };
    args.push(format!("--header-insertion={insertion}"));
    let style = if o.completion_style == "bundled" { "bundled" } else { "detailed" };
    args.push(format!("--completion-style={style}"));
    if !o.query_drivers.is_empty() {
        args.push(format!("--query-driver={}", o.query_drivers.join(",")));
    }
    args.push("--pch-storage=memory".into());
    args.extend(cforge_cmake::profile::split_args(&o.extra_args));
    args
}

/// clangd's `--version` output: `clangd version 19.1.2 (https://github.com/llvm/…)` →
/// `19.1.2`.
pub fn clangd_version(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("clangd version "))?;
    let v = line.split_once("clangd version ")?.1.split_whitespace().next()?;
    Some(v.to_string())
}

/// clang-format reading the file from stdin: `--assume-filename` picks the language and
/// the `.clang-format` file of its folder.
pub fn clang_format_args(abs_path: &str, fallback_style: &str) -> Vec<String> {
    let fallback = if fallback_style.trim().is_empty() {
        "LLVM"
    } else {
        fallback_style.trim()
    };
    vec![
        format!("--assume-filename={abs_path}"),
        "--style=file".into(),
        format!("--fallback-style={fallback}"),
    ]
}

/// What the debug launch of a C++ program adds for STL containers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSetup {
    /// Extra arguments for the debug adapter process (GDB: `-iex` commands).
    pub adapter_args: Vec<String>,
    /// Merged into the launch arguments (LLDB: `initCommands`).
    pub configuration: Value,
}

/// GDB: register libstdc++'s Python pretty printers from `printers_dir` (the folder with
/// `libstdcxx/`) before the program loads, quietly when the system already registers them;
/// `set print pretty on` shows structures on several lines. LLDB: its built-in C++
/// formatters (libc++, libstdc++, MSVC STL) are switched on.
pub fn debug_setup(adapter: &str, printers_dir: Option<&str>, stl_renderers: bool) -> DebugSetup {
    match adapter {
        "gdb" => {
            let mut args = vec!["-iex".to_string(), "set print pretty on".to_string()];
            if let (true, Some(dir)) = (stl_renderers, printers_dir) {
                let dir = dir.replace('\\', "/").replace('\'', "\\'");
                args.push("-iex".into());
                args.push(format!(
                    "python exec('import sys\\nsys.path.insert(0, \\'{dir}\\')\\ntry:\\n from libstdcxx.v6 import register_libstdcxx_printers\\n register_libstdcxx_printers(None)\\nexcept Exception:\\n pass')"
                ));
            }
            DebugSetup {
                adapter_args: args,
                configuration: Value::Null,
            }
        }
        "lldb" if stl_renderers => DebugSetup {
            adapter_args: Vec::new(),
            configuration: json!({ "initCommands": ["type category enable cplusplus"] }),
        },
        _ => DebugSetup::default(),
    }
}

/// Where a GCC installation keeps libstdc++'s printers, most specific first:
/// `<prefix>/share/gcc-<version>/python`, `<prefix>/share/gcc/python` (Debian, Ubuntu,
/// MSYS2), and the same below `/usr`.
pub fn printer_dir_candidates(compiler: &str, version: &str) -> Vec<String> {
    let c = compiler.replace('\\', "/");
    let mut out = Vec::new();
    let major = version.split('.').next().unwrap_or("");
    let mut prefixes = Vec::new();
    if let Some((bin, _)) = c.rsplit_once('/') {
        if let Some((prefix, _)) = bin.rsplit_once('/') {
            prefixes.push(prefix.to_string());
        }
    }
    if !prefixes.iter().any(|p| p == "/usr") {
        prefixes.push("/usr".into());
    }
    for p in prefixes {
        if !version.is_empty() {
            out.push(format!("{p}/share/gcc-{version}/python"));
        }
        if !major.is_empty() {
            out.push(format!("{p}/share/gcc-{major}/python"));
        }
        out.push(format!("{p}/share/gcc/python"));
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clangd_arguments() {
        let o = ClangdOptions {
            compile_commands_dir: Some("/home/dev/shapes/cmake-build-debug".into()),
            query_drivers: vec!["/usr/bin/arm-none-eabi-g++".into(), "/usr/bin/c++".into()],
            extra_args: "--log=error -j=4".into(),
            ..ClangdOptions::default()
        };
        assert_eq!(
            clangd_args(&o),
            [
                "--compile-commands-dir=/home/dev/shapes/cmake-build-debug",
                "--background-index",
                "--clang-tidy",
                "--header-insertion=iwyu",
                "--completion-style=detailed",
                "--query-driver=/usr/bin/arm-none-eabi-g++,/usr/bin/c++",
                "--pch-storage=memory",
                "--log=error",
                "-j=4",
            ]
        );
        let off = ClangdOptions {
            clang_tidy: false,
            background_index: false,
            header_insertion: "never".into(),
            ..ClangdOptions::default()
        };
        assert_eq!(
            clangd_args(&off),
            ["--header-insertion=never", "--completion-style=detailed", "--pch-storage=memory"]
        );
        assert_eq!(
            clangd_version("Ubuntu clangd version 18.1.3 (1ubuntu1)\nFeatures: linux+grpc\nPlatform: x86_64-pc-linux-gnu\n").as_deref(),
            Some("18.1.3")
        );
        assert_eq!(clangd_version("nope"), None);
    }

    #[test]
    fn clang_format_and_debugging() {
        assert_eq!(
            clang_format_args("/p/src/a.cpp", ""),
            ["--assume-filename=/p/src/a.cpp", "--style=file", "--fallback-style=LLVM"]
        );
        let gdb = debug_setup("gdb", Some("/usr/share/gcc/python"), true);
        assert_eq!(gdb.adapter_args.len(), 4);
        assert!(gdb.adapter_args[3].starts_with("python exec('import sys\\nsys.path.insert(0, \\'/usr/share/gcc/python\\')"));
        assert!(gdb.adapter_args[3].contains("register_libstdcxx_printers(None)"));
        assert_eq!(debug_setup("gdb", None, true).adapter_args, ["-iex", "set print pretty on"]);
        let lldb = debug_setup("lldb", None, true);
        assert_eq!(lldb.configuration["initCommands"][0], "type category enable cplusplus");
        assert_eq!(debug_setup("lldb", None, false), DebugSetup::default());
        assert_eq!(
            printer_dir_candidates("/usr/bin/g++", "13.3.0"),
            ["/usr/share/gcc-13.3.0/python", "/usr/share/gcc-13/python", "/usr/share/gcc/python"]
        );
        assert_eq!(
            printer_dir_candidates("C:\\msys64\\ucrt64\\bin\\g++.exe", "14.2.0")[2],
            "C:/msys64/ucrt64/share/gcc/python"
        );
    }
}

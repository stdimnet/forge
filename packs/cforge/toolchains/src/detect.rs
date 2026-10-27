//! Toolchains found on this machine, like CLion's Settings | Build, Execution, Deployment |
//! Toolchains: a C and C++ compiler with the build tools (CMake, Ninja, Make) and the
//! debugger that fit them.
//!
//! - the system GCC and Clang (`gcc`/`g++`, `clang`/`clang++` on the PATH) and versioned ones
//!   (`gcc-14`, `clang-19`),
//! - cross compilers on the PATH (`arm-none-eabi-gcc`, `aarch64-linux-gnu-gcc`,
//!   `x86_64-w64-mingw32-gcc`),
//! - MinGW-w64 on Windows (MSYS2's `mingw64`, `ucrt64` and `clang64`, `C:\mingw64`),
//! - Visual Studio's MSVC through vswhere, with `vcvarsall.bat` and the bundled CMake and Ninja.
//!
//! Everything the detection does to the machine goes through [`Probe`], so tests describe
//! machines with a fake.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::version::{self, CompilerFamily, CompilerInfo};
use crate::vswhere;

/// What detection may ask the machine.
pub trait Probe {
    /// The folders of the PATH.
    fn path_dirs(&self) -> Vec<PathBuf>;
    fn is_file(&self, path: &Path) -> bool;
    /// File names in a folder.
    fn list(&self, dir: &Path) -> Vec<String>;
    /// Run a program and return its stdout and stderr together; `None` when it cannot run.
    fn output(&self, program: &Path, args: &[&str]) -> Option<String>;
    fn var(&self, name: &str) -> Option<String>;
    fn windows(&self) -> bool;
}

/// The real machine.
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn path_dirs(&self) -> Vec<PathBuf> {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn list(&self, dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn output(&self, program: &Path, args: &[&str]) -> Option<String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .env("LC_ALL", "C");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console flashes while Forge probes compilers.
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().ok()?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Some(text)
    }

    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }

    fn windows(&self) -> bool {
        cfg!(windows)
    }
}

/// Where a toolchain comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolchainKind {
    /// The system compilers (`gcc`, `clang`, Apple's command line tools).
    #[default]
    System,
    MinGw,
    VisualStudio,
    /// A compiler for another target (`arm-none-eabi`).
    Cross,
    /// Entered by the user in the settings.
    Custom,
}

/// A tool with its version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub path: String,
    #[serde(default)]
    pub version: String,
}

/// The debugger of a toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DebuggerKind {
    /// GDB 14 or newer (`gdb -i dap`).
    Gdb,
    /// `lldb-dap` (`lldb-vscode` before LLVM 18).
    Lldb,
}

impl DebuggerKind {
    /// The debug adapter type of `forge.debugger`.
    pub fn adapter(self) -> &'static str {
        match self {
            DebuggerKind::Gdb => "gdb",
            DebuggerKind::Lldb => "lldb",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Debugger {
    pub kind: DebuggerKind,
    pub path: String,
    #[serde(default)]
    pub version: String,
}

/// A C/C++ toolchain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    /// Stable id: `system-gcc`, `clang-19`, `cross-arm-none-eabi`, `msvc-2022`, or the
    /// custom entry's name.
    pub id: String,
    /// `GCC 13.3.0`, `Visual Studio Community 2022`.
    pub name: String,
    pub kind: ToolchainKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c_compiler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cxx_compiler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<CompilerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmake: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ninja: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub make: Option<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debugger: Option<Debugger>,
    /// A script whose environment builds use (MSVC's `vcvarsall.bat`, a cross SDK's
    /// `environment-setup-*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_script: Option<String>,
    /// MSVC architecture (`x64`) for `vcvarsall.bat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// `detected` or `settings`.
    pub source: String,
    /// What is missing or wrong ("No C++ compiler", "GDB 12 has no DAP server").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl Toolchain {
    /// The default CMake generator: Ninja when there is one, else Unix / MinGW Makefiles
    /// (or NMake for MSVC).
    pub fn default_generator(&self) -> &'static str {
        if self.ninja.is_some() {
            "Ninja"
        } else if self.kind == ToolchainKind::VisualStudio {
            "NMake Makefiles"
        } else if self.kind == ToolchainKind::MinGw {
            "MinGW Makefiles"
        } else {
            "Unix Makefiles"
        }
    }

    fn check(&mut self) {
        self.problems.clear();
        if self.cxx_compiler.is_none() {
            self.problems.push("No C++ compiler".into());
        }
        if self.cmake.is_none() {
            self.problems.push("CMake is not found".into());
        }
        if self.ninja.is_none() && self.make.is_none() {
            self.problems
                .push("No build tool: install Ninja or Make".into());
        }
        match &self.debugger {
            Some(d) if d.kind == DebuggerKind::Gdb && !d.version.is_empty() && !version::at_least(&d.version, "14") => {
                self.problems.push(format!(
                    "GDB {} has no DAP server (gdb -i dap needs GDB 14 or newer)",
                    d.version
                ));
            }
            None if self.kind != ToolchainKind::VisualStudio => {
                self.problems.push("No debugger (GDB or LLDB)".into())
            }
            _ => {}
        }
    }
}

/// A toolchain the user entered (setting `cpp.toolchains`, one table per toolchain). Empty
/// fields are detected (the tools on the PATH).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolchainEntry {
    pub name: String,
    /// `system`, `mingw`, `visualStudio`, `cross` or `custom`.
    pub kind: String,
    pub c_compiler: String,
    pub cxx_compiler: String,
    pub cmake: String,
    pub make: String,
    pub ninja: String,
    pub debugger: String,
    pub environment_script: String,
}

/// Tools found once and shared by every toolchain that has no own ones.
#[derive(Debug, Clone, Default)]
struct Shared {
    cmake: Option<Tool>,
    ninja: Option<Tool>,
    make: Option<Tool>,
    gdb: Option<Debugger>,
    lldb: Option<Debugger>,
}

fn exe(name: &str, windows: bool) -> String {
    if windows && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The first `name` in the PATH.
pub fn which(probe: &dyn Probe, name: &str) -> Option<PathBuf> {
    let file = exe(name, probe.windows());
    probe
        .path_dirs()
        .into_iter()
        .map(|d| d.join(&file))
        .find(|p| probe.is_file(p))
}

fn tool(probe: &dyn Probe, path: PathBuf) -> Tool {
    let version = probe
        .output(&path, &["--version"])
        .and_then(|t| version::parse_tool_version(&t))
        .unwrap_or_default();
    Tool {
        path: path.to_string_lossy().into_owned(),
        version,
    }
}

fn find_tool(probe: &dyn Probe, names: &[&str]) -> Option<Tool> {
    names
        .iter()
        .find_map(|n| which(probe, n))
        .map(|p| tool(probe, p))
}

fn debugger(probe: &dyn Probe, kind: DebuggerKind, path: PathBuf) -> Debugger {
    let version = match kind {
        DebuggerKind::Gdb => tool(probe, path.clone()).version,
        // lldb-dap has no --version before LLVM 19; ask lldb next to it.
        DebuggerKind::Lldb => {
            let lldb = path.with_file_name(exe("lldb", probe.windows()));
            if probe.is_file(&lldb) {
                tool(probe, lldb).version
            } else {
                String::new()
            }
        }
    };
    Debugger {
        kind,
        path: path.to_string_lossy().into_owned(),
        version,
    }
}

fn shared(probe: &dyn Probe) -> Shared {
    let make_names: &[&str] = if probe.windows() {
        &["make", "mingw32-make"]
    } else {
        &["make", "gmake"]
    };
    Shared {
        cmake: find_tool(probe, &["cmake"]),
        ninja: find_tool(probe, &["ninja", "ninja-build"]),
        make: find_tool(probe, make_names),
        gdb: which(probe, "gdb").map(|p| debugger(probe, DebuggerKind::Gdb, p)),
        lldb: ["lldb-dap", "lldb-vscode"]
            .iter()
            .find_map(|n| which(probe, n))
            .map(|p| debugger(probe, DebuggerKind::Lldb, p)),
    }
}

fn compiler_info(probe: &dyn Probe, path: &Path) -> Option<CompilerInfo> {
    probe
        .output(path, &["-v"])
        .and_then(|t| version::parse_compiler(&t))
}

/// Build a toolchain from a compiler pair with the shared tools.
fn assemble(
    id: String,
    kind: ToolchainKind,
    cc: Option<PathBuf>,
    cxx: Option<PathBuf>,
    info: Option<CompilerInfo>,
    shared: &Shared,
) -> Toolchain {
    let family = info.as_ref().map(|i| i.family).unwrap_or_default();
    let name = match (&info, kind) {
        (Some(i), ToolchainKind::Cross) => format!("{} ({})", i.label(), i.target),
        (Some(i), ToolchainKind::MinGw) => format!("MinGW {}", i.label()),
        (Some(i), _) => i.label(),
        (None, _) => id.clone(),
    };
    // Clang prefers LLDB, everything else GDB; either when the other is missing.
    let debugger = match family {
        CompilerFamily::Clang | CompilerFamily::AppleClang => {
            shared.lldb.clone().or_else(|| shared.gdb.clone())
        }
        _ => shared.gdb.clone().or_else(|| shared.lldb.clone()),
    };
    let mut t = Toolchain {
        id,
        name,
        kind,
        c_compiler: cc.map(|p| p.to_string_lossy().into_owned()),
        cxx_compiler: cxx.map(|p| p.to_string_lossy().into_owned()),
        compiler: info,
        cmake: shared.cmake.clone(),
        ninja: shared.ninja.clone(),
        make: shared.make.clone(),
        debugger,
        source: "detected".into(),
        ..Toolchain::default()
    };
    t.check();
    t
}

/// A `<triple>-gcc` cross compiler name: the triple (`arm-none-eabi`), or `None` for host
/// compilers and versioned names.
pub fn cross_triple(file: &str) -> Option<&str> {
    let stem = file.strip_suffix(".exe").unwrap_or(file);
    let triple = stem.strip_suffix("-gcc")?;
    (triple.matches('-').count() >= 1 && !triple.is_empty()).then_some(triple)
}

/// `gcc-14` → `14`, `clang-19` → `19`.
fn versioned<'a>(file: &'a str, base: &str) -> Option<&'a str> {
    let v = file.strip_prefix(base)?.strip_prefix('-')?;
    (!v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')).then_some(v)
}

/// Detect every toolchain of this machine; the default one comes first.
pub fn detect(probe: &dyn Probe) -> Vec<Toolchain> {
    let shared = shared(probe);
    let windows = probe.windows();
    let mut out: Vec<Toolchain> = Vec::new();
    let push = |out: &mut Vec<Toolchain>, t: Toolchain| {
        let same = |o: &Toolchain| {
            o.id == t.id
                || (o.cxx_compiler.is_some() && o.cxx_compiler == t.cxx_compiler)
                || (o.compiler.is_some()
                    && o.compiler == t.compiler
                    && o.kind == t.kind
                    && o.kind != ToolchainKind::Cross)
        };
        if !out.iter().any(same) {
            out.push(t);
        }
    };

    // MSVC first on Windows: it is CLion's default there.
    if windows {
        for t in detect_msvc(probe) {
            push(&mut out, t);
        }
    }

    // System GCC and Clang.
    let mut host_target = None;
    for (id, cc, cxx) in [("system-gcc", "gcc", "g++"), ("system-clang", "clang", "clang++")] {
        let (Some(c), Some(cpp)) = (which(probe, cc), which(probe, cxx)) else {
            continue;
        };
        let info = compiler_info(probe, &cpp).or_else(|| compiler_info(probe, &c));
        let kind = if info.as_ref().is_some_and(CompilerInfo::is_mingw) {
            ToolchainKind::MinGw
        } else {
            ToolchainKind::System
        };
        if host_target.is_none() {
            host_target = info.as_ref().map(|i| i.target.clone());
        }
        push(&mut out, assemble(id.into(), kind, Some(c), Some(cpp), info, &shared));
    }

    // Versioned and cross compilers in the PATH folders.
    let mut seen_dirs = Vec::new();
    for dir in probe.path_dirs() {
        if seen_dirs.contains(&dir) {
            continue;
        }
        seen_dirs.push(dir.clone());
        let files = probe.list(&dir);
        let has = |n: &str| files.iter().any(|f| f == &exe(n, windows));
        let mut names: Vec<&String> = files.iter().collect();
        names.sort();
        for f in names {
            let stem = f.strip_suffix(".exe").unwrap_or(f);
            for (cc, cxx) in [("gcc", "g++"), ("clang", "clang++")] {
                if let Some(v) = versioned(stem, cc) {
                    let pair = format!("{cxx}-{v}");
                    if has(&pair) {
                        let cpp = dir.join(exe(&pair, windows));
                        let info = compiler_info(probe, &cpp);
                        push(
                            &mut out,
                            assemble(
                                format!("{cc}-{v}"),
                                ToolchainKind::System,
                                Some(dir.join(f)),
                                Some(cpp),
                                info,
                                &shared,
                            ),
                        );
                    }
                }
            }
            let Some(triple) = cross_triple(stem) else {
                continue;
            };
            // Debian's host aliases (`x86_64-linux-gnu-gcc`) are the system compiler.
            if host_target.as_deref() == Some(triple) {
                continue;
            }
            let pair = format!("{triple}-g++");
            if !has(&pair) {
                continue;
            }
            let cpp = dir.join(exe(&pair, windows));
            let info = compiler_info(probe, &cpp);
            let kind = if triple.contains("mingw") {
                ToolchainKind::MinGw
            } else {
                ToolchainKind::Cross
            };
            let mut t = assemble(
                format!("cross-{triple}"),
                kind,
                Some(dir.join(f)),
                Some(cpp),
                info,
                &shared,
            );
            // `<triple>-gdb` or gdb-multiarch debug other targets.
            let gdb = [format!("{triple}-gdb"), "gdb-multiarch".to_string()]
                .into_iter()
                .find_map(|n| which(probe, &n));
            if let Some(g) = gdb {
                t.debugger = Some(debugger(probe, DebuggerKind::Gdb, g));
                t.check();
            }
            push(&mut out, t);
        }
    }

    if windows {
        for t in detect_mingw_folders(probe, &shared) {
            push(&mut out, t);
        }
    }
    out
}

/// MSYS2 and standalone MinGW-w64 folders.
fn detect_mingw_folders(probe: &dyn Probe, shared: &Shared) -> Vec<Toolchain> {
    let mut out = Vec::new();
    let roots = [
        "C:\\msys64\\ucrt64",
        "C:\\msys64\\mingw64",
        "C:\\msys64\\clang64",
        "C:\\mingw64",
        "C:\\MinGW",
    ];
    for root in roots {
        let bin = PathBuf::from(format!("{root}\\bin"));
        let (cc, cxx) = if root.ends_with("clang64") {
            ("clang.exe", "clang++.exe")
        } else {
            ("gcc.exe", "g++.exe")
        };
        let (c, cpp) = (bin.join(cc), bin.join(cxx));
        if !probe.is_file(&cpp) {
            continue;
        }
        let info = compiler_info(probe, &cpp);
        let mut own = shared.clone();
        for (slot, names) in [
            (&mut own.make, ["mingw32-make.exe", "make.exe"]),
            (&mut own.ninja, ["ninja.exe", "ninja.exe"]),
            (&mut own.cmake, ["cmake.exe", "cmake.exe"]),
        ] {
            if let Some(p) = names.iter().map(|n| bin.join(n)).find(|p| probe.is_file(p)) {
                *slot = Some(tool(probe, p));
            }
        }
        let gdb = bin.join("gdb.exe");
        if probe.is_file(&gdb) {
            own.gdb = Some(debugger(probe, DebuggerKind::Gdb, gdb));
        }
        let lldb = bin.join("lldb-dap.exe");
        if probe.is_file(&lldb) {
            own.lldb = Some(debugger(probe, DebuggerKind::Lldb, lldb));
        }
        let id = format!(
            "mingw-{}",
            root.rsplit('\\').next().unwrap_or(root).to_ascii_lowercase()
        );
        out.push(assemble(id, ToolchainKind::MinGw, Some(c), Some(cpp), info, &own));
    }
    out
}

/// Visual Studio installations through vswhere.
fn detect_msvc(probe: &dyn Probe) -> Vec<Toolchain> {
    let Some(pf86) = probe.var("ProgramFiles(x86)") else {
        return Vec::new();
    };
    let vswhere = PathBuf::from(vswhere::vswhere_path(&pf86));
    if !probe.is_file(&vswhere) {
        return Vec::new();
    }
    let Some(json) = probe.output(&vswhere, vswhere::VSWHERE_ARGS) else {
        return Vec::new();
    };
    let Ok(list) = vswhere::parse(json.trim()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for vs in list {
        let tools_dir = PathBuf::from(format!("{}\\VC\\Tools\\MSVC", vs.installation_path));
        let names = probe.list(&tools_dir);
        let Some(toolset) = vswhere::newest_toolset(names.iter().map(String::as_str)) else {
            continue;
        };
        let paths = vswhere::msvc_paths(&vs.installation_path, &toolset, "x64");
        let cl = PathBuf::from(&paths.cl);
        let info = probe
            .output(&cl, &[])
            .and_then(|t| version::parse_compiler(&t))
            .or(Some(CompilerInfo {
                family: CompilerFamily::Msvc,
                version: toolset.clone(),
                target: "x64".into(),
                vendor: Some("Microsoft".into()),
                ..CompilerInfo::default()
            }));
        let bundled = |p: &str| {
            let p = PathBuf::from(p);
            probe.is_file(&p).then(|| tool(probe, p))
        };
        let mut t = Toolchain {
            id: format!(
                "msvc-{}",
                if vs.product_line.is_empty() {
                    vs.installation_version.clone()
                } else {
                    vs.product_line.clone()
                }
            ),
            name: vs.display_name.clone(),
            kind: ToolchainKind::VisualStudio,
            c_compiler: Some(paths.cl.clone()),
            cxx_compiler: Some(paths.cl.clone()),
            compiler: info,
            cmake: bundled(&paths.cmake).or_else(|| find_tool(probe, &["cmake"])),
            ninja: bundled(&paths.ninja).or_else(|| find_tool(probe, &["ninja"])),
            make: None,
            // Microsoft's debugger is not redistributable; LLDB debugs MSVC programs (PDB).
            debugger: ["lldb-dap", "lldb-vscode"]
                .iter()
                .find_map(|n| which(probe, n))
                .map(|p| debugger(probe, DebuggerKind::Lldb, p)),
            environment_script: Some(paths.vcvarsall),
            arch: Some("x64".into()),
            source: "detected".into(),
            problems: Vec::new(),
        };
        t.check();
        out.push(t);
    }
    out
}

/// A toolchain from the settings: the entry's tools, the rest detected like the system
/// toolchain's.
pub fn from_entry(probe: &dyn Probe, entry: &ToolchainEntry) -> Toolchain {
    let shared = shared(probe);
    let path = |s: &str| (!s.trim().is_empty()).then(|| PathBuf::from(s.trim()));
    let cxx = path(&entry.cxx_compiler);
    let cc = path(&entry.c_compiler);
    let info = cxx
        .as_deref()
        .or(cc.as_deref())
        .and_then(|p| compiler_info(probe, p));
    let kind = match entry.kind.as_str() {
        "system" => ToolchainKind::System,
        "mingw" => ToolchainKind::MinGw,
        "visualStudio" => ToolchainKind::VisualStudio,
        "cross" => ToolchainKind::Cross,
        _ => ToolchainKind::Custom,
    };
    let mut t = assemble(entry.name.clone(), kind, cc, cxx, info, &shared);
    t.name = entry.name.clone();
    t.source = "settings".into();
    if let Some(p) = path(&entry.cmake) {
        t.cmake = Some(tool(probe, p));
    }
    if let Some(p) = path(&entry.make) {
        t.make = Some(tool(probe, p));
    }
    if let Some(p) = path(&entry.ninja) {
        t.ninja = Some(tool(probe, p));
    }
    if let Some(p) = path(&entry.debugger) {
        let lldb = p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("lldb"));
        let kind = if lldb {
            DebuggerKind::Lldb
        } else {
            DebuggerKind::Gdb
        };
        t.debugger = Some(debugger(probe, kind, p));
    }
    if !entry.environment_script.trim().is_empty() {
        t.environment_script = Some(entry.environment_script.trim().to_string());
    }
    t.check();
    t
}

/// The toolchains of the Toolchains page: the user's entries first (the first one is the
/// default), then the detected ones.
pub fn all(probe: &dyn Probe, entries: &[ToolchainEntry]) -> Vec<Toolchain> {
    let mut out: Vec<Toolchain> = entries
        .iter()
        .filter(|e| !e.name.trim().is_empty())
        .map(|e| from_entry(probe, e))
        .collect();
    for t in detect(probe) {
        if !out.iter().any(|o| o.id == t.id) {
            out.push(t);
        }
    }
    out
}

/// The CMake cache variables a toolchain sets (`CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER`,
/// `CMAKE_MAKE_PROGRAM` for its build tool).
pub fn cmake_variables(t: &Toolchain, generator: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    if let Some(c) = &t.c_compiler {
        vars.insert("CMAKE_C_COMPILER".into(), c.clone());
    }
    if let Some(c) = &t.cxx_compiler {
        vars.insert("CMAKE_CXX_COMPILER".into(), c.clone());
    }
    let program = if generator.starts_with("Ninja") {
        t.ninja.as_ref()
    } else if generator.contains("Makefiles") && !generator.starts_with("NMake") {
        t.make.as_ref()
    } else {
        None
    };
    if let Some(p) = program {
        vars.insert("CMAKE_MAKE_PROGRAM".into(), p.path.clone());
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// Paths of the fake machine compare with `/` (Windows paths are joined with `/` on
    /// the test host).
    fn norm(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "/")
    }

    /// A machine described by its files and program outputs.
    #[derive(Default)]
    struct Fake {
        path: Vec<PathBuf>,
        files: Vec<PathBuf>,
        outputs: HashMap<(PathBuf, String), String>,
        vars: HashMap<String, String>,
        windows: bool,
    }

    impl Fake {
        fn file(&mut self, p: &str) -> &mut Self {
            self.files.push(PathBuf::from(p));
            self
        }
        fn out(&mut self, p: &str, args: &str, text: String) -> &mut Self {
            self.files.push(PathBuf::from(p));
            self.outputs.insert((PathBuf::from(p), args.into()), text);
            self
        }
    }

    impl Probe for Fake {
        fn path_dirs(&self) -> Vec<PathBuf> {
            self.path.clone()
        }
        fn is_file(&self, path: &Path) -> bool {
            self.files.iter().any(|f| norm(f) == norm(path))
        }
        fn list(&self, dir: &Path) -> Vec<String> {
            let prefix = format!("{}/", norm(dir));
            let mut out: Vec<String> = self
                .files
                .iter()
                .filter_map(|f| {
                    let f = norm(f);
                    let rest = f.strip_prefix(&prefix)?;
                    Some(rest.split('/').next()?.to_string())
                })
                .collect();
            out.sort();
            out.dedup();
            out
        }
        fn output(&self, program: &Path, args: &[&str]) -> Option<String> {
            self.outputs
                .iter()
                .find(|((p, a), _)| norm(p) == norm(program) && *a == args.join(" "))
                .map(|(_, t)| t.clone())
        }
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
        fn windows(&self) -> bool {
            self.windows
        }
    }

    fn ubuntu() -> Fake {
        let mut f = Fake {
            path: vec![PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")],
            ..Fake::default()
        };
        f.out("/usr/bin/gcc", "-v", fixture("gcc-v.txt"))
            .out("/usr/bin/g++", "-v", fixture("gcc-v.txt"))
            .out("/usr/bin/clang", "-v", fixture("clang-v.txt"))
            .out("/usr/bin/clang++", "-v", fixture("clang-v.txt"))
            .out("/usr/bin/cmake", "--version", fixture("cmake-version.txt"))
            .out("/usr/bin/ninja", "--version", fixture("ninja-version.txt"))
            .out("/usr/bin/make", "--version", fixture("make-version.txt"))
            .out("/usr/bin/gdb", "--version", fixture("gdb-version.txt"))
            .out("/usr/bin/lldb", "--version", fixture("lldb-version.txt"))
            .file("/usr/bin/lldb-dap")
            // Debian's host aliases and a second GCC.
            .file("/usr/bin/x86_64-linux-gnu-gcc")
            .file("/usr/bin/x86_64-linux-gnu-g++")
            .out("/usr/bin/g++-14", "-v", fixture("gcc-v.txt").replace("13.3.0", "14.2.0"))
            .file("/usr/bin/gcc-14")
            .out(
                "/usr/local/bin/arm-none-eabi-g++",
                "-v",
                fixture("arm-none-eabi-gcc-v.txt"),
            )
            .file("/usr/local/bin/arm-none-eabi-gcc")
            .file("/usr/bin/gdb-multiarch");
        f
    }

    #[test]
    fn linux_machine() {
        let list = detect(&ubuntu());
        let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["system-gcc", "system-clang", "cross-arm-none-eabi", "gcc-14"]);
        let gcc = &list[0];
        assert_eq!(gcc.name, "GCC 13.3.0");
        assert_eq!(gcc.cxx_compiler.as_deref(), Some("/usr/bin/g++"));
        assert_eq!(gcc.cmake.as_ref().unwrap().version, "3.28.3");
        assert_eq!(gcc.ninja.as_ref().unwrap().version, "1.11.1");
        let gdb = gcc.debugger.as_ref().unwrap();
        assert_eq!((gdb.kind, gdb.version.as_str()), (DebuggerKind::Gdb, "15.1"));
        assert!(gcc.problems.is_empty(), "{:?}", gcc.problems);
        assert_eq!(gcc.default_generator(), "Ninja");
        let clang = &list[1];
        assert_eq!(clang.name, "Clang 18.1.3");
        let lldb = clang.debugger.as_ref().unwrap();
        assert_eq!((lldb.kind, lldb.version.as_str()), (DebuggerKind::Lldb, "18.1.3"));
        let arm = &list[2];
        assert_eq!(arm.kind, ToolchainKind::Cross);
        assert_eq!(arm.name, "GCC 13.3.1 (arm-none-eabi)");
        assert_eq!(arm.debugger.as_ref().unwrap().path, "/usr/bin/gdb-multiarch");
        assert_eq!(list[3].name, "GCC 14.2.0");
        let vars = cmake_variables(gcc, "Ninja");
        assert_eq!(vars["CMAKE_CXX_COMPILER"], "/usr/bin/g++");
        assert_eq!(vars["CMAKE_MAKE_PROGRAM"], "/usr/bin/ninja");
        assert_eq!(
            cmake_variables(gcc, "Unix Makefiles")["CMAKE_MAKE_PROGRAM"],
            "/usr/bin/make"
        );
    }

    #[test]
    fn old_gdb_and_missing_tools_are_problems() {
        let mut f = Fake {
            path: vec![PathBuf::from("/usr/bin")],
            ..Fake::default()
        };
        f.out("/usr/bin/gcc", "-v", fixture("gcc-v.txt"))
            .out("/usr/bin/g++", "-v", fixture("gcc-v.txt"))
            .out("/usr/bin/gdb", "--version", "GNU gdb (GDB) 12.1\n".into());
        let t = &detect(&f)[0];
        assert_eq!(
            t.problems,
            [
                "CMake is not found",
                "No build tool: install Ninja or Make",
                "GDB 12.1 has no DAP server (gdb -i dap needs GDB 14 or newer)"
            ]
        );
    }

    #[test]
    fn windows_msvc_and_msys2() {
        let mut f = Fake {
            windows: true,
            path: vec![PathBuf::from("C:\\Tools")],
            ..Fake::default()
        };
        f.vars
            .insert("ProgramFiles(x86)".into(), "C:\\Program Files (x86)".into());
        let vs = "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community";
        f.out(
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe",
            &vswhere::VSWHERE_ARGS.join(" "),
            fixture("vswhere.json"),
        )
        .file(&format!("{vs}\\VC\\Tools\\MSVC\\14.41.34120\\bin\\Hostx64\\x64\\cl.exe"))
        .file(&format!("{vs}\\VC\\Tools\\MSVC\\14.38.33130\\bin\\Hostx64\\x64\\cl.exe"))
        .out(
            &format!("{vs}\\VC\\Tools\\MSVC\\14.41.34120\\bin\\Hostx64\\x64\\cl.exe"),
            "",
            fixture("msvc-cl-banner.txt"),
        )
        .out(
            &format!("{vs}\\Common7\\IDE\\CommonExtensions\\Microsoft\\CMake\\CMake\\bin\\cmake.exe"),
            "--version",
            fixture("cmake-version.txt"),
        )
        .out(
            &format!("{vs}\\Common7\\IDE\\CommonExtensions\\Microsoft\\CMake\\Ninja\\ninja.exe"),
            "--version",
            fixture("ninja-version.txt"),
        )
        .file("C:\\Tools\\lldb-dap.exe")
        .out("C:\\msys64\\ucrt64\\bin\\g++.exe", "-v", fixture("mingw-gcc-v.txt"))
        .out(
            "C:\\msys64\\ucrt64\\bin\\gdb.exe",
            "--version",
            fixture("gdb-version.txt"),
        )
        .out(
            "C:\\msys64\\ucrt64\\bin\\mingw32-make.exe",
            "--version",
            fixture("make-version.txt"),
        );
        // VS 2019 of vswhere.json has no tool set folder: skipped.
        let list = detect(&f);
        let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["msvc-2022", "mingw-ucrt64"]);
        let msvc = &list[0];
        assert_eq!(msvc.kind, ToolchainKind::VisualStudio);
        assert_eq!(msvc.compiler.as_ref().unwrap().version, "19.41.34123");
        assert!(msvc.cxx_compiler.as_ref().unwrap().contains("14.41.34120"));
        assert!(msvc
            .environment_script
            .as_ref()
            .unwrap()
            .ends_with("vcvarsall.bat"));
        assert_eq!(msvc.ninja.as_ref().unwrap().version, "1.11.1");
        assert_eq!(msvc.debugger.as_ref().unwrap().kind, DebuggerKind::Lldb);
        assert!(msvc.problems.is_empty(), "{:?}", msvc.problems);
        let mingw = &list[1];
        assert_eq!(mingw.name, "MinGW GCC 14.2.0");
        assert_eq!(mingw.make.as_ref().unwrap().version, "4.3");
        assert_eq!(mingw.default_generator(), "MinGW Makefiles");
        assert_eq!(mingw.debugger.as_ref().unwrap().version, "15.1");
    }

    #[test]
    fn user_entries_come_first() {
        let probe = ubuntu();
        let entry = ToolchainEntry {
            name: "ARM".into(),
            kind: "cross".into(),
            cxx_compiler: "/usr/local/bin/arm-none-eabi-g++".into(),
            c_compiler: "/usr/local/bin/arm-none-eabi-gcc".into(),
            debugger: "/usr/bin/gdb-multiarch".into(),
            ..ToolchainEntry::default()
        };
        let list = all(&probe, &[entry]);
        assert_eq!(list[0].name, "ARM");
        assert_eq!(list[0].source, "settings");
        assert_eq!(list[0].compiler.as_ref().unwrap().target, "arm-none-eabi");
        assert_eq!(list[1].id, "system-gcc");
        assert_eq!(cross_triple("aarch64-linux-gnu-gcc"), Some("aarch64-linux-gnu"));
        assert_eq!(cross_triple("gcc"), None);
        assert_eq!(cross_triple("x86_64-w64-mingw32-gcc.exe"), Some("x86_64-w64-mingw32"));
    }
}

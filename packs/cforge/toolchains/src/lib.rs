//! CForge's C/C++ toolchains, in the style of CLion's Settings | Build, Execution,
//! Deployment | Toolchains.
//!
//! - [`version`]: `gcc -v`, `clang -v`, MSVC's `cl` banner and the `--version` lines of
//!   CMake, Ninja, Make, GDB, LLDB and clang-format.
//! - [`vswhere`]: Visual Studio installations and their MSVC folders.
//! - [`detect`]: the toolchains of a machine (system GCC / Clang, versioned and cross
//!   compilers, MinGW-w64, MSVC) with their build tools and debugger, and the user's own
//!   toolchains from the settings.

pub mod detect;
pub mod version;
pub mod vswhere;

pub use detect::{
    all, cmake_variables, detect, from_entry, which, Debugger, DebuggerKind, Probe, SystemProbe,
    Tool, Toolchain, ToolchainEntry, ToolchainKind,
};
pub use version::{parse_compiler, parse_tool_version, CompilerFamily, CompilerInfo};

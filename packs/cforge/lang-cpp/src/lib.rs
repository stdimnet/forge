//! CForge's C/C++ language support, in the style of CLion. Pure Rust (no I/O), shared by the
//! desktop backend and the WebAssembly build of browser mode.
//!
//! - [`diagnostics`]: GCC, Clang, MSVC and linker messages of a build.
//! - [`compdb`]: compilation databases (`compile_commands.json`).
//! - [`discovery`]: GoogleTest and Catch2 tests for the gutter.
//! - [`provider`]: the GoogleTest, Catch2 and CTest providers of the shared test runner.
//! - [`coverage`]: gcov / llvm-cov coverage as LCOV.
//! - [`tools`]: clangd and clang-format command lines, debugger setup with STL pretty
//!   printers.

pub mod compdb;
pub mod coverage;
pub mod diagnostics;
pub mod discovery;
pub mod provider;
pub mod tools;

pub use diagnostics::{parse_build_output, BuildProblem};
pub use discovery::{discover, CppTest, Discovered, Framework};
pub use provider::{
    debug_command, is_cpp_source, register, ContextTarget, CppContext, CppTests, CATCH2,
    CONTEXT_KEY, CTEST, GTEST,
};

/// The run configuration type of CMake executables ("CMake Application").
pub const CMAKE_APP: &str = "cmake-app";

/// Whether a project file belongs to CForge (sources, headers, CMake files, compilation
/// databases, Makefiles, Meson builds).
pub fn is_cpp_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or("");
    is_cpp_source(name)
        || cforge_cmake::is_cmake_file(name)
        || matches!(
            name,
            "compile_commands.json"
                | "Makefile"
                | "GNUmakefile"
                | "meson.build"
                | ".clang-format"
                | ".clang-tidy"
                | ".clangd"
        )
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpp_paths() {
        for p in [
            "src/main.cpp",
            "include/a.hpp",
            "x.c",
            "CMakeLists.txt",
            "build/compile_commands.json",
            ".clang-format",
            "meson.build",
        ] {
            assert!(super::is_cpp_path(p), "{p}");
        }
        assert!(!super::is_cpp_path("main.go"));
    }
}

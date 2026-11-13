//! CForge's CMake support, in the style of CLion. Pure Rust (no I/O): the desktop backend
//! reads and runs, this crate decides; the WebAssembly build shares it.
//!
//! - [`presets`]: `CMakePresets.json` / `CMakeUserPresets.json` with includes, inheritance,
//!   conditions and macros.
//! - [`profile`]: CMake profiles (build type, toolchain, generator, options, build folder)
//!   and the configure, build, clean and CTest command lines.
//! - [`fileapi`]: the cmake-file-api query and the codemodel-v2 reply (targets, sources,
//!   artifacts, where targets are defined).
//! - [`log`]: configure output messages linked to `CMakeLists.txt` lines.
//! - [`complete`]: command, keyword and variable completion in CMake files.
//! - [`sanitizers`]: the sanitizers of a profile as compiler and linker flags.
//! - [`packages`]: vcpkg (manifest mode) and Conan 2: toolchain files, `conan install`,
//!   port completion in `vcpkg.json`.

pub mod complete;
pub mod fileapi;
pub mod log;
pub mod packages;
pub mod presets;
pub mod profile;
pub mod sanitizers;

pub use fileapi::{CMakeModel, Configuration, Location, Source, Target, TargetKind};
pub use log::{parse_log, ConfigureMessage, Severity};
pub use presets::{MacroContext, Presets, ResolvedConfigure};
pub use profile::{
    build_command, build_dir_of, configure_command, profiles, BuildAction, Command,
    ConfigureContext, Profile, ProfileEntry,
};

/// Whether a project file is a CMake file (`CMakeLists.txt`, `*.cmake`, presets).
pub fn is_cmake_file(path: &str) -> bool {
    let name = path.replace('\\', "/");
    let name = name.rsplit('/').next().unwrap_or("");
    name == "CMakeLists.txt"
        || name.ends_with(".cmake")
        || name == presets::PROJECT_FILE
        || name == presets::USER_FILE
}

#[cfg(test)]
mod tests {
    #[test]
    fn cmake_files() {
        assert!(super::is_cmake_file("src/CMakeLists.txt"));
        assert!(super::is_cmake_file("cmake/Warnings.cmake"));
        assert!(super::is_cmake_file("CMakePresets.json"));
        assert!(!super::is_cmake_file("src/main.cpp"));
    }
}

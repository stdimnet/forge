---
title: cforge-cmake
description: "CForge: CMake - profiles, CMakePresets.json / CMakeUserPresets.json, the cmake-file-api query and codemodel-v2 reply, configure logs, configure / build / clean / CTest command lines and CMakeLists completion; pure, also built to WebAssembly."
order: 301
tags: [crate, rust, pack, cpp, cmake]
---

# cforge-cmake

`packs/cforge/cmake` — CForge: everything about CMake that needs no I/O. The desktop backend
reads files and runs CMake; this crate decides. The WebAssembly build of browser mode shares
it.

## Purpose

- `presets`: `CMakePresets.json` and `CMakeUserPresets.json` (which includes the project
  file) with `include` files (cycles reported), configure, build and test presets,
  `inherits` (depth first, the first parent that sets a field wins; cache variables and
  environments merge, `null` unsets), `condition`s (`const`, `equals`, `notEquals`,
  `inList`, `notInList`, `matches`, `anyOf`, `allOf`, `not`) and macros (`${sourceDir}`,
  `${sourceParentDir}`, `${sourceDirName}`, `${presetName}`, `${generator}`,
  `${hostSystemName}`, `${fileDir}`, `${dollar}`, `${pathListSep}`, `$env{}`, `$penv{}`).
- `profile`: CMake profiles (`Profile`; the setting `cmake.profiles` holds `ProfileEntry`
  tables) with CLion's defaults (a Debug profile, `cmake-build-<name>`), preset profiles, and
  the command lines of configure (`-S`, `-B`, `-G`, `CMAKE_BUILD_TYPE`, the toolchain's
  variables, `CMAKE_EXPORT_COMPILE_COMMANDS`, options; `--preset=<name>` for presets), build,
  rebuild (`--clean-first`), clean and CTest's location.
- `fileapi`: the stateful client query (`.cmake/api/v1/query/client-forge/query.json`:
  `codemodel` 2, `toolchains` 1, `cmakeFiles` 1) and the reply: the newest index, the
  client's responses, configurations, projects, targets (kind, artifacts, sources with their
  language, dependencies, the `CMakeLists.txt` line that defines them from the backtrace
  graph), compilers and the project's CMake files.
- `log`: `CMake Error/Warning/Deprecation Warning/Warning (dev) at file:line (command):` blocks
  and `in file` / one-line `CMake Error:` messages of a configure, and whether it failed.
- `complete`: completion of CMake commands (signatures), the keywords of the command the
  caret is in, and `${VARIABLES}`.
- `sanitizers`: a profile's sanitizers (`address`, `undefined`, `thread`, `memory`, `leak`;
  `ProfileEntry.sanitizers`) as `-fsanitize=…` compiler and linker flags appended to the
  flags the options set, the combinations compilers reject (`Profile.sanitizerError`) and
  the runtime options of runs (`UBSAN_OPTIONS`, `ASAN_OPTIONS`).
- `packages`: vcpkg manifest mode (`vcpkg.json` dependencies, the root from the setting,
  `VCPKG_ROOT` or `VCPKG_INSTALLATION_ROOT`, its toolchain file, port completion in
  `vcpkg.json`) and Conan 2 (`conan install . -of <build> -s build_type=… --build=missing`,
  where `conan_toolchain.cmake` is generated). `ConfigureContext.package_toolchain` becomes
  `-DCMAKE_TOOLCHAIN_FILE` unless the profile sets its own.

## Modules

| Module | Summary |
| --- | --- |
| `presets.rs` | `load`, `Presets`, `RawPreset`, `ResolvedConfigure`, `ResolvedBuild`, `MacroContext`, `expand`, `condition_holds`. |
| `profile.rs` | `Profile`, `ProfileEntry`, `profiles`, `configure_command`, `build_command`, `BuildAction`, `Command`, `ConfigureContext`, `split_args`, `ctest_next_to`, `build_dir_of`. |
| `fileapi.rs` | `query_json`, `parse_reply`, `CMakeModel`, `Configuration`, `Target`, `TargetKind`, `Source`, `Location`, `QUERY_DIR`, `REPLY_DIR`. |
| `log.rs` | `parse_log`, `failed`, `ConfigureMessage`, `Severity`. |
| `complete.rs` | `complete`, `COMMANDS`, `VARIABLES`, `Completion`. |
| `sanitizers.rs` | `SANITIZERS`, `parse_list`, `conflict`, `compile_flags`, `link_flags`, `run_env`, `cmake_definitions`. |
| `packages.rs` | `PackageManagers`, `vcpkg_dependencies`, `vcpkg_root`, `vcpkg_toolchain`, `complete_vcpkg`, `conan_install_command`, `conan_toolchain_candidates`. |

## Used by

- IPC: `app/src-tauri/src/commands/cpp.rs` (`cmake_*`), `commands/cpp_tools.rs`
  (`cpp_packages`, `cpp_vcpkg_complete`, `cpp_conan_install_command`).
- [cforge-lang-cpp](cforge-lang-cpp.md) (command type, argument splitting).
- Browser mode: `crates/forge-wasm/src/cpp.rs`.

## Dependencies

- External: `serde`, `serde_json`

## Tests

Unit tests per module. `tests/fixtures/reply/` is a file API reply recorded from CMake 3.28
configuring a small project with Ninja (paths replaced by `/home/dev/shapes`);
`tests/fixtures/cmake-configure-*.txt` are recorded configure logs with errors and warnings.

```sh
cargo test -p cforge-cmake
```

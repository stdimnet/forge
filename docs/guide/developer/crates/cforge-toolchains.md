---
title: cforge-toolchains
description: "CForge: C/C++ toolchains on the user's machine - GCC, Clang, Apple Clang, MinGW, MSVC (vswhere) and cross compilers with CMake, Ninja, Make, GDB and LLDB; version outputs parsed in Rust."
order: 300
tags: [crate, rust, pack, cpp]
---

# cforge-toolchains

`packs/cforge/toolchains` — CForge: the toolchains of Settings | Build, Execution,
Deployment | Toolchains, like CLion's. A toolchain is a C and C++ compiler with CMake, a build
tool (Ninja, Make) and a debugger (GDB or LLDB).

## Purpose

- `version`: `gcc -v` / `clang -v` (target, thread model, installed dir, vendor such as
  `Ubuntu`, `MSYS2 project`, `Arm GNU Toolchain 13.3.Rel1`), Apple Clang, MSVC's `cl` banner
  and the first line of `--version` of CMake, Ninja, Make, GDB, LLDB and clang-format;
  `at_least` / `version_key` compare versions.
- `vswhere`: `vswhere -format json` installations (newest first), the newest MSVC tool set in
  `VC\Tools\MSVC`, `cl.exe` for a host / target architecture, `vcvarsall.bat` and the CMake and
  Ninja that Visual Studio bundles.
- `detect`: the toolchains of a machine — the system GCC and Clang, versioned compilers
  (`gcc-14`), cross compilers `<triple>-gcc` (with `<triple>-gdb` or `gdb-multiarch`; Debian's
  host aliases are skipped), MinGW-w64 folders on Windows and Visual Studio — with their
  problems ("GDB 12.1 has no DAP server", "No build tool"), the user's toolchains from the
  setting `cpp.toolchains` (`ToolchainEntry`, empty fields detected) and the CMake variables a
  toolchain sets (`CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER`, `CMAKE_MAKE_PROGRAM`).

Everything detection does to the machine goes through the `Probe` trait (`SystemProbe` for
the real one), so the tests describe Linux and Windows machines with a fake.

## Modules

| Module | Summary |
| --- | --- |
| `version.rs` | `parse_compiler`, `parse_tool_version`, `CompilerInfo`, `CompilerFamily`, `at_least`, `version_key`. |
| `vswhere.rs` | `parse`, `VsInstallation`, `newest_toolset`, `msvc_paths`, `VSWHERE_ARGS`. |
| `detect.rs` | `detect`, `all`, `from_entry`, `which`, `cmake_variables`, `Toolchain`, `ToolchainEntry`, `Probe`, `SystemProbe`. |

## Used by

- IPC: `app/src-tauri/src/commands/cpp.rs` (`cpp_toolchains`, `cpp_toolchain_inspect`, the
  configure command, the debugger and coverage tools of a profile).

## Dependencies

- External: `serde`, `serde_json`

## Tests

Unit tests with fixtures recorded on Ubuntu 24.04 (`gcc -v`, `clang -v`, `cmake --version`,
`ninja --version`, `make --version`, `gdb --version`, `lldb --version`,
`clang-format --version`) and documented-format samples (MinGW, `arm-none-eabi-gcc`, Apple
Clang, the `cl` banner, `vswhere` JSON) in `tests/fixtures/`.

```sh
cargo test -p cforge-toolchains
```

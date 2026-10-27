---
title: cforge-embedded
description: "CForge: embedded development - platformio.ini and the PlatformIO tasks, ESP-IDF and Zephyr projects with idf.py and west commands, OpenOCD's GDB server and flashing, CMSIS-SVD files with register decoding; pure, also built to WebAssembly."
order: 304
tags: [crate, rust, pack, cpp, embedded, platformio, esp-idf, zephyr, openocd, svd]
---

# cforge-embedded

`packs/cforge/embedded` — CForge's embedded development support without I/O: it reads
project files and builds command lines; the desktop backend starts them and reads memory
through the debugger. The WebAssembly build of browser mode shares it.

## Purpose

- `platformio`: `platformio.ini` (sections, multi-line values, `;` comments, `[env]`
  defaults, `extends` chains with loop detection, `${section.key}` / `${this.key}`
  interpolation, `default_envs`) into `PioProject` / `PioEnv` (platform, board, frameworks,
  every resolved option, the section's line), and the task commands (`pio run -e <env> -t
  upload|clean|compiledb`, `pio test`, `pio device monitor`).
- `espidf`: detection (`sdkconfig`, `idf_component.yml`, the IDF's `project.cmake`), the
  chip target of `sdkconfig`, and `idf.py build|flash|monitor|menuconfig|fullclean` with
  `IDF_PATH`, its Python, the serial port and baud rate.
- `zephyr`: detection (`west.yml`, `prj.conf` with `find_package(Zephyr)`), the board of a
  build's `CMakeCache.txt`, boards an application defines in `boards/`, and `west build -b
  <board> -d <dir> -p auto|always <app>`, `west flash`, `west debug`.
- `openocd`: `openocd -f <cfg>… -c "gdb_port N"` (the GDB server), `-c "program {<elf>} verify
  reset exit"` (flashing), the ready line and GDB's `target remote` address.
- `svd`: CMSIS-SVD (`device` → `peripherals` with `derivedFrom` → `registers` / nested
  `cluster`s with `dim` arrays → `fields` with `bitOffset`/`bitWidth`, `bitRange` or
  `lsb`/`msb`, `dim` and `enumeratedValues`), with size, access and reset value inherited from
  the device and peripheral; `read_ranges` (adjacent readable registers; write-only ones and
  those with a `readAction` are skipped) and `decode` (little-endian register values and
  field values with their enumerated names).
- `detect`: which of PlatformIO, ESP-IDF and Zephyr a project uses (`EmbeddedKinds`).

## Modules

| Module | Summary |
| --- | --- |
| `platformio.rs` | `parse`, `list`, `PioProject`, `PioEnv`, `PioTask`, `task_command`. |
| `espidf.rs` | `detect`, `target`, `IdfAction`, `IdfSetup`, `command`. |
| `zephyr.rs` | `detect`, `board_of_cache`, `local_boards`, `WestAction`, `command`. |
| `openocd.rs` | `OpenOcdSetup`, `server_command`, `program_command`, `is_ready_line`, `gdb_target`. |
| `svd.rs` | `parse`, `number`, `Device`, `Peripheral`, `Register`, `Field`, `read_ranges`, `decode`, `RegisterValue`, `DeviceSummary`. |
| `lib.rs` | `ToolCommand`, `EmbeddedKinds`, `detect`. |

## Used by

- IPC: `app/src-tauri/src/commands/embedded.rs` (`embedded_info`, `pio_command`,
  `idf_command`, `west_command`, `cpp_embedded_launch`, `cpp_svd_peripherals`,
  `cpp_svd_read`).
- Browser mode: `crates/forge-wasm/src/cpp_tools.rs`.

## Dependencies

- Internal: [cforge-analyzers](cforge-analyzers.md) (`xml`)
- External: `serde`

## Tests

Unit tests per module; fixtures `tests/fixtures/platformio.ini` (defaults, `extends`,
interpolation, multi-line values) and `tests/fixtures/stm32f4-mini.svd` (a trimmed STM32F401
description with `derivedFrom`, register and field arrays, a cluster and enumerated values).

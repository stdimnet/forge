---
title: dotnetforge-gamedev
description: "DotNetForge: Unity and Godot - Unity projects and Unity Hub editors, the Forge editor bridge (package source, protocol, async client), Unity tests in batch mode with NUnit results, Unity processes for debugging; Godot projects, the Godot executable, run configurations, the editor's GDScript language server and debug adapter, GdUnit4."
order: 118
tags: [crate, rust, pack, unity, godot]
---

# dotnetforge-gamedev

`packs/dotnetforge/gamedev` — DotNetForge: Unity and Godot in the style of Rider. See
[Unity and Godot](../../user/unity-godot.md).

## Purpose

The logic of the Unity (`forge.unity`) and Godot (`forge.godot`) plugins. The file system
and the process list come in as data or functions, so everything is tested with fixtures and
fakes; `app/src-tauri/src/commands/gamedev.rs` wires it to IPC.

### Unity (`unity`)

- `project`: `ProjectSettings/ProjectVersion.txt` (`m_EditorVersion`, the revision),
  `Packages/manifest.json`, the IDE packages that write project files, the Test Framework,
  the bridge package (embedded or in the manifest, update available), the root `.sln` /
  `.csproj` files, `Library/EditorInstance.json` (the editor's pid).
- `editors`: Unity Hub folders per OS, `secondaryInstallPath.json`, manually located editors
  (`editors-v2.json`, `editors.json`), Settings' editor; `match_editor` picks the exact
  version, else the newest of the same `major.minor`. `version_key` orders Unity versions
  (`a` < `b` < `f` < `p`).
- `package`: the bridge package (`dev.forge.unity`, C# in `unity-package/`, embedded with
  `include_str!`), its install plan below `Packages/dev.forge.unity`, version checks.
- `bridge`: protocol 1 (see [Unity editor bridge protocol](../unity-bridge-protocol.md)):
  `Library/ForgeBridge.json`, messages, console message locations (compiler messages, stack
  frames), and `BridgeClient` (tokio): handshake, commands with acknowledgements, events until
  `Disconnected`.
- `commands`: Start Unity (`-projectPath`), project files in batch mode (`-executeMethod`
  of the bridge or the Rider package).
- `test_runner`: the `unity-test` provider of `forge-testing`: NUnit / `[UnityTest]`
  discovery (DotNetForge's C# scanner), `-runTests -testPlatform -testResults -testFilter`,
  location rules from the test classes, Rerun Failed Tests, the refusal while an editor has
  the project open. Results: `forge_output::formats::nunit`.
- `processes`: Unity editors in a process list (`-projectPath`, the editor instance's pid)
  and their Mono debugger port (`56000 + pid % 1000`).

### Godot (`godot`)

- `project`: `project.godot` (multi-line values, `PackedStringArray`, autoloads, `[dotnet]`),
  scenes, `res://` paths.
- `executable`: Settings, `GODOT4` / `GODOT`, `PATH` names, Flatpak, macOS apps, downloaded
  `Godot_v4…` executables (also inside extracted .NET folders); .NET builds first for C#
  projects; `godot --version` parsing.
- `commands`: the `godot` run configuration (`--path`, scene, `--headless`, options,
  arguments after `--`), the editor (`--editor`), `dotnet build` before C# debugging.
- `lsp`: the GDScript server config (`tcp://host:6005`, see `forge_lsp::launch`) and a port
  probe.
- `dap`: `GodotAdapter`, a `forge_dap::DebugAdapterDescriptor` that connects to the editor's
  debug adapter (6006) and sends `launch` (`project`, `scene`, `playArgs`) or `attach`.
- `gdunit`: the `gdunit4` provider: `GdUnitTestSuite` discovery, GdUnit4's command line
  (`-a`, `-rd`), suite location rules, reruns by suite. The app reads GdUnit4's
  `report_<n>/results.xml` below the report folder.

## Tests

Unit tests per module; `tests/bridge.rs` (a fake Unity peer), `tests/godot_editor.rs` (a fake
GDScript language server and a fake Godot debug adapter on loopback, driven through
`forge-lsp` and `forge-dap`). No Unity, Godot or network is needed.

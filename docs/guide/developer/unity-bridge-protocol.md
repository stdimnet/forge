---
title: Unity editor bridge protocol
description: The localhost protocol between Forge and the Forge editor bridge package in the Unity editor (protocol 1) - discovery through Library/ForgeBridge.json, the token handshake, commands with acknowledgements and the events Unity streams.
order: 125
tags: [unity, protocol, editor bridge, dotnetforge]
---

# Unity editor bridge protocol

The **Forge editor bridge** (`dev.forge.unity`, MIT) is a Unity editor package whose C# source
ships with Forge in `packs/dotnetforge/gamedev/unity-package/dev.forge.unity`. Forge copies it
into a project's `Packages/` folder when the user installs it
([Unity and Godot](../user/unity-godot.md#the-forge-editor-bridge)). Inside the Unity editor it
listens on a loopback TCP port; Forge's client is `dotnetforge_gamedev::unity::bridge`. Rider's
Unity plugin works the same way (the editor listens, the IDE connects).

This page is protocol version **1** (`bridge::PROTOCOL`, `ForgeBridge.Protocol`).

## Discovery

When the editor loads the package (`[InitializeOnLoad]`, not in batch mode), the bridge:

1. listens on `127.0.0.1` with a free port (never on other interfaces);
2. creates a random 48-hex-digit token;
3. writes `Library/ForgeBridge.json` in the project:

   ```json
   {
     "port": 52817,
     "pid": 4242,
     "token": "3f9a1c…",
     "protocol": 1,
     "unityVersion": "2022.3.10f1",
     "projectPath": "/home/sam/Shooter"
   }
   ```

Before each script reload (`AssemblyReloadEvents.beforeAssemblyReload`) and when the editor
quits, it closes the listener and its connections and deletes the file if it still holds its
token. After the reload a new listener with a new port and token writes the file again; Forge
reconnects. Only processes of users who can read the project's `Library` folder learn the
token.

## Framing

- One JSON object per line, UTF-8, terminated by `\n` (`\r\n` is accepted).
- At most 1 MiB per message (`bridge::MAX_MESSAGE`); longer lines end the connection.
- Every message has a `type`. Unknown types and unknown fields are ignored on both sides, so a
  newer bridge stays compatible.

## Handshake

Forge sends first:

```json
{"type": "hello", "token": "3f9a1c…", "client": "Forge", "protocol": 1}
```

With the right token the editor answers with its state:

```json
{"type": "welcome", "protocol": 1, "unityVersion": "2022.3.10f1", "projectPath": "/home/sam/Shooter", "pid": 4242, "playMode": "edit", "compiling": false}
```

With a wrong token, or any other first message, it answers
`{"type": "error", "message": "wrong token"}` and closes the connection.

## Commands

Commands carry an `id` (a positive integer, unique per connection):

```json
{"type": "play", "id": 1}
```

| `type` | What Unity does |
| --- | --- |
| `play` | `EditorApplication.isPlaying = true` |
| `pause` | `EditorApplication.isPaused = true` |
| `resume` | `EditorApplication.isPaused = false` |
| `stop` | `EditorApplication.isPlaying = false` |
| `step` | `EditorApplication.Step()` |
| `refresh` | `AssetDatabase.Refresh()` |
| `syncSolution` | regenerate the `.sln` / `.csproj` files with the installed IDE package's generator (`ForgeProjectFiles.Sync`) |

Commands run on Unity's main thread; the editor answers each one:

```json
{"type": "ack", "id": 1, "ok": true, "message": ""}
```

`ok: false` carries the reason in `message` (an unknown command, no IDE package to generate
project files, an exception). Forge waits 15 seconds for an answer (Unity may be compiling or
show a modal dialog).

## Events

The editor streams to every connected client:

| `type` | Fields | When |
| --- | --- | --- |
| `state` | `playMode` (`edit`, `playing`, `paused`), `compiling` | Play mode or pause changes, compilation starts or ends |
| `log` | `level` (`log`, `warning`, `error`, `exception`, `assert`), `message`, `stackTrace`, `time` (`HH:mm:ss`) | every console message (`Application.logMessageReceivedThreaded`) |
| `openFile` | `path` (absolute, `/` separated), `line`, `column` (1-based) | a script is opened in Unity while Forge is the external script editor |

Forge computes the location of a console message in Rust: a compiler message's
`Assets/…/File.cs(line,column):`, else the first stack frame `(at Assets/…/File.cs:line)` or
`in /path/File.cs:line`.

## Opening files from Unity

`ForgeCodeEditor` implements `Unity.CodeEditor.IExternalCodeEditor`. When Unity opens a code
asset (`.cs`, `.shader`, `.hlsl`, `.uss`, `.uxml`, `.asmdef`, …) it sends `openFile` to the
connected Forge; without a connection it starts the chosen Forge executable with
`<project> <file>:<line>:<column>` (the [launcher](../user/browser-companion.md) syntax).
Other assets are left to Unity.

## Tests

`packs/dotnetforge/gamedev/tests/bridge.rs` drives `BridgeClient` against a fake Unity peer on
loopback (handshake, wrong tokens, commands and answers, events, closing).
`unity::package` tests check that the shipped C# source declares the same protocol version,
instance file and commands.

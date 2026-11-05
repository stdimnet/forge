# Plan: Code With Me (`forge.codewithme`)

Status: **proposal, decision-ready**. Nothing here is implemented yet. The feature ships as
**one branch and one pull request** (`feat/code-with-me`), built in ordered stages that each
build, pass the quality gates and can be tested on their own (see "Stages").

Code With Me lets a host invite guests into the project that is open in their Forge. Guests
edit the same files at the same time, see each other's carets and selections, follow each
other, and, with the host's permission, use the host's terminal and run configurations. It is
the Forge counterpart of JetBrains Code With Me in PHPStorm, with the same layout, wording and
flows, but without any JetBrains or other proprietary service: connections go directly from
guest to host, and an optional relay server can be self-hosted.

Context: JetBrains announced in March 2026 that it is phasing out Code With Me. 2026.1 is the
last IDE release that supports it, it is now a Marketplace plugin, and the public relay
infrastructure shuts down in Q1 2027
([announcement](https://blog.jetbrains.com/platform/2026/03/sunsetting-code-with-me/)). PHPStorm
users who pair-program will be looking for a replacement, and Forge's must not depend on a
vendor's relay.

Contents: [Goals](#goals-and-non-goals) · [Behaviour](#user-facing-behaviour) ·
[Architecture](#architecture) · [Technology decisions](#technology-decisions) ·
[Security](#security-and-threat-model) · [Testing](#testing-strategy) ·
[Performance](#performance-targets) · [Stages](#stages-one-pr) ·
[Reviewing one large PR](#keeping-one-large-pr-reviewable) ·
[Open questions](#open-questions) · [Risks](#risks)

## Goals and non-goals

**Goals**

- Real-time co-editing of the host's project files: concurrent typing converges, each
  participant has their own undo, and colored carets, selections and name labels are shown.
- PHPStorm's flows 1:1: the Code With Me toolbar icon and popup, Start Session with
  permissions, Copy Session Link, the host accepting each guest with a verification code, the
  participant list with Follow, Jump To, Make Everyone Follow You and Remove Guest, Leave and
  End Session, and Settings | Tools | Code With Me.
- A Forge guest gets the host's project as a **project backend**: project tree, Go to File,
  Find in Files, Search Everywhere, language server features (hover, completion, diagnostics,
  Go to Declaration), the PHP index and framework analysis, and git status and blame. All of
  it is computed on the host, the way remote SSH projects are computed in the agent.
- Permission levels Read-only, Edit files, Full access and Custom, **enforced on the host**. A
  guest never gets file, terminal, process, git or settings access beyond its level, whatever
  its client sends.
- A shared terminal (read-only or writable) and shared Run: guests see the host's runs, and
  with permission they start the host's run configurations.
- Works on a LAN, through a port forward, through an SSH tunnel, and across NATs with an
  optional relay that the user can host themselves. No account and no central service.
- End-to-end encryption between guest and host. Relays only see ciphertext.
- An audit log of what guests did.

**Non-goals (for this PR)**

- **Voice and video calls, and screen sharing.** PHPStorm offers "Join Call" in the toolbar.
  A call needs a media stack (WebRTC with echo cancellation, an SFU for more than two people)
  that is larger than the rest of this feature, and WebKitGTK builds often ship without WebRTC.
  People already have calls (Jitsi, Matrix, Signal, Teams). See [Open questions](#open-questions).
- **A browser guest** (a thin web client served by the host). It is feasible and is sketched
  under "Guest architecture", but it is a follow-up.
- **Port forwarding** (PHPStorm's Code With Me tool window has a Port Forwarding tab). This is
  a follow-up. The transport already multiplexes streams, so it is a small addition later.
- Shared debugging (stepping in the host's Xdebug session). Guests see the host's Debug tool
  window output read-only in a follow-up. Controlling the debugger is later.
- Sharing AI chat sessions, data sources (database credentials) or code host accounts.
  These run with the host's secrets and are never exposed to guests.
- Guests co-hosting, or a session that outlives the host. As in PHPStorm, the session ends
  when the host ends it or closes the project.
- "Pair Coding Mode" (PHPStorm keeps a single shared caret). Follow covers the common case.
- Offline editing with a later merge.

**Scope compared to PHPStorm**

| PHPStorm Code With Me | Forge |
| --- | --- |
| Toolbar icon, Start Session, Copy Session Link, End Session | Same |
| Read-only / Edit files / Full access / Custom | Same names; Custom has Files, Terminal, Run, Version control |
| Host accepts each guest, matching codes | Same |
| Guests use a downloaded JetBrains Client | Guests use Forge itself (the same binary); browser guest is a follow-up |
| Lobby server and JetBrains relays | Direct connection; optional self-hosted relay; no lobby |
| Follow, Jump To, Make Everyone Follow You, Remove Guest | Same |
| Pair Coding Mode | Not planned |
| Shared terminal, run and debug (Full access) | Terminal and Run with their own levels; debug later |
| Port forwarding | Follow-up |
| Voice and video calls, screen sharing | Not planned |
| Participant limit set by the licence | Tested with 1 host + 9 guests, no hard limit |

## User-facing behaviour

### Where it lives

- **Main toolbar** (`registerToolbarItem`, `app/ui/src/features/toolbarItems.ts`): a Code With
  Me icon (lucide `users`) left of the Run widget, as in PHPStorm's New UI. During a session
  the participants' avatars (colored circles with initials) appear next to it. Clicking an
  avatar follows that participant; its tooltip is the name and permission.
- **Main menu**: `Tools | Code With Me` (manifest `[[contributions.mainMenu]]`, path
  `Tools/Code With _Me`), with the same entries as the toolbar popup.
- **Welcome screen**: "Join Session…" under the project list (next to Remote Development), for
  guests who have no project open.
- **Code With Me tool window** (`registerToolWindow`, right anchor, only while a session is
  running, icon `users`): tabs **Participants** and **Session Log**. Port Forwarding is added
  in a follow-up.
- **Status bar** (`registerStatusItem`): for a guest, "Code With Me: <host name>" with the
  latency, like the SSH status item (`features/remote/RemoteStatusItem.tsx`); clicking it
  opens the toolbar popup.
- **Editor**: remote carets with a name label above them, remote selections in the
  participant's color, and a thin line marker in the scrollbar for each remote caret. While
  following someone, a banner above the editor: "Following <name>" with a **Stop** link.
- **Settings | Tools | Code With Me** (`registerSettingsPage` with id `tools.code-with-me`).

### Host: start a session

1. Click the toolbar icon (or Tools | Code With Me | **Start Session…**). The popup shows the
   permission choice and a **Start Session** button, as in PHPStorm:
   - **Read-only**: "Guests can view files, the terminal and run output." (default)
   - **Edit files**: "Guests can edit files, but cannot use the terminal or run code."
   - **Full access**: "Guests can edit files, use the terminal and run code on your machine."
   - **Custom…**: opens the Custom Permissions dialog (below).
2. **Start Session** starts listening and copies the link to the clipboard. A balloon says
   "Session started. The invitation link is copied to the clipboard." with **Copy Session
   Link** and **Settings** actions.
3. In a session the popup shows the participant list, then **Copy Session Link**,
   **Permissions** (submenu with the four levels, checked), **Make Everyone Follow You**,
   **Settings**, and **End Session**.
4. **End Session** asks "End the session for all participants?" (setting to skip the
   question). The link stops working at once; guests see "The host ended the session."

Custom Permissions dialog (one row per area, values as PHPStorm's admin restrictions name them):

| Area | Values |
| --- | --- |
| Files | Read-only · Edit |
| Terminal | No access · Read-only · Full access |
| Run | No access · Read-only (see output) · Full access (start, stop, rerun) |
| Version control | Read-only (status, diff, blame, log) · Full access (commit, push, checkout, …) |

The four presets map onto this table: Read-only = Read-only / Read-only / Read-only /
Read-only; Edit files = Edit / No access / No access / Read-only; Full access = Edit / Full /
Full / Full. Permissions apply to the whole session by default; the participant context
menu offers **Permissions** per guest as well, which PHPStorm also has.

### Guest: join

1. The host sends the link (`forge://cwm/join?...#...`, see "Session links"). Clicking it
   opens Forge through the existing `forge://` deep link handling (`crates/forge-open`,
   `tauri-plugin-deep-link`). Alternatively the guest pastes the link in **Join Session…**
   (Welcome screen, or Tools | Code With Me).
2. The Join Session dialog shows the host's name and machine as the link describes them, the
   user name to join with (from the settings), and **Join**. If the guest has a project open,
   the dialog says that it will be closed (unsaved changes are asked for as when opening
   another project).
3. While waiting: "Waiting for <host> to accept…" with the **verification code** (two groups
   of three digits, e.g. `482 913`) and **Cancel**.
4. The host gets a balloon that stays until answered: "**<guest name>** wants to join the
   session. Make sure the code is **482 913**." with **Accept**, **Decline** and the requested
   permission level (the session default; the host may change it before accepting).
5. Accepted: the guest's window shows the host's project (title `<project> — <host name>
   (Code With Me)`), the tool windows that make sense for guests, and the host's open
   editor; the guest follows the host by default (setting). Declined: "The host declined
   your request." Timeout after 2 minutes: "The host did not answer."
6. **Leave Session** in the popup or the status item ends it for this guest.

Guests see the tool windows Project, Structure, Find, Problems, Git (read-only unless Version
control is Full access), Terminal and Run (per permission), and Code With Me. They do not see
AI Chat, Database, Services, Debug or the code host tool windows.

### Participants, carets and follow

- Each participant gets a color from a fixed palette of 8 theme tokens (`--collab-1` …
  `--collab-8`, added to `app/ui/src/theme/theme.css` for Ember and Anvil) in join order.
  The host is always color 1.
- Participants list (popup and tool window): avatar, name, "(host)" / "(you)", permission,
  the file they are in, and a context menu:
  - **Follow** / **Stop Following**: open the participant's file, keep their caret and
    viewport in view. Following ends when you type, click into the editor or open another
    file (PHPStorm behaviour), and with **Stop** in the banner.
  - **Jump To**: open their file at their caret once, without following.
  - **Permissions ▸** (host, per guest).
  - **Remove Guest** (host): disconnects the guest at once. Removing does not ban: the guest
    can knock again with the same link, so the host may also **Regenerate Link** (see open
    questions).
- **Make Everyone Follow You** (host; PHPStorm's forced follow, the "summon"): every guest
  follows the host. Guests can still press Stop. The host sees who is following.
- Remote carets: a 2 px caret in the participant's color with a name label that fades after 2 s
  without movement, and selections at 25 % opacity. Rendered by the y-codemirror binding with
  Forge's theme (`editor/forgeTheme.ts`).

### Terminal and Run

- **Terminal**: the host shares a terminal tab from a new tab context menu (**Share with
  Guests**). New tabs that the host opens during a session are not shared unless they choose
  to (setting "Share new terminal tabs"). Guests with Terminal Read-only see shared tabs live
  and cannot type; with Full access they can type in shared tabs and open new terminals on the
  host (titled "<guest>: Local").
- **Run**: guests with Run Read-only see every run in the host's Run tool window live. With
  Full access they can start, stop and rerun the host's run configurations (the Run widget
  shows the host's list). Guests cannot edit run configurations unless Files is Edit and
  Run is Full access, because a configuration is a command line.

### Settings | Tools | Code With Me

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `codeWithMe.userName` | string | `""` (system name) | "User name", with a **Use System Name** button |
| `codeWithMe.defaultPermission` | enum | `readOnly` | Preselected permission in Start Session |
| `codeWithMe.listenPort` | int | `0` (any) | UDP port for direct connections; set it for a port forward |
| `codeWithMe.listenAddress` | string | `""` (all) | Bind to one interface, e.g. a VPN or `127.0.0.1` for SSH tunnels |
| `codeWithMe.relayUrl` | string | `""` | Relay server URL ("Relay server", replaces PHPStorm's "Lobby server URL"); empty = direct only |
| `codeWithMe.usePublicRelays` | bool | `false` | Opt in to the public iroh relays run by n0 (see open questions) |
| `codeWithMe.lanDiscovery` | bool | `true` | Find the host on the local network without the relay |
| `codeWithMe.excludedFiles` | string list | see "Secret files" | Files guests never see |
| `codeWithMe.maxSharedFileSize` | int (KB) | `5120` | Larger files open read-only for guests |
| `codeWithMe.followHostOnJoin` | bool | `true` | Guests start by following the host |
| `codeWithMe.confirmEndSession` | bool | `true` | Ask before ending a session |
| `codeWithMe.shareNewTerminals` | bool | `false` | Share terminal tabs opened during a session |
| `codeWithMe.auditLog` | bool | `true` | Keep the session log in the user data directory |

The relay token (for relays with token access control) is a secret and goes into the OS
keychain (service `forge-cwm`), not into the settings.

### Keymap actions

PHPStorm binds no shortcuts to Code With Me actions, so these are actions without default
shortcuts that show up in Find Action and in Settings | Keymap under "Plugins/Code With Me".
The ids below are Forge's proposal. Before implementation, read the real ids from PHPStorm
2026.1 with the Code With Me plugin (Keymap search "Code With Me"); where they differ, keep
PHPStorm's id as the command id and add Forge's as `ACTION_ALIASES` (`crates/forge-keymap`).

| Action id | Title |
| --- | --- |
| `CodeWithMe.StartSession` | Start Session… |
| `CodeWithMe.CopySessionLink` | Copy Session Link |
| `CodeWithMe.EndSession` | End Session |
| `CodeWithMe.JoinSession` | Join Session… |
| `CodeWithMe.LeaveSession` | Leave Session |
| `CodeWithMe.Permissions` | Permissions… |
| `CodeWithMe.FollowParticipant` | Follow Participant… (chooser popup) |
| `CodeWithMe.StopFollowing` | Stop Following |
| `CodeWithMe.MakeEveryoneFollow` | Make Everyone Follow You |
| `CodeWithMe.JumpToParticipant` | Jump To Participant… |
| `CodeWithMe.ShowToolWindow` | Code With Me (tool window) |

Registered with `registerKeymapActions` (`app/ui/src/keymap/store.ts`) and implemented with
`registerCommand`. Undo and Redo keep their PHPStorm actions (`$Undo`, `$Redo`); in a shared
document they run the participant's own undo manager (see "Undo").

## Architecture

### Overview

```
 Host Forge                                                     Guest Forge
┌──────────────────────────────────────────────┐            ┌─────────────────────────────────┐
│ UI (webview)                                 │            │ UI (webview)                    │
│  CodeMirror ─ y-codemirror.next ─ Y.Doc/file │            │  CodeMirror ─ y-codemirror ─ Y.Doc
│  features/code-with-me (toolbar, popup,      │            │  features/code-with-me          │
│   tool window, follow, approval balloon)     │            │  (same feature, guest mode)     │
│        │ collab_* IPC, collab://* events     │            │        │ collab_* IPC / events   │
├────────┼─────────────────────────────────────┤            ├────────┼────────────────────────┤
│ forge-collab::host                           │            │ forge-collab::guest             │
│  Session, Participants, Policy (permissions) │   QUIC     │  GuestSession                   │
│  DocStore: yrs Doc per shared file ──────────┼──(iroh)────┼─ forwards doc bytes to the UI   │
│  Disk bridge (save, reload diff, rename)     │  TLS 1.3   │  CollabBackend: impl Backend ───┼─▶ AppState::backend()
│  Host RPC: filtered Backend calls ───────────┤  direct or │   (tree, read, search, git,     │   (all existing commands
│  LSP / search / terminal / run forwarders    │  via relay │    php, framework, terminal,    │    work unchanged)
│        │                                     │            │    run, LSP requests)           │
│  AppState::backend() (Local or Remote)       │            └─────────────────────────────────┘
│  LspManager, terminals, runs, file watcher   │
└──────────────────────────────────────────────┘                optional: iroh-relay (self-hosted)
```

- **Star topology.** Every guest has one encrypted connection to the host; guests never talk
  to each other. The host fans out document updates and presence. This matches PHPStorm
  ("a separate end-to-end encrypted channel with each guest") and puts every permission
  check in one place.
- **The host's backend stays as it is.** The host may itself work on a remote SSH project:
  Code With Me sits on top of `AppState::backend()`, so a host on an SSH project can share it,
  and guests reach the SSH host through the Forge host.
- **The guest's backend is new**: `CollabBackend` implements `forge_backend::Backend`, so the
  guest's existing workspace, file system, search, git, PHP, framework, terminal and run
  commands work without changes, exactly as `RemoteBackend` made SSH projects work.

### Crates, plugin and contribution points

- **`crates/forge-collab`** (new), with features like `forge-remote`:
  - pure part (no tokio, no network; builds to WebAssembly for browser mode through
    `crates/forge-wasm/src/collab.rs`): `link` (session link and ticket encode and parse),
    `permissions` (levels, Custom table, the `Policy` that decides every guest request),
    `protocol` (frames, reusing `forge_backend::protocol::{encode_frame, FrameDecoder}`),
    `presence` (participants, colors, follow state), `excludes` (secret-file globs, built on
    the `.gitignore` matcher `forge-core` already uses), `verify` (verification code),
    `audit` (log records);
  - `runtime` feature: `host` (session, accept queue, `DocStore` with yrs, disk bridge, host
    RPC dispatcher), `guest` (`GuestSession`, `CollabBackend`), `transport` (the `Transport`
    trait, the iroh implementation and an in-memory implementation for tests).
- **Plugin manifest** `app/src-tauri/plugins/code-with-me.toml`: id `forge.codewithme`, pack
  `core`, no `projectTypes` (always active), contributions: commands, the tool window
  `code-with-me`, the `codeWithMe.*` settings in group `tools.code-with-me`, and the
  `Tools/Code With _Me` main menu entries. Regenerate the browser snapshot afterwards
  (`FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`).
- **Tauri shell** `app/src-tauri/src/commands/collab.rs`: thin `#[tauri::command]` wrappers
  and the event forwarding, plus a `CollabState` managed with `.manage(...)`. The project
  open path (`state.rs`) gains `set_collab_project(backend)` next to `set_remote_project`.
- **UI feature** `app/ui/src/features/code-with-me/`: `index.ts` (registrations), `store.ts`
  (session and participants signals), `ToolbarButton.tsx` and `SessionPopup.tsx`,
  `JoinDialog.tsx` (`registerDialog`), `ApprovalBalloon.tsx`, `ParticipantsView.tsx` and
  `SessionLogView.tsx` (tool window tabs), `CodeWithMePage.tsx` (settings page), `binding.ts`
  (the CodeMirror extension through `registerEditorExtension`), `follow.ts`.
  Typed client `app/ui/src/ipc/collab.ts`; browser mock `app/ui/src/mock/collabMock.ts`.
- Contribution points used: `registerToolbarItem`, `registerToolWindow`, `registerCommand`,
  `registerKeymapActions`, `registerSettingsPage`, `registerStatusItem`,
  `registerEditorExtension`, `registerMainMenuEntries` / manifest `mainMenu`,
  `registerDialog`, and a new context menu on terminal tabs (Terminal has none yet).
- Small changes to existing code (each listed in the stage that needs it):
  `state/editor.ts` (shared tabs: save through the session, `saved` state from the host),
  `editor/CodeEditor.tsx` (history compartment swap), `features/mainMenu` (`$Undo` /
  `$Redo` for shared views), `state/fsWatch.ts` (host skips its own reload for shared tabs),
  `features/terminal` (shared tabs), `features/run` (guest Run widget), `shell/MainToolbar.tsx`
  (title for guests), `features/lsp` (guests ask the host), `theme.css` (collab tokens).

### IPC commands and events

All commands live in `src/ipc/collab.ts` and have mocks in `src/mock/collabMock.ts`.

| Command | Side | Reply |
| --- | --- | --- |
| `collab_start({ permissions })` | host | `{ sessionId, link, addresses }` |
| `collab_end()` | host | `()` |
| `collab_link()` | host | `string` (Copy Session Link) |
| `collab_regenerate_link()` | host | `string` |
| `collab_answer_join({ requestId, accept, permissions? })` | host | `()` |
| `collab_set_permissions({ participantId?, permissions })` | host | `()` (no id = session default) |
| `collab_remove_guest({ participantId })` | host | `()` |
| `collab_share_terminal({ terminalId, shared })` | host | `()` |
| `collab_parse_link({ link })` | guest | `{ hostName, hostLabel, sessionId, relay? } \| error` |
| `collab_join({ link, userName })` | guest | `{ status: "waiting", code } \| { status: "failed", error }` |
| `collab_leave()` | both | `()` |
| `collab_status()` | both | `SessionView` (role, participants, permissions, me) |
| `collab_doc_open({ path })` | both | `{ docId, update: bytes, saved: SaveState, readOnly }` |
| `collab_doc_update({ docId, update })` | both | `()` (Yjs update from the UI) |
| `collab_doc_close({ docId })` | both | `()` |
| `collab_doc_save({ docId })` | both | `SaveState` |
| `collab_awareness({ docId, update })` | both | `()` |
| `collab_presence({ activeFile?, viewport?, caret? })` | both | `()` |
| `collab_follow({ participantId? })` | both | `()` (none = stop) |
| `collab_follow_everyone({ on })` | host | `()` |
| `collab_audit_log({ limit? })` | host | `AuditRecord[]` |

Events: `collab://status` (session state, participants, permissions), `collab://join-request`
`{ requestId, name, code, machine }` (host), `collab://doc/<docId>` `{ update?, awareness?,
saved?, closed?, renamed? }`, `collab://presence` `{ participantId, activeFile, viewport,
caret }`, `collab://follow` `{ leader?, forced }`, `collab://ended` `{ reason }`.

Doc updates cross the Tauri IPC as binary (`Uint8Array` / `tauri::ipc::Response` raw bytes)
to avoid base64 for large initial states.

**Browser mode**: `collabMock.ts` simulates a host with one guest, "Alex", so screenshots and
Playwright work without Rust. It keeps a Y.Doc per file, replays a short scripted edit session
(Alex types in `OrderController.php`, moves the caret, selects a block), and emits
`collab://join-request` on Start Session. Link parsing and permission decisions call the same
Rust code through WebAssembly (`callWasm("collab_parse_link")`, `collab_policy_check`). A
query parameter `?collab=host|guest` starts the demo in either role. The mock never writes
into a real project (the demo files are in memory, as for every mock).

### Document sync

**Model.** One Yjs document per shared file, with one `Y.Text` named `content`. The file tree
is not a CRDT; the host's file system is the truth for which files exist (see "File
operations").

**Replicas.**

- Each participant's webview holds a `Y.Doc` for each open shared file, bound to its
  CodeMirror view by `y-codemirror.next` (`yCollab(ytext, awareness, { undoManager })`).
  This is per-keystroke editor work, which the architecture rule leaves in TypeScript.
- The host's `forge-collab` holds the authoritative `yrs` replica of every shared file
  (`DocStore`). It is created with `OffsetKind::Utf16` so indexes match JavaScript and
  CodeMirror. It serves late joiners, writes to disk, applies disk changes, and gives the
  host's LSP and search the current text.
- A guest's Rust side holds no replica. It forwards bytes between its webview and the host.

**Wire protocol per document** (Yjs sync protocol, `yrs::sync::protocol` on the host,
`y-protocols/sync` in the UI):

1. Open: the participant sends `SyncStep1(stateVector)`; the host answers `SyncStep2(diff)`
   plus its own `SyncStep1`, the participant answers with what the host is missing.
2. Then incremental `Update(bytes)` messages. The host applies each guest update to its yrs
   replica **after the policy check** (a guest without Files Edit has its updates dropped and
   the session log notes it), then forwards it to every other participant that has the
   document open.
3. Awareness (`y-protocols/awareness`): `{ user: { name, color, colorLight }, cursor: {
   anchor, head } }` with Yjs relative positions, throttled to 20 Hz, forwarded by the host.
   The host stamps the participant id and color; a guest cannot pretend to be someone else.

Updates are idempotent and commutative, so reconnects simply rerun step 1/2.

**Opening a file.** `openFile(path)` (`state/editor.ts`) in a session calls `collab_doc_open`
instead of `readText`. The host creates the yrs document from the file on disk if no one has it
open, using a deterministic initial update (the host's client id) so concurrent opens never
duplicate the text. Files larger than `codeWithMe.maxSharedFileSize`, and files that are not
valid UTF-8 text, open read-only from a snapshot (`readOnly: true`).

**Editor tab state.** `EditorTab.content` is still updated from CodeMirror
(`CodeEditor.tsx` update listener), so all existing features (LSP sync, line markers, AI
context) keep working. The `createEffect` that calls `replaceDocument` on content changes
must skip shared tabs: remote edits arrive as Yjs changes through the binding, never as whole
documents. Line endings: the host's tab remembers the file's `eol`; buffers stay `\n`.

**Saving.**

- Ctrl+S on any participant with Files Edit sends `collab_doc_save`. The host writes the
  current yrs text (with the file's line ending) through `Backend::write_file`, then broadcasts
  `SaveState { savedHash, stateVector }`. Every participant sets `EditorTab.saved` from it, so
  the dirty marker is the same everywhere (dirty = local text differs from the saved text).
- Save All, autosave on frame deactivation (when Forge adds it) and the host's own Ctrl+S go
  the same way. A guest without Edit cannot save.
- Language servers get `didSave` on the host only.

**Reload from disk.** The host's file watcher (`fs://changed`, `Backend::watch`) reports a
change to a shared file (git checkout, composer, a formatter, an AI agent):

- If the document has no unsaved changes, the host diffs the disk text against the yrs text
  (`similar`, line then character level) and applies the minimal edits in one transaction with
  origin `disk`. Carets and selections of all participants survive, and no undo manager
  records it (they only track their own origin), which keeps the existing rule that a reload
  is not an undo step (`editor/reload.ts`).
- If the document has unsaved changes, the host gets PHPStorm's "File Cache Conflict" choice
  (Load File System Changes / Keep Memory Changes); guests see a balloon that the host is
  resolving a conflict. Loading applies the same minimal diff.
- `startReloadingChangedTabs` (`state/fsWatch.ts`) skips shared tabs on the host, because the
  host's Rust side already applied the change.

**Undo.** Each participant has its own `Y.UndoManager` with only its local origin tracked, so
Ctrl+Z undoes your own edits and never someone else's. For shared tabs `CodeEditor.tsx` swaps
the `historyCompartment` (`editor/reload.ts`) from CodeMirror's `history()` to nothing and
adds `yUndoManagerKeymap`; `$Undo` / `$Redo` in `features/mainMenu` call the undo manager of
shared views. The undo stack starts when the file is shared and ends when the session ends;
the local history before the session is not kept (as for reloads today). The spike for this
plan confirmed the one trap: remote updates must be applied with a non-tracked transaction
origin, or the undo manager undoes other people's edits as well.

**File operations.** File create, rename, move and delete are file system operations done by
the host (`Backend::create_file`, `rename`, `delete`), requested by guests through the host
RPC (Files Edit required, policy checked). Documents are keyed by a `docId` that the host
assigns, not by path: on rename the host updates its path map and sends `renamed` on
`collab://doc/<docId>`, and every participant's tab follows (`renameTab` plus the tab path).
Deleting an open shared file closes the document for everyone with a balloon "<file> was
deleted by <name>". The guest's project tree refreshes from the forwarded `FsChanged` events.

**Ending.** When the session ends, the host's tabs keep their text and become normal tabs
(history starts fresh); unsaved shared edits stay unsaved on the host. Guests' tabs close; a
guest with unsaved edits in a shared file gets nothing to lose, because the edits live on the
host.

### Presence and follow

Presence is session-wide (not per document) and goes through the host: `{ participantId,
activeFile, caret: { line, column }, viewport: { topLine, bottomLine } }`, sent on tab switch,
caret moves (throttled to 10 Hz) and scrolling (throttled). Following someone means: open
their `activeFile` (through `openFileAt`, `editor/views.ts`), scroll so their viewport's top
line is at the top, and keep doing so on each presence update. A local keystroke, click or
file switch ends following; Make Everyone Follow You sets `forced` so guests show the banner
"<host> asked everyone to follow" (they can still Stop).

### Guest project backend

`forge_collab::guest::CollabBackend` implements `forge_backend::Backend`; the guest's
`AppState` uses it through `set_collab_project`, which works like `set_remote_project`
(project settings come from the host through `BackendFiles`, read-only for the guest).
`BackendInfo.remote` gets a new variant or a `collab: Some(CollabHost { name, machine })`
field so the UI shows `<project> — <host> (Code With Me)`.

Each `Backend` method becomes one request in the collab protocol (`HostCall`, a new enum,
deliberately **not** `forge_backend::protocol::Call`, see "Why not reuse the agent protocol"):

| `Backend` method | Host call | Permission |
| --- | --- | --- |
| `list_dir`, `read_file`, `read_optional` | `ListDir`, `ReadFile` | any; excluded paths hidden |
| `write_file`, `create_file`, `create_dir`, `rename`, `delete` | same names | Files Edit |
| `find_files`, `find_in_files` | same names | any; excluded paths filtered from results |
| `detect_project` | `Detect` | any |
| `watch` | `Watch` (host forwards filtered `FsChanged`) | any |
| `git()` | `GitQuery(kind)`: status, diff, blame, log, show, branches | VCS Read-only |
| `git()` write operations | `GitAction(kind)`: commit, push, pull, checkout, stash, … | VCS Full access |
| `open_terminal` | `TerminalOpen` | Terminal Full access |
| `start_process` | refused; guests start runs by configuration (`RunStart { name }`) | Run Full access |
| `lsp_launcher()` | a launcher that refuses; guests use the LSP forwarding below | — |
| `php()` | `Php(PhpCall)` (index search, structure, hover, generate targets) | any; `Generate` needs Files Edit |
| `framework()` | `Framework(FrameworkCall)` | any |
| `langservers()` | refused (guests do not manage the host's servers) | — |
| `find_executable` | refused | — |
| `tunnels()` | `None` | — |

`GitExec` takes raw git arguments (`forge_vcs::GitRequest { args, … }`), and git can run
arbitrary programs (`-c core.sshCommand=…`, `-c alias.x=!…`, hooks). So the guest side
implements `GitExec` by translating the git commands that Forge's VCS code issues into typed
`GitQuery` / `GitAction` calls, and the host builds the git command line itself. Commands that
Forge's VCS code needs but the translation does not know fail with "not available in Code
With Me sessions" until they are added. `GitAction` runs hooks, so it requires VCS Full access.

**Why not reuse the agent protocol directly.** `forge_backend::protocol::Call` is the protocol
between Forge and its own agent, which is fully trusted: it has `ProcessStart` (any command),
`LspStart { command, args }`, `Git(GitRequest)` with raw args, `FindExecutable` and `Browse`
(any absolute directory). Exposing it to guests would make every permission level
equivalent to Full access plus a shell. The collab protocol reuses the framing
(`encode_frame`, `FrameDecoder`, MessagePack with named fields), the `Request` / `Response` /
`Event` / `Data` / `Close` frame shapes, and the `RpcClient` waiter logic, but its call enum
contains only what the policy can check. The guest's `CollabBackend` mirrors `RemoteBackend`
(`crates/forge-backend/src/remote.rs`): same blocking-call pattern, same channel handling for
terminals, same reattach after reconnects.

**LSP for guests.** Guests do not start language servers. A guest's `lsp_*` commands
(`app/src-tauri/src/commands/lsp.rs`) check for a collab project and forward the request to the
host as `Lsp(LspQuery)`: hover, completion, completion resolve, signature help, definition,
references, document symbols, workspace symbols and format (format needs Files Edit). The host
runs it against its `LspManager`, which already sees the current shared text because the host
syncs its yrs text to the server (`didChange` from the host's own editor, or from `DocStore`
for files only guests have open). Diagnostics are forwarded as `lsp://diagnostics` for the
files the guest has open. Results that point into excluded files are dropped.

**Search and navigation.** Go to File, Find in Files and the Text tab of Search Everywhere use
`find_files` / `find_in_files` and so run on the host. Search Everywhere ranking runs locally
in the guest's Rust (`search_rank`) over the candidates, as today. Go to Class / Symbol use the
forwarded `lsp_workspace_symbols` and the PHP index (`Php(Search)`).

**Browser guest (follow-up, feasibility).** The UI already runs in a plain browser against
`registerMock` handlers (`src/ipc/`). A browser guest would be the same bundle, served by the
host's Forge over HTTPS, with an IPC adapter that maps `invoke` / `listen` to the collab
protocol over a WebSocket (iroh supports browsers through relays; a direct WebSocket needs a
certificate the browser trusts, which a LAN host does not have). Everything that the guest's
Rust does locally today (search ranking, file types, keymap) is already available as
WebAssembly. Feasible, roughly three to four weeks, after this PR.

### Terminal and run sharing

- **Shared terminal.** The host's `TerminalState` gets a `shared` flag per tab. Output of a
  shared tab is copied to guests that have Terminal Read-only or better (`Event::Pty` shape,
  with a scrollback snapshot of the last 2 000 lines on join). Input from a guest is accepted
  only with Terminal Full access and is written to the host's PTY; the session log records
  that the guest typed into the terminal (not the keystrokes, unless the host turns on full
  logging, see open questions). Resizing: the host's size wins; guests render at the host's
  columns and rows.
- **Guest terminals.** With Terminal Full access, `CollabBackend::open_terminal` asks the host
  to open a new PTY in the project root through `Backend::open_terminal`; the host shows it
  as a tab "<guest>: Local" too, so the host always sees what runs on their machine.
- **Run.** The host forwards `run://<runId>` events (the rendered console and test tree from
  `forge-output`) to guests with Run Read-only or better. With Run Full access a guest sends
  `RunStart { configuration: name }`, `RunStop { runId }`, `RunRerun { runId }`; the host
  resolves the name against its own run configurations and starts it as if the host clicked
  Run. Guests cannot send command lines.

### Permission model and enforcement points

`forge_collab::permissions` (pure, unit-tested, also in WebAssembly):

```rust
pub enum FilesAccess { ReadOnly, Edit }
pub enum TerminalAccess { None, ReadOnly, Full }
pub enum RunAccess { None, ReadOnly, Full }
pub enum VcsAccess { ReadOnly, Full }
pub struct Permissions { files, terminal, run, vcs }       // + presets ReadOnly/EditFiles/FullAccess
pub struct Policy { permissions, excludes, max_file_size } // per participant
impl Policy { pub fn check(&self, call: &HostCall) -> Result<(), Denied>; }
```

Enforcement happens **only on the host** and **before** the host touches anything:

1. **Connection**: unknown endpoints without a valid join proof are dropped before an
   approval prompt is shown (see "Session links").
2. **Admission**: a participant exists only after the host clicked Accept.
3. **Every host call**: `Policy::check` against the participant's current permissions, then
   path checks: `forge_backend::paths::resolve` (refuses `..` and absolute paths), the
   project root itself is never writable, `.git/` is never exposed, excluded files are
   invisible (listing, reading, search results, LSP locations, `FsChanged`).
4. **Every document update**: dropped for participants without Files Edit, and for read-only
   documents (excluded or too large).
5. **Every terminal input frame and run request**: checked against Terminal / Run access.
6. **Permission changes take effect immediately**: lowering a guest's permission closes their
   guest terminals, makes their documents read-only, and is announced to them.

The guest UI also disables what the permission does not allow (read-only editors through
`EditorState.readOnly`, hidden actions), but that is only for usability.

## Technology decisions

### CRDT / sync algorithm: Yjs (`yjs` 13.6 + `y-codemirror.next` 0.3 in the UI, `yrs` 0.28 on the host)

| Option | Status (October 2026) | Verdict |
| --- | --- | --- |
| **Yjs + yrs** | `yjs` 13.6.33 (v14 in beta), `y-codemirror.next` 0.3.6, `y-protocols` 1.0.7, `yrs` 0.28.0 (Sept 2026); all MIT | **Chosen** |
| `@codemirror/collab` with the host as authority (OT-style rebasing) | Maintained by the CodeMirror author; tiny | Runner-up |
| Automerge (`automerge` 0.12 / `@automerge/automerge` 3.5) | Active, MIT | Not chosen |
| Loro (`loro` 1.16 / `loro-crdt` 1.16) | Active, MIT, Rust core | Not chosen |
| Zed's CRDT (`text` crate, collab server) | GPL / AGPL, tied to Zed's server | Not usable |
| Own host-authoritative OT | — | Not chosen |

Why Yjs:

- `y-codemirror.next` already renders remote carets, selections and name labels from
  awareness, and gives each participant a `Y.UndoManager` that undoes only their own edits:
  the three hardest UI pieces exist and are widely used.
- `yrs` is the same algorithm in Rust with the same binary update format, so the host keeps
  the authoritative replica in Rust (disk writes, reload diffs, LSP text, late joiners)
  without reimplementing anything. The spike for this plan checked interop: an update written
  by yrs 0.28 decodes in yjs 13.6.33 to the same text.
- Updates merge in any order, so reconnects, slow guests and a later relay need no special
  cases; the sync protocol (step 1 / step 2) catches a peer up from its state vector.
- Measured in the spike (release build): inserting a 4.8 MB file takes 2 ms; 10 000 single
  character edits at scattered positions in it, each its own transaction, take 1.6 s
  (160 µs each); the encoded state is 5.2 MB (about 1.1× the text). Typical files are 100×
  smaller.

Why not the others:

- `@codemirror/collab` fits Code With Me's always-online host well and keeps CodeMirror's own
  history. But remote carets, selections and labels would have to be written from scratch,
  every conflicting push costs a round trip and a rebase (noticeable over a relay), and the
  host's Rust side would need its own change-set implementation to save and to serve the LSP.
  Keep it as the fallback if `y-codemirror.next` stops being maintained.
- Automerge and Loro are good CRDTs, but their CodeMirror bindings are younger, the per-user
  undo has to be built, and Automerge keeps the full history (memory grows with edits).
- Zed's implementation is GPL / AGPL and built around its central server; Forge is MIT.

Constraints carried into the implementation: create yrs documents with
`OffsetKind::Utf16`; apply remote updates with a dedicated origin so undo managers ignore
them; pin `yjs` to 13.6 until `y-codemirror.next` supports v14; never send binary files
through Yjs.

### Transport: iroh (QUIC, hole punching, optional self-hosted relays)

| Option | Status | Verdict |
| --- | --- | --- |
| **`iroh` 1.4** (QUIC over `quinn`, Ed25519 endpoint ids, hole punching, `iroh-relay` 1.4) | 1.0 released 2026 with a wire-stability promise; MIT/Apache | **Chosen** |
| Plain QUIC (`quinn` 0.11) or TLS over TCP (`rustls` 0.23 + `tokio-rustls`), direct only | Mature | Covered: iroh is QUIC, and direct addresses work without a relay |
| WebSocket + TLS (`tokio-tungstenite` 0.30) | Mature | Only for the future browser guest |
| WebRTC in Rust (`webrtc` 0.21, `str0m` 0.24) | Active | Not chosen |
| WebRTC in the webview | WebKitGTK often built without WebRTC | Not chosen |
| Own Forge relay server | — | Not needed: `iroh-relay` is the self-hostable relay |
| SSH tunnel | Works with any of the above | Documented as a recipe, not a transport |

Why iroh:

- One stack covers every network case: on a LAN or through a forwarded UDP port the guest
  connects directly; behind NATs iroh punches holes (about 9 in 10 connections end up direct
  according to n0) and falls back to a relay, which only sees end-to-end encrypted QUIC.
- Peers are identified by public key (the `EndpointId`). The host's id is in the session
  link, so the guest authenticates the host by key, just like PHPStorm's certificate
  fingerprint in the link, without certificates or a lobby.
- QUIC streams give one stream per concern (control, each document, each terminal) without
  head-of-line blocking, and keep-alive and migration (Wi-Fi to cable) for free.
- The relay is `iroh-relay`, a single binary or Docker image with built-in ACME TLS and
  access control (open, allow list of endpoint ids, or a shared token). Forge documents how
  to run one; Forge does not run a service.

Forge-specific configuration:

- No relay by default (`relay_mode` disabled) unless `codeWithMe.relayUrl` is set or the user
  opts in to n0's public relays (`codeWithMe.usePublicRelays`). n0's public relays are free
  but rate-limited and versioned; they are open source, so using them is not a lock-in, but
  Forge must not depend on them.
- No n0 DNS discovery: the link carries the host's relay URL and direct addresses. Local
  network discovery (iroh's mDNS address lookup) only with `codeWithMe.lanDiscovery`.
- ALPN `forge/cwm/1`.
- Measure the binary size impact in stage 1 (iroh pulls in its own QUIC and crypto stack next
  to russh's `ring`).

Why not WebRTC: it still needs a signaling channel and, behind symmetric NATs, a TURN relay
(coturn), so it does not remove the relay; the Rust stacks are heavier for a data channel than
QUIC, and the webview route fails on Linux distributions whose WebKitGTK lacks WebRTC. WebRTC
only becomes interesting together with voice and video, which are out of scope.

SSH tunnel recipe (documented in the guide): iroh is UDP, so plain `ssh -L` does not work, and
UDP pushed through a TCP stream (`socat`) loses the packet boundaries. The guide documents
OpenSSH's tunnel device (`ssh -w`, needs root) and, as the recommended route for locked-down
networks, a VPN such as WireGuard or Tailscale, where the session link simply carries the VPN
address.

### Encoding and framing

MessagePack frames with `encode_frame` / `FrameDecoder` from `forge_backend::protocol`
(already used for the agent), named fields so older peers ignore new ones, a protocol version
in the hello, and `MAX_FRAME` raised per stream type only where needed (initial document
state). Yjs payloads are opaque byte strings inside the frames.

### UI integration

`y-codemirror.next` through `registerEditorExtension`, which returns the binding only for
tabs that belong to a running session. Theme colors via `EditorView.theme` with the
`--collab-*` tokens. No new UI framework.

## Security and threat model

**Assets**: the host's files (including secrets), the host's machine (shell, processes, git
credentials), the guests' machines, the integrity of the code, and the privacy of who works
with whom.

**Attackers considered**: someone who sees the link (chat logs, screen share, a leaked
clipboard); someone on the same network; a malicious relay operator; a malicious or
compromised guest client (a modified Forge sending arbitrary frames); a malicious host
(against guests).

### Session links

```
forge://cwm/join?v=1&name=Robert&s=<sessionId>#h=<hostEndpointId>&k=<joinSecret>&r=<relayUrl>&a=<addr>,<addr>
```

- `sessionId`: 16 random bytes, base32. `joinSecret`: 32 random bytes, base32. Both are new for
  every session and for Regenerate Link. `hostEndpointId`: the host's iroh public key for this
  session (a fresh key pair per session, so links cannot be correlated across sessions).
- Everything that grants access is in the **fragment**, so it never reaches a web server if
  the link is ever routed through a web page (a future browser guest landing page).
  `crates/forge-open` treats incoming `forge://` links as untrusted input already; the parser
  in `forge_collab::link` validates every field and length and is fuzzed.
- The same data has a copyable text form (`cwm1:…`) for the Join Session dialog.

### Connection, admission and verification

1. The guest dials the host's endpoint id; QUIC/TLS 1.3 authenticates the host by that key.
   A relay or a network attacker cannot impersonate the host without its private key.
2. The guest's first frame is `Knock { sessionId, name, proof }` with
   `proof = HMAC-SHA256(joinSecret, hostId ‖ guestId ‖ sessionId)`; the guest id is the guest's
   authenticated endpoint key. Wrong proof: the connection is closed without any prompt, so
   people who only know the host's id cannot spam the host with join requests. Knocks are
   rate-limited per endpoint and in total.
3. The host shows the approval balloon. The **verification code** shown on both sides is
   `SHA-256(hostId ‖ guestId ‖ sessionId)` truncated to six digits. It binds the two
   authenticated keys, so a person who stole the link and joins in parallel shows a different
   code than the real guest; the host compares the codes over the call or chat.
4. Only after Accept does the host create the participant and its policy.

### Encryption

End to end between guest and host with QUIC (TLS 1.3, forward secrecy). Relays forward
ciphertext only. In a session with several guests each guest has its own channel; guests never
connect to each other. Nothing is stored on any server.

### Enforcement against a malicious guest client

The guest's client is untrusted. The host enforces everything in "Permission model and
enforcement points"; the protocol has no call that maps to "run this command" or "read this
absolute path". Additional rules:

- **Path traversal**: every path goes through `paths::resolve` and is checked again after
  symlink resolution (`canonicalize` must stay under the root), so a symlink in the project
  pointing at `~/.ssh` does not leak it. Symlinks that leave the root are listed but not
  readable.
- **`.git/`** is never listed or readable, and git runs only through typed queries built by
  the host.
- **Code execution via edits**: Files Edit lets a guest change code that the host will run
  later (that is inherent, PHPStorm has the same). Forge additionally treats files that
  configure execution as requiring Full access to change: `.forge/runConfigurations*.toml`,
  `.forge/settings.toml`, `.vscode/`, `.idea/`, `composer.json` `scripts`, `package.json`
  `scripts` (whole file), `Makefile`, `.git/hooks` (never exposed anyway), `.env*` (excluded by
  default). The list is a setting with these defaults. The host sees a gutter marker on lines
  that guests changed until the next save.
- **Resource exhaustion**: frame size limits, per-guest rate limits for calls, document
  updates and search, a maximum number of open documents per guest, timeouts on host calls.
- **Awareness spoofing**: the host overwrites the participant id, name and color in awareness
  and presence before forwarding.
- **Removed guests** are disconnected and their endpoint id is refused for the rest of the
  session.

### Secret files

`codeWithMe.excludedFiles` defaults to `.env`, `.env.*` (except `.env.example` and
`.env.dist`), `*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, `auth.json`
(Composer credentials), `.npmrc`, `.pypirc`, `.netrc`, `*.kdbx`, `.forge/dataSources.toml`,
`.forge/*.local.toml`, and `config/master.key`. Excluded files are invisible to guests
everywhere (tree, read, search, LSP results, `FsChanged`, terminal file links). `.gitignore`d
files stay visible (guests need `vendor/` and `node_modules/` for navigation) unless they
match an exclude. The tool window shows the active excludes; the Start Session popup mentions
"Files matching N exclude patterns are hidden from guests." Terminals are not filtered: a
guest with terminal access can `cat .env`, which is why Terminal Full access is only in Full
access and Custom.

### Malicious host

A guest trusts the host with what it types and with what the host shows. The guest's Forge
runs nothing that the host sends: no language servers, no processes, no file writes on the
guest's machine. Terminal output is rendered by xterm.js as on any terminal; links in it open
only after a click. The guest's own project is closed while it is a guest, and the guest's
settings, keychain and AI providers are never sent.

### Auditing

The host's session log (`<data dir>/forge/code-with-me/<date>-<sessionId>.jsonl`, never in the
project) records: session start and end with permissions, knocks (accepted, declined, wrong
proof, rate-limited), permission changes, removals, files a guest saved, created, renamed or
deleted, git actions, terminals opened and input sessions (not keystrokes by default), and
runs started, plus denied requests. The Session Log tab shows it live. Logs older than 30
days are deleted.

## Testing strategy

No test touches the network, a relay, installed language servers or real accounts.

- **Pure unit tests** (`forge-collab`, no runtime feature): link encode / parse round trips
  and rejection of bad input, the verification code, every row of the permission table
  (`Policy::check` for every `HostCall` under every preset and Custom combination), exclude
  matching, presence and color assignment, audit records.
- **In-process transport**: `Transport` trait with a `MemoryTransport` built on
  `tokio::io::duplex` (the pattern `forge_backend::rpc` tests already use), with injectable
  latency, reordering of independent streams, drops and disconnects.
- **Multi-peer simulation tests** (Rust): one host with a `LocalBackend` on a temp folder and N
  simulated guests (yrs replicas standing in for the UI), scripted and random edits, joins and
  leaves mid-edit, reconnects, a save from a guest, a disk change during concurrent typing, a
  rename while two guests edit the file, a permission downgrade while a guest types. Assert
  convergence (all replicas equal, disk equal after save), that denied updates never reach the
  host replica or other guests, and the audit log contents.
- **Security tests**: a "hostile guest" that sends every `HostCall` variant with traversal
  paths (`../`, absolute, symlinks out of the root, `.git/config`, excluded files), oversized
  frames, wrong proofs, spoofed awareness; all must be refused and logged.
- **Property tests and fuzzing**: `proptest` for convergence (random interleavings of edits
  across 2 to 5 replicas, plus the disk-diff path: applying `diff(a, b)` to a doc holding `a`
  yields `b` for random Unicode text including surrogate pairs and CRLF); `cargo-fuzz` targets
  for the link parser, the frame decoder and the host call decoder (run locally, not in CI).
- **UI unit tests** (vitest): the binding with two in-memory Y.Docs connected through a fake
  IPC (remote edits, carets, undo only reverting own edits, `$Undo` routing), follow logic,
  the dirty marker from `SaveState`, the guest permission UI.
- **Playwright in browser mode with two browser contexts**: a scenario
  (`app/ui/scripts/scenarios/code-with-me.mjs`) opens `?collab=host` and `?collab=guest` in two
  contexts connected through a `BroadcastChannel` inside the mock, so the two pages really
  exchange Yjs updates. It types in both, checks that the texts converge and remote carets
  are drawn, accepts the join request, follows, and captures the screenshots listed under
  "Keeping one large PR reviewable".
- **Manual desktop test plan** (in the PR description): two machines on one LAN; host behind
  NAT with a self-hosted `iroh-relay` (Docker) on a VPS; a host on an SSH remote project.

## Performance targets

| Metric | Target |
| --- | --- |
| Local keystroke | No added latency (applied locally first) |
| Remote edit visible, LAN | p50 < 30 ms, p99 < 100 ms |
| Remote edit visible, via relay | RTT + 30 ms |
| Caret and selection updates | ≤ 20 Hz per participant, visible within RTT + 50 ms |
| Join with 20 open files, LAN | Workbench usable < 2 s after Accept |
| Open a 1 MB file as a guest, LAN | < 300 ms |
| Shared file size | Up to 5 MB editable (setting); larger read-only |
| Applying one update, 100 000-line file | < 1 ms in the UI, < 1 ms on the host |
| Host memory per shared document | < 3 × the text size |
| Participants | 1 host + 9 guests, each typing 5 characters/s, no growing backlog; host CPU < 10 % of one core |
| Idle session | < 1 % CPU, < 1 KB/s per guest |
| Find in Files for a guest | Host's search time + one RTT |

Measured with the simulation tests (latency injected) and a bench target in `forge-collab`;
the 10-participant target is a stress test that runs on demand.

## Stages (one PR)

The whole feature lands as **one pull request from `feat/code-with-me`** into the integration
branch. The stages below are ordered; each one is a group of small commits that builds,
passes all quality gates (`cargo fmt`, `clippy -D warnings`, `cargo test --workspace`,
`pnpm typecheck`, `pnpm test`, `pnpm build`) and can be tried on its own, so the branch can be
committed and backed up continuously. Later stages never require rewriting earlier ones.

Estimates are working days for one developer familiar with Forge, including tests, docs and
screenshots.

| Stage | Content | Estimate |
| --- | --- | --- |
| S0 | Foundations | 3 d |
| S1 | Shared editing with carets, direct / LAN | 8 d |
| S2 | Host approval, permissions, audit log | 5 d |
| S3 | Guest project backend: tree, search, LSP, git, PHP | 8 d |
| S4 | Follow, Jump To, Make Everyone Follow You | 3 d |
| S5 | Shared terminal and Run | 5 d |
| S6 | Relay and NAT traversal | 3 d |
| S7 | Polish, guide, screenshots, stress tests | 5 d |
| | **Total** | **40 days (about 8 weeks; plan 10 with review and rebases)** |

### S0 — Foundations (3 d)

- `crates/forge-collab` with the pure modules (`link`, `permissions`, `protocol`, `verify`,
  `excludes`, `presence`), the `Transport` trait and `MemoryTransport`.
- Manifest `code-with-me.toml` with commands, settings and the main menu entries; snapshot
  regenerated.
- UI feature skeleton: toolbar icon with the Start Session popup (disabled "Start Session"
  until S1), settings page, keymap actions, `collab.ts` client, mock skeleton, theme tokens.
- **Accept**: gates pass; the settings page and the popup render in both themes; link and
  permission unit tests pass; nothing is reachable over the network yet.

### S1 — Shared editing with carets, direct / LAN (8 d)

- iroh transport (direct addresses only, no relay), host session, knock with proof (accepted
  automatically in this stage, behind a development flag), `DocStore` with yrs, Yjs sync and
  awareness, `collab_doc_*` commands, the `y-codemirror.next` binding, colored carets and
  labels, per-user undo, saving through the host, reload-from-disk diff, rename and delete of
  open files, session end. Guests open files by path from a minimal "Open shared file" list
  (the full tree comes in S3).
- **Accept**: two Forge instances on one machine and on two LAN machines co-edit a PHP file;
  random-edit simulation tests converge for 2 to 5 peers; undo reverts only own edits;
  `git checkout` of a shared file updates all participants without moving their carets;
  dirty markers agree; Playwright two-context scenario converges; binary size growth measured
  and noted in the PR.

**S1 as built (deviations from the text above)**

- Hosting is off unless `FORGE_CWM_DEV=1` is set in the environment. Without it Start Session
  explains why and nothing listens. Sessions show a "development preview" notice in the popup,
  because guests are admitted automatically (`DevAutoAccept`, the seam S2 replaces with the
  approval balloon). Only the Read-only, Edit files and Full access presets exist; Custom
  arrives with S2.
- `collab_start({ level })` takes the preset name (not a permissions table) and answers with
  the `SessionView`; `collab_join({ link })` does the same for guests. `collab_doc_open`
  answers with raw bytes (u32 length, JSON header, Yjs update). `collab_doc_update` and
  `collab_awareness` carry base64 strings, and `collab://doc` events carry base64 updates;
  raw binary events are a later optimization.
- `SaveState` is `{ hash }`: `<UTF-16 length>:<FNV-1a 32>` of the saved text, computed
  identically in Rust (`protocol::text_hash`) and in `contentHash` (`state/editor.ts`). A tab
  is dirty when `contentHash(content)` differs from it. The state vector is not part of it.
- On Start Session the host shares every project file that is open in the editor (excluded
  and oversized files stay normal tabs) and every file opened later; the guest opens the
  host's shared files from the session popup ("Shared files", Tools | Code With Me | Open
  Shared File…) and opens the first ten on join.
- Guest tabs have the path `cwm://<path>` so they never mix with the guest's own project,
  which stays open in S1 (S3 replaces this with the host's project as backend).
- The transport is iroh 1.3 over QUIC with direct addresses only (no relay, no discovery);
  `MemoryTransport` drives the simulation tests.
- Browser mode: `?collab=host` and `?collab=guest` pages exchange Yjs updates through a
  `BroadcastChannel`, or through `window.__cwmOut` / `__cwmReceive` that the Playwright
  scenario bridges between two browser contexts. Link parsing in the mock is a plain regex;
  the WebAssembly bridge from the plan is not used yet.
- Binary size was not measured in S1 (the build machine had too little disk for a release
  build); do it before merging.

### S2 — Host approval, permissions, audit log (5 d)

- Approval balloon with verification code, Join Session dialog and waiting state, Decline and
  timeout, Read-only / Edit files / Full access / Custom, per-guest permissions, Remove Guest,
  Regenerate Link, rate limits, excludes, the session log and the Session Log tab, End Session
  confirmation. Remove the S1 development auto-accept.
- **Accept**: hostile-guest tests refuse every disallowed call and log it; a read-only guest's
  edits never reach the host (test and manual); a wrong proof never shows a prompt; codes
  match on both sides; `.env` is invisible to guests.

**S2 host approval as built**

- `forge_collab::approval::Approvals` is the `Admission`: a guest with a valid proof becomes a
  request (`collab://join-request` with the verification code), the host answers with
  `collab_answer_join`; unanswered requests expire after 110 s (`collab://join-closed`, the
  guest hears "The host did not answer."), a guest has one open request, at most 5 wait, and
  ending the session refuses all of them. The guest gets `collab://waiting` with the same code
  and can cancel with `collab_cancel_join`.
- `DevAutoAccept` and the `FORGE_CWM_DEV` flag are gone from the app; `DevAutoAccept` stays
  for tests. Hosting is available in every build.
- Custom permissions (Start Session and the session's Permissions), per-guest permissions
  (the participant's menu), Remove Guest and Regenerate Link: `HostSession::set_permissions`,
  `set_guest_permissions`, `remove` and `regenerate_link`; a guest's new policy counts from
  its next request, and its open editors follow the new Files access (`guest.ts`).
- Session log: `forge_collab::audit::SessionLog` (JSON Lines in `<data dir>/forge/code-with-me`,
  pruned after 30 days, setting `codeWithMe.auditLog`), recorded by the host for knocks,
  admissions, permission changes, removals, Regenerate Link, guests' file changes and git
  actions, and refused requests. Shown in a Session Log dialog from the popup and, since S4,
  in the Session Log tab of the Code With Me tool window.
- Finished on `feat/code-with-me-s6`: the **Join Session dialog** (`JoinSession.tsx`, behind the
  action `CodeWithMe.JoinSession`, mounted on first use so it also works without a project)
  shows the host's name and machine, direct or through the link's relay, and the name to join
  with (`collab_parse_link` answers a `JoinPreview`: `LinkSummary` with the new `machine` plus
  `userName`), the note that an open project is closed, then **Waiting for <host> to accept…**
  with the verification code and Cancel inside the dialog (the waiting balloon only appears for
  joins without the dialog); declined or unanswered requests are shown in the dialog with Try
  Again.
- Finished on `fix/code-with-me-guest-polish`: **unsaved changes** of the window's own project
  are asked about when Join is pressed (`JoinSession.tsx`, step `unsaved`: the files, Save /
  Don't Save / Cancel inside the dialog, so it never stacks with another modal). Shared tabs do
  not count; without a project nothing is asked; Save uses the explicit trigger, so in Safe Mode
  only the files are written (Actions on Save that run tools stay off there). Don't Save leaves
  the edits in place until the host's project actually takes the window, so a declined join
  loses nothing. A failed save stays in the dialog with the error.
- **`forge://cwm/join?…` deep links**: the `forge` scheme was already registered
  (`plugins.deep-link.desktop.schemes` in `tauri.conf.json`: `.desktop` MimeType
  `x-scheme-handler/forge` on Linux, the URL protocol on Windows, `CFBundleURLTypes` on macOS;
  runtime registration for AppImages and dev builds). `forge_collab::link::is_join_url` tells
  session links from `forge_open`'s links and `route_join_url` parses them in full (also the
  `forge://cwm/join/?` spelling) and picks the window: `JoinRoute::ThisWindow` (Welcome screen
  or project; the unsaved question applies) or `JoinRoute::NewWindow` when this window is in a
  session or joining. `companion::open_join_link` handles links from the deep-link plugin
  (`on_open_url`, second launches through the single-instance plugin), launch arguments and the
  Test button (`companion_open_link`); this window gets `PendingEvent::JoinSession` /
  `companion://join-session` (queued until the UI is ready), a new window is a project-window
  instance started with the canonical link in `FORGE_CWM_JOIN` (environment, not argv). The UI
  (`features/code-with-me/join.ts`, `browserCompanion/openRequests.ts`) opens the Join Session
  dialog filled in; nothing connects before Join. Browser mode: `?welcome&cwmLink`.
- The **protected files are a setting**, `codeWithMe.protectedFiles` ("Files only Full access
  may change", default `forge_collab::excludes::DEFAULT_PROTECTED`, the former
  `PROTECTED_FILES`), passed as `HostConfig::protected` into every guest's `Policy`
  (`Policy::with_protected`); a manifest test keeps the defaults equal to the crate's.

### S3 — Guest project backend (8 d)

- `CollabBackend: Backend`, `set_collab_project`, `HostCall` dispatcher on the host with the
  policy, filtered `FsChanged`, the project tree, Go to File, Find in Files, Search
  Everywhere, PHP index and framework calls, typed git queries and actions, LSP forwarding
  (hover, completion, diagnostics, definition, references, symbols, format), file operations
  from the Project view context menu, the guest title, status item and tool window set.
- **Accept**: a guest on the demo Laravel project can browse, search, Ctrl+B into a
  controller, see diagnostics and completion from the host's Intelephense (fake language
  server in tests), see git status and blame; a guest with Edit files creates and renames
  files; the host on an SSH remote project can share it.

**S3 as built** (`crates/forge-collab/src/guest_backend/`, branch `feat/code-with-me-guest-backend`).
Where the plan was unclear or wrong, this is what the code does:

- `HostCall` is its own closed enum (`calls.rs`) carried as an opaque MessagePack payload in
  `GuestMessage::Call` / `HostMessage::CallResult`, so `protocol.rs` stays pure. The host
  decodes it, runs `Policy::check_call` (`policy.rs`) and only then answers (`host_calls.rs`,
  a child module of `host` so it can use the session's state). New host messages:
  `CallResult`, `FsChanged`, `Diagnostics`. A guest can have 64 calls in flight; answers are
  limited to the frame size; errors never carry the host's absolute paths; the guest's
  `BackendInfo.root` is the placeholder `/<project>`.
- **Paths**: `is_plain_relative`, no `.git/`, secret excludes (now case-insensitive, because
  `.ENV` is `.env` on macOS and Windows), then `Gate::verify`: the path is resolved through
  symlinks (for a path that does not exist yet, its nearest existing folder), must stay inside
  the project, and its real name must pass the rules again (a link to `.env` or `.git/config`
  is refused). Symlinks that leave the project are listed but not readable or writable.
  Deleting a folder that contains hidden files is refused. **Limit:** when the host's own
  project is on an SSH host, symlinks cannot be resolved from the host's Forge, so only the
  path rules apply there.
- **Files that configure what runs** (`PROTECTED_FILES`: `composer.json` and `package.json`
  as whole files, `Makefile`, `artisan`, `bin/console`, `.forge/settings.toml`,
  `.forge/runConfigurations*.toml`, `.vscode/`, `.idea/`, `.husky/`) need Run Full access to
  change, in file calls and in shared documents (read-only on open, updates and saves
  denied). The plan only protected `composer.json`'s `scripts`; whole-file is simpler and
  safe. Making the list a setting is S2.
- **Documents**: a guest can open any visible file as a shared document (the host creates it
  from disk); the shared files list stays the host's own tabs.
- **Git**: `forge_vcs::Repo` runs unchanged on the guest through `CollabGit`, which turns the
  exact command lines Forge's version control code issues into `GitQuery` (read-only) and
  `GitAction` (VCS Full access) values; the host builds the command line again, validates
  revisions and pathspecs (no options, no `:` magic, no `..`) and filters status, diffs,
  commit file lists and file lists, so hidden files and files outside the project folder of a
  larger repository never show up. Everything else fails with "not available in Code With Me
  sessions".
- **Framework analysis never runs project code for guests**: Laravel routes always come from
  the static analysis and the artisan command list from the built-in list (the guest's `php`
  value is ignored), Symfony and Doctrine settings may only name project folders, and `env`
  completions lose their values (they come from `.env`). The plan allowed artisan for
  guests; that would run code a guest with Edit files can change.
- **Language servers**: the guest's `lsp_*` commands forward `LspQuery` to the host
  (`lsp_host.rs` answers with the host's `LspManager`, synchronized with the shared
  document's current text before each request); locations in hidden files or outside the
  project are dropped. `lsp_did_change` is a no-op for guests (the host reads the document);
  diagnostics reach the guests that told the host they opened the file (`Sync`).
- **Guest window**: `AppState::set_collab_project`, `BackendInfo.collab`, status item "Code
  With Me: <host>" with latency, title `<project> — <host> (Code With Me)`, read-only editors
  and a trimmed Project context menu without Files: Edit, no AI Chat, Database, Services,
  Debug, Pull Requests, Terminal or Run windows (Terminal and Run return with S5), and the
  Commit window only with Full access to version control. Browser mode:
  `?project=cwm-guest[&permission=edit]`.
- Not in S3: terminals and processes (`CollabBackend` refuses them), the audit log and the
  approval UI (S2: denials are logged with `tracing` where S2 plugs in), hooking guest file
  opening into S1's shared tab source (the host side is ready: every visible file opens).

### S4 — Follow, Jump To, Make Everyone Follow You (3 d)

- Presence, participant list in the popup and the tool window, avatars in the toolbar, Follow
  / Stop Following, the banner, Jump To, Make Everyone Follow You, follow on join.
- **Accept**: following switches files and keeps the leader's viewport; typing stops
  following; forced follow reaches all guests; vitest covers the follow state machine.

**S4 as built**

- `forge_collab::presence::Presence` `{ file?, caret?, anchor?, viewport?, following? }`
  (1-based lines, UTF-16 columns). Guests send `GuestMessage::Presence`; the host stores the
  sanitized presence (`Presence::sanitized`: only plain project-relative files keep positions,
  `following` must name another participant) and sends `HostMessage::Presence` to every other
  guest **as that guest may see it** (`Presence::visible_to` with the receiver's `Policy`: an
  excluded file, `.git/` or a path outside the project shows as "no file"). New guests get a
  snapshot after `Welcome`; a permission change resends everyone's presence under the new
  policy; leaving drops the presence. `PROTOCOL_VERSION` is 2.
- Make Everyone Follow You: `HostSession::set_follow_everyone` sends `HostMessage::FollowHost
  { on }` to every guest and to guests that join while it is on. Whom someone follows travels
  in their presence, so the host sees who follows it ("following you", "1 of 2 following").
- IPC: `collab_presence({ presence })` (both), `collab_follow_everyone({ on })` (host);
  events `collab://presence` and `collab://follow-host`; `SessionView` carries `presence` and
  `followEveryone`. The plan's `collab_follow` command is not needed: following is local to
  the follower's window, and the host learns it from the presence.
- UI: `presence.ts` sends the active editor's file, caret, selection and lines in view
  (throttled to 10 Hz, repeats skipped); `followState.ts` is the state machine (vitest);
  `follow.ts` drives the editor (opens the leader's file, scrolls its top line to the top),
  stops on typing, caret moves, clicks into the editor and switching files, and shows the
  banner (a CodeMirror panel in the leader's color). Participants list (popup and the new
  Code With Me tool window with the Session Log tab), toolbar avatars (click follows), Jump
  To, Follow Participant… / Jump To Participant… choosers, setting
  `codeWithMe.followHostOnJoin` (default on; guests follow the host after the shared files
  opened).
- Finished on `feat/code-with-me-s5`: remote carets get a mark on the error stripe (the
  existing `stripeMarks` facet, `remoteCarets.ts`, click moves the caret there) and their name
  label shows only while the caret moved in the last 2 s (and on hover), faded with CSS; the
  editor gets `cwm-label-<n>` classes for the active colors.
- A guest's window no longer restores (or saves) a per-project layout: the layout restore ran
  at the same time as following the host on join and could switch tabs, which stops following.
  `layoutPersistence.keepsLayout()` is false for collab guests, so follow wins deterministically
  (vitest).
- Fixed on the way: the guest's "files are read-only now" balloon showed on every status update
  (Solid's `on` passes the previous input, not a changed one), and the `cwm-guest-hover`
  scenario hovered a line that rendered doc comments had pushed below the editor.

### S5 — Shared terminal and Run (5 d)

- Share with Guests on terminal tabs, read-only and writable sharing, guest terminals on the
  host, scrollback snapshot, Run output forwarding, starting / stopping / rerunning the host's
  configurations, the guest Run widget, permission downgrade closing guest terminals.
- **Accept**: a Read-only guest sees terminal and run output but every input frame is refused
  (test); a Full access guest runs PHPUnit through the host's configuration and sees the test
  tree; the host sees guest terminals as tabs; audit entries exist.

**S5 as built** (`forge_collab::share`, `share_host.rs` / `share_guest.rs` as child modules of
`host` / `guest`, `crates/forge-collab/tests/terminals_runs.rs`):

- Protocol 3. Ordered fire-and-forget input travels as `GuestMessage::TerminalInput` /
  `RunInput` (so keys are never reordered); requests with an answer are `HostCall`s:
  `TerminalOpen { cols, rows }`, `TerminalClose`, `RunStart { configuration }`, `RunStop`. The
  host pushes `Terminals` / `Runs` (the full list the guest may see, after every change) and
  `TerminalOutput` / `RunEvents`, each with a one-time `snapshot` when an item becomes visible
  (join, permission upgrade, newly shared). Terminal scrollback keeps the last 2000 lines
  (512 KB); a run keeps its rendered events up to 8 MB, dropping the oldest console deltas
  first. Raw `output` run events are not sent (the UI renders `console`).
- Enforcement on the host: `Policy::check` / `check_call` require Terminal or Run Full access;
  the share module also checks that the terminal or run is visible to that guest and running,
  that a guest closes only its own terminals, at most 8 guest terminals each. Refused input is
  logged and answered once per guest and kind, not per key. Losing Terminal Full access,
  leaving, removal and End Session close the guest's terminals.
- The host application plugs in through `HostServices` (default methods: "not available"):
  `terminal_open/write/close`, `run_start/stop/write`. In the app (`ShellServices` in
  `collab.rs`) a guest's terminal is `terminal::open` with the host's shell in the project
  folder, announced with `collab://terminal-opened`; the host's UI attaches to it with the new
  `terminal_attach`. Every terminal session keeps its scrollback and size in `TerminalState`
  (`pty_sink`), and terminal/run commands call the `collab::on_terminal_*` / `on_run_*` hooks.
- Runs are started by name by the **host's UI** (`collab://run-request`, answered with
  `collab_run_answer`), as if the host clicked Run: launch plans, before-launch tasks and
  UI-built command lines stay where they are. `RunRequest` got `name` / `typeId` so runs with
  UI-built command lines are shared under their configuration's name. Every run of the host's
  Run tool window is shared; host paths in run events become the guest's `/<project>` root.
- Guest: `CollabBackend::open_terminal` opens a terminal on the host (Terminal Full access), so
  the Terminal tool window's "+" works unchanged; the host's shared terminals are tabs with a
  `source` (`collab_terminal_attach` registers a proxy in the guest's `TerminalState`, so
  `terminal_write` / `terminal_close` work as for any tab; the host's size wins, read-only tabs
  disable input). The host's runs are Run tool window tabs (`showSharedRun`,
  `collab_run_attach`, events on `run://cwm-run-<id>`), and `setRemoteRuns` routes Run, Rerun,
  Stop and console input to the host. Debug and Run with Coverage say they are not available.
  Terminal and Run tool windows show for guests with Terminal / Run access.
- Settings: `codeWithMe.shareNewTerminals`. Action `CodeWithMe.ShareTerminal` (Share Terminal
  with Guests) and the terminal tab menu entry Share with Guests (`registerTerminalTabActions`).
- Finished on `feat/code-with-me-s6`: when the host stops sharing a terminal, the guest's tab
  stays as ended with **[The host stopped sharing this terminal]** (`TerminalSource.ended`,
  `endSharedSession`), like a terminated session, until the guest closes it; shared again it
  opens fresh. The guest's backend no longer reports a fake exit for an unshared terminal, and
  a host shell that ends leaves the guest's tab with **[Process completed]** instead of closing
  it. Port forwarding and shared debugging stay follow-ups.

### S6 — Relay and NAT traversal (3 d)

- `codeWithMe.relayUrl`, relay token in the keychain, opt-in public relays, LAN discovery,
  connection type in the participants list ("direct" / "relay"), reconnects with backoff (as
  `forge-remote::connection` does), the guide page "Running your own relay" (Docker compose for
  `iroh-relay` with ACME and token access).
- **Accept**: tests with an in-process `iroh-relay` on localhost (no internet) connect a host
  and a guest whose direct addresses are blocked; manual test through a VPS relay; with an
  empty relay URL Forge makes no connection to any third-party host (checked with a test that
  asserts the endpoint configuration).

**As built** (`forge_collab::relay`, `forge_collab::secrets`, `forge_collab::transport::relay`,
`docs/guide/user/code-with-me-relay.md`):

- `RelayOptions` (pure) says what the user allowed: relay URL, token, public relays, LAN
  discovery. A relay server wins over public relays. `transport::relay` turns it into the iroh
  endpoint: `RelayMode::Disabled` without a relay (the default, nothing is contacted),
  `RelayMode::Custom` with one relay and its bearer token, or n0's default map. Address lookup
  stays off in every case; LAN discovery adds only iroh's mDNS lookup.
- The host waits for the relay connection in Start Session and reports why it fails (wrong
  token, unreachable, certificate) instead of putting a dead relay into the link. The link's
  `r=` is the normalized relay URL (for public relays the endpoint's home relay).
- A guest dials the link's direct addresses and the link's relay at the same time and uses the
  link's relay, never the public relays on its own. Its own token is sent only to the relay it
  was set for (same URL as in the link). Hole punching and the move from relay to direct path
  are iroh's; Forge reads the selected path (`ConnectionPath`: direct or relay, remote
  address, RTT).
- Settings: "Relay server", "Use the public relay servers of n0", "Discover hosts on the
  local network", a "Relay token" row (Set Token…) and Test Connection (connects once to the
  relay as typed in the page). The token lives in the keychain (service `forge-cwm`, account
  `relay-token`), never in settings, links, logs or the IPC.
- UI: the popup lists the participants with a Direct / Relay badge and the relay state, and a
  status bar item shows the path in use. The backend re-emits the status when a path changes,
  because iroh does not announce the move from relay to direct.
- Tests (no network): an in-process `iroh-relay` on loopback (self-signed certificate)
  connects a guest without direct addresses, the connection reaches the direct path through
  hole punching, the token is enforced and its denial is reported, Test Connection works; the
  default configuration asserts no relay, no lookup and nothing to report.

Deviations from the plan above:

- Reconnects with backoff came later, on `feat/code-with-me-s6` (see "S6 reconnect as built"
  below). iroh keeps a connection alive across path changes (Wi-Fi to cable, relay to direct),
  so only a real outage is visible.
- The guest-side relay state is only shown while iroh has a home relay for it; the path badge is
  the reliable indicator on guests.
- With a relay configured, iroh's HTTPS latency probes run against that relay only (needed to
  pick the home relay on relays without QUIC address discovery). Captive portal checks stay
  off; without a relay no probe runs at all.
- The SSH recipe uses OpenSSH's tunnel device (`ssh -w`), not `socat`: UDP over a TCP stream
  loses the packet boundaries QUIC needs.
- Added Test Connection and the connection badges (the participants list itself arrives with
  S4, so the badges are in the popup and the status bar until then).

**S6 reconnect as built** (`forge_collab::reconnect`, `host.rs`, `guest.rs`,
`crates/forge-collab/tests/simulation.rs` "reconnect" tests, `features/code-with-me/reconnect.ts`):

- Protocol 4. `Welcome` carries a random 32-byte `resume` token. A guest whose connection
  dropped dials again and sends `GuestMessage::Resume { session, participant, token }` instead
  of a knock; the host takes it back only from the same endpoint key with the right token
  (compared in constant time) while it keeps the place, without asking the host's user again
  and independent of Regenerate Link. Anything else gets `Refused` ("The host no longer keeps
  your place…", or "The host removed you…" for a guest removed while away); the place stays
  its owner's.
- Host: a dropped connection (read error or EOF without `Leave`) marks the guest
  `Participant.reconnecting` (everyone sees it, the session log says "The connection of Alex
  dropped") and replaces its queue with a closed one, so nothing is queued for it. Its id,
  color, permissions, open documents, terminals, runs and calls state stay for
  `reconnect::HOST_GRACE` (2 minutes, `HostSession::set_reconnect_grace`, zero = old
  behaviour); then it is removed ("Alex lost the connection and did not come back."). A resume
  bumps the guest's connection epoch, so a late read error of the old connection is ignored, and
  sends `Welcome { resumed: true }`, the participants, everyone's presence, Make Everyone Follow
  You, and the terminals and runs again with fresh snapshots ("Alex reconnected").
- Guest: `GuestSession` supervises its connection. On a drop it fails the requests in flight,
  sends nothing (documents resynchronize, presence is sent again; terminal and console keys
  typed meanwhile are lost), keeps participants, presence, terminals and runs, and tries again
  with `ReconnectPolicy` (1, 2, 4, 8, then every 15 s; gives up after
  `reconnect::GUEST_GIVE_UP` = 105 s, before the host frees the place) — like
  `forge-remote::connection`. Requests fail fast with "The connection to the host is being
  restored." `GuestSession::connection()` is the `ConnectionState`; `GuestEvent::Resumed` tells
  the UI to resync. Giving up ends the session with `reconnect::GAVE_UP`.
- Documents: `GuestMessage::DocSync { state_vector }` -> `HostMessage::DocSynced { update,
  state_vector, path, saved, read_only }` (Yjs sync step 1/2 on the host's `yrs` replica,
  `SharedDoc::diff_since`); the UI (`resyncDoc` in `docs.ts`) applies the host's part and sends
  what the host is missing as a normal `DocUpdate`, so edits made while the connection was down
  are kept. A document closed meanwhile answers `DocError` and the tab closes as for
  `closed`; a rename meanwhile is followed.
- Terminals and runs after a resume: a snapshot for something the guest already shows is turned
  into the missed part only (`snapshot_tail`: the new suffix, or a terminal reset and the whole
  scrollback when the host's scrollback dropped lines; for runs the events after the ones
  shown).
- IPC: `collab_doc_sync({ docId, stateVector })` (raw bytes like `collab_doc_open`, header with
  the host's `stateVector`), event `collab://resumed`, `SessionView.connection` (guest),
  `Participant.reconnecting`. UI: status item "Code With Me: Reconnecting…" (its popup shows the
  next attempt), a sticky balloon "Connection to <host> lost" with Leave Session, "Reconnected
  to <host>." when back, and "Reconnecting…" with a faded avatar in the host's participant
  list. Browser mode: `?project=cwm-guest&reconnecting[=resume]`, `?cwmAway` (host).
- Missed file changes (`fix/code-with-me-guest-polish`): while a watching guest is away the host
  collects what its watcher reports (`reconnect::MissedChanges`, filtered by the guest's
  policy, latest state per path, at most `MAX_MISSED_PATHS` = 10 000) and sends it as one
  `FsChanged` with the resume. Changes queued on the dead connection before the host noticed
  the drop are still lost, so the UI (`catchUpAfterResume` in `reconnect.ts`) also re-reads every
  listed folder of the Project view (`refreshTree`, expansion and selection stay) and dispatches
  a `git` change so Git status, Find and the other file-event views refresh. Test:
  `a_resumed_guest_gets_the_changes_it_missed` (`tests/guest_backend.rs`).
- Status bar popups (the guest's Code With Me item, the remote host item) are rendered by
  `shell/StatusPopup.tsx`: in `document.body`, `position: fixed`, z-index 345 (above
  CodeMirror's sticky panels at 300, which had covered their bottom ~26 px, and the balloons),
  placed above the widget, shifted into the window, flipped below only with more room there,
  and scrolling when the window is too low (`placeStatusPopup`, unit-tested).
- Tests (in-memory network, `MemoryNetwork::set_offline` takes an endpoint's network down):
  resume with unsent edits on both sides converging, place and color kept, presence resent;
  grace expiry; giving up; removed while away; another endpoint or a wrong token cannot take
  a place; documents deleted while away; an attached terminal gets the missed output exactly
  once. Not covered: a real QUIC outage (iroh's idle timeout decides how fast a dead
  connection is noticed) and a host that restarts (its new session has a new key; guests give
  up).

### S7 — Polish, docs, screenshots, stress tests (5 d)

- File Cache Conflict flow for shared files, large and binary files read-only, the gutter
  marker for guest-changed lines, balloons and wording review against PHPStorm, keyboard
  navigation in the popup, accessibility labels, the 10-participant stress test, fuzz targets,
  `docs/guide/user/code-with-me.md`, the settings and keymap reference rows, ARCHITECTURE.md
  section and IPC contracts, `llms-full.txt` regenerated, README and ROADMAP updated.
- **Accept**: all screenshots in the list below reviewed; the performance targets met in the
  bench and stress tests; the manual desktop test plan passes on Linux, Windows and macOS.

**S7 as built** (branch `feat/code-with-me-s7`):

- **File Cache Conflict** (`docs::SharedDoc::apply_disk` / `resolve_conflict` /
  `settle_conflict`, `HostSession::resolve_conflict`, protocol 5): a disk change to a shared file
  with unsaved changes keeps the document and opens a conflict holding the disk's text; a
  second change while it is open replaces that text without asking again. The host's UI gets
  `collab://doc` `{ conflict: { open, path } }` (also without the tab open) and shows PHPStorm's
  dialog ("Changes have been made to '<path>' in memory and on disk.") with Show Difference,
  Keep Memory Changes and Load File System Changes (`features/code-with-me/conflict.ts`, one
  dialog at a time); Show Difference and closing the dialog leave the choice in a sticky
  balloon. `collab_doc_resolve_conflict({ docId, load })` answers: Load applies the minimal diff
  for everyone, Keep makes the disk's text the saved text so the document stays unsaved and the
  next save overwrites the file. Guests get `HostMessage::DocConflict` (and `conflict` in
  `DocOpened` / `DocSynced` for late joiners and resumes), a balloon, and their saves are
  refused while it is open. The conflict also ends when the host saves (memory wins), when
  the disk changes back to the saved text, and when an edit makes the document equal to the
  disk. The former notice "changed on disk. The unsaved shared changes are kept." is gone.
- **Lines changed by guests** (`features/code-with-me/guestChanges.ts`): `HostEvent::DocUpdate`
  and `collab://doc` carry `from` (the guest; absent for disk changes). The UI applies the
  update inside `withAuthor`, a transaction extender tags the editor transaction
  `y-codemirror.next` dispatches synchronously, and a gutter next to the line numbers marks the
  lines in the guest's color (title "Changed by Alex"), one marker per line, the latest author
  wins, positions follow later edits; every save (and a reload from disk) clears them. Host
  only; it is per-keystroke editor work, so it is TypeScript with vitest, not Rust.
- **Stress test** (`tests/stress.rs`): the host and 9 guests over the in-memory network type
  into two files, move carets and follow each other, the host's shared terminal prints and the
  guests type into it. Asserts identical replicas, everyone's last presence everywhere, the
  terminal output exactly once per guest, every key at the host's shell, no refusals, and the
  backlog: `HostSession::backlog` / `GuestSession::backlog` (new) peak far below the
  cut-off (`GUEST_QUEUE` = 4096; measured 21 and 2 at 200 rounds) and drain to zero. About two
  seconds in `cargo test`; the plan's target load (5 characters per second each, 5 ms latency,
  300 rounds) is `#[ignore]`d: `cargo test -p forge-collab --release --test stress -- --ignored`
  (`FORGE_CWM_STRESS_ROUNDS` sets the rounds). CPU per core is not measured.
- **Fuzzing**: cargo-fuzz targets in `crates/forge-collab/fuzz` (own workspace, `Cargo.lock`
  ignored): `link_parse` (parse, then the URL and text forms round-trip), `frame_decode`
  (guest and host frames from arbitrary chunks), `host_call_decode` (decode, then round-trip).
  `cargo +nightly fuzz run <target>`; not run in CI or `cargo test`. The always-run proptest
  sibling is `tests/decoding.rs` (proptest was already a dev-dependency).
- **Wording and accessibility**: the popup's entries and shared files and the participants are
  keyboard items (`a11y.ts`: Up / Down / Home / End move, Enter / Space choose; native radio
  buttons keep their keys), action groups are `role="menu"`, the permission radio group, the
  shared files list and the session log have labels, the Custom Permissions and Session Log
  dialogs are `aria-modal`, the tool window tabs are focusable with `aria-selected`. The Join
  Session dialog and the guest's status bar popup were done on `fix/code-with-me-guest-polish`.
- **Docs**: the Code With Me section in `docs/ARCHITECTURE.md` (layers, contribution points,
  protocol, host enforcement, documents and conflicts, reconnect, IPC, tests, binary size), the
  guide's "Saving and changes on disk" and keyboard notes, README and ROADMAP.
- **Binary size**: still not measured in bytes (no release build on the development machine,
  no cargo-bloat). `cargo tree --prune forge-collab` shows 72 of the app's 871 crates are only
  reachable through forge-collab (iroh, its QUIC stack `noq`, relay client, DNS / mDNS,
  `netwatch` / `netlink-*`, `yrs`, `blake3`, AES-GCM, `postcard`); the method for the release
  machine is in ARCHITECTURE.md.
- **Per-guest rate limits** (`rate`, `HostSession::within_rate` in `run_guest`, before a
  message is checked or dispatched): token buckets per admitted guest, kept across
  reconnects, with the clock passed in for the unit tests. Project calls (`CALL_RATE`: burst
  1000, 200 a second; the hostile-guest test alone sends about 450 in a row; at most
  `MAX_PENDING_CALLS` run at once anyway), document open / save / sync (`DOC_RATE`: 100, 20 a second) and terminal
  and run input (`INPUT_RATE`: 500, 100 a second) over their rate get their normal answer
  (`CallResult` error, `DocError`, `SaveResult` with error, `Denied` once per burst for input)
  with "Too many requests. Try again in a moment.", and the session log one `Denied` entry per
  burst (refusals less than `QUIET`, 5 seconds, apart); requests never disconnect. Edits,
  awareness, presence and document close are never refused, which would break convergence;
  instead they count against `FLOOD_RATE` (burst 1000, 300 a second, about ten times fast
  typing with carets), and a guest over it is disconnected (`HostMessage::Ended` "... sent far more messages than anyone can type",
  session log entry, host notice). `tests/rate_limits.rs`: a guest flooding calls is refused
  and logged once while another guest is unaffected; a guest flooding edits is disconnected,
  the host keeps every edit up to the cut-off, the other guest stays. The stress test (both
  variants) stays below the limits; with `FORGE_CWM_STRESS_ROUNDS` in the thousands, its
  1 ms pause between edits (several hundred messages a second per guest) would exceed
  `FLOOD_RATE` after a few seconds.
- Not done in S7: large and binary files are still refused for sharing rather than opened
  read-only for guests (oversized files stay normal host tabs); the manual desktop test plan
  on Linux, Windows and macOS.

## Keeping one large PR reviewable

A PR of this size (estimated 12 000 to 15 000 changed lines including tests) stays reviewable
if it reads as a sequence of small, self-contained commits and the description tells the
reviewer where to start.

**Commit structure**

- One commit group per stage, each group starting with a commit whose subject names the stage
  ("Add Code With Me session links and permission model"). Within a group, commits are small
  and in dependency order: pure Rust model → runtime → Tauri commands → typed client and mock
  → UI → tests → docs.
- Every commit builds and passes the gates, so `git bisect` works and reviewers can check out
  any stage.
- Mechanical changes (snapshot regeneration, `llms-full.txt`, WebAssembly bundle under
  `app/ui/src/wasm/pkg`, lock files) go into their own commits with subjects that say so
  ("Regenerate browser plugin snapshot"), so reviewers can skip them.
- Changes to existing code (`state/editor.ts`, `CodeEditor.tsx`, `commands/lsp.rs`, terminal,
  run) are separate commits from the new feature code, each explaining why the hook is needed.
- Fixups during review are squashed into the commit they fix (`git commit --fixup` +
  `rebase --autosquash` locally) before merge; no "address review" commits remain.

**PR description outline**

1. Summary: what Code With Me is in Forge, a 30-second GIF or two screenshots.
2. How to try it: browser mode (`pnpm dev`, `?collab=host` and `?collab=guest` in two
   windows), desktop on one machine (two instances with separate `XDG_CONFIG_HOME` and
   `XDG_DATA_HOME` on Linux), two machines, with a relay.
3. Architecture in one diagram (from this plan) and a link to this plan.
4. Stage map: the stage table with the commit range of each stage.
5. Security notes: enforcement points, the hostile-guest tests, excludes, what is never sent.
6. Changes to existing code, listed by file with one line each.
7. Tests added (counts per area), the bench numbers, the binary size change.
8. Manual test plan with check boxes.
9. Screenshots (below).
10. Follow-ups: browser guest, port forwarding, shared debugging, voice and video.

**Screenshots** (`app/ui/scripts/scenarios/code-with-me.mjs`, Ember and Anvil each)

1. Toolbar with the Code With Me icon; the Start Session popup with the four permissions.
2. Custom Permissions dialog.
3. Session popup during a session (participants, Copy Session Link, Permissions, End Session).
4. Host's approval balloon with the verification code.
5. Guest's Join Session dialog and the waiting state with the code.
6. Editor with two remote carets, labels and a remote selection.
7. Guest window: title, status item, Project tree, read-only editor for a Read-only guest.
8. Following banner and the toolbar avatars.
9. Code With Me tool window: Participants and Session Log tabs.
10. Shared terminal on the guest (read-only) and the host's guest terminal tab.
11. Run tool window on the guest with the host's PHPUnit run.
12. Settings | Tools | Code With Me.

**Stages that can be reviewed separately**

- S0 alone (model, link format, permissions) is the best first review: it fixes the security
  rules before any network code exists.
- S1 + S2 together are the core (sync and admission) and need the most careful review,
  ideally by two people: one for the CRDT and editor integration, one for transport and
  security.
- S3 (guest backend) is mostly mechanical against the `Backend` trait and can be reviewed by
  someone who knows `RemoteBackend`.
- S4, S5 and S6 are independent of each other and can be reviewed in any order or in
  parallel.
- S7 is docs, polish and tests.

Reviewers can mark stages as reviewed in the PR description's stage map; the author keeps the
commit ranges up to date after each rebase.

## Open questions

Each question has a recommended answer; the plan above assumes the recommendation.

1. **Is voice / video in scope?** Recommended: **no**. Link to the team's existing call tool;
   revisit with WebRTC (`str0m`) and a self-hosted SFU (LiveKit) only if users ask.
2. **Default relay: none, n0's public iroh relays, or a Forge-run relay?** Recommended:
   **none by default**, a "Relay server" URL setting, an opt-in checkbox for n0's public relays
   with a note that they are rate-limited, and a documented `iroh-relay` Docker setup. No
   Forge-run service.
3. **Should guests need Forge, or also a browser?** Recommended: **Forge only in this PR**;
   browser guest as the first follow-up (about 3 to 4 weeks).
4. **Default permission for new sessions?** Recommended: **Read-only**, as PHPStorm.
5. **Exclude `.env` and other secret files by default?** Recommended: **yes**, with the list
   in "Secret files", editable in the settings.
6. **Can Edit files guests change files that configure execution (`.forge/`, `composer.json`,
   `package.json`, `Makefile`)?** Recommended: **no, Full access only**, with the list as a
   setting. Stricter than PHPStorm, but a guest with Edit files must not be able to turn the
   host's next Run into their command.
7. **Should Remove Guest also ban, or should the host regenerate the link?** Recommended:
   removal refuses that endpoint for the rest of the session, and **Regenerate Link** exists
   for leaked links.
8. **Record terminal keystrokes of guests in the audit log?** Recommended: **no** by default
   (passwords typed in the terminal would end up in a file); log that input happened, with an
   opt-in for full logging.
9. **Can a host share a remote SSH project?** Recommended: **yes**; it comes for free because
   Code With Me sits on `AppState::backend()`. Note in the UI that guests reach the SSH host
   through the Forge host.
10. **Guests' own project while joining?** Recommended: **close it** (one project per window,
    as today); a separate guest window is a follow-up together with multi-window support.
11. **PHPStorm action ids**: confirm the real ids from PHPStorm 2026.1 with the plugin before
    implementation and use those (Forge ids as aliases).
12. **Maximum number of guests?** Recommended: **no hard limit**, a warning above 10.

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| The single PR drifts from `dev/gallant-cerf-630lu4` while that branch moves fast | Painful rebases, especially in `state/editor.ts`, `CodeEditor.tsx`, `commands/lsp.rs`, terminal and run | Rebase at least twice a week; keep changes to existing files in small separate commits so conflicts are local; put new code in new files; add hooks to existing code as small extension points (e.g. a "tab content source" in `openFile`) instead of editing call sites; regenerate snapshots and generated files after each rebase instead of resolving conflicts in them |
| A long-lived branch hides problems until the end | Late surprises in review | Every stage passes the gates and is pushed for backup when asked; S0 reviewed early as a draft PR if the user agrees; screenshots per stage |
| Reviewer fatigue on a 12–15k line PR | Shallow review of security-relevant code | Commit structure and stage map above; security code isolated in `forge-collab::permissions` and the host dispatcher; hostile-guest tests as executable documentation |
| Guest permission bypass | Code execution on the host | Host-only enforcement, closed `HostCall` enum, no raw git or process calls, path canonicalization, hostile-guest tests, fuzzing |
| `y-codemirror.next` maintenance (0.x, single maintainer) or Yjs v14 migration | Binding breaks on CodeMirror updates | Pin versions; the binding is ~1 000 lines and can be vendored; `@codemirror/collab` as documented fallback |
| CodeMirror history vs. Y.UndoManager mismatch | Undo surprises in shared files | Swap history per shared tab only; tests for undo with concurrent edits; history restarts at session start and end |
| iroh dependency size and churn | Larger binary, upgrades | 1.x wire-stability promise; transport behind the `Transport` trait; size measured in S1 |
| NAT traversal fails on strict networks without a relay | Users cannot connect | Clear error with the options (relay URL, VPN, port forward); connection type shown |
| Large files or generated files | Memory and latency | Size limit with read-only fallback; no binary files in Yjs |
| Disk reload during concurrent editing | Lost or duplicated text | Minimal diff applied on the host as one transaction; property tests on the diff path |
| WebKitGTK / platform differences (clipboard for links, deep links on Linux) | Join flow broken on some desktops | Paste-link dialog as fallback; manual test plan on all three OSes |
| JetBrains-like wording without the JetBrains service | Users expect JetBrains links to work | Forge links use `forge://cwm/…`; the guide says Forge and JetBrains sessions do not interoperate |

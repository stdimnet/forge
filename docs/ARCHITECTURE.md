# Architecture

```
crates/            Rust core libraries (forge-*), language-agnostic
packs/<pack>/      Plugins grouped by pack: phpforge, webforge, luaforge
plugins/           Generic plugins (git, database, docker, xdebug, ...)
app/src-tauri/     Desktop shell: Tauri setup + IPC commands
app/ui/            SolidJS UI
```

## Backend (Rust)

- `forge-core` — workspace (sandboxed path resolution), file system, settings, events.
- `forge-backend` — the `Backend` trait every project command goes through (local folder or a
  remote host via the Forge agent), the agent protocol and its client (see "Project backends
  and remote development").
- `forge-plugin-api` — `Plugin` trait, `PluginManifest`, `Contributions`. Packs depend only on this.
- `forge-project` — project type detection and the New Project generators/templates.
- Plugins declare contributions (commands, keybindings, tool windows, typed settings, LSP
  servers, run configuration types, AI providers, chat agents, code generators) as serde data;
  `PluginRegistry` tracks enablement (cascading over `depends`) and answers lookups. Built-in
  manifests are TOML files in `app/src-tauri/plugins/`; `app/ui/src/mock/builtinPlugins.json`
  is their browser-mode snapshot (regenerate with `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`).
- Settings (`forge_core::Settings`) are layered: schema defaults < user
  (`<config dir>/forge/settings.toml`) < project (`<root>/.forge/settings.toml`), keyed by dotted
  names. IPC: `settings_get_all`, `settings_set(key, value, scope)`, `settings_schema`,
  `plugins_list`, `plugin_set_enabled`; changes are emitted as `settings://changed`. In the UI read
  them with `getSetting(key)` from `src/state/settings.ts`.
- Feature crates (`forge-lsp`, `forge-vcs`, `forge-forges`, `forge-ai`, `forge-agent-cli`, ...) are plain libraries
  with their own tests; `app/src-tauri` wires them to IPC.
- IPC commands live in `app/src-tauri/src/commands/<area>.rs`, are registered in `lib.rs`, and
  return `forge_core::Result<T>` (errors serialize to strings).

## UI (SolidJS)

- `src/ipc/` — `invoke`/`listen` wrappers. In a plain browser, calls go to mock handlers
  registered with `registerMock` (see `src/mock/`), so every feature works without Rust and can
  be screenshotted.
- Contribution points (the UI side of plugins):
  - `registerToolWindow` (`src/features/registry.ts`) — panels on the left/right/bottom stripes
    (`anchor` and optional `split` are the default place; users move them, see `state/layout.ts`).
  - `registerStatusItem` (`src/features/statusItems.ts`) — status bar widgets.
  - `registerCommand` (`src/state/commands.ts`) — actions with PHPStorm-style ids, bindable to keys.
  - `registerSettingsPage` (`src/features/settingsPages.ts`) — pages in the Settings dialog.
  - `registerEditorExtension` (`src/editor/extensions.ts`) — CodeMirror extensions per file.
  - `registerSearchProvider` (`src/features/search/providers.ts`) — Search Everywhere tabs
    (Classes, Files, Symbols, Actions, Text); language packs plug in class/symbol indexes.
    Providers only collect candidates; ranking happens in Rust (see "Search, file types and the
    Settings model" below).
  - `registerKeymapActions` (`src/keymap/store.ts`) — plugin actions with default shortcuts.
  - `registerMainMenuEntries(path, entries, { before?, after?, when? })`
    (`src/features/mainMenu`) — entries in the main menu ("Tools", "Tools/_Laravel",
    "View/Tool Windows"); manifests do the same with `[[contributions.mainMenu]]`.
    `registerMainMenuDynamic(source, provider)` fills dynamic lists (`recentProjects`,
    `toolWindows`), `registerMainMenuToggle(actionId, isOn)` reports check marks.
  - `registerRunConfigurationType` / `registerDebugLauncher` / `registerBeforeLaunchHandler`
    (`src/features/run`) — Run/Debug configuration types, debuggers and Before launch task
    kinds (see "Terminal and Run/Debug" below).
  - `registerRunAnythingProvider` (`src/features/runAnything`) — commands offered in Run
    Anything (double Ctrl), e.g. Artisan and Symfony console commands.
- Tool windows may render tabs next to their title with `headerContent` (Terminal, Run).
- Keymap focus contexts: `editor`, `input`, `toolwindow`, `diff` (diff and merge windows, `[data-focus-context="diff"]`; "diff" actions such as Next Difference only fire there), `global` and `terminal` (xterm.js has
  focus). In the terminal plain Ctrl+letter, Alt+letter and Escape go to the shell; function
  keys, Alt+digit, Ctrl+Shift and Ctrl+Alt combinations still reach the IDE. With Vim
  emulation on, the focused editor's Vim takes the keys it needs first (see "Vim emulation").
  - `registerLineMarkerProvider` (`src/editor/lineMarkers.ts`) — gutter icons per line (route on a
    controller action, "go to template", entity <-> repository), recomputed after edits.
  - `registerProblemsTab({ id, title, order?, count?, visible?, component })`
    (`src/features/lsp/problemsTabs.ts`) — a tab of the Problems tool window after "Current
    File" and "Project Errors" (Composer's "Vulnerable Dependencies").
  - `registerDeclarationProvider` (`src/features/lsp/goto.ts`) — framework targets for Ctrl+B /
    Ctrl+Click that a language server cannot resolve (route names, service ids, DQL fields); tried
    before the language servers and the PHP index (see "Code intelligence" below). Several targets (from a provider or the language server) open the shared
    "Choose Declaration" popup: `chooseDeclarationAt(view, pos, targets, { title?, open? })`
    (`src/features/navigation/chooseDeclaration.ts`; targets `{ path, line, column?, preview? }`,
    0-based). WebForge, LuaForge, Laravel, Symfony, Doctrine and the LSP use it.
  - `claimFrameworkStatus` (`src/features/project-type`) — a framework plugin with its own status
    widget hides the generic "Laravel 12" / "Symfony 7" one.
  - `registerFileEditor({ id, accepts(path), component })` (`src/shell/editorViews.ts`) — an
    editor for some text files instead of the plain `CodeEditor` (PHPStorm's file editor
    providers): Markdown and SVG use `TextEditorWithPreview` (`src/editor`, layout actions
    `TextEditorWithPreview.Layout.*`), CSV/TSV the Text / Table tabs. The component gets the
    tab and its editor group and embeds `CodeEditor` itself.
    `registerBinaryFileView({ view, accepts(path) })` (`src/state/editor.ts`) makes `openFile`
    open binary files (images) as a view tab without reading text.
  - `openCustomTab({ id: "forge:<kind>/<id>", title, icon?, component })` (`src/state/editor.ts`) —
    editor tabs that show a feature view instead of a file (merge requests, issues). `activeTab()`
    stays file-only; use `isCustomTab(id)` where an id may belong to such a tab.
- Keymap: the engine is the Rust crate `crates/forge-keymap` (shortcut parsing and display,
  PHPStorm's default keymaps for Windows/Linux and macOS in `data/phpstorm.json`, action
  aliases, plugin actions, user overrides from the `keymap.*` settings, conflicts and the
  resolver state machine for chords, double taps and focus contexts). Action ids are PHPStorm's
  and double as command ids: implementing an action is just `registerCommand({ id: "GotoClass",
  ... })`; unimplemented actions show "not available yet". `src/keymap/` is a thin client: it
  captures DOM key events, classifies the focus context, forwards the event to the resolver and
  runs the returned action. Key events are resolved synchronously by the same Rust code compiled
  to WebAssembly (desktop and browser), because the handler must decide on `preventDefault`
  before it returns; the keymap data and all changes go through the `keymap_*` commands.
- A feature is a folder `src/features/<name>/` with an `index.ts` that registers its
  contributions; add one import line to `src/features/index.ts`. Browser mocks go to
  `src/mock/<name>Mock.ts` and are imported from `src/mock/index.ts`.
- Styling uses the CSS variables from `src/theme/theme.css` only (dark + light), icons from
  `lucide-solid`.
- Editor reloads and line endings: content that changed on disk (rollback, checkout, pull, agent
  edits) is applied with `replaceDocument` (`src/editor/reload.ts`). The reload is not an undo
  step and the undo history restarts, so Ctrl+Z never steps back over it. Editor buffers always
  use `\n`; the tab remembers the file's line ending (`EditorTab.eol`) and `saveFile` writes it
  back, so CRLF files open unmodified.

## Conventions

- Layout, wording and shortcuts follow PHPStorm; colors are Forge's own.
- Every UI change is checked with `pnpm screenshots` (see `.claude/skills/verify-ui-with-screenshots`).
- Project tree: single click selects, double click opens (setting `projectView.openFilesWithSingleClick`).
- Commits: small, imperative subject lines, no co-author trailers.

## Shared IPC contracts

- Keymap (`src/ipc/keymap.ts`, `forge-keymap`): `keymap_actions() -> KeymapSnapshot` (selected
  keymap, platform, every action with its shortcuts, display strings, `modified` and aliases);
  `keymap_register_actions({ actions?, commands? }) -> KeymapSnapshot`;
  `keymap_set_override({ change }) -> KeymapSnapshot` where `change` is `{ op: "selectKeymap" |
  "set" | "add" | "remove" | "reset" | "resetAll", … }` and is stored in the user settings;
  `keymap_conflicts({ shortcut, actionId? }) -> ShortcutCheck`;
  `keymap_find_by_shortcut({ shortcut }) -> string[]`; `keymap_resolve({ event }) -> Resolution`
  (`{ consumed, action?, pending?: "chord" | "doubleTap" }`, the IPC fallback of the in-process
  resolver); `keymap_import_legacy({ active, overrides })` moves a pre-settings keymap from
  localStorage into the settings.
- Main menu (`src/ipc/keymap.ts`, `forge-keymap::menu`): `keymap_main_menu({ request }) -> MainMenu`.
  The request carries `contributions` (manifest `mainMenu` of active plugins plus
  `registerMainMenuEntries`), `dynamic` lists (`{ key, label, action?, detail?, enabled?,
  checked? }` by source), the `checked` toggle action ids and `git` (Git menu instead of VCS).
  Rust merges the contributions into PHPStorm's menu structure and answers one submenu per
  top-level menu, each item with `label`, `mnemonic` (character index), `shortcut`, `enabled`
  (some command implements the action or an alias; unimplemented actions stay visible but
  disabled), `checkable`/`checked` and tidy separators; empty submenus are dropped. The UI
  renders it with the context menu renderer (`showResolvedMenu`) as the ☰ popup or a menu bar,
  depending on `appearance.mainMenu` (`hamburger` | `merged` | `toolbar`).

- `fs_list_dir({ path }) -> { name, path, kind, ignored, excluded? }[]` — `excluded` marks excluded
  entries (see "Project structure"); `ignored` is set for entries matched by
  `.gitignore`, `.git/info/exclude` or the global excludes file (ignored folders ignore their
  content); the Project view grays them out. The browser mock mirrors this with
  `src/mock/gitignoreMatcher.ts`.
- `fs_create_file({ path })`, `fs_create_dir({ path })` (missing parents are created, existing paths are
  refused), `fs_rename({ from, to })` (never replaces, never moves a folder into itself) and
  `fs_delete({ path })` (a folder with its content; the UI asks first). All paths are
  workspace-relative and refused for the project root or outside it. They back the Project view
  context menu (`features/project/fileMenu.ts`); the browser mock keeps the changes in memory.
- Editor actions (`src/ipc/editing.ts`, `forge-editing`; offsets are UTF-16 like CodeMirror
  positions): `editor_postfix_templates() -> TemplateInfo[]` (id `php.if`, key, language,
  group, description, before/after example); `editor_postfix_complete({ request: { language,
  line, indent?, disabled? } }) -> { keyFrom, key, items: { id, key, description, from,
  expression, text, selectionFrom, selectionTo }[] } | null` for `expr.key` at the end of
  `line` (the caret's line up to the caret); `editor_complete_statement({ request: {
  language, text, offset, indent? } }) -> { changes, cursor } | null` (Complete Current
  Statement); `editor_expand_word({ request: { text, offset, others?, backward?, language? }
  }) -> { from, prefix, candidates }` (Cyclic Expand Word, nearest first). Browser mode runs
  the same code through `forge-wasm` (`editing_*`).
- Clipboard history (`src/ipc/clipboard.ts`, `forge_editing::clipboard`, Paste from History):
  `clipboard_history_add({ entry: { text, carets? }, max? }) -> bool` (dedupes, newest first,
  keeps `max` = `editor.clipboard.maxContents`; `carets` are the per-caret parts of a
  multi-caret copy); `clipboard_history_list({ query?, max? }) -> { index, text, carets?,
  preview, lines, chars }[]`; `clipboard_history_remove({ index }) -> bool`;
  `clipboard_history_promote({ index }) -> { text, carets? } | null`. Session memory only
  (`ClipboardState` in `app/src-tauri`, a thread-local in `forge-wasm` for browser mode).
- File and code templates (`src/ipc/templates.ts`, `forge-file-templates`):
  `templates_list({ custom? }) -> { templates: TemplateInfo[], variables }` (built-ins merged
  with the `fileTemplates.custom` setting, or with `custom`, the settings page's draft; each
  with `kind`, `name`, `extension`, `text`, `builtin`, `modified`, `defaultText`, `when`
  (project type), `group`, `phpClass`, `variables` (asked for in the New dialog), `references`,
  `error`); `templates_validate({ text }) -> string | null`;
  `templates_php_namespace({ dir }) -> string | null` and
  `templates_php_directory({ namespace }) -> string | null` (PSR-4 rules of the composer.json
  from `php.composer.json`, read through the backend); `templates_create({ request: { template,
  dir, name, fileName?, namespace?, extends?, implements?, variables?, context?: { now? } } })
  -> { path, text }` renders the template and creates the file through the backend (never
  replaces one). `php_generate` fills `GenerateOptions.codeTemplates` from the setting. Browser
  mode runs the same code through `forge-wasm` (`templates_*`); the mock writes into the
  in-memory demo project. The Project view's New submenu asks `providedEntries("project.new",
  dir)` for the template entries.
- `fs_find_files({ query, limit? }) -> { path, name, score, positions, matchedPath }[]` — fuzzy
  "Go to File" (camel humps, respects `.gitignore`).
- `fuzzy_filter({ request: { query, candidates, strict?, preferShorter?, limit? } }) -> { index, text, score, positions }[]`
  — speed search over in-memory lists (Recent Files, popups); a candidate is a text or a name
  with aliases.
- `search_rank({ request: { tab, query, perGroup, includeDimmed?, sources } }) -> { groups }` —
  Search Everywhere ranking (`forge_fuzzy::everywhere`).
- `filetypes_detect({ request: { files, inactivePlugins, dataLanguages, softWrapFiles } }) -> FileTypeInfo[]`
  — file type id, status bar label, indent, template data language and soft wrapping.
- `settings_search({ request: { query, pages?, settings?, entries?, highlight? } })` and
  `settings_validate({ key, value }) -> string | null` — the Settings dialog model.
- Git menu commands (`forge-vcs`, `commands/git.rs`): `git_add_to_gitignore({ paths }) -> string[]`
  (anchored patterns appended to the repository's root `.gitignore`), `git_create_tag({ name,
  rev?, message? })`, `git_stash({ message, keepIndex, includeUntracked }) -> string`,
  `git_stash_list() -> { index, branch, message, time }[]`, `git_unstash({ index, pop,
  reinstateIndex }) -> string`, `git_drop_stash({ index })`, `git_remotes() -> { name, fetchUrl,
  pushUrl }[]`, `git_add_remote({ name, url })`, `git_remove_remote({ name })`. They back the
  Git submenu of the Project view (`features/git/fileMenu.ts`); file actions there run PHPStorm
  action ids (`CheckinFiles`, `Git.Add`, `Compare.Selected`, …) inside `withVcsTargets`, and the
  same commands use the focused tree, Commit window or editor (`vcsTargets()`) for shortcuts.
- Git menu dialogs and the shelf (`forge_vcs::ops`, `repo/ops.rs`, `repo/shelve.rs`,
  `forge_vcs::shelf`, `commands/git_ops.rs`, UI `features/git/gitDialogs.ts`, `GitDialogs.tsx`,
  `StashShelfPanels.tsx`, `operation.ts`): `git_pull_with({ options: { remote, branches, rebase?,
  ffOnly?, noFf?, squash?, noCommit?, noVerify? } }) -> { output }`, `git_merge_with({ options: {
  branches, noFf?, ffOnly?, squash?, message?, noCommit?, allowUnrelatedHistories?, noVerify? }
  }) -> string`, `git_rebase_with({ options: { onto, from?, branch?, root?, rebaseMerges?,
  keepEmpty?, updateRefs? } }) -> string` (incompatible options are rejected in Rust; conflicts
  fail and leave unmerged files for the resolver), `git_operation_state() -> { operation:
  "rebase" | "merge" | "cherryPick" | "revert" | null, branch, onto }` (branch widget label
  `branchLabel()` in `state/vcs.ts`), `git_operation_action({ action: "continue" | "abort" |
  "skip" })`, `git_reset_head({ rev, mode: "soft" | "mixed" | "hard" | "keep" })`,
  `git_tag_with({ options: { name, commit?, message?, force? } })`, `git_resolve_commit({ rev })
  -> sha | null`, `git_edit_remote({ oldName, name, url })`, `git_stash_files({ index }) ->
  ChangedFile[]`, `git_stash_file({ index, path }) -> { before, after }`, `git_stash_clear()`,
  `git_stash_branch({ index, branch })`; shelf (stored in `<data dir>/forge/projects/<name>-
  <hash>/shelf/<id>/{changes.json, shelved.patch}`, never in the project): `git_shelf_list() ->
  { id, name, date, branch, files: { path, status: "added" | "modified" | "deleted", binary }[]
  }[]`, `git_shelve({ paths, name })`, `git_unshelve({ id, paths?, remove })` (plain `git apply`,
  else `--3way`), `git_shelf_delete({ id })`, `git_shelf_rename({ id, name })`,
  `git_shelf_file({ id, path }) -> { before, after, binary }`. Browser mode: `mock/gitOpsMock.ts`
  (`?rebasing` starts in a stopped rebase).
- Changelists, partial commits and patches (`forge_vcs::{changelists, hunks, patch}`,
  `repo/partial.rs`, `repo/patches.rs`, `commands/git_changes.rs`, UI `features/git/
  changelists.ts`, `changesDialogs.ts`, `ChangesDialogs.tsx`, `partialGutter.ts`,
  `commitChecks.ts`): changelists live in `<data dir>/forge/projects/<name>-<hash>/
  changelists.json`; every changelist command answers the lists `{ id, name, comment, active,
  files: { path, hunks: string[] | null }[] }[]`: `git_changelists()`,
  `git_changelist_create({ name, comment, active })`, `git_changelist_edit({ id, name, comment,
  active })`, `git_changelist_set_active({ id })`, `git_changelist_delete({ id })`,
  `git_changelist_move({ paths, hunks?, target, create? })`. Hunks:
  `git_file_hunks({ path, content? }) -> { id, oldStart, oldLen, newStart, newLen, removed,
  added }[]` (0-based lines; ids stay stable while other parts change). Commit:
  `git_commit_files({ message, files: { path, hunks }[], options: { amend, signOff, noVerify,
  author } }) -> sha`. Patches: `git_create_patch({ paths, reverse })`,
  `git_read_patch({ path })` / `git_write_patch({ path, patch })` (absolute = local file,
  else project file), `git_patch_preview({ patch, base }) -> { path, oldPath, status, binary,
  additions, deletions, before, after, applies, message }[]`, `git_apply_patch({ patch, base,
  paths? }) -> { applied, conflicts, failed: [path, reason][] }`. Browser mode:
  `mock/gitChangesMock.ts` with the Rust models as WebAssembly (`vcs_changelists`,
  `vcs_change_hunks`, `vcs_patch_preview`).
- Get from VCS, worktrees, Commit Message History and signing (`forge_vcs::{clone, worktree,
  message_history, signing}`, `repo/worktrees.rs`, `forge_backend::vcs`, `commands/git_clone.rs`,
  `commands/git_extras.rs`, UI `features/git/extras.ts`, `ExtrasDialogs.tsx`,
  `SigningSettings.tsx`): `forges_repositories({ accountId }) -> HostRepository[]`
  (`CodeHost::repositories`), `git_clone_directory_name({ url })`, `git_clone_home({
  connectionId? })`, `git_clone_test({ url, accountId?, connectionId? }) -> { sha, name }[]`
  (`git ls-remote --heads`), `git_clone({ url, directory, accountId?, connectionId? }) -> { path
  }` with `git://clone-progress` `{ phase, percent, fraction, text }` and `git_clone_cancel()`.
  Clones run through a `Backend` (`LocalBackend` at home, the project's backend or a
  `RemoteBackend` of a browsing connection, `remote::clone_target`) with
  `process_io::run_progress_lines` (lines also end at `\r`); HTTPS tokens of the matching
  account (`ForgeService::credential_for_url`) reach git only through `-c credential.helper=`
  plus an inline helper reading `FORGE_GIT_USERNAME` / `FORGE_GIT_PASSWORD`.
  `remote_project_connection() -> string | null`. Worktrees: `git_worktrees() -> Worktree[]`,
  `git_worktree_add({ options: { path, branch?, newBranch?, base?, detach? } })`,
  `git_worktree_remove({ path, force })`, `git_worktree_prune()`, `git_worktree_lock({ path,
  reason? })`, `git_worktree_unlock({ path })`; `BranchInfo.worktree` comes from
  `%(worktreepath)`. `git_message_history() -> string[]` (Forge's history in `<data
  dir>/forge/projects/<project>/commit-messages.json`, recorded after `git_commit` /
  `git_commit_files`, plus `git log --author=<git var GIT_AUTHOR_IDENT email>`).
  `git_signing_info() -> { config, gpgKeys, gpgError?, sshKeys, remote }`; commits read
  `vcs.git.commit.gpgSign` / `vcs.git.commit.signingKey` (`SigningKey::from_setting`) into
  `CommitOptions.signing` (`-c gpg.format=… -c user.signingkey=… commit -S`). `Commit.signature`
  (`%G?`) and `CommitDetails.{signature, signer, signingKey}` (`%G? %GS %GK`). Settings with
  `hidden = true` have no row; `registerObjectSettingEditor` can replace any row. The commit
  message textarea opts into tool window shortcuts with `data-focus-context="toolwindow-input"`
  (`FocusContext::ToolwindowInput`). Browser mode: `mock/gitExtrasMock.ts`.
- Git Log actions (`forge_vcs::rebase`, `repo/rewrite.rs`, `commands/git_log.rs`, UI
  `features/git/logActions.ts`, `logIndex.ts`, `LogDialogs.tsx`, `rebaseModel.ts`); commit lists
  are in log order (newest first): `git_log_actions({ shas }) -> { cherryPick, revert,
  undoCommit, editMessage, fixup, squashInto, squashCommits, dropCommits, interactiveRebase:
  { enabled, reason?, warning? } }` (protected branches from `vcs.git.protectedBranches`),
  `git_cherry_pick({ shas })` (`-x` per `vcs.git.cherryPickSuffix`), `git_revert_commits({ shas
  })`, `git_undo_commit({ sha }) -> { message }`, `git_reword({ sha, message })`,
  `git_squash_commits({ shas, message })`, `git_drop_commits({ shas })`,
  `git_interactive_rebase({ sha, entries: { sha, action: "pick" | "edit" | "reword" | "squash" |
  "fixup" | "drop", message? }[] /* oldest first */ })` -> `{ output, oldHead, newHead?,
  stopped }`, `git_undo_rewrite({ oldHead, newHead })`, `git_rebase_plan({ sha }) -> { base?,
  branch?, commits: { sha, subject, message, author, time }[], entries }` (autosquash applied),
  `git_commits_patch({ shas }) -> string`, `git_tree_files({ rev }) -> string[]`. Rewrites run
  `git rebase -i --autostash` with `-c sequence.editor="git cat-file blob <todo> >"` and `exec`
  amend steps reading message blobs, so no editor opens and remote projects work; stops
  (conflicts, `edit`) hand over to `git_operation_action` and the conflict resolver. Browser
  mode: `mock/gitLogMock.ts` (states, autosquash and the todo via WebAssembly:
  `vcs_log_action_states`, `vcs_rebase_autosquash`, `vcs_rebase_todo`).
- Merge conflicts (`forge_vcs::merge`, `Repo::conflicts`, `features/git/conflicts.ts`):
  `git_conflicts() -> { operation: "merge" | "rebase" | "cherryPick" | "revert" | "unstash",
  description, yoursName, theirsName, yoursTitle, theirsTitle, reversed, files: { path, yours,
  theirs: "modified" | "added" | "deleted" }[] }` (yours is git's `theirs` in a rebase);
  `git_merge_file({ path }) -> { path, yoursTitle, theirsTitle, base?, yours?, theirs?, binary,
  crlf, model?: { chunks: { kind: "left" | "right" | "both" | "conflict", base, left, right
  (0-based line ranges), baseText, leftText, rightText, simple? }[], changes, conflicts,
  simpleConflicts } }` (stage versions `:1:`/`:2:`/`:3:` with `\n` line breaks and their
  three-way merge; `simple` is the word-level merge of "Resolve Simple Conflicts");
  `git_accept_conflict_side({ paths, side: "yours" | "theirs" })` (checkout of that stage and
  `git add`, or `git rm` when that side deleted the file); `git_resolve_merge({ path, text,
  crlf })` writes the merged result through the backend and stages it. The merge window keeps
  the chunk positions and applied/ignored sides in a CodeMirror state field
  (`features/git/mergeState.ts`, undoable); its actions use the keymap's `diff` focus context
  (`[data-focus-context='diff']`: `NextDiff`/`PreviousDiff` on `F7`/`Shift+F7` there,
  `Diff.ApplyNonConflicts[.Left|.Right]`, `Diff.MagicResolveConflicts`). Browser mode runs the
  merge as WebAssembly (`vcs_merge`); merging `feature/checkout-flow` in the demo (or
  `?conflicts`) stops with conflicts.
- History and comparisons (`forge_vcs` `repo/history.rs`, `forge-diff`, `commands/git_history.rs`,
  `commands/compare.rs`): `git_file_history({ path, dir, query: { skip?, limit? } }) ->
  { sha, parents, author, email, time, subject, files: ChangedFile[] }[]` (files follow renames;
  for a folder every changed file below it), `git_block_history({ path, start, end }) ->
  { sha, parents, author, email, time, subject, path, before: string | null, after, startLine }[]`
  (`git log -L`, lines as in `HEAD`), `git_blame_revision({ rev, path }) -> Blame`,
  `git_changes_since({ rev, path }) -> ChangedFile[]` (rev against the working tree),
  `compare_dirs({ left, right }) -> { entries: { path, dir, depth, status: "equal" | "different" |
  "leftOnly" | "rightOnly", leftSize, rightSize, leftModified, rightModified }[], truncated }`
  (computed where the files are: `Backend::compare_dirs`, agent call `CompareDirs`) and
  `compare_copy({ from, to, dir })`. History tabs live in the Git tool window
  (`features/git/history.ts`); files of a revision open as read-only `gitrev://<sha>/<path>`
  tabs. `CompareTwoFiles` (`Ctrl+D`) runs the handler registered for the focused view with
  `registerCompareHandler(selector, run)` (`features/compare/state.ts`), else compares the
  Project view selection (`projectTree.selectedPaths()`, Ctrl+click adds rows); the Project
  view menu asks `providedEntries("project.compare", entry)` below Reload from Disk.
- Context-dependent menu entries: `registerContextMenuProvider(menuId, (context) => MenuEntry[])`
  (`features/contextMenu/state.ts`); the Project view asks `providedEntries("project", entry)`
  for its menu. `item(label, run, actionId, { disabled, shortcutAction })` shows greyed-out items.

- `ai_complete({ request: { system?: string, messages: { role: "user" | "assistant", content: string }[], provider?: string, model?: string, maxTokens?: number } }) -> string`
  — one-shot completion with the default (or given) provider. Used by e.g. AI commit messages.
- `ai_stream({ request, streamId })` emits `ai://stream/<streamId>` events `{ delta?: string, done?: boolean, error?: string }`.
- `fs://changed` `{ changed: string[], removed: string[], git: boolean }` — debounced file watcher
  batches (workspace-relative paths, ignored and excluded paths filtered out; `git` = index,
  HEAD or refs changed). Subscribe with `onFsChanged` (`src/state/fsWatch.ts`); the backend
  already updates the PHP index from it.
- `agent_respond_permission({ sessionId, requestId, decision })` answers an agent's
  `permissionRequest` event; `decision` is `{ behavior: "allow", scope?: "once" | "session",
  updatedInput? }` or `{ behavior: "deny", message? }`.
- Language servers of language packs are registered in Rust with `LspState::register_server`
  (PHP: `commands::php::sync_lsp_server`, run after project detection and on PHP settings
  changes); there are no built-in fallback servers.
  Subscribe before invoking; `ai_cancel({ streamId })` stops it (a final `{ done: true }` follows).
  Requests may also carry `temperature?`, `stop?: string[]` and `feature?: "chat" | "commitMessages" | "completion"`
  (picks the model configured for that feature). Use the typed client `src/ipc/ai.ts` (`aiApi`).
- Provider management (Settings | Tools | AI Providers): `ai_providers_list`, `ai_provider_save({ config, apiKey? })`,
  `ai_provider_delete({ id })`, `ai_provider_test({ id }) -> { ok, latencyMs, error?, modelCount? }`,
  `ai_models({ providerId })`, `ai_set_default({ providerId, model?, feature? })`. Config lives in
  `<config dir>/forge/ai.toml`; API keys in the OS keychain (service `forge-ai`) with
  `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `OPENROUTER_API_KEY` as fallback. Backend: `crates/forge-ai`.
- Agent CLIs (`forge-agent-cli`, AI Chat tool window): `agent_list({ executables? }) -> AgentInfo[]`
  (`{ id, name, models, installHint, permissionPrompts, permissionModes: { value, label, short, hint }[],
  supported, installed, path }`),
  `agent_start({ agentId, options: { model?, permissionMode?, resume?, allowedTools?, executable? } }) -> sessionId`,
  `agent_send({ sessionId, text, context?: { files, selection? }, turnId? })`, `agent_interrupt({ sessionId })`,
  `agent_stop({ sessionId })`. Events arrive on `agent://<sessionId>` as `AgentEvent` JSON
  (`{ type: "sessionStarted" | "assistantText" | "thinking" | "toolUse" | "toolResult" | "usage" |
  "permissionRequest" | "turnFinished" | "interrupted" | "error" | "exited", ... }`, see `app/ui/src/ipc/agent.ts`),
  each followed by `{ type: "chat", conversationId, model, cwd, turn?: { id, status, summary?, blocks:
  [index, block][], changedFiles, awaitingPermission, working }, changedFiles, turnEnded }`: the
  rendered conversation (`forge_agent_cli::conversation`: text blocks with their Markdown AST, tool
  cards with paths and edit diffs, permission summaries, duration/token/cost texts). The UI only
  applies these updates (`features/ai-chat/model.ts`).
  `exited` means only that a process ended: the session spawns (and resumes) a new one for the
  next message, so the UI keeps the session. See "Adding a CLI chat agent" below.
- Markdown (`forge-markdown`): `markdown_parse({ text, flavor?: "chat" | "docs" }) -> MdBlock[]` and
  `markdown_hover({ markdown }) -> { definition?, signature?, body, sections, deprecated? }` return a
  safe AST (no raw HTML) that the UI renders with Solid components (`features/lsp/MarkdownDoc.tsx`,
  `features/ai-chat/Markdown.tsx`); see `app/ui/src/ipc/markdown.ts`.
  `markdown_document({ text }) -> { blocks, lines: { start, end }[], frontMatter?: { format,
  entries: { key, value }[], endLine }, footnotes?: { label, index, blocks, line }[] }` is the
  Markdown preview (`Flavor::Preview`: GitHub-flavored tables, task lists, footnotes, autolinks,
  alerts, heading `id`s, `image` inlines; raw HTML sanitized into the AST); `lines[i]` are the
  0-based source lines of `blocks[i]` for synchronized scrolling.
  `markdown_anchor_line({ text, anchor }) -> number | null` finds a heading for links like
  `guide.md#install` (`features/markdown`).
- File viewers (`forge-viewers`, `src/ipc/viewers.ts`): `image_read({ path }) -> { info: {
  format, mime, width, height, size, bitDepth? }, base64 }` reads an image through the backend
  (remote projects work; images over 32 MB are refused) for the image editor and Markdown
  images. `csv_parse({ text, path?, delimiter? }) -> { rows, quoted, delimiter, eol,
  trailingNewline, columns }`; `csv_edit({ text, delimiter, edits }) -> string` applies
  `setCell` / `insertRow` / `removeRows` / `insertColumn` / `removeColumns` and writes untouched
  cells exactly as they were; `csv_sort_order({ rows, column, descending, headerRows }) ->
  number[]` is the display order of a sorted column. Browser mode runs the same code through
  `forge-wasm`.
- Editor navigation: `openFileAt(path, line?, column?, { flash?, focus? })` from
  `src/editor/views.ts` opens a workspace-relative file, puts the caret at the 1-based position,
  centers it and briefly flashes the line.
- TODO tool window (`src/ipc/workbench.ts`, `forge-find::todo`): `todo_scan({ request: {
  patterns?: { pattern, caseSensitive? }[], paths?, buffers?, limit? } }) -> { files: { path,
  items: { line, column, length, text, pattern }[] }[], count, truncated, error? }` — comments
  only, `.gitignore` respected, through `Backend::find_todos` (agent call `FindTodos`, protocol
  4). `paths` rescans single files (live updates); `buffers` are unsaved editor contents that
  replace the files on disk. Browser mode runs the same scanner as WebAssembly.
- Bookmarks (`src/ipc/workbench.ts`, `forge-bookmarks`): `bookmarks_get() -> Bookmarks` and
  `bookmarks_apply({ ops: BookmarkOp[] }) -> Bookmarks` (`toggle`, `setMnemonic`, `remove`,
  `clear`, `describe`, `moveToList`, `addList`, `renameList`, `removeList`, `setDefaultList`,
  `applyEdits` with line edits in old-document coordinates, `renamePath`, `removePath`). Stored
  in `<data dir>/forge/projects/<name>-<hash of the root>/bookmarks.json`, never in the
  project.

- Local History (`src/ipc/localHistory.ts`, `forge-localhistory`, see the next section):
  `local_history_list({ path, dir, filter? }) -> LhRevision[]`,
  `local_history_content({ path, rev }) -> { state, text? }` (`state` `current` means
  unchanged since the revision), `local_history_folder_changes({ path, rev })`,
  `local_history_selection({ path, text, start, end })`, `local_history_recent({ limit })`,
  `local_history_change_files({ from, to })`, `local_history_put_label({ name })`,
  `local_history_patch({ path, dir, revPath, rev }) -> string`,
  `local_history_revert({ target: { kind: "file" | "folder" | "change", … }, name })`,
  `local_history_record({ kind, name, files })`, `local_history_begin({ kind, name }) -> token`,
  `local_history_end({ token })`, `local_history_snapshot({ paths })`; event
  `local-history://changed` after new change sets are written.

## Local History (`forge-localhistory`)

PHPStorm's Local History: Forge records the contents of project files and restores them, with
or without Git, for local and remote projects.

- **Store.** One per project in `<data dir>/forge/projects/<name>-<hash>/local-history/`
  (never in the project): `changes.jsonl` (change sets: id, time, kind, name, entries with the
  path's state before and after) and `content/` (zlib-compressed contents, stored once per
  SHA-256 hash). A revision is the state *before* its change set; labels are change sets
  without entries. Retention: the last `localHistory.keepDays` working days (days with
  changes), then `localHistory.maxStorageMb`; run when a project opens and every six hours.
  `localHistory.maxFileSizeKb`, binary detection and `localHistory.excludedFolders` decide what
  is recorded.
- **Recorder** (`app/src-tauri/src/commands/local_history.rs`). `project_opened` starts a
  thread per project that receives `Msg`s over a channel, reads what it needs through the
  project's `Backend` (outside the store lock) and batches the writes, so recording never
  delays saving. Hooks: `fs_write_text` / `fs_write_file` (Saved; local projects read the old
  bytes first, remote ones use the last recorded state), `fs_read_text` (baseline: the
  content Forge saw, which also detects changes made while Forge was closed),
  `fs_create_file` / `fs_create_dir`, `fs_delete` (contents read before deleting, folders
  included), `fs_rename` (Rename / Move with `from`), `fs_copy`, `code_apply_edit`,
  `replace_in_files`, `ssr_replace`, `templates_create`, `code_create_test` and the file
  watcher (External change: a content that differs from the last recorded one; Forge's own
  writes match and are skipped).
- **Operations.** `local_history_begin(kind, name)` / `local_history_end(token)` bracket an
  operation; everything recorded meanwhile, and watcher batches up to three seconds after the
  end, becomes one change set with the operation's name. The UI uses `withLocalHistory`
  (`src/ipc/localHistory.ts`) for refactorings (`applyWorkspaceEdit`, which also records the
  open editors' before/after text with `local_history_record`) and VCS operations (Update /
  Pull, Checkout, Merge, Rebase, Reset, Rollback, Stash, Unstash, Shelve, Unshelve);
  Rollback, Shelve and Unshelve first call `local_history_snapshot` for the files they touch.
- **Queries and revert** are computed in Rust (`LocalHistory`); `local_history_revert`
  applies the plan through the backend and records it as a `Revert` change set.
- **UI** (`src/features/localHistory`): history tabs (file, folder, selection, a Recent
  Changes entry) as editor custom tabs reusing the Git history layout and `InlineDiff`; the
  Recent Changes popup (`RecentChanges`, `Alt+Shift+C`); actions `LocalHistory.ShowHistory`,
  `LocalHistory.ShowSelectionHistory`, `LocalHistory.PutLabel` in the File / VCS menus and the
  Project view and editor context menus.
- **Browser mode** runs the same crate as WebAssembly over an in-memory store
  (`local_history_call`); `mock/localHistoryMock.ts` wraps the file mocks and records a few
  days of demo history.

## Scopes, file colors, Structural Search and Analyze Stack Trace

- **Scopes** (`crates/forge-scopes`, pure): PHPStorm's scope language (`file:`, `src:`,
  `test:`, `lib:` path patterns with `*`, `?` and `//`, `&&`, `||`, `!`, parentheses),
  `Predefined` scopes, named scopes from the settings `scopes.shared` (project) and
  `scopes.local` (user), Include / Exclude editing, the preview tree and file colors. A
  search carries a `ScopeFilter { scope: { predefined } | { pattern }, context: { testRoots,
  openFiles, changedFiles, currentFile } }`: `FindRequest.scope` (Find in Files, the Find
  window and Replace in Files; applied in `Finder::wants`, so the index, the walk and the agent
  honor it), `SsrRequest.scope` and, for Find Usages, `UsageScope { kind: custom, pattern,
  scopeName, testRoots }` (a pattern with `:` is the scope language, otherwise globs). The UI
  (`features/scopes/model.ts`) resolves a named scope to its pattern and calls the crate
  synchronously through WebAssembly in both modes (`scopes_entries`, `scopes_check`,
  `scopes_edit`, `scopes_tree`, `scopes_contains`, `file_colors_resolve`). IPC:
  `scopes_files({ limit? }) -> string[]` (`Backend::list_files`, agent call `ListFiles`) and
  `scopes_test_roots() -> string[]` (`composer.json` `autoload-dev`).
- **Project structure** (Settings | Directories, Project view "Mark Directory as"):
  `forge_scopes::structure` computes the excluded paths from the project settings
  `projectStructure.folders` (`{ path, kind: excluded | resource | testResource | included }`,
  `included` cancels a default exclusion) and `projectStructure.excludePatterns` (`;`-separated
  name globs). `node_modules` and tool folders are excluded by default, `vendor` is a library,
  `.git` is hidden. `forge_core::Excludes` is the shared handle: `Workspace::list_dir` flags
  `FileEntry.excluded`, every walk (Go to File, Find in Files, TODO, `collect_files`,
  `list_files`), the watcher (`ProjectWatcher::start_with_excludes`), the search index
  (`SearchService::with_excludes`) and the PHP index (`IndexConfig::structure`) skip excluded
  paths. `Backend::set_structure(config) -> bool` (agent call `SetStructure`, protocol 11)
  sets them; the app calls it when a project opens and when the settings change
  (`commands/project_structure.rs`), and emits `project-structure://changed` when they changed
  (the UI re-reads the tree and reindexes PHP; the search index refreshes itself).
  `php_inspect` and local history skip excluded files too.
- **Project view rows**: `project_tree_list({ path, options: ViewOptions }) -> TreeRow[]`
  (`forge_scopes::view::arrange` over `Backend::list_dir`): `TreeRow { name, path, kind,
  ignored?, excluded?, compacted?: string[], nested?: TreeRow[] }`, `ViewOptions {
  showExcluded, foldersOnTop, sortByType, compactMiddlePackages, fileNesting, nestingRules }`
  (app settings `projectView.*`). Show Members is UI-only and reuses the Structure view
  providers (`loadFileStructure`, `caretNodeId`). The UI calls `project_structure_state`,
  `project_structure_mark`, `project_structure_describe` and `project_view_nesting_defaults`
  through WebAssembly in both modes; browser mode answers `project_tree_list` with
  `project_view_arrange` and flags excluded demo files with `project_structure_excluded`
  (`mock/projectStructureMock.ts`).
- **File colors**: `fileColors.enabled`, `useInEditorTabs`, `useInProjectView`,
  `fileColors.local` (default Non-Project Files: Yellow, Tests: Green) and
  `fileColors.shared`; `forge_scopes::colors::resolve` picks the first row whose scope
  contains a file or a whole folder. The UI sets `--file-color` on editor tabs and Project
  view rows (`tabColor`, `projectViewColor`) from the palette variables `--file-color-*` of
  `theme.css`.
- **Structural Search and Replace** (`crates/forge-ssr`, tree-sitter PHP, JavaScript,
  TypeScript/TSX behind the `engine` feature): `ssr_templates() -> SavedTemplate[]`,
  `ssr_check({ request }) -> { variables, unknownInReplacement?, error? }`,
  `ssr_search({ request: SsrRequest }) -> SsrResult` (`Backend::structural_search`, agent call
  `StructuralSearch`, so remote projects are parsed on their host) and
  `ssr_replace({ request }) -> ReplaceDone` (`forge_backend::ssr::replace_all`; the undo goes
  to the same state as Replace in Files, `undo_replace`). `SsrRequest { template, fileType,
  constraints: { name, minCount, maxCount|null, text, invertText, type, invertType }[],
  matchCase, replacement?, scope?, buffers?, limit }`; matches carry line/column (UTF-16),
  byte offsets, the listed line with ranges, bindings and the `replacement`. The UI
  (`features/ssr`) shows them as a Find tool window tab (`openInFindWindow` with `actions`:
  Replace All; `UsageRow.after` for Preview Replacement). Browser mode cannot build the C
  grammars to WebAssembly: `forge-wasm` has the library and `variables`, the mock approximates
  matches with a regular expression.
- **Analyze Stack Trace** (`forge_output::stacktrace`, pure): `stacktrace_analyze({ text }) ->
  { lines: ConsoleLine[], kind, frames, resolved }`. The command maps paths with the PHP
  servers' mappings (`.forge/php-servers.json`), the bind mounts of the root Compose file and
  the backend's root, and checks candidates with `Backend::list_dir` (cached per folder); the
  UI opens the lines in a Run tool window tab without a process (`openOutputTab`, Rerun
  reopens the dialog). Browser mode runs the analyzer as WebAssembly over the demo files.

## Workbench: editor groups, Find, TODO and Notifications

- Split editors: `state/editor.ts` keeps one document per file (`editor.tabs`) shared by every
  editor group; `state/tabGroups.ts` is the tab layout as plain data (groups, active and
  recently used tabs, pinned tabs, tab limit and closing policy from `editor.tabs.*`). Each
  group renders its own CodeMirror view; `editor/docSync.ts` forwards the changes of one view
  to the other views of the same file as synced transactions (one undo history per view).
  `features/editorTabs` registers the tab context menu, Window | Editor Tabs actions and
  Recent Locations (`Ctrl+Shift+E`, places shifted through edits).
- Find tool window (`features/codeintel`): tabs of usages or Find in Files matches
  (`openInFindWindow(answer, { kind: "usages" | "text", rerun? })`), Next/Previous Occurrence.
- Notifications: `showBalloon` (`keymap/dispatch.ts`) logs every balloon in
  `state/notifications.ts` (group, type, time, actions, read state; at most 200) and asks the
  group's display mode (`notifications.balloons`, `notifications.groups`: balloon, sticky or
  log only). `balloonsFor(group)` gives a feature its own group; git balloons use "Version
  Control". `features/notifications` renders the tool window.

## Startup splash and updates (`forge-update`)

- **Windows.** `tauri.conf.json` declares two windows with `create: false`: `splash`
  (640×400, undecorated, transparent, centered, `skipTaskbar`, loads `index.html?splash`) and
  `main`. `startup::begin` (called from `setup`) opens the splash unless `appearance.showSplash`
  is off and runs the loading steps on the `forge-startup` thread: keymap, plugin listeners and
  servers, the CLI / `forge://` / last project (`system.reopenLastProject`) with framework
  detection, language server registration, file watcher, data sources, Docker, the update
  check. Then it creates `main` hidden; the UI calls `startup_workbench_ready` after its first
  render, Rust shows `main` and closes the splash (after 20 s it shows `main` anyway). Closing
  the splash early quits Forge. Open requests (second launch, `forge://` links) are queued
  until the main UI calls `companion_ui_ready`, as before.
- **Progress.** Each step emits `splash://progress { step, detail, percent }` *before* its
  work; the splash eases the wordmark's molten fill (`--p`) towards `percent`.
  `splash_state() -> { progress, version, theme, offer? }` gives a freshly loaded splash the
  current state. The splash (`src/splash/`) imports no features.
- **Updates.** `forge-update` decides (`plan`: simulated, dev build, no public key, manual,
  `updates.checkOnStartup`, once a day; `evaluate`: newer, channel, skipped, postponed) and
  reads the feed (`check`, tested against a local HTTP server). The shell
  (`commands/updates.rs`) downloads, verifies and installs with `tauri-plugin-updater` (public
  key from `FORGE_UPDATER_PUBKEY` at build time) and restarts. An offer during startup is sent as
  `splash://update` (`UpdateOffer { version, currentVersion, notes?, size?, summary, simulated }`)
  and the startup thread waits for `updates_respond({ action: "download" | "later" | "skip" })`.
  Download progress goes to every window as `updates://download { version, phase, downloaded,
  total?, bytesPerSecond, message? }` with `phase` = `downloading | verifying | verified |
  installing | restarting | simulated | failed`. Other commands: `updates_check() ->
  CheckResult` (`available | upToDate | notChecked | error`, manual: ignores the daily limit
  and skipped versions), `updates_status() -> { mode, currentVersion, endpoint, lastCheck?,
  skippedVersion? }`, `updates_clear_skipped()`. State: `<config dir>/forge/updates.json`.
- **Simulation.** `FORGE_SIMULATE_UPDATE=<version>` (desktop) and `?simulateUpdate=<version>`
  (browser mode, `mock/updatesMock.ts`) offer a fake update; nothing is downloaded or installed.

## Window chrome and dialog windows (`forge-window`)

- **Title bars.** All windows are undecorated (`decorations: false`); Forge draws the title
  bar itself. The main toolbar *is* the title bar (PHPStorm New UI): empty areas drag the
  window (`dragRegion` in `src/shell/window/chrome.ts`, a double click maximizes), the
  controls sit at its ends, and `ResizeEdges` adds resize handles. The Welcome screen gets a
  plain `WindowTitleBar`.
- **Styles.** Setting `appearance.windowControls` = `auto | windows | macos | gnome | kde |
  linux | native`. `forge_window::detect_desktop` reads the target OS and
  `XDG_CURRENT_DESKTOP` / `XDG_SESSION_DESKTOP` / `DESKTOP_SESSION` / `KDE_FULL_SESSION`;
  `forge_window::resolve` picks the style and button layout (GNOME's `button-layout` from
  `gsettings`, KDE's `ButtonsOnLeft/Right` from `kwinrc`, read once by the shell). `native`
  switches every window back to OS decorations (`commands::window::apply_decorations`, also on
  setting changes). IPC: `window_chrome({ style? }) -> WindowChrome` (`{ os, desktop, setting,
  style, native, buttons, dialogButtons, titleAlign }`); `style` forces a style
  (`?windowControls=gnome` in browser mode, where "auto" means the browser's own frame).
  Colors are the `--wc-*` tokens in `theme.css`.
- **Dialog windows.** Large modal dialogs (Settings, Run/Debug Configurations, New Project,
  New SSH Connection, Data Sources and Drivers) are registered with
  `registerDialog({ id, title, width, height, component, prepare?, onClosed?, onMessage? })`
  and opened with `openDialogWindow(id, props) -> Promise<result | null>`
  (`src/shell/window/dialogs.ts`). In the desktop app `dialog_window_open({ request })` creates
  the window `dialog-<id>` owned by the calling window (centered on it, or at the geometry
  remembered in `<config dir>/forge/dialog-windows.json`, see `forge_window::place_dialog`),
  disables the owner and refocuses the dialog when the owner gets focus. The window loads the
  same bundle with `?dialog=<id>` (props injected as `window.__FORGE_DIALOG__`) and renders only
  `DialogWindowApp`. The dialog ends with `closeDialog(id, result)` (`dialog_window_close`);
  the owner receives `dialog://closed { window, id, result }` and runs `onClosed` there.
  `notifyDialogOpener(id, msg)` (`dialog_window_message`, event `dialog://message`) tells the
  owner about Apply. Alt+F4 sends `dialog://close-requested`, which runs the dialog's Cancel
  path (`onDialogCancel`). Data lives in the backend, so settings arrive everywhere through
  `settings://changed`; `onClosed` / `onMessage` reload what the owner shows (plugins, run
  configurations, data sources, the opened project). Code that starts background work at
  startup checks `isDialogWindow()` (`src/shell/window/mode.ts`). In browser mode dialogs open
  in-page (`DialogHost`) inside the same `WindowFrame`. Small popups stay in-page everywhere.
  Window labels `dialog-*` are listed in `capabilities/default.json`.

## Project backends and remote development (`forge-backend`, `forge-remote`, `forge-remote-agent`)

- Every project command goes through the open project's `forge_backend::Backend`
  (`AppState::backend()`): `list_dir`, `read_file`, `write_file`, `find_files`, `git()` (a
  `forge_vcs::GitExec`; `Backend::repo()` = `Repo::open_with`), `detect_project`, `watch`,
  `open_terminal`, `start_process`, `lsp_launcher()` (a `forge_lsp::ServerLauncher`; the app's
  `LspManager` is created with `LspManager::with_launcher`), `php()` (the PHP index and the
  Intelephense / phpactor lookup, `forge_backend::PhpBackend`), `read_optional` (a file or
  `None`, one round trip), `framework(FrameworkCall)` (Laravel / Symfony / Doctrine analysis,
  see below), `langservers(LangServersRequest)` (catalog server planning), `find_executable`
  (agent CLIs and tools on the project's machine) and `tunnels()` (SSH port forwarding,
  `None` for local projects). Methods block; commands call them on blocking threads
  (`commands::workspace::blocking`).
- `LocalBackend` serves a folder on this machine (the desktop app's own projects).
  `RemoteBackend` serves a folder on another machine through the **Forge agent**
  (`crates/forge-remote-agent`): a headless `LocalBackend` behind the agent protocol on
  stdin/stdout. Git output is parsed locally (`forge-vcs` only runs `git` remotely), language
  servers run on the host with their stdio proxied over a byte channel (Intelephense indexes
  the remote files itself; its config is resolved on the host), the PHP index, file watcher,
  terminals and processes live in the agent.
- Remote projects have the features of local ones; whatever reads many project files runs
  next to them:
  - **Frameworks** (`forge_backend::frameworks`): `LocalFrameworks` scans and caches Laravel
    (fingerprint recheck every 2 s), Symfony and Doctrine (10 s, keyed by their settings)
    where the files are: in the app for local projects, in the agent for remote ones
    (`Call::Framework`). Editor requests carry the buffer, so completion, Go to Declaration,
    gutter marks and DQL highlighting cost one round trip; `framework_call::<T>` decodes the
    typed reply. Artisan and `bin/console` run through `Backend::start_process`
    (`process_io::run_lines`: lines, stderr flag, cancel), so they run on the host.
  - **Catalog language servers** (`forge_backend::langservers`): the project scan, server
    detection (`node_modules/.bin`, PATH, npm/nvm/volta, …) and config building run on the
    project's machine (`Call::LangServers` with the `langservers.*` settings, disabled
    plugins and the running servers' states); the servers then start there through the LSP
    byte channel. **Installs** run where the servers run: `Backend::install_langserver`
    (`forge_backend::langservers::install`: `forge_langservers::managed` downloads Node.js and
    npm packages with integrity checks into the host's Forge directories) is
    `Call::LangServerInstall { channel, request }` in the agent, whose steps, output and
    download progress stream back as `Event::Install { channel, event }`; closing the channel
    cancels it. `Call::LangServerUpdates` checks the host's installed packages. The Intelephense
    licence is still accepted in the UI before the call.
  - **WebForge** (`forge_backend::web`): `LocalWeb` loads and caches (5 s) the project's script,
    template and manifest files next to them and answers `Call::Web(WebCall)`: npm packages
    with scripts and package manager detection, React / Vue / Angular components, Go to
    Declaration and gutter marks (the editor buffer travels with the request). npm scripts run
    as run configurations through `Backend::start_process`.
  - **Language indexes**: `Call::CollectFiles(CollectRequest)` returns the text of every
    project file with some extensions in one reply (gitignore-aware, limits on count, file size
    and total bytes); the Lua index is built from it. StyLua runs through
    `process_io::capture` with `Backend::find_executable`, so on the host.
  - **Services** (`forge_containers::project::ProjectHost`, the app's `BackendProject`): compose
    files and `.env` are read through the backend and `docker compose` / `docker build` run
    with `Backend::start_process` on the host. With `docker.remoteProjectHost` the hub uses
    `Endpoint::ProjectHost`: the Docker Engine API over `Tunnels::open_socket`
    (`direct-streamlocal` to the host's socket on the project's SSH session), and exec
    terminals carry `TerminalCommand.on_host`; otherwise they open with `local: true` on this
    machine.
  - **HTTP client**: environment and `< file` bodies are read through the backend; with
    `httpClient.sendFrom = remote` (default) each request goes through
    `Tunnels::forward_local` (a `direct-tcpip` channel per connection) to the URL's host and
    port as the host sees them (`forge_scratches::client::send_via`: the original host stays
    the `Host` header and TLS name).
  - **Code hosts** detect the repository with git through the backend; API calls stay local.
  - **Project settings and `.forge/` files** go through `forge_core::ProjectFiles`
    (`LocalFiles`, `forge_backend::BackendFiles`): `.forge/settings.toml`
    (`Settings::load_project_files`), `.forge/dataSources.toml` and `.env` (database tools),
    `.forge/breakpoints.json` and `.forge/php-servers.json` (debugger).
  - **Databases**: project data sources of a remote project connect through an SSH
    `direct-tcpip` tunnel to their host and port as the remote host sees them
    (`forge_db::DbTunnel`, `DbService::set_project_context`; `DataSourceInfo.via` names the
    host); SQLite files on the host are refused; global data sources connect directly.
  - **Xdebug**: "Start Listening" listens on a free local port and asks the host to listen
    on `127.0.0.1:<php.debug.port>` (`tcpip-forward`, requested again after reconnects;
    `DebugServer::listen_with`). Xdebug's paths are the host's, which are the project's, so
    no path mappings are needed. `debug_status` reports `via`.
  - **AI agent CLIs** run on the host: `forge_agent_cli::AgentHost` (the app's
    `BackendHost`) finds them with `Backend::find_executable` and starts them with
    `process_io::spawn_stdio` (async stdin/stdout/stderr over the process API;
    `Call::ProcessCloseStdin` ends the input of per-turn CLIs).
  - The **Browser Companion**'s project profile and `.env` edits use the backend; files the
    browser reports are mapped to the remote project (`forge_open::resolve_remote`: paths below
    the host root, relative paths, or the longest existing tail of another machine's or a
    container's path, checked with `list_dir`), and symbol lookups use `find_files` /
    `read_file` (`forge_open::symbols::SymbolSource`). `AppState::with_workspace` and its
    "not available in remote projects yet" error are gone; `local_workspace()` only answers
    questions about this machine.
- Protocol (`forge_backend::protocol`, version `PROTOCOL_VERSION`): frames are a big-endian
  `u32` length + a MessagePack map (`rmp_serde::to_vec_named`). `Request { id, call }` /
  `Response { id, result }` (answers in any order; the result is the MessagePack encoding of
  the call's reply type; protocol 2 added `ReadOptional`, `ProcessCloseStdin`,
  `FindExecutable`, `Framework` and `LangServers`; protocol 3 added `FindInFiles` and the PHP
  navigation calls `Usages`, `Navigate`, `Definition`, `LineMarkers` and `Intentions`;
  protocol 4 added `FindTodos`, `CollectFiles`, `Web`, `LangServerInstall` and
  `LangServerUpdates`; protocol 5 added `FindStart`, `FindPage`, `FindSymbols` and
  `SearchStatus`, and `exact` on the search calls; protocol 6 added `ReadBytes` / `WriteBytes`
  (files in other encodings than UTF-8) and `TerminalSpec.shell_integration`; protocol 7 added
  the PHP index's `Hierarchy` call), `Event`

  (files in other encodings than UTF-8) and `TerminalSpec.shell_integration`; protocol 7
  added `PhpCall::Highlights` and the Find Usages `options` of `PhpCall::Usages`; protocol 8
  added `ListFiles`, `StructuralSearch` and the `scope` of `FindRequest`), `Event`
  (`FsChanged`, `Pty`, `Run`, `PhpIndexProgress`, `Install`), `Data` /
  `Close { channel }` (language server stdio, terminal and process input; channel ids are chosen
  by the client). The connection starts with `Call::Hello`. `forge_backend::rpc::RpcClient`
  is the client (async `call`, `call_blocking`, events, channels); `forge_remote_agent::serve`
  the server. Tests drive both through in-memory pipes.
- SSH (`crates/forge-remote`, russh with ring): `session` (password, key file with passphrase,
  ssh-agent plus `IdentityFile`s; `known_hosts` check: unknown or changed keys fail with
  `ConnectError::HostKey` and are trusted after the user confirmed the fingerprint, which
  updates `~/.ssh/known_hosts`; `Route::Jump` connects through `ProxyJump` hosts, each hop
  tunnelled through the previous one with a `direct-tcpip` channel and asked about its own
  host key, `Route::Command` runs a `ProxyCommand` with `%h %p %r %n` expanded; port forwards
  `open_direct_tcpip` / `forward_remote`), `agent` (probe with one shell command, upload the bundled
  agent through `cat` into `~/.cache/forge-agent/<version>/` when missing or different by
  SHA-256, start it in a login shell; output before the `FORGE-AGENT-READY` line is skipped),
  `connection` (pings every 5 s for the latency, reconnects with backoff 1–30 s after a
  dropped connection and calls `RemoteBackend::reattach`; host key and authentication errors
  stop retrying; `forward_local` / `forward_remote` and the `forge_backend::Tunnels` it hands
  to `RemoteBackend`), `store` (saved configurations in `<config dir>/forge/ssh.toml`, passwords
  and passphrases in the OS keychain, service `forge-ssh`, account = configuration id;
  `~/.ssh/config` with `Include`). The pure modules (`ssh_config`, `known_hosts`, `deploy`,
  `config`) build without the `client` feature and back the browser mocks through
  `crates/forge-wasm/src/remote.rs`.
- Agent binaries: `pnpm --dir app/ui agents` builds static musl agents into
  `target/agents/forge-remote-agent-<arch>-linux.gz` (x86_64, aarch64; compressed because
  AppImage tools patch ELF files in the bundle), and `pnpm release` ships them as the bundle's
  `agents/` resources. Plain or `.gz` binaries are accepted; Forge looks in
  `FORGE_REMOTE_AGENT_DIR`, the bundle's `agents/` resources, `agents/` next to its executable
  and `target/agents`; a development build of `forge-remote-agent` next to the executable serves
  hosts of the same architecture. Remote hosts must run Linux.
- IPC (`app/ui/src/ipc/remote.ts`): `remote_ssh_configs() -> SshConfigView[]`,
  `remote_ssh_config_save({ config, secret? })` (an empty `secret` deletes the stored one),
  `remote_ssh_config_delete({ id })`, `remote_validate_config({ config }) -> string[]`,
  `remote_ssh_hosts() -> HostEntry[]`, `remote_test_connection({ config, secret?,
  acceptFingerprint? }) -> { ok, latencyMs?, system?, error?: ConnectError }`,
  `remote_connect({ config, secret?, acceptFingerprint? }) -> { status: "connected",
  connectionId, label, home, hostname, system } | { status: "failed", error }` (progress on
  `remote://progress` `{ message }`), `remote_browse({ connectionId, path }) -> { path, parent,
  entries: { name, path, dir, project }[] }`, `remote_open_project({ connectionId, path }) ->
  WorkspaceInfo`, `remote_disconnect({ connectionId })`, `remote_status()`, `remote_reconnect()`.
  `remote://status` carries `{ connectionId, label, host, state: "connecting" | "connected" |
  "reconnecting" | "disconnected", latencyMs?, attempt?, error?, nextRetrySecs? }`.
  `ConnectError` is `{ kind: "hostKey", hostKey: { host, port, keyType, fingerprint, status:
  "unknown" | "changed" | "revoked", line?, oldFingerprint? } } | { kind: "auth" | "network" |
  "other", message }`.
- A remote project's `WorkspaceInfo.root` is `user@host:/path` (it identifies the project in
  the UI) and `remote` holds `{ host, port, user, label, configId?, path }`; Recent Projects keep
  remote entries (with an SSH badge on the Welcome screen). Browser mode: `?project=remote`
  opens `deploy@shop.example.com:/var/www/shop` (the demo project), `?welcome=remote` shows
  Welcome > Remote Development.

## Opening files from outside (`forge-open`, Browser Companion)

- Entry points: the browser extension (`browser-extension/`) via the local HTTP API, `forge://`
  links (`tauri-plugin-deep-link`, scheme registered in `tauri.conf.json`) and the launcher
  (`forge path/to/file:12[:3]`, `forge --line 12 file`, `forge file 12 3`). A second launch is
  forwarded to the running window by `tauri-plugin-single-instance`.
- Links: `forge://open?file=<abs or project-relative>&line=&column=` (aliases `url=file://…`, `l`,
  `col`) and `forge://open-symbol?name=UserCardComponent&kind=component`.
- HTTP API on `127.0.0.1:63342` (next nine ports if taken; settings `browserCompanion.*`):
  `GET /api/forge/ping`, `POST /api/forge/pair { code, name }`, `POST /api/forge/open
  { file, line?, column?, project?, symbol? }`, `POST /api/forge/open-symbol { name, kind? }`.
  Requests need a loopback `Host`, no web-page `Origin` (extension origins only) and, except
  ping/pair, `Authorization: Bearer <token>`. Tokens come from one-time 6-digit pairing codes;
  only their SHA-256 hashes are stored (`<config dir>/forge/browser-companion.json`).
- Every request is resolved against the open and recent projects into `companion://open`
  `{ source: "browser" | "link" | "cli", client?, file, line?, column?, target }` with
  `target.kind` `open` (shown right away), `recent` or `outside` (the UI asks first unless the
  source is the CLI). Requests that arrive before the UI is ready are returned by
  `companion_ui_ready()`. Directories from a second launch arrive as `companion://open-project`.
- IPC (Settings | Tools | Browser Companion): `companion_status`, `companion_start_pairing`,
  `companion_cancel_pairing`, `companion_revoke_client({ id })`, `companion_open_link({ url })`,
  `companion_project_profile({ envKeys, files })`, `companion_set_env_var({ file: ".env" |
  ".env.local", key, value })`; `companion://status` reports server restarts.

## Project types and New Project (`forge-project`)

- Detection reads markers in the project root and one level below (`composer.json`/`composer.lock`,
  `artisan`, `bin/console`, `wp-config.php`, `package.json`/lock files/`node_modules`, `angular.json`,
  `tsconfig.json`, `*.lua`, `.luarc.json`, `*.rockspec`, `conf.lua`, `init.lua`, `Cargo.toml`, `go.mod`,
  `pyproject.toml`, `Dockerfile`/`compose.yaml`, `.git`).
- `project_detect() -> ProjectInfo { languages, frameworks: { id, name, version?, language, dir? }[],
  packs, tools, primary? }`; `workspace_open` also emits `project://detected` `{ root, info }`, and
  `workspace_recent` returns each project's `primary` for the Welcome screen badge.
- Plugins opt in with `projectTypes = ["laravel"]` in their manifest (language, framework, pack or
  tool ids); without it they are always active. In the UI use `projectInfo()`, `usesFramework("laravel")`
  and `isPluginActive("forge.laravel")` from `src/state/project.ts`.
- New Project wizard (command `NewProject`): `project_generators_list() -> Generator[]`,
  `project_plan({ generator, location, name, options }) -> { target, preview, files?, missingTools,
  fallback?, error? }`, `project_create({ generator, location, name, options }) -> root` streaming
  `project://create` events (`{ type: "step" | "output" | "done" | "failed", ... }`),
  `project_create_cancel()`. Generators run official CLIs (composer, symfony, npm, npx) or write
  offline templates embedded from `crates/forge-project/templates/`. The browser copy of the generator
  list is `app/ui/src/mock/projectGenerators.json` (`FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`).
- The default location is the setting `system.defaultProjectDirectory` (`~/Projects`).

## Terminal and Run/Debug (`forge-process`)

- Terminal: `terminal_open({ options: { id?, cwd?, shell?, env?, cols, rows } }) -> { id, shell, cwd }`,
  `terminal_write({ id, data })`, `terminal_resize({ id, cols, rows })`, `terminal_close({ id })`.
  Output arrives on `terminal://<id>` as `{ type: "output", data } | { type: "exited", code }`; pass
  your own `id` to subscribe before the shell starts. Defaults: `terminal.shellPath` or `$SHELL` /
  pwsh / `%ComSpec%`, start directory `terminal.startDirectory` or the project root, extra
  variables from `terminal.environment`. Every session (local or on a remote agent) gets
  `forge_process::termenv`'s environment (`TERM_PROGRAM=Forge`, `TERM_PROGRAM_VERSION`, a UTF-8
  `LANG` fallback, other terminals' variables removed); `close` sends SIGHUP and SIGKILL after
  a grace period. With `terminal.shellIntegration` bash and zsh start with Forge's scripts
  (`forge_process::integration`, written to the cache dir of the machine the shell runs on),
  which report OSC 7 (working directory) and OSC 133 (prompt, command start, exit code).
- Terminal sequences: OSC 9;4 progress, OSC 9 / 777 notifications, OSC 7 working directories,
  OSC 133 boundaries and OSC 8 link targets are parsed by `forge_output::osc`, which the UI calls
  synchronously through WebAssembly (`terminal_parse_osc`, `terminal_link_target`) from
  xterm.js's OSC handlers. The `terminal.progress` status item and the stripe badge render it.
- Configurations: `run_configurations_load() -> { configurations, selected?, templates? }` and
  `run_configurations_save({ configurations })`. Shared ones ("Store as project file") are written
  to `<project>/.forge/runConfigurations.toml`, local ones, the selection and templates to
  `.forge/runConfigurations.local.toml`. A configuration is `{ name, type, storeAsProjectFile?,
  workingDirectory?, env?, interpreter?, interpreterOptions?, arguments?, options?: Record<string,
  string>, beforeLaunch?: { type: "runConfiguration" | "shell", value }[] }`.
- Runs: `run_start({ request: { runId?, configuration? | command?, env?, interpreterArgs?, testRunner? } }) ->
  { runId, commandLine }` streams `run://<runId>`: `{ type: "started", pid, commandLine }`,
  `{ type: "output", stream: "stdout" | "stderr", text }`, `{ type: "exited", code, signal }` and
  what the UI renders, computed by `forge-output` (re-exported as `forge_process::output`):
  `{ type: "console", replaceLast, lines: { segments: { text, style, link? }[] }[], dropFront? }`
  (`dropFront`: the oldest lines leave the console cycle buffer, `run.console.cycleBufferKb`; ANSI styles,
  `\r` progress lines, file links resolved into the project and URLs) and, with `testRunner`,
  `{ type: "tests", nodes, summary }` (test tree node updates from TeamCity service messages,
  parents first, plus the status line). `output_file_links({ text })` finds file links in a
  terminal line.
  `run_stop({ runId })` (SIGTERM to the process group, SIGKILL after 2 s; `taskkill /T` on
  Windows), `run_write({ runId, data })` (stdin), `run_dispose({ runId })`,
  `run_command_line({ request })` (preview). `RunRequest.outputFilters` adds External Tools
  output filters to the console's links.
- Configuration types: the backend's `KindRegistry` builds command lines for `php-script`, `php-builtin-server`,
  `phpunit` (always `--teamcity`), `artisan`, `composer`, `shell` and `npm` (npm, pnpm, yarn or
  bun; `--` before script arguments only for npm); the PHP interpreter
  defaults to the setting `php.interpreter`. `run_start` / `run_command_line` then wrap PHP
  kinds (and Composer kinds that name an interpreter) with the resolved CLI interpreter
  (see "PHP CLI interpreters"); `RunOutputOptions.pathMappings` (remote → local) make
  container paths in the output clickable. A manifest `runConfigurationTypes` entry can declare
  `fields` and a `command` template (`["${interpreter}", "${file}", "${arguments}"]`, see
  `lua-script`) and works without code; `interpreterSetting` names the setting with the project's
  default interpreter (`lua.interpreter`). In the UI, `registerRunConfigurationType({ id, name, icon,
  fields, interpreter?, argumentsLabel?, testRunner?, buildCommand?, start?, afterStart? })` adds
  the form to the Run/Debug Configurations dialog; `buildCommand` builds the command in the UI
  instead, `start` starts runs through another command that streams `run://` events (HTTP
  requests, PHP Web Page) and `afterStart(cfg, handle, debug)` runs once a run started (the
  built-in web server opens the browser). `enum` field `options` may be a function (servers).
- Launch plans, Run Anything and External Tools (`crates/forge-runtools`, pure, also in
  `forge-wasm`; IPC in `commands/runtools.rs`): `run_launch_plan({ configurations, target })
  -> { runs: { configuration, tasks }[], compound, allowMultipleInstances }` expands compound
  configurations (type `compound`, members in `options.configurations`, one per line;
  `options.allowMultipleInstances`) and flattens "Before launch" tasks (`runConfiguration`,
  `externalTool` (`Group/Name`), `browser`, `npm`, `composer`, `artisan`, `shell`) in order;
  missing configurations and cycles are errors. The UI (`features/run/runStore.ts`) runs the
  tasks one after another, aborts on the first failure and starts each member in its own tab;
  kinds it does not run itself come from `registerBeforeLaunchHandler`. Run Anything:
  `run_anything_suggest({ request: { query, recent, configurations, providers, laravel } }) ->
  { groups: { id, title, items: { label, detail?, icon, typeId?, matches, action }[], more }[],
  help }` (the backend adds npm and Composer scripts and PHP files; `action` is `configuration`,
  `command` or `insert`), `run_anything_resolve({ command, cwd? }) -> RunConfiguration` (a
  temporary configuration of the matching type), `run_anything_history()` /
  `run_anything_history_add({ entry })` (state dir, `forge_core::project_state::load_list`).
  External Tools live in the user setting `tools.externalTools` (`objects`):
  `external_tool_macros({ context }) -> { name, description, preview? }[]`,
  `external_tool_command({ tool, context }) -> { program, args, cwd, outputFilters,
  commandLine, missing? }` (context: project-relative `filePath`, caret, selection, clipboard,
  `$Prompt$` answers) and `external_tool_validate({ tool, others }) -> string[]`. A run's
  `RunRequest.outputFilters` (`$FILE_PATH$:$LINE$`, `forge_output::filters`) link file
  references in the console.
- Debugging: Debug (Shift+F9) calls the first `registerDebugLauncher({ id, canDebug(cfg, type),
  launch(cfg, ctx) })` that accepts the configuration; `ctx.run({ env, interpreterArgs })` starts it
  in the Run tool window (e.g. with `XDEBUG_SESSION` and `-dxdebug.mode=debug`) and returns
  `{ started, exited }`.
- Test runners: the backend turns TeamCity service messages into the test tree (`forge-output`);
  PHPUnit test classes and methods get gutter run icons that create temporary configurations.

## Language servers (`forge-lsp`)

- Packs contribute `LspServerConfig { id, name?, languageIds, fileExtensions, command, args,
  initializationOptions, settings }`; Rust code calls `LspState::register_server`, the UI or a
  plugin calls `lsp_register_server({ config })`. Servers start lazily for the first matching file
  and are routed by extension: every server sharing the longest match serves the file (TypeScript +
  ESLint + Tailwind on `.ts`; `blade.php` still beats `php`). An entry `=Dockerfile` matches a whole
  file name. With one `languageIds` entry per `fileExtensions` entry, the matching one is announced
  in `didOpen` (`tsx` → `typescriptreact`).
- Results of several servers are merged in `LspManager`: diagnostics per server (each server
  replaces only its own; missing `source` = server name), completion lists concatenated (items carry
  `raw.forgeServer` so `lsp_completion_resolve` goes back to their server), locations deduplicated,
  hover / signature help / document symbols from the first server with an answer (registration
  order = priority), formatting by the first server that supports it.
- `lsp_did_open|did_change|did_save({ path, text })`, `lsp_did_close({ path })`,
  `lsp_hover|lsp_completion|lsp_definition|lsp_references|lsp_signature_help({ path, line, character })`,
  `lsp_document_symbols({ path })`, `lsp_format({ path, tabSize?, insertSpaces? })`, `lsp_status()`.
  Paths are workspace relative, positions 0-based line / UTF-16 character.
- `lsp_workspace_symbols({ query, limit? }) -> WorkspaceSymbol[]` asks every running server
  (`workspace/symbol`); `{ name, kind, containerName?, path, range?, server }`. It backs the
  Search Everywhere providers `lsp.symbols` (Symbols tab) and `lsp.classes` (Classes tab).
- Events: `lsp://diagnostics` `{ path, diagnostics }` and `lsp://status` `ServerStatus[]`
  (`busy` carries `$/progress` message and percentage; the status bar shows
  "Indexing… TypeScript 45%").

### Catalog of language servers (`forge-langservers`)

- `crates/forge-langservers/catalog.toml` lists languages (extensions, file names, LSP language
  ids) and ~32 servers (TypeScript, Vue, Angular, Svelte, Astro, HTML/CSS/JSON/ESLint, Tailwind,
  YAML, TOML, XML, Markdown, Docker, Bash, SQL, LaTeX, Twig, Lua, Python, Go, Rust, C/C++, Java,
  Kotlin, C#, Ruby, Zig) with command, initialization options, settings, install options and
  licence. `pack_servers.toml` lists servers that packs start themselves (Intelephense, phpactor)
  so they show up on the same settings page.
- When a project's LSP manager is created, `commands::langservers::register_enabled` scans the
  project (honouring `.gitignore`, capped by `langservers.scanLimit`) and registers every enabled
  and installed server whose languages occur (and whose `when` markers exist, e.g. an ESLint config).
  Servers in the same `alternative` group (vtsls / typescript-language-server, pyright /
  basedpyright) are exclusive. Detection looks, in this order, at the project's
  `node_modules/.bin`, Forge's own server directory (`<user data dir>/forge/langservers/node_modules/.bin`,
  e.g. `~/.local/share/forge/langservers`), PATH, npm/nvm/volta/fnm, `~/.local/bin`, composer,
  cargo, go, dotnet, gem dirs, Mason (`~/.local/share/nvim/mason/bin`) and Windows `.cmd` shims.
  The `node_modules` of the project and of Forge's directory are also searched for packages a
  server needs (TypeScript SDK, `@vue/typescript-plugin`).
- npm servers are installed by Forge itself, with no prerequisites (`forge_langservers::managed`):
  a catalog option `npm install -g <packages>` becomes a managed install into Forge's server
  directory, never a global one.
  1. Node.js: Forge's own download (`<user data dir>/forge/runtimes/node-<version>`) or a `node`
     >= 18 on the machine; otherwise the newest LTS build for the platform is downloaded from
     `langservers.download.nodeMirror` (default `https://nodejs.org/dist`, `index.json`),
     checked against the release's `SHASUMS256.txt` and unpacked (`.tar.gz` on Linux/macOS,
     `.zip` on Windows) into a staging directory that is renamed into place (`managed::node`).
  2. Packages without runtime dependencies (Intelephense) come straight from
     `langservers.download.npmRegistry` (`<registry>/<name>/latest` or a pinned version): the
     tarball is checked against `dist.integrity` (sha512), unpacked into
     `<dir>/node_modules/<name>`, `.bin` entries are created (symlink / `.cmd` shim) and the
     package is recorded in `<dir>/package.json` so npm keeps it (`managed::npm`). Packages
     with dependencies (typescript-language-server, vscode-langservers-extracted,
     yaml-language-server, ...) run `npm install --prefix <dir> <name>@<version>` with the npm
     next to the Node.js in use (Forge's Node.js ships its own `npm-cli.js`).
  3. Servers in Forge's directory are started as `<node> <package script> <args>`
     (`Detector::launch`, used by `build_config`), so they need no `node` on PATH. Intelephense
     (PHP pack) gets the same through `PhpLspSettings::intelephense_node`
     (`commands::php::for_host`, local projects only).
  4. Updates: `langservers_check_updates({ force }) -> PackageUpdate[] | null` asks the registry
     for newer versions of the recorded packages at most once a day
     (`<dir>/.forge-update-check.json`, setting `langservers.updates.check`);
     `langservers_update({ package })` installs the latest version and restarts the servers.
  Downloads report `langservers://install-progress` `{ id, label, done, total }` (status bar
  `langservers.install`, `src/features/langservers/InstallStatus.tsx`) and their steps as
  `install-output` lines. `ServerRow.install[].managed` marks such options; the settings page
  shows a one-click "Install" for them and keeps "Install…" (dialog) for the others. Other tools
  (brew, pipx, go, cargo, dotnet, ...) run as listed in the catalog. Tests use a local HTTP
  server (`managed::testserver`) with fake tarballs, never the network.
- Intelephense is never bundled or committed: its licence forbids redistribution. When a local
  PHP project opens without it, `src/features/langservers/managed.ts` shows a balloon
  "PHP language server Intelephense is not installed" with Install / Not now / Don't ask again
  (`php.intelephense.autoInstall`: `ask` | `always` | `never`; decisions in `autoInstall.ts`).
  The first install asks for the licence (`IntelephenseConsent.tsx`, stored as
  `php.intelephense.licenceAccepted`); after the install `langservers_install` registers the
  catalog servers and runs `php::sync_lsp_server`, so Intelephense starts without reopening the
  project. Browser mode simulates all of it with `?intelephense=missing` (and `?updates=1`).
- Per-server settings: `langservers.servers.<id>.enabled|path|settings` (settings = JSON string
  merged into the server settings; `initializationOptions` key for startup options). Global:
  `langservers.autoStart`, `langservers.scanLimit`, `langservers.lua.environment|version`,
  `langservers.hints.enabled|dismissed`.
- Install hint: files that use a feature only a server provides get a banner above the editor
  when that server is not installed (`src/features/langservers/hints.ts`, `serverHint.ts`). Add
  an entry to `SERVER_HINTS` for a server (it is empty: `$schema` and YAML schema modelines are
  handled by Forge's own JSON Schema support). "Install…" opens the install dialog (`requestInstall`),
  "Not now" hides it until Forge restarts, "Don't show again" stores the server id in
  `langservers.hints.dismissed`.
- IPC: `langservers_list() -> ServerRow[]`, `langservers_apply() -> ServerRow[]` (re-plan after
  settings changed), `langservers_install({ id, index }) -> InstallOutcome` with output on
  `langservers://install-output` `{ id, stream, line }`, `langservers_cancel_install({ id })`,
  `langservers://changed`. Install commands come from the catalog only and run without a shell.
  Managed installs add `langservers://install-progress`, `langservers_check_updates` and
  `langservers_update` (see above).
- Intelephense licence: `intelephense_licence_status() -> { premium, maskedKey?, error?,
  purchaseUrl }`, `intelephense_set_licence({ key | null })`. The key lives in the OS keychain
  (service `forge-lsp`, account `intelephense`); free mode never sends `licenceKey`. A registered
  `intelephense` config gets the key applied (and restarts) when the manager is created or the key
  changes; pack code building Intelephense configs can call
  `commands::langservers::with_intelephense_licence(&mut config)`. A plaintext
  `php.lsp.licenceKey` setting (or the PHP page's old localStorage value) is migrated once.
- Events: `lsp://diagnostics` `{ path, diagnostics }` and `lsp://status` `ServerStatus[]`.

## Code intelligence: navigation and usages (`forge-codeintel`)

PHPStorm's navigation for every language, with the PHP index as fallback for PHP files. Rust
computes everything; `app/ui/src/features/codeintel` and `features/lsp/goto.ts` render.

- `crates/forge-codeintel` (pure, also WebAssembly): `UsageKind` and `classify_usage`
  (PHPStorm's usage types from the line around a reference), `group_usages` (the tree by usage
  type and file, without the declaration), workspace edits with LSP positions, context action
  ordering, the Find Usages settings (`usage_options`: `UsageOptions` per `ElementKind`,
  `UsageScope`, `text_occurrences`, `apply_options`) and highlighting (`highlight`: read /
  write access, `exit_points`, `lexical_highlights`).
- `packs/phpforge/lang-php`: `navigate` resolves the symbol at a position on the index
  (classes through imports, aliases, group uses and the namespace; functions and constants with
  PHP's global fallback; members through `$this`, `self`/`static`/`parent`, typed variables,
  `catch`, `@var`, property and return-type chains; variables) and answers `definition`
  (`PhpDefinition { onDeclaration, range, title, targets, external?, info? }`), `find_usages`,
  implementations, super methods, type declarations and `line_markers`. Names the index does not
  declare come back as an `ExternalSymbol` (class / function / constant candidates, or a member
  of candidate classes); `stubs::StubIndex` answers them from a phpstorm-stubs folder. These run
  through `forge_backend::PhpBackend` (`PhpCall::Definition`, `LineMarkers`, `Usages`,
  `Navigate`), so remote projects answer from the agent; the stubs live on this machine.
- PHP stubs (`forge_langservers::managed::stubs`, app state `commands::library::LibraryState`):
  `php.stubs.path`, else a pinned JetBrains phpstorm-stubs release downloaded on first use into
  `<cache dir>/forge/php-stubs/<version>` (the unpacked files are checked against a pinned
  `content_digest`, which `git archive <commit>` reproduces; `LICENSE` kept), else
  Intelephense's own `lib/stub`. Progress is reported as
  `langservers://install-progress` with id `php-stubs`. Indexed once per folder.
- Library files: `library://php-stubs/<path>` (the stubs) and `library://file/<absolute path>`
  (a file outside the project a language server pointed to; only paths handed out by a
  navigation answer can be read). The UI registers a read-only `FileSystemProvider` for
  `library://` with a "Read-only library file" banner and breadcrumbs
  `PHP Runtime › Core › Core_c.php › Throwable`. Language server and PHP index editor
  extensions skip library tabs.
- IPC (`app/ui/src/ipc/codeintel.ts`; positions are LSP positions):
  - `code_goto_declaration({ path, line, character, content? }) -> { onDeclaration, title?,
    targets: NavTarget[], source: "lsp" | "index" | "stubs" | "none", message? }` — language
    servers' `definition` first (a location containing the caret means "on a declaration"),
    then the PHP index, then the stubs. `NavTarget` is `{ path, line, column, name, container? }`.
  - `code_symbol_info({ path, line, character, content? }) -> { range, container, signature } |
    null` — the Ctrl+hover link from the index (and the loaded stubs, never a download).
  - `code_find_usages({ path, line, character, content?, buffers?, options?, scope? }) -> {
    title, source, declaration?, byType: UsageTree, byFile: UsageTree, element, scope }` —
    `references` of the servers, else the index (members also through parent and child
    classes; unknown receivers as dynamic usages). `options` are the `findUsages.options.*`
    settings by field name (`UsageOptions`; Rust applies those of the element: usages of a
    class's methods / fields, derived / implementing classes, derived interfaces, overriding
    methods through `PhpCall::Usages { options }`, read / write access, comments and strings,
    text occurrences found with `Backend::find_in_files`); `scope` is `UsageScope { kind:
    projectFiles | projectAndLibraries | openFiles | currentFile | recentlyChangedFiles |
    directory | custom, directory, includeSubdirectories, pattern, currentFile, openFiles,
    changedFiles }`, `scope` in the answer its label ("Directory 'app'").
  - `code_highlight_usages({ path, line, character, content? }) -> HighlightAnswer | null` —
    Highlight Usages in File and the element at the caret: exit points on `return` / `throw`,
    `documentHighlight` of the servers, else the PHP index (`PhpCall::Highlights`, also
    implementing / overriding methods on `implements` / `extends`), else the lexical fallback.
    `HighlightAnswer { target: usages | exitPoints | implementingMethods | overridingMethods,
    title, highlights: { range, kind: read | write | text }[], source, element, summary }`.
  - `code_usage_target({ path, line, character, content? }) -> { element, title, label,
    fields: { key, label, section: find | options }[], scoped } | null` — the Find Usages
    dialog's checkboxes for the element at the caret.
  - `code_navigate({ path, line, character, kind: "implementation" | "superMethod" |
    "typeDeclaration", content? }) -> { targets, missing? }` — `missing` explains a server
    without the feature (Intelephense Premium) when the index found nothing either.
  - `code_line_markers({ path, content? }) -> { line, kind: "subclassed" | "implemented" |
    "overridden" | "implements" | "overrides", title, targets }[]` — PHP gutter icons.
  - `library_read({ path }) -> { text, library, detail }`.
  - Hierarchies (`app/ui/src/ipc/hierarchy.ts`, `commands/hierarchy.rs`, model in
    `forge_codeintel::hierarchy`): `code_type_hierarchy({ path, line, character, content? })`
    and `code_method_hierarchy(…)` `-> { title, graph: TypeGraph | null, source, message? }`
    (`TypeGraph { root, types: { item, extends, implements, method? }[], method? }`);
    `code_call_hierarchy(…) -> { title, root: HierarchyItem | null, source, message? }` and
    `code_call_hierarchy_calls({ item, direction: "callers" | "callees" }) -> CallNode[]`
    (`{ item, calls: { path, range, preview }[] }`, one level per call). Language servers
    (`LspManager::type_graph`, `call_root`, `call_nodes`; items carry the server's item as
    `handle`) first, the PHP index (`PhpCall::Hierarchy`, `phpforge_lang_php::hierarchy`)
    for PHP files, merged (`merge_type_graphs`, `merge_call_nodes`). The Hierarchy tool window
    (`features/hierarchy`, right side) computes its trees with `forge_codeintel::hierarchy`
    through WebAssembly (`hierarchy_type_view`, `hierarchy_method_view`,
    `hierarchy_call_view`), so views, scopes and sorting need no new query.
  - Documentation actions of the View menu (`forge_codeintel::docs`):
    `code_external_doc({ path, line, character, content? }) -> { url?, name? }` (Shift+F1; PHP
    symbols resolved like `code_goto_declaration`),
    `code_quick_definition({ path, line, character, content? }) -> { title?, definitions:
    { path, line, column, name, container?, startLine, text, truncated }[], message? }`
    (Ctrl+Shift+I; on a declaration: the declaration itself),
    `code_type_info({ path, line, character, content? }) -> string | null` (Ctrl+Shift+P; the
    servers' hover, else the PHP index's signature) and
    `code_context_info({ path, line, content? }) -> { line, text }[]` (Alt+Q).
  - `code_related({ path, content? }) -> { kind, group, name, path, line, column }[]` —
    Related Symbol (Ctrl+Alt+Home, `forge_codeintel::related`): tests, views, templates,
    routes, migrations, factories, policies, stories and styles. It is a
    `FrameworkCall::Related`, so the project's files are listed and read where they are (the
    agent for a remote project). The UI (`features/related`) opens one result directly and
    several in the shared chooser, grouped (`DeclarationTarget.group`).
  - `code_prepare_rename({ path, line, character, content? }) -> { range?, placeholder?,
    invalid, message?, missing?, source: "lsp" | "index", occurrences, label?, classFile?,
    sigil }` — starts Rename (Shift+F6): the server's `prepareRename` and the occurrences in the
    file (a rename to the same name); PHP files fall back to the PHP index when the server
    cannot rename (`missing`, Intelephense without a licence) or none runs. `$` is not part of
    the ranges (`sigil`).
  - `code_rename({ path, line, character, newName, buffers?, renameFile? }) -> { edit, preview,
    conflicts, source, missing?, fileRename?, skipped }` — the server's edit, else the index's
    (`forge_codeintel::edit_from_locations` over `find_usages`); invalid names are errors. A class
    file named after the class is renamed with it (`fileRename`, in `edit` unless `renameFile`
    is false). `conflicts` lists overlapping edits, renames onto existing files and name clashes
    (`phpforge_lang_php::rename::clashes`).
  - Extract / Introduce and Inline (`app/src-tauri/src/commands/refactor.rs`,
    `phpforge_lang_php::refactor`: pure functions over the tree-sitter syntax tree with byte
    offsets; call sites and usages come from the PHP index through the backend, so remote
    projects work): `refactor_introduce_prepare({ path, kind: "variable" | "constant" | "field" |
    "parameter", start, end, content }) -> { expressions: { range, text }[], info? }` (several
    expressions at the caret: the "Expressions" popup; `info`: occurrences, name suggestions,
    field init places, call sites), `refactor_introduce({ ..., options: { name, replaceAll,
    visibility?, init?, defaultValue? }, buffers? }) -> RefactorAnswer`,
    `refactor_extract_method_prepare({ path, start, end, content }) -> ExtractMethodInfo`
    (parameters, returned variables, `usesThis`, duplicates), `refactor_extract_method({ ...,
    options: { name, visibility?, isStatic, parameters: { original, name }[], replaceDuplicates }
    }) -> RefactorAnswer`, `refactor_inline_prepare({ path, position, content, buffers? }) -> {
    kind, title, name, onReference, occurrences, readOnly }` and `refactor_inline({ ..., mode:
    "all" | "allKeep" | "this" }) -> RefactorAnswer`. `RefactorAnswer` is `{ edit, preview,
    conflicts, nameRanges, skipped }`; `nameRanges` are the new name's ranges in the current file
    after the edit (the in-place name of Introduce Variable). Errors are PHPStorm's "Cannot
    perform refactoring. …" hints. Other languages: `code_refactor_actions({ path, line,
    character, endLine, endCharacter, refactoring }) -> ContextAction[]`, the servers'
    `refactor.extract.*` / `refactor.inline.*` code actions for a Refactor menu action
    (`forge_codeintel::refactor_actions`).
  - Move, Copy, Safe Delete, Change Signature and Make Static
    (`app/src-tauri/src/commands/refactor_move.rs`, `phpforge_lang_php::refactor::project` over
    the `Project` trait: Find Usages, Go to Declaration, symbol search and files through the
    backend, so remote projects work; the plan is `FileOp`s (byte edits, create, rename,
    delete) and conflicts, turned into a `RefactorAnswer`): `refactor_move_prepare({ path,
    position, content, copy? }) -> { kind: "class" | "members" | "function", class?, members?,
    function? }` (the declaration's path and name position, also from a reference),
    `refactor_move_class({ path, position, content?, targetNamespace, targetDirectory,
    searchInComments })`, `refactor_copy_class({ ..., newName, targetNamespace,
    targetDirectory })`, `refactor_move_members({ ..., members, target })`,
    `refactor_move_function({ ..., target })`, `refactor_move_files({ moves: { from, to }[],
    searchForReferences, searchInComments }) -> { answer, files }` (text edits for the paths
    after the move; the UI moves the files first), `refactor_safe_delete_prepare({ path,
    position, content, searchInComments }) -> { kind, name, message, count, inComments,
    usages: UsagesAnswer, answer, paths }` and `refactor_safe_delete_files({ paths,
    searchInComments })` (`paths`: folders and files the UI deletes itself),
    `refactor_change_signature_prepare(...) -> SignatureInfo & { path, position, signature }`,
    `refactor_change_signature({ ..., options: { name, visibility?, returnType, parameters: {
    original, name, typeHint?, defaultValue?, callValue?, byRef?, variadic? }[] } })`,
    `refactor_make_static_prepare(...) -> { name, className, usesThis, fields, needsObject,
    parameter, path, position }` and `refactor_make_static({ ..., options: { objectParameter?,
    fieldParameters } })`. All take `buffers` (unsaved editors). `fs_copy({ from, to })` copies
    a file or folder through the backend (never replaces).
  - `code_plan_edit({ edit, open }) -> EditPlan` — how the UI applies an edit: the text edits of
    open editors, the backend's part (`code_apply_edit`), files to save first and tabs to
    reopen or close. `code_apply_edit({ edit, skip? }) -> AppliedFile[]` (all or nothing) and
    `code_revert_edit({ files })` (refused when a file changed since) write through the backend.
  - `code_context_actions({ path, line, character, endLine?, endCharacter?, content? }) -> {
    actions, bulb?: "fix" | "intention", missing? }` — the servers' code actions and the PHP
    intentions in PHPStorm's order ("fix all" variants in `children`); `code_resolve_action`
    fills an edit, `code_execute_command` runs a command (edits it makes arrive as
    `lsp://apply-edit` and are applied like any other edit). An action may carry `caret: {
    path, range }` (where the caret goes after the edit, opening that file) and `data` (the
    arguments of its UI `action`, run by a handler from `registerContextActionHandler`; such
    an entry is a fix, not a link).
  - Create from usage (`phpforge_lang_php::create`, part of the PHP intentions): "Create
    method / field / constant / enum case / function" edit the current file or, as a
    workspace edit, the target class's file; "Create class / interface / trait" is the
    action `PhpCreateClass` with `{ name, namespace, template, extends?, import, path }`,
    which opens the Create New PHP Class dialog (`features/fileTemplates`); afterwards
    `php_import_class({ path, content, fqn }) -> { start, end, newText }[]` (UTF-16) imports
    the class when it went to another namespace. The command drops fixes for symbols the PHP
    stubs declare (`Intention::unresolved`) or a language server already creates
    (`forge_codeintel::lsp_creates`). `PhpCall::Intentions` carries the code style and code
    templates (`GenerateOptions`).
- UI: Go to Declaration (`GotoDeclaration`, `Ctrl+Click`, middle click) shows usages on a
  declaration (`showUsagesAt`); `ShowUsages` (popup), `FindUsages` (Find tool window `find`,
  `Alt+3`), `GotoImplementation` (also `Ctrl+Alt+Click`), `GotoTypeDeclaration`,
  `GotoSuperMethod`, `TypeHierarchy` (Ctrl+H), `MethodHierarchy` (Ctrl+Shift+H) and
  `CallHierarchy` (Ctrl+Alt+H, `features/hierarchy`) are registered commands, so the keymap and the main menu enable them;

  `Alt+3`), `FindUsagesInFile` (scope Current File), `ShowSettingsAndFindUsages` (the Find
  Usages dialog, `FindUsagesDialog.tsx`; also the Settings buttons of the popup and the Find
  tool window; choices saved as settings), `HighlightUsagesInFile` (`highlightUsages.ts`:
  sticky until Escape, F3 / Shift+F3 through `gotoHighlightedUsage`, the status bar text via
  `showStatusMessage`) and the automatic highlighting at the caret
  (`editor.highlightUsagesAtCaret`; state in `editor/usageHighlight.ts`, marks on the error
  stripe `editor/errorStripe.ts`, which features feed through the `stripeMarks` facet), `GotoImplementation` (also `Ctrl+Alt+Click`), `GotoTypeDeclaration`,
  `GotoSuperMethod` are registered commands, so the keymap and the main menu enable them;
  several targets use `chooseDeclarationAt`. Browser mode: `mock/codeintelMock.ts` over the demo
  project and `mock/demoStubs.ts`, grouping through `forge-codeintel` as WebAssembly.
- Rename and context actions UI (`features/codeintel`): `RenameElement` (Shift+F6) starts the
  in-place rename (`rename.ts`: a CodeMirror state field with the occurrences; keystrokes in
  the edit box are mirrored to the others in the same transaction and kept out of the undo
  history; Enter / Tab / leaving the box commits, Escape cancels, Shift+F6 again opens the
  Rename dialog); in the Project view it renames the selected file. Committing reverts the live
  edits, asks `code_rename`, shows the Refactoring Preview / Problems Detected dialog for
  conflicts and applies the edit with `applyWorkspaceEdit` (`refactoring.ts`). Every affected
  editor gets one history entry tagged with the refactoring (a `StateEffect` with
  `invertedEffects`), so `Ctrl+Z` in any of them undoes the others (if unchanged since), reverts
  the backend's files and moves renamed tabs back; redo re-applies. `ShowIntentionActions`
  (Alt+Enter) opens the intention popup (`IntentionPopup.tsx`); the light bulb
  (`intentions.ts`) asks `code_context_actions` after the caret rests for 500 ms.
- Extract / Introduce and Inline UI (`features/codeintel/extract.ts`, `ExtractDialogs.tsx`):
  `IntroduceVariable`, `IntroduceConstant`, `IntroduceField`, `IntroduceParameter`,
  `ExtractMethod`, `Inline` and `Refactorings.QuickListPopupAction` (Refactor This) are
  registered commands (editor context, so `Ctrl+Alt+V` beats the global Vim toggle in the
  editor). The chooser popup ("Expressions", "Multiple occurrences found") highlights the row's
  expression; Introduce Variable applies the edit as a preview and edits the name in place
  through the rename machinery (`startLocalSession` in `rename.ts`: the live changes are kept
  out of the history and reverted right before the final edit is applied with
  `applyWorkspaceEdit`, so the refactoring is one undo step). The dialogs return their options;
  conflicts go through the Problems Detected dialog. Browser mode approximates the refactorings
  on the demo project in `mock/refactorMock.ts` (the tree-sitter grammar is not part of the
  WebAssembly build).

- Move, Copy, Safe Delete, Change Signature and Make Static UI
  (`features/codeintel/moveRefactorings.ts`, `MoveDialogs.tsx`): `Move` (F6), `CopyElement`
  (F5), `SafeDelete` (Alt+Delete), `ChangeSignature` (Ctrl+F6) and `MakeStatic` are registered
  commands; with the focus in the Project view the first three act on the selected files and
  folders (also drag and drop onto a folder row). The dialogs return their choices; the edit is
  applied with `applyWorkspaceEdit`, whose `fileOps` (`move` / `copy` of files and folders)
  run before the edit and are reversed on undo (and when the edit fails). Browser mode
  approximates the refactorings in `mock/moveMock.ts`.
- Extract Interface / Superclass, Pull Members Up / Push Members Down and Refactor This
  (`phpforge_lang_php::refactor::{hierarchy, available}`, `commands/refactor_hierarchy.rs`,
  UI `features/codeintel/hierarchyRefactorings.ts`, `HierarchyDialogs.tsx`, `refactorThis.ts`):
  `refactor_hierarchy_prepare({ path, position, content, refactoring })` answers the class,
  its members (`id` like `orders()` / `$count` / `ROLE`, `uses`: the ids each member uses, for
  the dependency highlighting) and the superclasses / direct subclasses;
  `refactor_extract_interface`, `refactor_extract_superclass`, `refactor_pull_up` and
  `refactor_push_down` answer a `RefactorAnswer` whose workspace edit creates the new file
  (rendered from the user's PHP Class / Interface file templates) together with the edits.
  `ExtractInterface`, `ExtractSuperclass`, `MembersPullUp` and `MemberPushDown` are registered
  commands. `code_refactor_this({ path, line, character, endLine, endCharacter, content })`
  answers `{ actions, lsp }`: the action ids that apply at the caret or selection (Rust decides;
  `null` for files without an analysis of their own) and the language servers' refactorings;
  the popup numbers the rows and filters by typing. Browser mode:
  `mock/refactorHierarchyMock.ts`.
- Extract Component and the JS/TS refactorings (`webforge_fw_vue::extract`,
  `webforge_fw_react::extract`, shared pieces in `webforge_lang_web::{scope, extract}`,
  `commands/extract_component.rs`, UI `features/codeintel/extractComponent.ts`):
  `refactor_extract_component_prepare({ path, start, end, content })` answers the props and
  imports of the selected elements, `refactor_extract_component({ ..., name, directory })` a
  `RefactorAnswer` that creates the component and edits the parent. `WebForge.ExtractComponent`
  is a keymap action (Refactor | Extract/Introduce | Component…, Refactor This, a link in
  `Alt+Enter`). The language servers' code actions are worded like PHPStorm
  (`forge_codeintel::phpstorm_title`, used for `Alt+Enter`) and filtered for Refactor This
  (`refactor_this_actions`). Browser mode runs the same Rust code through
  `web_extract_component*` and `codeintel_refactor_this_actions` (`mock/extractComponentMock.ts`).

## PHPStorm metadata, JetBrains attributes, include paths and source roots

Rust computes everything in `packs/phpforge/lang-php` next to the PHP index (so a remote
project answers from the agent); the UI renders (`app/ui/src/features/php/{meta.ts,
libraries.tsx, sourceRoots.tsx}`).

- `meta` parses `.phpstorm.meta.php` files (and `.phpstorm.meta.php/` folders) of the project,
  vendor packages (`vendor/<v>/<p>/…`, read even when vendor is lazy) and include paths into
  `PhpMeta`; `ProjectIndex::meta()` is the merged view, cached with the index (cache version 2)
  and updated by `update_file` / `apply_changes`. `navigate::receiver_types` applies
  `override()` (`map`, `type`, `elementType`) to function and method calls (method targets
  match the receiver's class chain), infers variables from their last assignment and
  `$data['key']` from `#[ArrayShape]` (`callsite::subscript_classes`). `definition` goes from a
  mapped key string to its class.
- `attributes` reads the JetBrains attributes (parameters keep their attributes in
  `Param::attributes`). `inspections::inspect` walks the tree-sitter tree, resolves calls and
  names with `navigate::resolve_word` and reports deprecated, expected values, immutable
  writes, pure results, unreachable code and the PSR-4 namespace (`ProjectIndex::
  source_roots`); `fixes_at` feeds the PHP intentions (Alt+Enter). `complete::complete`
  answers expected arguments / return values and shape keys; `injections::language_places`
  lists `#[Language]` parameters for `forge-injections` (`InjectionSettings::places`, rule
  `php-language-attribute`).
- Include paths and source roots: `project::PhpProjectSettings` (`php.includePaths`,
  `php.sourceRoots`, `php.composer.json`) reaches the backend through
  `PhpBackend::configure` before every `php_index_workspace`; include path files are indexed
  with absolute paths (`ProjectIndex::is_library`), the app maps them to granted
  `library://file/…` paths (`LibraryState::ui_path`; a remote project's are read through
  `PhpBackend::library_read`). Source roots are composer.json's PSR-4 rules with the user's
  changes (`forge_file_templates::psr4::{SourceRootSetting, apply_overrides,
  describe_roots}`), used by `templates::psr4_roots` (New PHP Class, Move) and the namespace
  inspection; browser mode runs the same code through `templates_php_namespace` /
  `php_source_roots` as WebAssembly.
- Backend calls (`PhpCall`, protocol 10): `Configure`, `Inspect`, `Complete`,
  `LanguagePlaces`, `Libraries`, `LibraryList`, `LibraryRead`.
- IPC (`app/ui/src/ipc/phpMeta.ts`, `commands/php_meta.rs`; offsets UTF-16):
  `php_inspect({ path, content? }) -> { start, end, severity, message, code, deprecated?,
  unnecessary?, fix?: { title, edits } }[]`, `php_complete({ path, offset, content? }) -> {
  from, to, items: { label, insert, kind: value | constant | key | class, detail? }[] } |
  null`, `php_language_places() -> { name, argument, method, language }[]`,
  `php_libraries() -> { name, path, exists }[]`, `php_library_list({ path }) -> { name, path,
  dir, open? }[]` (`open`: the `library://file/…` path), `php_source_roots({ overrides? }) ->
  { path, prefix, kind: source | test | none, origin: composer | user, overridden }[]`.
- UI: the inspections publish as diagnostics source `php-inspections`; the completion source
  sits next to the language server's; "External Libraries" is a Project view section
  (`registerProjectViewSection`), source roots decorate tree rows (`registerRowDecorator`) and
  add "Mark Directory as" to the Project view menu; Settings | Directories is a top-level page
  (`GROUP_ORDER` in `forge-plugin-api`). Browser mode: `mock/phpMetaMock.ts` over the demo
  project and `mock/demoLibraries.ts`.

## PHP debugger (`forge-debug`)

- `DebugServer` is a DBGp server for Xdebug 3: it listens on `php.debug.port` (9003), accepts any
  number of connections, identifies the server (`PHP_IDE_CONFIG=serverName=…`, else
  `$_SERVER['SERVER_NAME']`/`SERVER_PORT`), maps paths and sets the project's breakpoints.
  Breakpoints live in `<root>/.forge/breakpoints.json`, servers with path mappings in
  `<root>/.forge/php-servers.json`. Unknown servers whose script is not a project file wait for
  the "Incoming Connection From Xdebug" dialog (`debug_accept_connection` / `debug_ignore_connection`).
- IPC: `debug_status`, `debug_listen_start() -> port`, `debug_listen_stop`, `debug_sessions`,
  `debug_step({ sessionId, kind: "run" | "stepInto" | "forceStepInto" | "stepOver" | "stepOut" })`,
  `debug_pause|debug_stop|debug_detach({ sessionId })`, `debug_run_to_cursor({ sessionId, file, line })`,
  `debug_stack`, `debug_contexts`, `debug_variables({ sessionId, depth, context? })`,
  `debug_property({ sessionId, fullname, depth, context?, page? })`, `debug_eval({ sessionId, expression, depth? })`,
  `debug_set_value`, `debug_breakpoints`, `debug_breakpoint_add|update|remove|toggle`,
  `debug_breakpoints_mute`, `debug_watches|debug_watches_set`, `debug_servers|debug_servers_save`,
  `debug_launch_env({ serverName? }) -> env`, `debug_complete({ sessionId, text, depth? }) ->
  { label, insert, detail }[]` (console / Evaluate completion: variables after `$`, members after
  `->`), `debug_page_url({ serverId?, startUrl, debug? }) -> url`,
  `debug_web_page_start({ runId, serverId?, startUrl, debug }) -> { runId, commandLine, url }`
  (PHP Web Page: reports on `run://<runId>`, the UI opens `url`), `debug_validate_cli({
  interpreter? })` and `debug_validate_web({ documentRoot, url })` -> `ValidationReport { checks:
  { title, value, status: ok|warning|error|info, hint? }[], ok, environment }` (Run | Web Server
  Debug Validation; the web variant writes `forge_xdebug_validator_*.php` into the project folder
  through the backend, requests it like the HTTP client and deletes it). Files are project
  relative, lines 1-based.
- Breakpoints have PHPStorm's properties: `condition`, `suspend` (false = logpoint),
  `logMessage`, `logStack`, `logExpression`, `temporary` (remove once hit, DBGp `-r 1`) and
  `passCount` (DBGp hit condition `>=`). The session acts on a hit in Rust: it logs
  (`debug://output` with `stream: "log"`), removes temporary breakpoints (`debug://breakpoints`)
  and resumes logpoints without reporting a pause.
- Events (tagged `DebugEvent` JSON, `type` = last path segment): `debug://listening`, `debug://session`
  (`SessionInfo` with status and location), `debug://incoming`, `debug://ended`, `debug://output`,
  `debug://breakpoints`, `debug://error`. Typed client: `app/ui/src/features/debug/api.ts`.
- Web debugging: the PHP Built-in Web Server kind (`forge_process::config::PhpBuiltinServer`,
  `php -S host:port -t <docroot> [router]`; through Docker / Compose / SSH interpreters the
  port is published with `Invocation::publish`) is debugged with
  `-dxdebug.start_with_request=yes`; PHP Web Page and PHP HTTP Request
  (`features/debug/runTypes.ts`) and `.http` requests add `XDEBUG_SESSION_START=FORGE` or the
  `XDEBUG_SESSION` cookie (`scratches_http_run({ …, debug })`,
  `forge_scratches::http::HttpRequest::add_debug_cookie`).
- Run configurations start debugging through `debugLauncher` (`app/ui/src/features/debug/index.tsx`):
  `await debugLauncher.prepare(serverName?)` starts listening and returns the environment
  (`XDEBUG_MODE`, `XDEBUG_SESSION`, `XDEBUG_CONFIG`, `PHP_IDE_CONFIG`) for the PHP process;
  `debugLauncher.setStopFallback(fn)` lets "Stop" (Ctrl+F2) stop a run when no debug session is active.
- Main toolbar widgets left of the run configuration selector: `registerToolbarItem`
  (`src/features/toolbarItems.ts`).
## PHP CLI interpreters (`packs/phpforge/interpreters`)

- `phpforge-interpreters` (pure except `detect`; also in `forge-wasm`,
  `crates/forge-wasm/src/php_interpreters.rs`): the model (`PhpInterpreter`: `local | docker |
  compose | ssh | wsl`, PHP executable, image, compose files / service / lifecycle `run |
  exec`, SSH configuration id, WSL distribution, user path mappings, `-d` configuration
  options, debugger extension and the cached Reload results), `probe` (parsers for `php -v`,
  `php --ini`, `php -m`, `wsl.exe -l -q`), `detect` (PATH, system folders, Homebrew, phpenv,
  phpbrew, asdf, mise, Herd, Laragon, XAMPP; versions via `php -v` with a timeout),
  `mapping` (automatic mappings: Docker `/opt/project`, compose bind mounts, WSL `/mnt/<drive>`;
  `Mapper` both ways, `map_arg`) and `command::wrap` (the local, `docker run`, `docker compose
  run/exec`, `ssh`, `wsl.exe` command lines; debug runs get `host.docker.internal`, `ssh -R`
  and `PHP_IDE_CONFIG=serverName=<debug_server_name>`).
- Settings: `php.interpreters` (user) and `php.projectInterpreters` (project) are `objects`
  settings with string values (compose files one per line, mappings `local<TAB>remote`, options
  `name=value`, extensions comma separated; the UI codec is `features/php/interpreters.ts`).
  `php.interpreter` is an interpreter name or a local path (`InterpreterSet::resolve`).
  `php.languageLevel = "interpreter"` takes the level from the interpreter's detected version
  (`PhpLspSettings::from_settings`, `level_from_interpreter`).
- IPC (`app/src-tauri/src/commands/php_interpreters.rs`): `php_interpreters_detect() ->
  { path, version }[]`, `php_interpreter_info({ interpreter }) -> { version, configFile,
  additionalIni, xdebug, extensions }`, `php_interpreter_phpinfo({ interpreter }) -> string`,
  `php_interpreter_mappings({ interpreter }) -> { local, remote }[]` (automatic ones),
  `php_interpreter_compose_services({ files }) -> string[]`,
  `php_interpreter_wsl_distributions() -> string[]`. Probes run through the project backend.
  Framework helpers (Laravel routes, Symfony console, Composer phar) use `local_php`, the
  project interpreter's path when it is local.
- Debugging: a debug run through a remote interpreter registers a server
  (`DebugServer::set_interpreter_servers`, not stored) named like the interpreter, so
  `PHP_IDE_CONFIG` picks its mappings.

## Database tools (`forge-db`)

- Data sources live in `<root>/.forge/dataSources.toml` (project) and
  `<config dir>/forge/dataSources.toml` (global) as `[[dataSource]]` tables; passwords go to the
  OS keychain (service `forge-db`, account = data source id). MySQL/MariaDB, PostgreSQL and SQLite
  via `sqlx`; SQL is sent as raw text, values come back as JSON-safe cells (`null`, booleans,
  numbers within ±2^53, everything else as strings, binary as `{ "$hex": "…" }`).
- IPC (`app/ui/src/ipc/db.ts`): `db_data_sources_list`, `db_data_source_save({ source, password? })`,
  `db_data_source_delete({ id })`, `db_test_connection({ source, password? })`, `db_disconnect`,
  `db_schemas`, `db_introspect({ id, schema })`, `db_table_info`, `db_table_ddl`,
  `db_table_data({ id, request: { schema, table, filter?, order?, offset?, limit? } })`,
  `db_apply_changes({ id, schema, table, changes })` (one transaction, refused without a primary key),
  consoles `db_session_open|close`, `db_execute({ sessionId, sql, pageSize? })`, `db_fetch`,
  `db_set_auto_commit`, `db_commit`, `db_rollback`, `db_cancel`, `.env` suggestions
  `db_detect_env` / `db_add_from_env`, `db_export({ format: "csv"|"json"|"sql", dialect, table?, columns, rows, path? })`
  and `db_statement_at({ sql, from, to, dialect }) -> { text, start, end } | null` (what Ctrl+Enter
  runs; editor offsets). SQL text handling (dialects, splitting, literals, row changes, export) is
  the pure crate `forge-sql`, re-exported by `forge-db`.
- Events: `db://dataSources` (full list after changes and after a project's sources were loaded),
  `db://session` (`SessionInfo` when a console starts/stops running or its transaction changes).
- UI: non-file editor tabs are registered with `registerEditorView` (`src/shell/editorViews.ts`) and
  opened with `openViewTab`; consoles and table editors use it. Editor-area panels can opt into the
  tool-window key context with `data-focus-context="toolwindow"`.
## Scratches and Consoles (`forge-scratches`)

- Scratch files are global (like PHPStorm's): `<config dir>/forge/scratches/<name>`, per-scratch
  settings (the data source of a SQL scratch) in `<config dir>/forge/scratches.json`. New
  scratches are numbered `scratch.php`, `scratch_1.php`, … (`rest-api.http` for HTTP requests),
  buffers `buffer1.txt` … `buffer5.txt` (then the oldest is reused). The language list of the
  New Scratch File popup is every `forge-filetypes` type with an extension (first extension wins).
- IPC (`app/ui/src/ipc/scratches.ts`): `scratches_list`, `scratches_languages({ request: { query,
  current?, recent? } }) -> { id, name, extension, positions }[]`, `scratches_create({ language,
  content? })`, `scratches_create_buffer`, `scratches_read({ name })`, `scratches_write({ name,
  contents })`, `scratches_rename({ name, newName })`, `scratches_delete({ name })`,
  `scratches_set_data_source({ name, dataSourceId })`, `scratches_markers({ name, content }) ->
  { line, kind: "run" | "http", title, runName }[]`, `scratches_http_run({ runId, content, line,
  env?, source? })` (`source`: the editor path, `scratch://<name>` or a project path, that
  `< ./file` bodies and `> ./handler.js` files are relative to; project files are read through
  the backend), `scratches_http_environments() -> { names, default? }`; list changes are
  emitted as `scratches://changed`.
- Runs: PHP/JS/TS/Python/shell/Lua scratches are the backend run kind `scratch` (option
  `scratch` = name): PHP runs with `php.interpreter` and `-d auto_prepend_file=<root>/vendor/autoload.php`
  when the project has one (option `useAutoloader=false` turns it off), TypeScript with
  `node --experimental-strip-types`. `.http` requests are parsed in Rust (`###` separators,
  `@var = value`, `{{var}}`, `# @name`, the project's `http-client.env.json` /
  `.private.env.json`, environment `dev` by default, `< ./body.json` / `<@` body files, dynamic
  variables `{{$uuid}}`, `{{$random.uuid}}`, `{{$timestamp}}`, `{{$isoTimestamp}}`,
  `{{$randomInt}}`, `{{$random.integer(a, b)}}`), sent with reqwest and streamed to
  `run://<runId>` like a process (`started`, `console`, `exited`). Variables: environment <
  `client.global` < in-file. Response handlers (`> {% … %}` or `> ./handler.js`) run in
  `forge_scratches::handler`, a small safe interpreter (no JavaScript engine, no loops):
  `client.global.set/get/clear/clearAll/isEmpty`, `client.test`, `client.assert`, `client.log`,
  `response.status/body/headers/contentType`, `JSON.stringify/parse`; their report follows the
  response in the Run output. Globals live for the IDE session (`HttpClientState`). Every `.http`
  editor (scratch or project file) has a toolbar with the environment selector (project setting
  `httpClient.environment`, `$none` = no environment); project `.http` files get gutter ▶ icons
  and run as the `http-request` configuration (option `file` instead of `scratch`). SQL scratches open as a query
  console of their attached data source (`openFileConsole` in `features/database/store.ts`).
- UI: editor tabs at `scratch://<name>` are read and written through `registerFileSystem`
  (`src/state/editor.ts`), which also gives them the scratch icon and breadcrumbs. The Project
  view lists "Scratches and Consoles" (scratches and open database consoles) through
  `registerProjectViewSection`. Run configuration types can set `start` (another backend command
  streaming `run://` events) and `hidden` (not offered in the Run/Debug Configurations dialog).

## Laravel (`packs/phpforge/fw-laravel`)

- `Laravel::load(root)` analyses a project statically with the PHP pack's tree-sitter parser:
  routes from `routes/*.php` (verbs, `match`, `view`, `redirect`, `resource`/`apiResource`,
  groups with prefix/name/middleware/controller, included files, the `api` prefix), views and
  Blade components (anonymous, class based, Livewire), custom directives (`Blade::directive`,
  `Blade::if`), config and `.env` keys, Eloquent models (columns from migrations, `$fillable`,
  casts, relations with the related model, scopes, accessors) and translations. The project
  backend caches it (`forge_backend::frameworks`, in the agent for remote projects) and
  reloads when the files change.
- `laravel_detect() -> LaravelInfo { detected, version?, routes, views, components, models,
  configKeys, translations, directives }`.
- `laravel_routes() -> { routes: Route[], source: "static" | "artisan" }`. With the setting
  `laravel.routes.source` = `artisan` (or `auto` when `vendor/` exists) it runs
  `php artisan route:list --json` and keeps the static source locations. A `Route` is
  `{ methods, uri, name?, action, controller?, controllerMethod?, middleware, domain?, source?,
  actionLocation? }`; locations are `{ path, line, character }` (0-based, UTF-16).
- `laravel_complete({ path, offset, content }) -> { from, to, items: { label, kind, detail?,
  documentation?, insertText? }[] } | null` and `laravel_goto({ path, offset, content }) ->
  Location[]` for `route()`, `view()`/`@include`, `config()`, `env()`, `__()`, query builder
  columns (`->where('…')`), relations (`with('…')`), `$model->property`, `Model::scope`,
  `[Controller::class, 'method']`, `<x-component>` and custom directives. Offsets are UTF-16.
- `laravel_gutter({ path, content }) -> { line, kind: "route" | "controller" | "view", tooltip,
  targets }[]`.
- Artisan commands run in the Command Line Tools Console (see "Command Line Tool Support");
  `artisan::builtin_commands` is its fallback list when artisan cannot run.
- UI (`app/ui/src/features/laravel`): the Blade language (`registerLanguage("blade")`), a
  completion source next to the LSP one, `GotoDeclaration` / Ctrl+Click that try Laravel first,
  gutter icons, the Routes tool window, Run Artisan Command… (`Laravel.RunArtisan`: the
  Run Command popup with the artisan alias), the `framework.laravel` status item and Settings | Languages & Frameworks |
  PHP | Laravel. Tool windows and status items can declare `when` to show only for matching
  projects.
## Symfony (`phpforge-fw-symfony`) and Doctrine (`phpforge-orm-doctrine`)

- Both plugins activate only for projects whose detection reports `symfony` / `doctrine`.
  Settings live under Languages & Frameworks | PHP | Symfony (`symfony.*`) and | Doctrine
  (`doctrine.*`). In browser mode `?project=symfony` opens the Symfony demo project.
- `symfony_detect() -> SymfonyInfo | null` (`{ version, environment, consolePath, hasConsole,
  containerXml?, kernelClass?, routes, services, templates, commands }`), `symfony_refresh()`.
  The backend caches a project snapshot for 10 s; refresh rescans.
- `symfony_routes() -> Route[]` (`{ name, path, methods?, controller?, host?, source:
  "attribute" | "annotation" | "yaml", location? }`), `symfony_services() -> Service[]` (`{ id,
  class?, alias?, public, autowire, autoconfigure, tags?, source: "yaml" | "resource" |
  "container", location? }`), `symfony_parameters()`, `symfony_templates()`,
  `symfony_console_commands()` (application commands first, then framework/bundle ones). Services
  and parameters merge `config/services*.yaml` with `var/cache/<env>/*Container.xml` when present.
- `symfony_complete({ path, content, offset }) -> { from, items: { label, kind: "route" | "service"
  | "template" | "parameter" | "tag" | "filter" | "function" | "test" | "variable", detail?, info?,
  apply? }[] } | null` and `symfony_goto({ path, content, offset }) -> Location[]` for PHP, Twig and
  YAML files. `symfony_gutter({ path, content }) -> { line, kind: "route" | "template" | "controller"
  | "command", title, targets?, command? }[]`. Offsets are UTF-16, `Location` is `{ path, line,
  column }` (0-based).
- `symfony_console_run({ command, args, runId })` streams `symfony://console` events `{ type:
  "started", runId, commandLine } | { type: "output", runId, text, stderr } | { type: "exit", runId,
  code }`; `symfony_console_cancel({ runId })`.
- `doctrine_entities({ refresh? }) -> Entity[]` (`{ class, name, table?, repositoryClass?, path,
  line, source: "attribute" | "annotation" | "xml" | "yaml", embeddable?, mappedSuperclass?,
  fields: { name, type, column?, length?, nullable?, unique?, id?, generated?, embedded?, line }[],
  associations: { name, kind: "manyToOne" | "oneToMany" | "manyToMany" | "oneToOne", target,
  mappedBy?, inversedBy?, line }[] }`).
- DQL inside PHP (`createQuery()` incl. concatenations, heredocs and `Entity::class`, QueryBuilder
  `select/where/join/orderBy/...` and `expr()` fragments with the aliases of the builder chain):
  `doctrine_complete({ path, content, offset }) -> { from, items: { label, kind: "keyword" |
  "function" | "entity" | "alias" | "field" | "association" | "embedded", detail?, apply? }[] } |
  null`, `doctrine_goto`, `doctrine_diagnostics({ path, content }) -> { start, end, message }[]`,
  `doctrine_gutter({ path, content })`, `doctrine_highlight({ path, content }) -> { injected: [from,
  to][], spans: { from, to, kind }[] }` (editor offsets; the editor maps old spans through edits
  until the new answer arrives). The DQL logic is the pure crate `phpforge-dql` (sites found with
  tree-sitter on the desktop, with its lexical finder in browser mode).
## Code hosts (`forge-forges`)

- `CodeHost` trait with GitLab (REST v4, `PRIVATE-TOKEN`), Forgejo/Gitea (`/api/v1`, `token`) and
  GitHub (REST + GraphQL for review threads, `Bearer`) implementations. Numbers are the per-project
  ones (`!12` on GitLab, `#12` elsewhere); `Capabilities` tell the UI the wording and which actions
  the host supports (e.g. Forgejo cannot resolve threads through its API).
- The repository is detected from `git remote -v` (https, ssh, scp-like, nested GitLab groups);
  accounts make self-hosted servers known, also with relative URL roots and SSH ports.
- Accounts live in `<config dir>/forge/forges.toml`; tokens in the OS keychain (service
  `forge-forges`, account = account id) with `GITLAB_TOKEN`, `FORGEJO_TOKEN`/`GITEA_TOKEN`,
  `GITHUB_TOKEN`/`GH_TOKEN` as fallback for the public servers.
- IPC (`app/ui/src/ipc/forges.ts`): `forges_accounts`, `forges_login({ kind, server, token, apiUrl? })`
  (verifies before storing), `forges_logout({ id })`, `forges_verify({ id })`, `forges_repo() ->
  { repo, repos, account, capabilities } | null`, `forges_merge_requests({ filter }) -> Page<MergeRequest>`,
  `forges_merge_request({ number })`, `forges_mr_files({ number })` (parsed hunks),
  `forges_discussions({ number })`, `forges_create_mr({ request })`, `forges_comment({ number, body,
  position? })`, `forges_reply({ number, discussionId, body })`, `forges_resolve({ number,
  discussionId, resolved })`, `forges_approve`, `forges_merge({ number, options })`, `forges_close`,
  `forges_issues({ filter })`, `forges_issue`, `forges_issue_comment`, `forges_pipelines({ branch? })`,
  `forges_ci_status({ gitRef })`, `forges_members()`, `forges_file({ path, gitRef })`,
  `forges_checkout({ number, sourceBranch }) -> branch` (emits `git://changed`).
- Errors are worded for balloons: rejected tokens, missing permissions, rate limits (with the reset
  time), unreachable servers.

## Services: Docker, Docker Compose and Kubernetes (`forge-containers`)

- Pure modules (also in `forge-wasm`, `crates/forge-wasm/src/containers.rs`): Engine API JSON
  into models (`docker`), compose files with profiles, env files, `${VAR}` interpolation from
  the project's `.env`, overrides and the `docker compose` command lines (`compose`), kubeconfig
  files and pods / deployments / services (`kube`), the multiplexed log stream (`logs`), pull
  progress (`pull`), the daemon endpoint from settings, `DOCKER_HOST`, the current Docker
  context (`~/.docker/contexts/meta/*/meta.json`, `currentContext` / `DOCKER_CONTEXT`) or the
  platform default, including `ssh://user@host[:port][/socket]` hosts (`endpoint`), exec /
  attach / `kubectl exec` / `kubectl attach` command lines (`exec`), deployment log prefixes
  (`logs::LinePrefixer`, `[pod/<pod>/<container>]` like `kubectl logs --prefix`), WebSocket
  framing (`ws`), the port-forward channel protocol and forward specs (`portforward`), gutter
  markers (`gutter`) and the whole Services tree, details pane and toolbar state (`tree`).
- Runtime (`runtime` feature): a small HTTP/1.1 client over Unix sockets, Windows named pipes
  and TCP with rustls (`http`), the Engine client (`engine`: snapshot, lifecycle, inspect,
  logs, pulls with `X-Registry-Auth`, `/events`), the Kubernetes client (`kubeclient`: CA,
  bearer tokens, client certificates, basic auth, exec credential plugins), `docker compose` /
  `docker build` processes (`cli`) and registry passwords in the OS keychain (service
  `forge-containers`, account `registry:<server>`; `secrets`). Only compose, builds and the
  interactive terminals use the docker CLI (which reaches `ssh://` hosts itself).
- Docker over SSH (`ssh`): `forge-remote`'s `SshSession` (ssh-agent, `~/.ssh` keys,
  `~/.ssh/config` aliases, `known_hosts`; unknown host keys are refused with their
  fingerprint) and one `direct-streamlocal` channel to the remote Docker socket per HTTP
  request (`SshSession::open_streamlocal`); the session is shared and reconnects once.
- Kubernetes port forwarding (`forward`, `portforward`): `GET …/pods/<pod>/portforward?ports=<p>`
  upgraded to a WebSocket (`v4.channel.k8s.io`); channel 0 carries data, channel 1 errors, and
  each starts with the port (2 bytes, little endian). The hub listens on `127.0.0.1:<local>`
  and opens one WebSocket per accepted connection; forwards show in the tree (`⇄ 8080→80`)
  and on the pod / Kubernetes details (`portForwards`).
- Pods: `containers` / `initContainers`, the `kubectl.kubernetes.io/default-container`
  annotation (else the first container) and declared `ports`; Log, Exec and the Attached
  Console take the picked container. Deployment logs follow every container of every pod
  matching the deployment's selector.
- `Hub` holds the state: a watcher connects, follows `/events` (debounced refresh) and retries
  every 10 s; compose files are read from the project root and one level below; contexts load
  their namespaces and the default namespace's workloads when expanded.
- Node ids: `docker`, `docker/containers|images|networks|volumes`, `docker/container/<id>`,
  `docker/image/<id>`, `docker/volume/<name>`, `docker/network/<id>`, `compose/<project>`,
  `compose/<project>/<service>`, `kube/<context>/<namespace>/pods|deployments|services/<name>`
  (segments percent-encoded).
- IPC (`app/ui/src/ipc/services.ts`): `services_tree() -> Node[]`, `services_refresh()`,
  `services_expand({ nodeId })`, `services_details({ nodeId }) -> Details | null` (title, status,
  tabs `dashboard | log | console | inspection`, Dashboard sections with rows and tables,
  available actions, pod `containers` / `ports` / `portForwards`), `services_action({ nodeId,
  action: "start" | "stop" | "restart" | "remove" | "pull" | "up" | "down" | "connect" |
  "refresh" })` (`forwardPorts` is handled by the UI's dialog), `services_inspect({ nodeId }) -> string`,
  `services_logs_start({ nodeId, container? }) -> key`, `services_logs_stop`,
  `services_console({ key }) -> ConsoleLine[]`, `services_terminal({ nodeId, attach,
  container? }) -> { title, program, args, env }` (opened with `openTerminalWith`;
  `terminal_open` takes `args`), `services_port_forward_start({ nodeId, spec }) ->
  PortForward[]` (`spec`: `8080:80, 9000, :5432`), `services_port_forward_stop({ id })`,
  `services_port_forwards()`, `docker_contexts() -> DockerContext[]`, `services_pull({ image })`,
  `services_compose_up({ path, service? })`, `services_dockerfile_run({ path })`,
  `services_gutter({ path, content })`, `services_compose_changed()`, `services_status()`,
  `docker_test_connection({ settings? })`, `docker_registry_login|logout|status`.
- Events on `services://event`: `{ type: "tree", nodes }`, `{ type: "console", key, delta, reset? }`
  (keys `log:<nodeId>` for followed logs, `run:<nodeId>` for compose, build and pull output;
  deltas are `forge-output` console lines), `{ type: "ended", key }`, `{ type: "status", state,
  endpoint, error?, version? }`.
- Settings: Build, Execution, Deployment | Docker (`docker.connection`: `auto | unix | pipe |
  tcp | ssh | context`, `docker.socketPath`, `docker.tcp.url|tls|certPath`, `docker.ssh.url`,
  `docker.ssh.socketPath`, `docker.context`, `docker.cliPath`, `docker.logs.tail`; the page
  lists the Docker contexts with Use), | Docker | Docker
  Registry (`docker.registries`: `server<TAB>user`), | Kubernetes (`kubernetes.kubeconfig`,
  `kubernetes.defaultContext`, `kubernetes.defaultNamespace`, `kubernetes.kubectlPath`).

## WebForge (`packs/webforge/*`)

- Plugins (`app/src-tauri/plugins/webforge.toml`): `forge.web` (HTML, CSS, JavaScript and
  TypeScript, npm, project type `webforge`), `forge.react` (`react`, `next`), `forge.vue` (`vue`,
  `nuxt`) and `forge.angular` (`angular`); frameworks are detected from `package.json` /
  `angular.json` by `forge-project`. The web language servers (TypeScript, Vue/Volar, Angular
  `ngserver`, HTML/CSS, ESLint) come from the `forge-langservers` catalog; their paths, enablement
  and auto-start are shown on Settings | Languages & Frameworks | TypeScript
  (`LanguageServersPage groups=…`) next to `web.navigation.components` and `angular.gutterIcons`.
  Node.js settings: `web.packageManager` (`auto` or a manager), `web.npm.gutterIcons`,
  `web.npm.debugArguments` (NODE_OPTIONS for Debug).
- Crates: `webforge-lang-web` (pure; `fs` feature for loading from disk): position-keeping JSON,
  `package.json` (scripts with lines, dependencies, workspaces, `packageManager`), package manager
  detection (setting > `packageManager` field > lock file in the package's or a parent directory >
  npm), the npm package list and gutter, ES imports/re-exports, module resolution (relative,
  `tsconfig`/`jsconfig` `paths`, index files, workspace packages imported by name through their
  `package.json` `exports`/`module`/`main`/`types`, falling back to sources when `dist/` is not
  built) and HTML/JSX tag scanning. React components include anonymous default exports
  (`export default () => …`, `export default function () {}`), named after their file. `webforge-fw-react`,
  `webforge-fw-vue` and `webforge-fw-angular` implement `FrameworkIndex { components, goto,
  gutter }` over `Sources`.
- IPC (`app/ui/src/ipc/web.ts`): `npm_packages({ refresh? }) -> NpmPackage[]` (`{ path, dir, name?,
  version?, packageManager: { manager, version?, reason }, scripts: { name, command, line, column }[] }`),
  `npm_gutter({ path, content }) -> GutterMark[]`, `web_components({ framework })`,
  `web_goto({ framework, path, content, offset }) -> { path, line, column }[]` (component tags,
  props/inputs, events/outputs, Angular selectors and pipes, `templateUrl`/`styleUrls`; on a
  component's own name, selector or Vue block tag: its usages) and `web_gutter({ framework, path,
  content }) -> { line, kind: "template" | "styles" | "component" | "usages" | "run", title,
  targets }[]`. `framework` is `react`, `vue` or `angular`; offsets UTF-16, lines 0-based. The
  backend reads the project at most every 5 s. Browser mode runs the same crates as WebAssembly
  (`crates/forge-wasm/src/web.rs`); `?project=react|vue|angular` opens a demo project.
- UI (`app/ui/src/features/webforge`): the npm tool window (`ActivatenpmToolWindow`; double-click
  runs a script as a temporary `npm` run configuration), run/debug icons in `package.json`, a
  declaration provider per framework (several targets open a chooser), Angular line markers and
  the Node.js / TypeScript settings pages. Debug on an `npm` configuration sets `NODE_OPTIONS`
  (`--inspect` by default) for an external inspector client.

## LuaForge (`packs/luaforge/lang-lua`)

- The plugin `forge.lua` (`app/src-tauri/plugins/luaforge.toml`, `projectTypes = ["luaforge"]`)
  is active for Lua projects: `forge-project` reports the `lua` language (and so the `luaforge`
  pack) for Lua files in the root, two or more in a folder, `.luarc.json`, `.luacheckrc`,
  `stylua.toml`, `selene.toml` or a `*.rockspec`, and the frameworks `love` (`conf.lua`, or a
  `main.lua` using `love.`) and `neovim` (`init.lua` next to `lua/`, or `lua/` + `plugin/`).
- `luaforge-lang-lua` is pure Rust (also built to WebAssembly, `crates/forge-wasm/src/lua.rs`):
  a tolerant lexer and structural parser record locals with their scopes, parameters, globals,
  table fields, functions and methods (`function M.f`, `function M:f`, `M.f = function`,
  constructor fields), references, `require` calls and the main chunk's `return`; doc comments
  (`---`) attached to the next statement are parsed as EmmyLua/LuaLS annotations (`@class`,
  `@field`, `@param`, `@return`, `@type`, `@alias` with `---|` values, `@deprecated`, `@see`).
  `ProjectIndex` resolves values across files: tables, `---@class`/`---@type`/`---@param`
  types, `---@return` of called functions (`Class.new()` without annotations returns the
  class), `setmetatable`, class parents and `require("a.b")`. Modules resolve through
  `package.path`-like templates (`lua.requirePaths`, `.luarc.json` `runtime.path`, then `?.lua`,
  `?/init.lua`, `lua/?.lua`, `src/?.lua`, `lib/?.lua`).
- IPC (`app/ui/src/ipc/lua.ts`, offsets UTF-16): `lua_index_workspace() -> { files, functions,
  classes }` (files come through the project's backend, so remote projects work),
  `lua_files_changed({ changed, removed })`, `lua_search_symbols({ query, kind: "class" |
  "symbol", limit }) -> SymbolHit[]` (pre-ranked), `lua_file_structure({ path, content? })`,
  `lua_hover({ path, offset, content? }) -> { doc: HoverDoc, start, end, language }` (the
  `forge-markdown` hover AST, rendered by `buildHoverDom`), `lua_goto({ path, offset, content? })
  -> { path, offset, line, column }[]`, `lua_run_markers({ path, content? }) -> { line, kind:
  "script" | "love", name, file, interpreter? }[]` and `lua_format({ path, content }) -> string |
  null` (StyLua with `--stdin-filepath`; `null` when it is off or not installed).
- UI (`app/ui/src/features/lua`): Structure tool window through `registerStructureProvider`
  (`features/php/StructureView.tsx`), Search Everywhere providers `lua.classes` /
  `lua.symbols`, a declaration provider (tried before the language server), an index hover
  that yields to lua-language-server, gutter run icons and Ctrl+Shift+F10 for main chunks,
  Reformat Code through `registerFormatProvider` (`features/lsp/format.ts`) and
  Settings | Languages & Frameworks | Lua.
- lua-language-server comes from the catalog (`forge-langservers`): `langservers.lua.environment`
  (`auto` detects LÖVE/Neovim from the project root), `langservers.lua.version`,
  `langservers.lua.libraries` (appended to `Lua.workspace.library`) and the executable
  `langservers.servers.lua-language-server.path`.
- Runs: the manifest type `lua-script` (`["${interpreter}", "${interpreterOptions}", "${file}",
  "${arguments}"]`) takes its default interpreter from `lua.interpreter` through
  `interpreterSetting`; LÖVE games run as `love <dir>`. Browser mode: `?project=lua` opens the
  LÖVE demo game `pixel-dungeon` (`mock/demoLuaProject.ts`).

## Scripting and config language packs

- `app/ui/src/features/languages/` holds the packs for Bash, Python, YAML, JSON, INI, `.conf`,
  dotenv, ignore files, Dockerfile, Docker Compose, Postfix (`main.cf`, `master.cf`), E-Mail and
  Symfony services. Each file calls `registerLanguage` (the syntax mode for a file type id; same
  ids replace the built-in modes in `src/editor/languages.ts`) and `registerEditorExtension`; `shared.ts` has the stream parser token
  vocabulary (`src/editor/tokens.ts`), hover docs, folding and `syntaxDiagnostics`, which feeds the
  Problems view unless one of the listed language servers (ids from the catalog) runs for the
  file. `yamlSchema.ts` completes and documents keys of YAML files from a schema tree (Compose,
  Symfony services).
- The file types themselves (extensions, `fileNames` with `*` anywhere and `/` for path
  suffixes like `config/services*.yaml`, `interpreters` for shebangs, `indentSize`) are declared
  in the manifests `app/src-tauri/plugins/langs.toml` (`[[contributions.fileTypes]]`) and matched
  in Rust by `crates/forge-filetypes`. Language servers for
  these languages come from the catalog (`forge-langservers`), not from the manifests.
- `forge.symfony-services` only applies to Symfony projects (`projectTypes = ["symfony"]`); in other
  projects `config/services*.yaml` is plain YAML: the UI sends inactive plugins with each
  `filetypes_detect` request and their file types are skipped.


## Adding a CLI chat agent (`forge-agent-cli`)

The AI Chat runs the user's own installed agent CLI headless (no terminal) and renders its
structured output. Claude Code and Gemini CLI are built in; another CLI is one adapter plus a
plugin manifest entry, and the chat UI needs no changes:

1. **Profile** (`src/profile.rs`, pure): id, name, models, install hint, whether the CLI can ask
   before running a tool (`permission_prompts`) and the permission modes it offers, worded as
   that CLI behaves (Gemini CLI: Read-only / Auto-accept edits / YOLO). Forge's
   `PermissionMode` (`default`, `acceptEdits`, `plan`, `bypassPermissions`) is the shared
   vocabulary; `AgentProfile::effective_mode` maps unsupported ones to the most restrictive.
2. **Parser** (`EventParser`, pure when possible): one stdout line → `AgentEvent`s
   (`sessionStarted`, `assistantText`/`thinking` with delta semantics, `toolUse`, `toolResult`,
   `permissionRequest`, `turnFinished` with usage, `error`). Ignore non-JSON lines and unknown
   events. Map the CLI's built-in tools to the common names the chat renders (`Read`, `Edit`,
   `Write`, `Bash`, `Grep`, `Glob`, `WebFetch`, `WebSearch`, `TodoWrite`, inputs `file_path`,
   `command`, `pattern`, ...), as `gemini::normalize_tool` does; other tools keep their name and
   get a generic card.
3. **Adapter** (`AgentCli`, `runtime` feature): executable names for detection, the command
   line from `StartOptions` (model, permission mode → CLI flags, `resume` = the conversation id
   from `sessionStarted`), `encode_user_message`, and the `Transport`:
   `Persistent` keeps one process per conversation and writes every message as a stdin line
   (Claude Code `--input-format stream-json`); `PerTurn` spawns one process per message, writes
   the prompt to stdin and closes it (Gemini CLI `--output-format stream-json`, resumed with
   `--resume`). CLIs with permission prompts implement `encode_permission_response`; the session
   handles "allow for this session" rules. `AgentSession` does the rest: spawning in the project
   root, filling in `cwd` when the CLI does not report it, stderr on crashes, interrupts.
   Register the adapter in `builtin_agents()`.
4. **Tests**: a recorded stream in `tests/fixtures/*.jsonl` for the parser and a fake CLI shell
   script (`fake_claude.sh`, `fake_gemini.sh`) for the process lifecycle. No network, no real CLI.
5. **Plugin manifest** (`app/src-tauri/plugins/packs.toml`): a plugin with a `chatAgents`
   contribution and the settings `agents.<id>.path`, `.model` and `.permissionMode` in group
   `tools.ai-agents` with the agent's name as section (Settings | Tools | AI Agents; add the
   section to `AGENT_SECTIONS` in `AgentsSettingsPage.tsx`). Regenerate the browser snapshot.
6. **UI**: `registerChatAgent({ id, name, icon, models, tagline })` in
   `features/ai-chat/index.ts` adds it to the agent picker; modes, install hint and the
   "not installed" state come from `agent_list`.
7. **Browser mock** (`mock/agentMock.ts`): a scripted session. If the parser is pure, expose it
   through `pure_parser` (forge-wasm `agent_parse_lines`) and replay recorded CLI lines through
   it, as the Gemini demo does, so the mock exercises the same mapping as the desktop app.

## Text files, code style and per-project state (`forge-text`, `forge_core::project_state`)

- `crates/forge-text` (pure, also WebAssembly) knows everything about a text file besides its
  language: line separators, encodings (UTF-8 with or without BOM, UTF-16 LE/BE, ISO-8859-1,
  Windows-1252; detection and conversion), indent detection, the on-save cleanup (trailing
  spaces on none / modified / all lines, final line break), EditorConfig and the effective
  format of a file (`resolve_format`: EditorConfig > detected indent > the language's
  `<language>.codeStyle.*` settings of the selected code style scheme > the file type's
  indent), and commit message inspections.
- IPC (`app/ui/src/ipc/text.ts`): `fs_read_text({ path, encoding? }) -> { text, eol, encoding,
  bom, malformed }` (`\n`-only text), `fs_write_text({ path, text, eol, encoding, bom })`,
  `text_prepare_save({ request: { text, saved?, caretLines, options } }) -> { text, changed }`,
  `editor_file_format({ request: { path, languageId?, text?, fileTypeIndent? } }) -> FileFormat`
  (settings and `.editorconfig` files are added in Rust), `vcs_inspect_commit_message({
  message, rules })`. Files go through `Backend::read_bytes` / `write_bytes`, so remote projects
  keep their encodings. `state/editor.ts` keeps `eol`, `encoding`, `bom` and the resolved
  `format` per tab; `editor/codeStyle.ts` turns the format into CodeMirror extensions (indent
  unit, tab size, continuation indent, right margin and guides); `shell/FileStatusItems.tsx` are
  the status bar popups.
- `forge_core::project_state`: the workbench layout per project in Forge's state dir
  (`layout_load() -> ProjectLayout | null`, `layout_save({ layout })`; `{ active, sizes, splits,
  toolWindows, tabs, openFiles, activeFile }`, sanitized in Rust; `active` is keyed by slot:
  `left`/`leftSplit`/`right`/`rightSplit`/`bottom`/`bottomSplit` = PHPStorm's Left Top, Left
  Bottom, Right Top, Right Bottom, Bottom Left, Bottom Right; `toolWindows` holds the user's
  `{ slot, order, removed }` per moved tool window; `layout_default_load()` /
  `layout_default_save({ layout })` keep Store Current Layout as Default, tool windows only).
  Moving, ordering and resolving placements is the pure `state/toolWindowLayout.ts` (vitest);
  `state/layout.ts` applies it (`showToolWindow(id)`, `toggleToolWindow(id)`,
  `moveToolWindowTo`, `removeFromSidebar`, `isToolWindowVisible`) and the AI chat history per project in the data
  dir (`agent_history_list() -> ChatSummary[]`, `agent_history_load({ id })`,
  `agent_history_save({ record }) -> ChatSummary | null`, `agent_history_delete({ id })`). The
  UI saves the layout a moment after every change (`state/layoutPersistence.ts`; tool windows
  with tabs use `toolWindowTab` / `setToolWindowTab` from `state/layout.ts`) and restores it
  when the project opens.
- Inlay hints: `lsp_inlay_hints({ path, startLine, endLine, text, languageId? }) ->
  EditorInlayHint[]` asks the servers (`textDocument/inlayHint`); `forge_codeintel::inlay`
  sorts them into parameter names, variable types and return types and applies the Inlay Hints
  settings. Code Vision ("2 implementations") comes from the PHP line markers
  (`LineMarker.vision`, `editor/lineMarkers.ts`).
- Code style schemes (`crates/forge-codestyle`, pure, also WebAssembly; UI
  `features/codeStyle/`): the model of Settings | Editor | Code Style (General and 13
  language pages, PHPStorm's tabs, "Set from…" presets), the schemes (Default = user layer and
  Project = project layer under the `<language>.codeStyle.*` / `editor.codeStyle.*` keys,
  further IDE schemes under `editor.codeStyle.schemes.<id>.*`, the selection in
  `editor.codeStyle.scheme`), import/export (Forge JSON, PHPStorm XML), the previews and the
  pass after Reformat Code. IPC (`app/ui/src/ipc/codeStyle.ts`): `code_style_model()`,
  `code_style_schemes() -> { schemes, selected, hasProject }`, `code_style_save({ schemes })`
  (written with `Settings::set_many`), `code_style_preview({ language, values }) -> string |
  null`, `code_style_export({ name, values, format: "json" | "xml", path? }) -> string`,
  `code_style_import({ text?, path? }) -> { name, values, applied }`,
  `code_style_post_format({ request: { fileType, original, text } }) -> string`. The pages
  keep the edited schemes as a draft of the Settings dialog (`registerDraftSource` in
  `features/settings/dialogState.ts`). `editor_file_format`, the PHP code style and
  Intelephense's settings read the selected scheme (`forge_codestyle::effective_settings`);
  `lsp_format({ path, tabSize, insertSpaces, languageId })` pushes the language's options to the
  server (`forge_codestyle::lsp::settings_patch`, `LspClient::update_settings`, which also
  answers later `workspace/configuration` requests), and `lua_format` passes StyLua's options
  when the project has no `stylua.toml`.
- PHP code style beyond indents (`phpforge_lang_php::style`): brace placement (Intelephense's
  `format.braces`, Generate) and `php_tidy_blank_lines({ text }) -> string | null`, the blank
  line pass after Reformat Code. Format post-processors (`registerFormatPostProcessor(fn,
  order)` in `features/lsp/format.ts`) run after the language server or a format provider and
  get the text before formatting too; the code style pass runs last (order 100).
  Composer (`phpforge_lang_php::composer`): `php_language_level() -> { level, fromComposer }`,
  `php_composer_outdated() -> OutdatedPackage[]`.
- Optimize Imports and Auto Import (`phpforge_lang_php::imports`, `forge_codeintel::imports`,
  `app/src-tauri/src/commands/imports.rs`, UI `features/imports/`, client `src/ipc/imports.ts`):
  `imports_optimize({ path, content }) -> { edit: TextEdit | null, message }` (PHP with the
  import style; JS/TS/Vue through the TypeScript server's `source.organizeImports`);
  `imports_optimize_paths({ paths, includeSubdirectories, onlyVcsChanged, buffers? }) -> { edit:
  WorkspaceEdit, files, changed, message }` (Project view, applied as one undo step);
  `php_import_hint({ path, content, offset }) -> UnresolvedClass | null` (the import popup);
  `php_import_reference({ content, start, end, fqn, doc?, auto? }) -> { start, end, newText }[]`
  (the chosen class of "Class to Import" and completed class names: import or FQN);
  `php_imports_on_the_fly({ path, content, request: { caret, editedFrom?, editedTo?, add,
  optimize } }) -> { edits, added, removed }`; `code_imports_on_the_fly({ path, content, add,
  optimize, caret, edited? }) -> WorkspaceEdit | null` (TypeScript server's
  `source.addMissingImports` / `source.removeUnusedImports`, only away from the edit). Offsets
  UTF-16. The import style (`ImportStyle`: code style `php.codeStyle.imports.*` and
  `php.autoImport.*`) reaches the refactorings through `imports::style::set_default` (kept
  current from settings changes and project opening) and remote intentions through
  `GenerateOptions.style.imports`; every `use` statement is inserted by
  `imports::insert_imports`. Unresolved classes come from the backend's index
  (`PhpBackend::unresolved_classes`, agent call `PhpCall::UnresolvedClasses`) plus PHP's own
  classes from the stubs; "Import class" is built in `code_context_actions` (the ambiguous one
  is the action `PhpImportClass` with `{ start, end, name, doc, candidates }`). Completion
  items carry `additionalTextEdits`; `registerCompletionApplyHook` lets PHP replace the
  server's `use` edit. The JavaScript settings `javascript.autoImport.pathStyle` /
  `typeImports` become the TypeScript server's `importModuleSpecifierPreference` /
  `preferTypeOnlyAutoImports` (`forge-langservers`). Browser mode: notifications, edits and
  folder targets through WebAssembly, the PHP analysis approximated in `mock/importsMock.ts`.
- Actions on Save (`crates/forge-save-actions`, `phpforge_lang_php::{imports, rearrange,
  cleanup}`, UI `features/actionsOnSave/`): `saveFile(path, trigger)` in `state/editor.ts` takes
  a `SaveTrigger` (`"explicit"` for `Ctrl+S`, `"autosave"`, `"plain"` for saves of other
  operations) and calls the runner registered with `setSaveActionsRunner`.
  `save_actions_plan({ path, trigger }) -> SavePlan` (`{ reformat: "whole" | "changed" | null,
  optimizeImports, rearrange, codeCleanup, tools }`, from the `actionsOnSave.*` settings);
  Reformat code uses `formattedText` (`features/lsp/format.ts`: format providers, LSP,
  post-processors, without touching the document) and
  `save_actions_changed_lines({ path, original, formatted }) -> string` (HEAD through the
  backend's repo); `save_actions_php({ text, actions }) -> { text, applied }`; after the file is
  written `save_actions_run_tools({ path, tools }) -> { changed, outcomes }` runs ESLint,
  Prettier, PHP CS Fixer and Pint through `Backend::start_process` (rolling back a failing
  tool); `save_actions_info() -> { fileTypes, tools }` feeds the settings page. The editor gets
  the combined result as one transaction (`applyLineChanges` with `isolateHistory`), only
  when nobody typed meanwhile. Browser mode: plan and changed lines through WebAssembly, the
  rest mocked (no tool runs, no writes).
- Git: `git_crlf_files({ paths })` (files that would commit CRLF), `git_pull({ rebase? })`
  follows `vcs.git.updateMethod` without `rebase`, `git_commit` signs off with
  `vcs.git.commit.signOff`, and `vcs.git.path` selects the git binary
  (`forge_vcs::set_git_program`).

## Live templates (`forge-livetemplates`)

- `crates/forge-livetemplates` (pure, also WebAssembly) owns the template sets
  (`data/*.toml`; Laravel, Blade and Symfony groups carry a `framework` and apply only when the
  UI passes it in `frameworks`), the user's changes (`editor.liveTemplates.custom` entries:
  string tables with `group`, `abbreviation`, `description`, `text`, `contexts`, `variables`
  as JSON, `builtin`, `removed`, `key`; `editor.liveTemplates.disabled` ids
  `group/abbreviation`), context detection, variable expressions, expansion, the Surround With
  surrounders and Unwrap/Remove. All offsets are UTF-16.
- IPC (`app/ui/src/ipc/liveTemplates.ts`, `commands/live_templates.rs`):
  `live_templates_catalog({ settings }) -> { groups, contexts, functions }`,
  `live_templates_applicable({ request: { text, offset, path, languageId?, settings } }) ->
  { prefix, items, contexts }`, `live_templates_expand({ request: { …, offset, templateId?,
  env } }) -> Expansion | null` (`{ from, to, text, fields: { from, to, index }[], end,
  templateId }`; `env` carries the clock, time zone, clipboard, user and indent unit),
  `live_templates_surround_items({ request: { …, from, to, env, templatesOnly? } })`,
  `live_templates_surround({ request: { …, id } }) -> Expansion | null`,
  `live_templates_unwrap({ request: { text, offset, path, languageId? } }) -> { label, from,
  to, keep?, edits }[]`, `live_templates_variables({ text })`, `live_templates_user()`.
- The UI (`features/liveTemplates`) binds the expand key (Tab/Space/Enter, a CodeMirror editor
  key like Tab's indentation) and calls `live_templates_expand` in-process through
  `forge-wasm` (`liveTemplatesApi.expandSync`), because the key handler must decide at once;
  an expansion becomes a CodeMirror snippet (`toSnippet`), so Tab/Shift+Tab walk the fields.
  The actions use PHPStorm's ids (`InsertLiveTemplate`, `SurroundWith`,
  `SurroundWithLiveTemplate`, `Unwrap`, `SaveAsTemplate`). Save as Live Template opens the
  settings page with `openSettings(page, args)`; pages read such arguments with
  `takePageArgs(page)` (`features/settings/dialogState.ts`), which also works when the dialog
  is its own window.

## Emmet and colors (`forge-emmet`, `forge-colors`)

- `crates/forge-emmet` (pure, also WebAssembly) parses Emmet abbreviations and expands them as
  HTML, JSX or CSS (snippets, implicit tags, numbering, lorem, Emmet's formatting), decides
  where Emmet applies for a file type (text of HTML-like files and PHP outside `<?php`, not in
  `{{ }}` / `{% %}` / `@php`, `<style>` as CSS, JSX positions, CSS declaration starts) and
  computes Surround with Emmet, edit points, Update Tag and Remove Tag. Offsets are UTF-16.
- `crates/forge-colors` (pure, also WebAssembly) finds colors in CSS, markup and scripts
  (hex, `rgb()`/`hsl()`/`hwb()` in both syntaxes, names, Tailwind classes), parses them and
  formats a picked color in the notation of the original text.
- IPC (`app/ui/src/ipc/emmet.ts`, `commands/emmet.rs`): `emmet_expand({ request: { text,
  offset, languageId?, options, preview? } }) -> Expansion | null` (`{ from, to, text, fields,
  end, abbreviation, syntax }`, the shape of a live template expansion; `options`: `{ html,
  css, jsx, jsxClassAttr, hrefRecognition, indent }` from the `editor.emmet.*` settings),
  `emmet_wrap({ request: { text, from, to, abbreviation, languageId?, options } }) ->
  Expansion` (error text for invalid abbreviations), `emmet_edit_point({ request: { text,
  offset, forward } }) -> number | null`, `emmet_update_tag` / `emmet_remove_tag({ request: {
  text, offset, abbreviation?, options } }) -> { from, to, text }[]`, `colors_find({ request:
  { text, languageId? } }) -> { from, to, color: { r, g, b, a }, hex, text, editable }[]`,
  `colors_format({ request: { color, original } }) -> string`, `colors_parse({ text }) -> {
  color, hex } | null`.
- UI: `features/emmet` binds the expand key (`editor.emmet.expandKey`) as a `Prec.high`
  CodeMirror key registered after live templates (which, like postfix completion, win when
  they match) and calls `emmet_expand` in-process through `forge-wasm`; the expansion becomes
  a snippet (`toSnippet` of live templates). The abbreviation preview is a CodeMirror tooltip
  computed on typing. `features/colors` registers a line marker provider (gutter swatches,
  `registerLineMarkerProvider`), an inline swatch plugin and the color picker, which writes
  each change through `colors_format` (`userEvent: "input.color"`); recent colors are the
  setting `editor.colorPicker.recentColors`.

## Language injections (`forge-injections`)

- `crates/forge-injections` (pure, also WebAssembly) decides which string literals of PHP and
  JavaScript/TypeScript files carry another language (SQL, HTML, CSS, JSON, RegExp,
  JavaScript, XML): `language=` comments, heredoc labels, then the built-in rules (calls,
  tagged templates, assignments, strings starting with a statement or a tag). Literals joined
  with `.` / `+` are one injection; interpolations are gaps. Settings: `editor.injections.disabled`
  (rule ids, user) and `editor.injections.uninjected` (`path#key`, project). All offsets are
  UTF-16.
- IPC (`app/ui/src/ipc/injections.ts`, `commands/injections.rs`):
  `injections_find({ request: { text, path, disabled, uninjected } }) -> Injection[]`
  (`{ language, label, source: "comment" | "rule", rule?, from, to, ranges, literals: { from,
  to, contentFrom, contentTo, kind }[], comment?, key, text }`), `injections_at({ request: { …,
  offset } })`, `injections_fragment({ request }) -> { language, label, kind, from, to, text,
  map } | null` (`map`: source offset of each UTF-16 unit of `text` and its end),
  `injections_escape({ kind, text }) -> string`, `injections_inject({ request: { text, path,
  offset, language } }) -> { from, to, text } | null`, `injections_rules()`,
  `injections_languages()`. `code_context_actions` adds the intentions ("Inject language or
  reference", "Edit SQL Fragment", "Un-inject Language"); they run the UI actions
  `InjectLanguageAction`, `EditInjectedFragment` and `UnInjectLanguageAction` (or remove the
  comment as an edit). Context actions that run a UI action rank as intentions unless their
  title ends with "..." (links such as "Generate...", `ContextAction::is_link`).
- UI (`features/injections`): the editor asks `injections_find` in-process (WebAssembly) shortly
  after each edit and maps the last answer through changes meanwhile; each injected range is
  parsed with the injected language's CodeMirror parser on every change (`tokenize`, cached)
  and drawn as `cm-inj-*` marks over the host string. Edit Fragment is a CodeMirror bottom
  panel whose edits are mapped through the fragment's offset map and escaped by Rust. `Ctrl+Enter`
  (`EditorSplitLine` / `Console.Jdbc.Execute`) and the gutter run icon open a query console
  with the injected SQL (`runSqlInConsole`).

## Code structure in the editor (Sticky Lines, breadcrumbs, regions, rendered docs, File Structure, Navigation Bar)

PHPStorm's Sticky Lines, structure breadcrumbs, custom folding regions, rendered doc
comments and Reader mode (`app/ui/src/features/codeStructure`, user guide
`docs/guide/user/code-structure.md`).

- Rust computes: `forge_codeintel::structure::editor_structure(text, language)` scans a
  document once (braces, tags, Lua block keywords outside strings and comments; no parser)
  into scopes (kind, crumb name, header line/column, end, parent, sticky, crumb) and custom
  folding regions; `group_by_regions` nests Structure view nodes under regions.
  `forge_markdown::doc_comments` finds PHPDoc / JSDoc / LuaDoc comments and parses each into
  a `HoverDoc` (the documentation popup's parts).
- No IPC: Sticky Lines follow every scroll, breadcrumbs every caret move and folding asks per
  line, so the UI calls these functions synchronously through WebAssembly
  (`crates/forge-wasm/src/structure.rs`: `editor_structure`, `editor_doc_comments`,
  `structure_group_by_regions`, `editor_structure_languages`) in the desktop app and in
  browser mode, like the keymap resolver. `features/codeStructure/model.ts` caches the
  answers per document version (CodeMirror's immutable `Text`).
- The UI only draws: `sticky.ts` (a measured overlay at the top of the scroller, syntax colors
  from the editor's tree), `breadcrumbs.tsx` (a CodeMirror panel at the bottom or top),
  `regions.ts` (a `foldService` plus the placeholder with the region's name), `renderedDocs.ts`
  (block replace decorations with the documentation popup's DOM, `buildHoverDom`) and
  `readerMode.ts` (the Reader Mode button and editor classes). The Structure tool window
  groups every provider's nodes by regions and gets a provider for web files.
- Settings: `editor.stickyLines.*`, `editor.breadcrumbs.*`, `editor.renderDocComments` and
  `editor.readerMode.*` (`core.toml`); per-language toggles are `list` settings of switched-off
  file type ids, edited with checkboxes (`LanguageToggles`).
- File Structure popup (`FileStructurePopup`, Ctrl+F12, `features/fileStructure`): the
  Structure view's providers load the tree (`loadFileStructure` in
  `features/php/StructureView.tsx`, PHP inherited members from the index), and
  `forge_codeintel::file_structure::file_structure_view` (WebAssembly, synchronous) applies
  the toggles and the fuzzy speed search and picks the node at the caret.
- Navigation Bar (`features/navBar`): `forge_codeintel::navbar::nav_bar_segments`
  (WebAssembly) turns the path and the scopes at the caret into segments; dropdowns list
  `fs_list_dir` entries or structure nodes and filter them with `fuzzy_filter`. Placement
  `appearance.navigationBar.location` (`top`: above every editor group, with the plain path in
  the status bar; `statusBar`; `hidden`: `ShowNavBar` / Alt+Home shows it floating) and
  `appearance.navigationBar.showMembers`; actions `ShowNavBar`, `NavBarLocationTop`,
  `NavBarLocationBottom`, `NavBarLocationHide`, `ShowMembersInNavigationBar`.
- Actions: `ToggleRenderedDocPresentation` (Ctrl+Alt+Q), `CollapseDocComments`,
  `EditorToggleShowBreadcrumbs` (PHPStorm ids) and the Forge actions
  `ToggleRenderedDocPresentationForAll`, `EditorToggleStickyLines`, `ToggleReaderMode`.

## Spelling and grammar (`forge-spell`)

- `crates/forge-spell` (pure, also WebAssembly) owns the dictionaries (a minimal Hunspell
  `.aff`/`.dic` reader; the bundled SCOWL en_US dictionary, licence in
  `crates/forge-spell/data/en_US.LICENSE`; programming terms; custom dictionaries), word
  extraction per context (identifier splitting, skipped URLs/paths/code), suggestions
  (weighted edit distance with keyboard proximity) and Forge's own rule-based English grammar
  rules. All offsets are UTF-16.
- IPC (`app/ui/src/ipc/spell.ts`, `commands/spell.rs`): `spell_inspect({ request: { segments:
  { from, text, kind: "comment" | "docComment" | "string" | "identifier" | "text" |
  "commitMessage" | "fileName" }[], options, words } }) -> { from, to, kind: "typo" |
  "grammar", context, message, word?, identifier?: { from, to, text }, rule?, fixes?: { title,
  from, to, text }[] }[]` (`options` from the `spelling.*` / `grammar.*` settings, `words` the
  project-level and application-level dictionaries); `spell_context_actions({ request: {
  problems, words, hasProject, limit? } }) -> ContextAction[]` (UI actions
  `SpellCheck.ChangeTo` / `RenameTo` / `SaveTo` / `Replace` with `data`);
  `spell_dictionaries() -> { name, path?, words, builtin, error? }[]`. The app loads
  `spelling.customDictionaries` once per list of paths (absolute paths from this machine,
  relative ones through the backend). Browser mode runs the same code through `forge-wasm`.
- UI (`features/spelling`): an editor extension collects segments with the editor's own
  highlighting (`highlightTree` over the Lezer tree: comment, doc comment and string tags,
  definition tags and PHP `$variables`; Markdown and plain text files as prose without code),
  checks whole files up to 200,000 characters and larger ones as they scroll into view, and
  after edits only the changed lines grown to the surrounding comment or string. Problems
  stay in document offsets (mapped through edits) and are published to the diagnostics store
  as source `spelling` (`publishDiagnostics(path, diags, source)` keeps language server
  diagnostics and Forge's own inspections side by side) with the severities `typo` and
  `grammar`. Alt+Enter entries come from `registerContextActionProvider`
  (`features/codeintel/intentions.ts`), suggestions open a second intention popup
  (`showActionsPopup`), "Rename to..." runs the Rename refactoring. The commit message is
  checked as one `commitMessage` segment with an underline overlay.

## JSON Schema (`forge-jsonschema`)

- `crates/forge-jsonschema` (pure, also WebAssembly; downloads behind the `remote` feature):
  position-keeping JSON/JSON5 and YAML parsers (`Node` with byte ranges, tolerant of unfinished
  input), schema documents and a `Registry` (`$id`, anchors, `$ref` to other files through
  `missing()`), a validator for draft 4–2020-12 with PHPStorm's messages, completion contexts
  and items, hover, Go to Declaration (`At::decl`), mappings (user > `$schema`/modeline >
  built-in > SchemaStore catalog) and the bundled schemas of the built-in mappings
  (`schemas/`, licences in `schemas/NOTICE`; compressed by `build.rs`). Answers use UTF-16
  offsets.
- IPC (`app/ui/src/ipc/jsonSchema.ts`, `commands/json_schema.rs`): `json_schema_validate({ path,
  language, content }) -> { schema: { name, uri, source: "user" | "inline" | "builtin" |
  "catalog", location?, error? } | null, problems: { from, to, message, severity: "warning" |
  "weak" }[] }`, `json_schema_complete|hover|goto({ path, language, content, offset })`
  (`{ from, replaceFrom, replaceTo, items }`, `{ from, to, title, markdown }`, `{ path, line,
  column, preview }[]`), `json_schema_read({ location }) -> { text, detail }` for the read-only
  `json-schema://bundled/…`, `json-schema://remote/<url>` and `json-schema://file/<path>`
  tabs, `json_schema_builtins()`. Settings: `jsonSchema.mappings` (project objects `{ name,
  schema, version, files }`), `jsonSchema.remote.enabled` (off), `jsonSchema.remote.schemaStore`.
  Project schemas are read through the backend (remote projects work); URLs are downloaded only
  with remote schemas on, into `<cache dir>/forge/json-schemas` (`RemoteCache`, daily refresh,
  stale copy offline, failures retried after five minutes).
- UI (`features/jsonSchema`): an editor extension for `json`, `yaml`, `docker-compose` and
  `symfony-services` files publishes problems as source `json-schema`, adds a completion source
  (options styled with `styleOption` from `features/lsp/completion.ts`: required bold,
  deprecated struck through) and a hover; Compose and Symfony services keep their hand-written
  key completion and hover. While `vscode-json-language-server` / `yaml-language-server` runs
  for a file, problems, completion and hover are left to it. A declaration provider opens the
  schema; `registerFileSystem("json-schema://")` shows schemas read-only. The status item
  `json-schema` shows the file's schema and assigns another one (a project mapping); Settings
  pages `lang.schemas`, `lang.json-schema` (master-detail mappings editor) and
  `lang.json-schema.remote`. Browser mode runs the crate through `forge-wasm` (`json_schema.rs`;
  the mock answers `needs` with demo files).

## Composer (`packs/phpforge/composer`)

- `phpforge-composer` (pure, also WebAssembly; Packagist and composer.phar downloads behind the
  `net` feature): Composer versions and constraints, composer.json requirements with ranges,
  installed packages (composer.lock, installed.json), outdated markers, `composer audit`
  reports, Packagist p2 metadata and search, completion and hover in `require` /
  `require-dev` (cursor context from `forge_jsonschema::complete::json_cursor`). Answers use
  UTF-16 offsets plus line/character positions for files that are not open.
- IPC (`app/ui/src/ipc/composer.ts`, `commands/composer.rs`): `composer_project() -> {
  composerJson, exists, hasLock, requirements, installed }`, `composer_markers({ path, text })
  -> OutdatedMarker[]`, `composer_complete({ path, text, offset })` (the JSON Schema completion
  shape), `composer_hover({ path, text, offset })`, `composer_search({ query })`,
  `composer_package({ name }) -> { …PackageInfo, suggested, url }`, `composer_audit() -> {
  advisories, abandoned, composerJson, composerLock, jsonProblems, lockProblems }`,
  `composer_download_phar() -> path`. Packagist is reached from Rust only, while
  `php.composer.packagist` is on, cached in `<cache dir>/forge/packagist`; project files and
  `composer audit` go through the backend (remote projects work).
- Composer commands that change the project run as configurations of the hidden run type
  `composer-command` (`forge_process` kind: `composer <command> --ansi --no-interaction` in the
  folder of composer.json; `forge_process::composer_program` runs a `.phar` with the PHP
  interpreter) and stream into the Run tool window; the UI reloads composer.json/lock and
  bumps `composerRevision` afterwards.
- UI (`features/composer`): the composer.json editor extension (toolbar panel, completion
  source, hover, outdated markers as a line marker provider plus weak warnings of source
  `composer-outdated`, the `Composer.UpdatePackage` context action), the Manage Composer
  Dependencies dialog, the "Vulnerable Dependencies" Problems tab (`composer-audit` problems),
  `Composer.*` keymap actions under Tools | Composer and the Settings page `lang.php.composer`.
  Browser mode runs the crate through `forge-wasm` with a demo catalog instead of Packagist
  (`mock/composerMock.ts`, `mock/demoComposer.ts`).

## Command Line Tool Support (`phpforge-cli-tools`)

- Plugin `forge.command-line-tools` (PHP and Composer projects): Tools | Run Command…
  (`CommandLineToolsAction`, Ctrl+Shift+X), the "Command Line Tools Console" tool window
  (`commandLineTools`) and Settings | Tools | Command Line Tool Support
  (`tools.command-line-tools`; `commandLineTools.tools` is an `objects` setting with `kind`,
  `name`, `alias`, `path`, `interpreter`, `showConsoleIn`, `enabled`;
  `commandLineTools.autoDetect`).
- The pure crate (also WebAssembly, `forge-wasm/src/cli_tools.rs`) holds the tool model and
  detection, parsing of Symfony Console's `list --format=json` (Artisan, Symfony, Composer,
  Drush, custom Symfony tools) and `wp cli cmd-dump`, console input tokenizing with UTF-16
  offsets, completion with the command's help, the invocations and the cache fingerprint.
- IPC (`app/ui/src/ipc/cliTools.ts`, `commands/command_line_tools.rs`): `cli_tools_list() ->
  Tool[]` (configured, then detected), `cli_tools_commands({ alias, reload }) -> { alias,
  commands, source: "cache" | "tool" | "builtin" | "none", error? }`, `cli_tools_complete({
  input, offset }) -> { from, to, items, help?, hint? }` (loads the tool's commands on demand),
  `cli_tools_run({ input, runId }) -> { runId, commandLine, alias, tool, showConsoleIn }`.
  Lists run through the project backend (`process_io::capture`, 60 s timeout) and are cached
  in `<cache dir>/forge/command-line-tools/<kind>-<hash>.json` until the tool's path,
  interpreter or composer.lock changes. Scripts and `.phar` files run with the tool's PHP
  interpreter (or the project's), programs (`composer`, `wp`) directly or inside the
  interpreter's environment when the tool names one; runs start through
  `commands::run::start_built`, so they stream `run://<runId>` (`console` lines with file links
  mapped back from containers) and `run_stop` / `run_write` / `run_dispose` apply.
- UI (`features/cliTools`): `ConsoleInput` (completion list, help panel, history, Ctrl+C),
  the Run Command popup and the tool window with a tab per command.

## Vim emulation (`forge-vim`)

Tools | Vim (`VimPluginToggle`, setting `vim.enabled`, plugin `forge.vim`) turns on IdeaVim-style
Vim emulation in the editor. The engine is the pure crate `crates/forge-vim`; because a
keydown handler must decide on `preventDefault` before it returns, it runs in-process as
WebAssembly in the desktop app too (`forge-wasm/src/vim.rs`, `features/vim/engine.ts`), like
the keymap resolver.

- **Editor side** (`features/vim/extension.ts`, registered with `registerEditorExtension`): a
  `ViewPlugin` attaches each editor (`vim_attach({ text }) -> { handle }`), records the
  CodeMirror changes of every transaction it did not make itself, and a `Prec.highest`
  `domEventHandlers.keydown` sends each key: `vim_key({ handle, event, sync }) -> Response`.
  `sync` holds the change batches since the last call (UTF-16, `iterChanges` order), the
  selection, the visible lines and the indent unit; the full text is sent only when the
  copies went out of step (the response's `docLength` does not match). The response's
  sequential `changes` are composed into one transaction (one undo step, annotated so they
  are not sent back), with the new `selection`, `mode`, `cursor`, `cmdline` and `highlight`
  in a `StateField`. `effects` (`undo`, `redo`, `action`, `actionOnRange`, `open`,
  `clipboard`, `reveal`, `scrollTo`, `reloadRc`) run afterwards; with `resume` the queued
  keys of a mapping or macro continue (`vim_key` without `event`) once the asynchronous
  effects are done, and `wait` asks for `vim_timeout` after `timeoutlen`.
- **Keymap**: no new listeners. `keymap_handle_key` asks `forge_vim::Vim::claims` for keydown
  events in the `editor` focus context (unless a chord is pending): plain keys outside Insert
  mode, `Escape`, and the `Ctrl` shortcuts whose handler is Vim in the current mode are left
  unresolved, so they reach CodeMirror and the Vim handler. The focused editor is reported
  with `vim_focus({ handle | null })`. The handler table (`vim_handlers`) comes from
  `forge_vim::handlers` with the overrides of the `vim.shortcutHandlers` setting
  (`vim_configure({ enabled, handlers })`) and `sethandler` lines of the rc file.
- **`~/.ideavimrc`**: `vim_read_rc({ path? }) -> { path, text }` (desktop: `~/.ideavimrc`,
  `~/_ideavimrc`, `$XDG_CONFIG_HOME/ideavim/ideavimrc` or the `vim.rcFile` setting, with
  `source` lines inlined; browser mock: a demo file); the UI applies it with
  `vim_load_rc({ text }) -> { applied, problems }`.
- Rendering stays in TypeScript: the block caret, search highlights (the pattern is
  translated to a regular expression valid in JavaScript and Rust), relative line numbers, the
  command line panel (`showPanel`) and the status bar widget.

## Rust computes, TypeScript renders

Logic, data models and parsers live in Rust crates; the TypeScript UI only renders and forwards
events. Exception: per-keystroke editor work (CodeMirror tokenizers, decorations) stays in
TypeScript because a round trip per token would be slower.

- Desktop app: the UI calls Rust through Tauri IPC (`src/ipc/*`).
- Browser mode (`pnpm dev`, screenshots, vitest): pure crates (no file system, processes,
  network or tokio) are compiled to WebAssembly by `crates/forge-wasm` and loaded with
  `forgeWasm()` / `callWasm()` from `app/ui/src/wasm`. Mocks call the same Rust functions
  instead of re-implementing them. Each area adds a module in `crates/forge-wasm/src/<area>.rs`
  with `#[wasm_bindgen]` functions that take and return JSON strings, mirroring the IPC command.
- Rebuild the bundle after changing Rust code used by it: `pnpm --dir app/ui wasm` (needs the
  `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version pinned in
  `crates/forge-wasm/Cargo.toml`). The generated `app/ui/src/wasm/pkg` is committed so a plain
  `pnpm dev` works without the Rust toolchain.
- Keep the pure part of a crate with runtime dependencies separate so it can be built to
  WebAssembly: a small crate (`forge-output` for `forge-process`, `forge-sql` for `forge-db`,
  `phpforge-dql` for `phpforge-orm-doctrine`) or a cargo feature (`forge-agent-cli` builds its
  adapters and sessions only with `runtime`). Stateful models (run output, chat) are exposed to
  WebAssembly through numeric handles (`run_output_new` / `run_output_handle`, `chat_new` /
  `chat_apply`); the parser bridges live in `crates/forge-wasm/src/parsers/`.
- DQL injection highlighting comes from the backend shortly after each edit (old spans are mapped
  through the change meanwhile); it is not per keystroke-critical, so no TypeScript tokenizer is
  kept.
- Code that cannot wait (editor setup) may use `callWasmSync` once `forgeWasm()` resolved;
  `mock/index.ts` waits for the module before the UI starts.

### Search, file types and the Settings model

- **Fuzzy matching** lives in `crates/forge-fuzzy` (pure; `forge-core` re-exports it for the
  file walker). Camel humps, separators and strict word-start matching (actions, classes,
  symbols) are scored there; every highlight position the UI draws comes from it. Lists in
  popups use `createFuzzyFilter` (`features/search/fuzzyFilter.ts` → `fuzzy_filter`).
- **Search Everywhere**: providers registered with `registerSearchProvider` only *collect*
  candidates and return them with their render data (icon, detail, `run`). `runSearch` sends
  all candidates of the tab in one `search_rank` request; Rust matches text candidates (`match`
  on the item: strict, alternative texts such as action ids, keep-unmatched for pre-filtered
  language-server results), takes pre-ranked ones as they are (`score` + `matches` from
  `fs_find_files` or the PHP index), merges providers per tab, orders groups, dedupes the "All"
  tab, hides dimmed items while others match and cuts each group. The UI maps the answer back
  to its items. UI-only providers keep working unchanged; backends stay where their data is.
- **Find in Files** (Ctrl+Shift+F, `FindInPath`, `features/find`): `find_in_files({ request: {
  query, caseSensitive?, wholeWords?, regex?, fileMask?, directory?, limit? } }) -> { files: {
  path, matches: { line, column, length, text, ranges }[] }[], matchCount, truncated, error? }`
  (UTF-16 columns; `ranges` are inside the listed `text`). The pure crate `crates/forge-find`
  compiles the query (`regex`), file masks (`*.php, !*Test.php`) and matches lines;
  `Workspace::find_in_files` walks the project honoring `.gitignore`, skipping binary and huge
  files. It is a `Backend` method (agent call `FindInFiles`), so remote projects are searched on
  their host. Browser mode runs `forge-find` as WebAssembly over the demo files. The Text tab of
  Search Everywhere uses the same command.
- **Search index** (`crates/forge-search-core`, `crates/forge-search`): a persistent trigram
  index with a symbol table. `forge-search-core` is pure (builds for `wasm32`; `rayon` sits
  behind its `parallel` feature): trigram postings in roaring bitmaps, the literal prefilter of a
  query, class and symbol extraction for PHP, Rust, JS/TS, Python, Go, Java-like languages, C/C++,
  Ruby and Lua, and fuzzy ranking. `forge-search` adds the walk (`.gitignore`, excluded folders),
  chunked parallel reads, the cache file (user cache dir, `Forge/search/<hash>.bin`) and
  `SearchService`. `LocalBackend::with_search_index` (the app and the agent call it; the agent
  behind its `search` feature) builds the index in the background and keeps it current from
  `Backend::watch`. Until it covers the project, `find_in_files` and `find_files` walk the
  directory as before; afterwards the index only narrows the candidate files and `forge-find`
  still reads the lines, so both paths return identical results (test
  `find_text_matches_a_directory_walk`). New `Backend` methods: `find_symbols(query, types_only,
  limit, skip_extensions)` and `search_status()`; agent calls `FindSymbols` and `SearchStatus`.
  The Classes and Symbols tabs (`core.classes`, `core.symbols`) skip the languages a running
  language server or a pack index (PHP, Lua) already answers. The **Find tool window** (Alt+3,
  "Open in Find Window" in the popup) lists every match; its limit is the setting
  `search.findInFiles.maxMatches` (0 = unlimited), while Search Everywhere stays at a page. Browser
  mode answers `search_find_symbols` with `forge-search-core` as WebAssembly.
- **File types**: `crates/forge-filetypes` resolves files (bundled types plus manifest file
  types, inactive plugins skipped, `templates.dataLanguages` and soft-wrap patterns applied).
  `editor/fileTypes.ts` caches answers per path (and `#!` line) so `languageFor`,
  `languageLabel`, `languageIdFor` and `indentFor` stay synchronous: browser mode answers
  synchronously from WebAssembly; the desktop app resolves over IPC, and the editor and diff
  viewer `await ensureFileType(path)` before they build. A cache miss in the desktop app
  answers "Plain text" and updates reactive readers when the answer lands.
- **Settings dialog**: `forge_plugin_api::settings_model` builds the page tree, filters it,
  finds matching pages and settings, picks the page to select and splits every shown text into
  highlighted segments; `settings_validate` checks values against the schema. The dialog sends
  pages, the schema and its texts (`features/settings/dialogSearch.tsx`) and renders the answer.

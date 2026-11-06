---
title: CLI arguments and forge:// links
description: Command-line forms of the forge launcher (forge inspect, forge plugin validate and pack with the JSON report schema), the forge://open and forge://open-symbol links, and the Browser Companion's local HTTP API.
order: 50
tags: [cli, launcher, forge links, deep links, reference]
---

# CLI arguments and forge:// links

## The `forge` launcher

```text
forge [PROJECT_DIR | FILE[:LINE[:COLUMN]] | forge://…] …
```

| Form | Meaning |
| --- | --- |
| `forge` | start with the Welcome screen |
| `forge ~/Projects/shop` | open a directory as the project (with the This Window / New Window question when Forge runs with a project) |
| `forge src/App.tsx` | open a file: in the project window when it belongs to the open project (or to a folder of the same launch), else in the [LightEdit window](../user/open-from-the-os.md) |
| `forge -e notes.txt` / `forge --edit notes.txt` | open a file in the LightEdit window, also a project file |
| `forge src/App.tsx:12` / `forge src/App.tsx:12:3` | at line 12 (column 3) |
| `forge --line 12 --column 3 src/App.tsx` | JetBrains style; short `-l`, `-c` |
| `forge src/App.tsx 12 3` | `launch-editor` style: numbers after a file are its line and column |
| `forge 'forge://open?file=…&line=…'` | a link (treated like a link from a web page) |

- Relative paths are resolved against the current directory. Arguments that name nothing on
  disk are skipped with a warning (in the log). Empty or blank arguments (`forge ""`, for
  example from a launcher with an unset variable) are ignored; use `forge .` to open the
  current directory.
- `file:///…` URLs are accepted as the path they name (file managers launching `%U`).
- Other options starting with `-` are ignored.
- If Forge is already running, the arguments are handed to the running window
  (single instance) and the new process exits.
- Files given on the command line are **trusted**: they open without a confirmation, also
  outside known projects.
- Log output: set `RUST_LOG` (e.g. `RUST_LOG=debug forge`).

### `forge --mcp-stdio`

A stdio MCP server for clients that only start stdio servers (Claude Desktop, Codex): it
forwards newline-delimited JSON-RPC from stdin to the running Forge's MCP server and writes the
answers to stdout. It finds the server through `<config dir>/forge/mcp-server.json` and takes
the token from the keychain or `FORGE_MCP_TOKEN`. See [MCP Server](../user/mcp-server.md).

### `forge inspect`

```text
forge inspect <project> <profile.xml|profile name> <output dir> [-format xml|json|html] [-d <directory>] [-fail-on <severity>] [-v0|-v1|-v2]
```

The command-line inspector: runs the profile's inspections over the project without a window
and writes the results (one XML file per inspection plus `.descriptions.xml`, `inspections.json`
or `index.html`). Exit code 0, 1 with `-fail-on` when a problem of that severity or worse was
found, 2 for wrong arguments. See [Inspections](../user/inspections.md#command-line-inspector).

### `forge plugin validate`

```text
forge plugin validate <folder|file.forgeplugin> [--json] [--strict] [--forge-version <version>]
```

Checks a plugin the way Forge checks it on install, and reports every problem at once, so
plugin and marketplace repositories do not have to reimplement the rules. Runs headless (no
window, no display needed). The input is a plugin folder with `forge-plugin.json`, which is
packed in memory exactly like `forge plugin pack` would pack it, or a `.forgeplugin` file.
It checks:

- the package: safe zip reading (no absolute paths, `..`, drive letters or symbolic links),
  at most 64 MB packed and unpacked, at most 4096 files;
- the manifest: id, version, `engines.forge` against the running Forge version (or
  `--forge-version`), permissions, contributions and activation events;
- the UI bundle of code plugins: `main` is in the package and is text; contributed commands,
  status items, tool windows or settings pages without a `main` are a warning;
- icon themes: the theme file format and every SVG it names (Forge's `forge_icons` rules: no
  scripts, event handlers, foreign objects or external references, size limit);
- files Forge does not use (warnings), e.g. SVGs no theme names or stray files in a package;
- finally `Package::read` itself, the installer's check.

| Option | Meaning |
| --- | --- |
| `--json` | print the JSON report (below) instead of text |
| `--strict` | warnings fail too (exit code 1) |
| `--forge-version <v>` | check `engines.forge` against this Forge version instead of the running one |

Exit codes: **0** valid, **1** errors (or warnings with `--strict`), **2** wrong arguments or
a path that is neither a plugin folder nor a package file.

```text
$ forge plugin validate sdk/examples/shapes-icons
example.shapes-icons 1.0.0 (folder, 27 files, 8435 bytes) for Forge 0.1.0
valid (0 errors, 0 warnings)
```

#### JSON report (schema 1)

```json
{
  "schema": 1,
  "forgeVersion": "0.1.0",
  "path": "plugins/acme-icons",
  "kind": "folder",
  "strict": false,
  "valid": false,
  "errors": 1,
  "warnings": 1,
  "plugin": { "id": "acme.icons", "name": "Acme Icons", "version": "1.0.0" },
  "package": { "size": 18234, "sha256": "<64 hex digits>", "files": 31 },
  "diagnostics": [
    { "severity": "error", "code": "svg", "message": "contains a script", "field": "icons.php", "file": "theme/icons/php.svg" },
    { "severity": "warning", "code": "unknown-file", "message": "is in an icon theme folder, but no icon theme uses it", "file": "theme/icons/old.svg" }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `schema` | `1`. Fields may be added within schema 1; renamed or removed fields get a new number |
| `forgeVersion` | the Forge version `engines.forge` was checked against |
| `path`, `kind` | the input as given; `folder` or `package` |
| `strict`, `valid` | `valid` is what the exit code says: `true` exactly for exit code 0 |
| `errors`, `warnings` | counts of the diagnostics |
| `plugin` | id, name and version from the manifest, or `null` when it cannot be read |
| `package` | the (packed) package: size in bytes, SHA-256 (what a marketplace index lists), number of files; `null` when the zip cannot be read |
| `output` | `forge plugin pack --json` only: the file written |
| `diagnostics[]` | `severity` (`error` / `warning`), `code`, `message`; optional `field` (JSON path in the manifest or theme file) and `file` (package path) |

Diagnostic codes (stable):

| Code | Meaning |
| --- | --- |
| `package` | the zip cannot be read, a limit is exceeded, unsafe path, symbolic link |
| `layout` | a folder that cannot be packed (an icon theme file at the plugin root) |
| `manifest` | `forge-plugin.json` missing, not UTF-8, not valid JSON, or a manifest rule (`field` names it) |
| `main` | the UI bundle is missing, empty or not text; code contributions without `main` (warning) |
| `backend` | `backend` is declared but not run by this Forge version (warning) |
| `icon-theme` | the theme file is missing, invalid or breaks a theme rule |
| `svg` | an icon's SVG is missing, leaves the plugin or breaks an SVG rule |
| `unknown-file` | a file Forge does not use (warning) |

### `forge plugin pack`

```text
forge plugin pack <folder> [-o <file.forgeplugin>] [--json] [--forge-version <version>]
```

Packs a plugin folder into `<id>-<version>.forgeplugin` (in the folder, or `-o`): the
manifest, `README.md`, `LICENSE`, `CHANGELOG.md`, `THIRD_PARTY_NOTICES.md` and `NOTICE` when
present, `main`, everything in `dist/`, and the folder of each icon theme file. That is the
file selection of the SDK's `pack.mjs`. The zip is reproducible (fixed order, timestamps and
permissions), so the same sources give the same SHA-256. The package is validated first;
with errors nothing is written and the exit code is 1. Prints the report and `sha256 <hex>`
(or the JSON report with `output`).

The deb and rpm packages put `forge` on the `PATH`. For the AppImage, create a link yourself,
e.g. `ln -s ~/Applications/Forge_0.1.0_amd64.AppImage ~/.local/bin/forge`.

## forge:// links

Registered as a URL scheme by the installers; AppImages and development builds register it when
they start (Linux, Windows).

### `forge://open`

```text
forge://open?file=/home/me/shop/app/Models/User.php&line=42&column=5
```

| Parameter | Aliases | Meaning |
| --- | --- | --- |
| `file` | `path`, `url` | absolute path, `file://` URL, or a path relative to `project` (required) |
| `line` | `l` | 1-based line |
| `column` | `col`, `c` | 1-based column |
| `project` | | project root for relative paths |
| `symbol` | | symbol to locate when no line is given |

`forge:open?…` (without `//`) works too. Values are percent-decoded. Absurd line/column numbers
and over-long paths are rejected. When `file` is the absolute path of a **folder**, Forge asks
whether to open it as a project (a link can come from any web page), then opens it like
File | Open (with the This Window / New Window question).

### `forge://open-symbol`

```text
forge://open-symbol?name=UserCardComponent&kind=component
```

Finds the declaration of a component (`kind=component`) or class (`kind=class`) by name in the
open and recent projects and opens it.

### Confirmation

Links can come from any web page, so files outside the open project ask first: a file of a
recent project offers to open that project; a file outside any project asks before it opens.

## Browser Companion HTTP API

A small HTTP server on `127.0.0.1:63342` (or the next nine ports; setting
`browserCompanion.port`) serves the browser extension:

| Request | Body | Auth |
| --- | --- | --- |
| `GET /api/forge/ping` | — | none |
| `POST /api/forge/pair` | `{ "code": "123456", "name": "Chrome" }` → token | none (one-time code) |
| `POST /api/forge/open` | `{ "file", "line"?, "column"?, "project"?, "symbol"? }` | `Authorization: Bearer <token>` |
| `POST /api/forge/open-symbol` | `{ "name", "kind"? }` | `Authorization: Bearer <token>` |

Requests need a loopback `Host` header and no web-page `Origin` (browser extension origins
only). Pairing codes are 6 digits, valid for five minutes and for one use. See
[Browser Companion](../user/browser-companion.md).

---
title: CLI arguments and forge:// links
description: Command-line forms of the forge launcher, the forge://open and forge://open-symbol links, and the Browser Companion's local HTTP API.
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
| `forge ~/Projects/shop` | open a directory as the project |
| `forge src/App.tsx` | open a file (in the project it belongs to) |
| `forge src/App.tsx:12` / `forge src/App.tsx:12:3` | at line 12 (column 3) |
| `forge --line 12 --column 3 src/App.tsx` | JetBrains style; short `-l`, `-c` |
| `forge src/App.tsx 12 3` | `launch-editor` style: numbers after a file are its line and column |
| `forge 'forge://open?file=…&line=…'` | a link (treated like a link from a web page) |

- Relative paths are resolved against the current directory. Arguments that name nothing on
  disk are skipped with a warning (in the log).
- Other options starting with `-` are ignored.
- If Forge is already running, the arguments are handed to the running window
  (single instance) and the new process exits.
- Files given on the command line are **trusted**: they open without a confirmation, also
  outside known projects.
- Log output: set `RUST_LOG` (e.g. `RUST_LOG=debug forge`).

### `forge inspect`

```text
forge inspect <project> <profile.xml|profile name> <output dir> [-format xml|json|html] [-d <directory>] [-fail-on <severity>] [-v0|-v1|-v2]
```

The command-line inspector: runs the profile's inspections over the project without a window
and writes the results (one XML file per inspection plus `.descriptions.xml`, `inspections.json`
or `index.html`). Exit code 0, 1 with `-fail-on` when a problem of that severity or worse was
found, 2 for wrong arguments. See [Inspections](../user/inspections.md#command-line-inspector).

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
and over-long paths are rejected.

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

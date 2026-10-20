---
title: Xdebug debugging
description: Step-debug PHP with Xdebug 3 - listening for connections, breakpoints and conditions, stepping, variables, watches, Evaluate Expression, servers and path mappings.
order: 150
tags: [debug, xdebug, php, breakpoints, dbgp]
---

# Xdebug debugging

The Xdebug plugin (`forge.xdebug`, PHPForge pack) is a DBGp server for **Xdebug 3**. It
accepts any number of connections at once, from the CLI, a web server, Docker containers or
another machine.

## Preparing PHP

Install Xdebug 3 and enable step debugging in your `php.ini` (or the container's):

```ini
zend_extension=xdebug
xdebug.mode=debug
xdebug.client_port=9003
; for web requests: start on every request, or use a browser extension / XDEBUG_SESSION cookie
xdebug.start_with_request=trigger
; when PHP runs in Docker or on another machine
; xdebug.client_host=host.docker.internal
```

Forge listens on port **9003** (`php.debug.port`).

## Two ways to start

### Debug a run configuration

Press **Debug** (`Shift+F9`) on a PHP Script, PHPUnit, Artisan or Composer configuration (or a
gutter ▶ menu). Forge starts listening and runs the process with:

```text
XDEBUG_MODE=debug
XDEBUG_SESSION=FORGE
XDEBUG_CONFIG=client_host=127.0.0.1 client_port=9003 idekey=FORGE
PHP_IDE_CONFIG=serverName=<server>   (when a server is chosen)
```

so no `php.ini` change is needed for CLI scripts and tests.

### Listening for connections

For web requests, click the telephone icon **Start Listening for PHP Debug Connections** in
the main toolbar (action `PhpListenDebugAction`). The status bar shows that Forge is listening;
click again to stop. Then open the page with Xdebug triggered (`XDEBUG_SESSION` cookie, query
parameter or browser extension).

![Incoming connection](../../screenshots/debugger-incoming.png)

When a connection comes from an unknown server whose script is not a project file, the
**Incoming Connection From Xdebug** dialog asks which project file the server file maps to and
creates a server with that path mapping. Turn on **Ignore external connections through
unregistered server configurations** (`php.debug.ignoreExternalConnections`) to detach such
connections silently.

## Breakpoints

- Click a line number in a PHP file, or press **Toggle Line Breakpoint** (`Ctrl+F8`, macOS
  `Cmd+F8`).
- Right-click a breakpoint to set a **condition** (a PHP expression; the script suspends only
  when it is true) or disable it.
- **View Breakpoints…** (`Ctrl+Shift+F8`) lists all breakpoints, including **PHP Exception
  Breakpoints** (an exception class, or `*` for any).
- **Mute Breakpoints** turns all of them off temporarily.

![Breakpoint condition](../../screenshots/debugger-condition-popup.png)

![Breakpoints dialog](../../screenshots/debugger-breakpoints.png)

Breakpoints are stored in `<project>/.forge/breakpoints.json`.

## The Debug tool window

![Paused](../../screenshots/debugger-paused.png)

While paused, the execution point is highlighted in the editor and variable values are shown
inline at the end of lines (`php.debug.showInlineValues`). The **Debug** tool window (`Alt+5`)
has a tab per session with:

- **Threads & Variables**: the frames on the left (click to switch), variables on the right.
  Arrays and objects expand lazily.
- **Console**: the script's output.
- The evaluate field at the top: type an expression and press `Enter` to evaluate it, or
  `Ctrl+Shift+Enter` to add it as a watch.

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Resume Program | `F9` | `Cmd+Alt+R` |
| Pause Program | toolbar | toolbar |
| Step Over | `F8` | `F8` |
| Step Into | `F7` | `F7` |
| Smart Step Into | `Shift+F7` | `Shift+F7` |
| Force Step Into | `Alt+Shift+F7` | `Alt+Shift+F7` |
| Step Out | `Shift+F8` | `Shift+F8` |
| Run to Cursor | `Alt+F9` | `Alt+F9` |
| Force Run to Cursor | `Ctrl+Alt+F9` | `Cmd+Alt+F9` |
| Evaluate Expression… | `Alt+F8` | `Alt+F8` |
| Quick Evaluate Expression | `Ctrl+Alt+F8` | `Cmd+Alt+F8` |
| Show Execution Point | `Alt+F10` | `Alt+F10` |
| Stop | `Ctrl+F2` | `Cmd+F2` |
| Add to Watches | context menu | context menu |

![Evaluate Expression](../../screenshots/debugger-evaluate.png)

Smart Step Into currently behaves like Step Into. Pause depends on the engine and may
report "Pause is not supported by this engine".

## Servers and path mappings

When PHP runs in a container or on another machine, its file paths differ from yours. Define a
**server** in Settings | Languages & Frameworks | PHP | **Servers**: **Name**, **Host**,
**Port**, **Debugger** (Xdebug) and **path mappings** (project file or directory → absolute
path on the server).

![Servers](../../screenshots/debugger-servers.png)

A session finds its server from `PHP_IDE_CONFIG=serverName=<name>`, otherwise from
`$_SERVER['SERVER_NAME']` and `SERVER_PORT`. Servers are stored in
`<project>/.forge/php-servers.json`.

## Settings | Languages & Frameworks | PHP | Debug

![Debug settings](../../screenshots/debugger-settings.png)

| Option | Setting | Default |
| --- | --- | --- |
| Debug port | `php.debug.port` | 9003 |
| Ignore external connections through unregistered server configurations | `php.debug.ignoreExternalConnections` | off |
| Break at first line in PHP scripts | `php.debug.breakAtFirstLine` | off |
| Step out of files that are not mapped into the project | `php.debug.ignoreUnmappedFiles` | on |
| Show variable values in editor while debugging | `php.debug.showInlineValues` | on |
| Max. children (array elements / properties per page) | `php.debug.maxChildren` | 100 |
| Max. depth (levels fetched at once) | `php.debug.maxDepth` | 1 |
| Max. string length (bytes) | `php.debug.maxData` | 65536 |

> **Note:** the Xdebug listener runs on this machine; it is not available in
> [remote projects](remote-development.md) yet. JavaScript and Node.js debugging are not built
> in; see [WebForge](webforge.md#debugging-scripts).

---
title: Xdebug debugging
description: Step-debug PHP with Xdebug 3 - listening for connections, web pages and HTTP requests, breakpoints, logpoints and temporary breakpoints, stepping, variables and Set Value, watches, Evaluate Expression, the debugger console, servers and path mappings, Web Server Debug Validation.
order: 150
tags: [debug, xdebug, php, breakpoints, logpoints, dbgp, web server, validation]
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

### Debugging web pages and HTTP requests

These configurations start listening by themselves when you press **Debug** (`Shift+F9`):

| Configuration | Debug |
| --- | --- |
| **PHP Built-in Web Server** | runs `php -S host:port -t <document root> [router script]` with the Xdebug environment and `-dxdebug.start_with_request=yes`, so every request it serves is debugged; opens the page in the browser |
| **PHP Web Page** | opens the start URL on a server of Settings \| PHP \| Servers with `?XDEBUG_SESSION_START=FORGE` (Xdebug then sets the `XDEBUG_SESSION` cookie for the following requests) |
| **PHP HTTP Request** | sends the request (method, URL, headers, body) with the cookie `XDEBUG_SESSION=FORGE` |
| Requests in `.http` files | click the gutter ▶ and choose **Debug '…' with Xdebug**: the request is sent with the cookie |

![Debug an .http request](../../screenshots/http-debug-menu.png)

The Run tool window shows the cookie of a debugged request:

![Debugged request](../../screenshots/http-debug-run.png)

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

### Breakpoint properties, logpoints and temporary breakpoints

The right side of **View Breakpoints…** (and the popup of a right-click on a breakpoint) has
PHPStorm's properties:

| Property | Effect |
| --- | --- |
| **Enabled** | off keeps the breakpoint without stopping |
| **Suspend execution** | off makes a **logpoint**: Forge logs and lets the script run on (yellow dot) |
| **Condition** | a PHP expression; Xdebug stops only when it is true |
| **Log: "Breakpoint hit" message** | prints `Breakpoint reached at <file>:<line>` to the session's Console |
| **Log: Stack trace** | prints the frames |
| **Evaluate and log** | evaluates a PHP expression in the frame and prints its value |
| **Remove once hit** | a **temporary breakpoint**: removed after it suspended once (dot with a ring) |
| **Pass count** | stops only from the n-th hit on (Xdebug's hit condition `>=`) |

- **Shift+click** on a line number adds a logpoint ("Breakpoint hit" message, no suspend) and
  opens its properties.
- **Toggle Temporary Line Breakpoint** (`Ctrl+Alt+Shift+F8`, macOS `Cmd+Alt+Shift+F8`) adds a
  breakpoint that is removed once hit.
- **PHP Exception Breakpoints** (**+** in the dialog) stop when an exception of the class (or
  a subclass) is thrown; `*` stops on any exception. They have the same log options.

![Logpoint properties](../../screenshots/debugger-logpoint.png)

Logged lines appear in the **Console** tab of the debug session:

![Logpoint output](../../screenshots/debugger-logpoint-output.png)

## The Debug tool window

![Paused](../../screenshots/debugger-paused.png)

While paused, the execution point is highlighted in the editor and variable values are shown
inline at the end of lines (`php.debug.showInlineValues`). The **Debug** tool window (`Alt+5`)
has a tab per session with:

- **Threads & Variables**: the frames on the left (click to switch), variables on the right.
  Arrays and objects expand lazily.
- **Console**: the script's output, logpoint messages, and a field at the bottom that
  evaluates PHP expressions in the selected frame (see below).
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
| Set Value… | `F2` | `F2` |
| Toggle Temporary Line Breakpoint | `Ctrl+Alt+Shift+F8` | `Cmd+Alt+Shift+F8` |

![Evaluate Expression](../../screenshots/debugger-evaluate.png)

### Variables: Set Value and Copy Value

Right-click a variable or watch for **Set Value…** (`F2`), **Copy Value** (arrays and objects
as `print_r` output), **Copy Name**, **Evaluate Expression…** and **Add to Watches**. Set Value
edits the value in place: type a PHP expression (`'paid'`, `42`, `null`, `$other`) and press
`Enter` (Xdebug's `property_set`); `Escape` cancels. The variables and watches reload after the
change.

![Set Value](../../screenshots/debugger-set-value.png)

![Variable context menu](../../screenshots/debugger-variable-menu.png)

### Evaluate Expression and the debugger console

The **Evaluate** dialog (`Alt+F8`) and the field at the bottom of the **Console** tab evaluate a
PHP expression in the selected frame (Xdebug's `eval`). Both complete variable names after `$`
and properties after `->` (from the suspended script, `Enter` or `Tab` accepts) and keep a
history of evaluated expressions: `Up` / `Down` in the field, or the history button. Console
results are trees that expand like variables.

![Debugger console](../../screenshots/debugger-console.png)

Smart Step Into currently behaves like Step Into. Pause depends on the engine and may
report "Pause is not supported by this engine".

## Servers and path mappings

When PHP runs in a container or on another machine, its file paths differ from yours. Define a
**server** in Settings | Languages & Frameworks | PHP | **Servers**: **Name**, **Host**,
**Port**, **Debugger** (Xdebug) and **Use path mappings**: the project's folders as a tree,
with the **Absolute path on the server** of each mapped folder (usually just the project root,
e.g. `/var/www/html`; a deeper mapping such as `vendor` wins for its files).

![Servers](../../screenshots/debugger-servers.png)

A session finds its server from `PHP_IDE_CONFIG=serverName=<name>`, otherwise from
`$_SERVER['SERVER_NAME']` and `SERVER_PORT`. Servers are stored in
`<project>/.forge/php-servers.json`.

## Web Server Debug Validation

**Run | Web Server Debug Validation…** (PHPStorm's "Validate Debugger Configuration on Web
Server") checks whether PHP can connect to Forge:

- **Local Web Server or Shared Folder**: Forge writes a temporary script
  `forge_xdebug_validator_*.php` into the project folder that the web server serves (**Path to
  create validation script**, e.g. `public`), requests it under **URL to validation script**
  and deletes it right after the check. Nothing is written before you press **Validate**.
- **CLI Interpreter**: reads `php -i` of a [CLI interpreter](php.md#cli-interpreters) (also
  Docker, Compose, SSH or WSL ones).

Each check shows ✓ (fine), ⚠ (works with conditions) or ✗ (must be fixed) with a hint: the
Xdebug version, `xdebug.mode` (must include `debug`), `xdebug.client_host` (localhost cannot
reach Forge from a container or another machine), `xdebug.client_port` (must be Forge's
`php.debug.port`), `xdebug.start_with_request` (`trigger` needs the cookie or
`XDEBUG_SESSION_START`) and the server name used to find path mappings. For CLI interpreters,
Forge sets mode, host, port and session itself for debug runs, so only the extension matters.
The path and URL are remembered per project (`php.debug.validation.documentRoot`,
`php.debug.validation.url`).

![Web Server Debug Validation](../../screenshots/debugger-validation.png)

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
| Path to create validation script | `php.debug.validation.documentRoot` | `public` |
| URL to validation script | `php.debug.validation.url` | `http://localhost:8080` |

## Remote projects

In a [remote project](remote-development.md), **Start Listening** listens on a free local port
and asks the remote host, over SSH, to listen on `127.0.0.1:<php.debug.port>` (9003) and forward
connections to Forge (a reverse tunnel, re-established after reconnects). Configure Xdebug on
the host with `xdebug.client_host=127.0.0.1`. Xdebug's paths are the host's paths, which are the
project's, so no path mappings are needed. The status bar shows that the listener goes through
the host.

![Listening through the host](../../screenshots/remote-parity-debug-listening.png)

![Remote debug session](../../screenshots/remote-parity-debug-session.png)

> **Note:** JavaScript and Node.js debugging are not built in; see
> [WebForge](webforge.md#debugging-scripts).

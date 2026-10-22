---
title: Run/Debug configurations
description: Create and run Run/Debug configurations for PHP scripts, PHPUnit, Artisan, Composer, npm, shell scripts and Lua, the run widget, the Run tool window and the PHPUnit test tree.
order: 140
tags: [run, debug, configurations, phpunit, tests, npm]
---

# Run/Debug configurations

A **run configuration** describes how to start something: a script, a test suite, a command.
Forge's configurations work like PHPStorm's.

## Run widget

The run widget in the main toolbar shows the selected configuration with **Run** (▶) and
**Debug** (🐞) buttons. Its dropdown lists all configurations and has **Edit
Configurations…**; without configurations it reads **Add Configuration…**.

![Run widget](../../screenshots/run-toolbar-dropdown.png)

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Run the selected configuration | `Shift+F10` | `Ctrl+R` |
| Debug the selected configuration | `Shift+F9` | `Ctrl+D` |
| Run… (choose a configuration) | `Alt+Shift+F10` | `Ctrl+Alt+R` |
| Debug… (choose a configuration) | `Alt+Shift+F9` | `Ctrl+Alt+D` |
| Run Context Configuration (the test, script or Lua chunk at the caret) | `Ctrl+Shift+F10` | `Ctrl+Shift+R` |
| Debug Context Configuration | `Ctrl+Shift+F9` | `Ctrl+Shift+D` |
| Stop | `Ctrl+F2` | `Cmd+F2` |
| Rerun | `Ctrl+F5` | `Cmd+R` |
| Edit Configurations… | Find Action | |

In the **Run…** chooser, `Enter` runs and `Shift+Enter` (or holding `Shift`) debugs; it also
offers **Run Shell Command…** and **Edit Configuration…**.

## Run/Debug Configurations dialog

![Run/Debug Configurations](../../screenshots/run-configurations-dialog.png)

- **+** (`Alt+Insert`) adds a configuration from a type; copy, remove and reorder with the
  toolbar. **Edit configuration templates…** sets the defaults for new configurations of a
  type.
- Common fields: **Name**, **Store as project file**, the type's own fields, **Interpreter**
  and **Interpreter options** (PHP and Lua types), **Arguments**, **Working directory**
  (default: the project root), **Environment variables** and **Before launch** tasks (another
  run configuration or a shell command).
- The **Command Line** preview shows exactly what will run.

### Configuration types

| Type | Fields | Runs | Debug |
| --- | --- | --- | --- |
| PHP Script | File | `<php> <options> <file> <arguments>` | Xdebug |
| PHPUnit | Test scope: Directory, File, Class, Method or "Defined in the configuration file"; Configuration file (`phpunit.xml` detected); Test runner options | `phpunit --teamcity …` (from `vendor/bin`) | Xdebug |
| Artisan | Command (`migrate --seed`) | `<php> artisan <command>` | Xdebug |
| Composer Script | Script | `composer run <script>` with `php.composer.executable` | Xdebug |
| npm | package.json, Command (`run`, `install`, …), Scripts, Package manager | `npm`/`pnpm`/`yarn`/`bun` | sets `NODE_OPTIONS=--inspect` |
| Shell Script | Execute: Script file or Script text; Script path / text | the shell | — |
| Lua Script | File | `<lua> <options> <file> <arguments>`, LÖVE games as `love <dir>` | — |

The PHP interpreter defaults to the project's CLI interpreter (`php.interpreter`, Settings |
Languages & Frameworks | PHP), the Lua interpreter to `lua.interpreter`. The **Interpreter**
field of PHP Script, PHPUnit and Artisan configurations lists the project default, every
[CLI interpreter](php.md#cli-interpreters) and **Custom…** (a path to a local `php`):

![Interpreter of a run configuration](../../screenshots/php-interpreters-run-config.png)

A Docker, Docker Compose, SSH or WSL interpreter wraps the command line: `docker run --rm …`,
`docker compose run/exec …`, `ssh user@host …` or `wsl.exe -d … --exec php …`, with paths in
the arguments and the working directory mapped into the container or host, the environment
variables passed along (`-e NAME=value`, `env NAME=value`) and the interpreter's
configuration options as `-d name=value`. Paths in the output (stack traces, PHPUnit
failures) are mapped back to project files, so they stay clickable in the Run console.
Composer Script and Composer command configurations run `composer` through an interpreter
when their Interpreter field names one; otherwise they use `php.composer.executable` locally.

![Artisan through a Docker Compose interpreter](../../screenshots/php-interpreters-run.png)

Debugging (`Shift+F9`) through an interpreter works the same way: Xdebug connects to
`host.docker.internal` from containers (`docker run` adds
`--add-host=host.docker.internal:host-gateway`; Compose services on Linux need an
`extra_hosts: ["host.docker.internal:host-gateway"]` entry), SSH runs forward the debug port
back with `ssh -R 9003:127.0.0.1:9003`, and the run sets `PHP_IDE_CONFIG=serverName=<interpreter>`
so breakpoints use the interpreter's path mappings.

> **Note:** the types "PHP Built-in Web Server" and "PHP Remote Debug" are declared by the PHP
> plugins but not available yet. To debug a web request, use
> [Start Listening for PHP Debug Connections](debugging.md#listening-for-connections).

### Where configurations are stored

| | File |
| --- | --- |
| Shared ("Store as project file") | `<project>/.forge/runConfigurations.toml` — commit it to share with your team |
| Local configurations, the selected one, templates | `<project>/.forge/runConfigurations.local.toml` |

## Temporary configurations

Gutter ▶ icons create temporary configurations: PHPUnit test classes and methods, `package.json`
scripts, Lua main chunks, scratch files and HTTP requests. A double-click in the npm tool
window does the same.

## Run tool window

The **Run** tool window (`Alt+4`) has one tab per run. Tabs can be pinned and closed (`Ctrl+Shift+F4`).

- Output is shown with ANSI colors; `\r` progress lines update in place.
- File references (`app/User.php:42`, stack traces) and URLs are clickable.
- Toolbar: **Rerun**, **Stop**, **Soft-Wrap**, **Scroll to the End**, **Clear All**.
- Typing sends input to the process (stdin).
- Stop sends SIGTERM to the process group, then SIGKILL after 2 seconds (`taskkill /T` on
  Windows).

Settings | Editor | General | Console: **Use soft wraps in console** (`run.console.softWrap`)
and **Override console cycle buffer size (KB)** (`run.console.cycleBufferKb`, 1024): a console
keeps the newest output up to that size and drops the oldest lines.

## Analyze Stack Trace

**Code | Analyze Stack Trace or Thread Dump…** (`AnalyzeStacktraceOnError`, no default
shortcut) opens a dialog to paste a stack trace from a log file, a bug report, an error page or
a server. When the clipboard holds a trace, the dialog starts with it.

![Analyze Stack Trace dialog](../../screenshots/analyze-stack-trace.png)

**Analyze** opens it in a **Stack Trace** tab of the Run tool window. Every frame that names a
project file is a link to its line; the exception message is red, frames in libraries
(`vendor/`, `node_modules/`) and files outside the project are dimmed. **Rerun** opens the
dialog again with the same text.

![Stack trace in the Run tool window](../../screenshots/stack-trace-tab.png)

Recognized formats:

| Language | Frames |
| --- | --- |
| PHP | `#0 /var/www/app/User.php(12): …`, `… in /path/File.php:42`, `… on line 7`, Laravel and Symfony exception traces and logs, Xdebug's `0.0012 402456 1. {main}() /path/index.php:0` |
| JavaScript / Node | `at fn (/app/src/cart.ts:10:5)`, `at /app/src/x.js:3:1`, `file:///…`, `webpack:///./src/…` |
| Lua | `main.lua:12: attempt to …`, `lib/util.lua:4: in function 'load'` |
| Others | any `path:line[:column]` or `path(line)` |

A trace copied from a JSON log as one line with escaped line breaks (`\n#1 …`) is split into
lines first.

Paths of a server or container are mapped into the project:

1. the path mappings of the PHP servers (Settings | PHP | Servers,
   [Xdebug debugging](debugging.md)),
2. the bind mounts of the project's Docker Compose file (`./:/var/www/html`),
3. the project root,
4. otherwise the longest trailing part of the path that exists in the project
   (`/srv/app/releases/42/app/Models/Order.php` → `app/Models/Order.php`).

Files are looked up through the project's backend, so this works in
[remote projects](remote-development.md) too. Detecting traces copied to the clipboard outside
Forge automatically is not supported.

## Test results

Test runners that speak TeamCity service messages (PHPUnit with `--teamcity`, Pest, Jest)
show a **Test Results** tree next to the console:

![PHPUnit test tree](../../screenshots/run-phpunit-tree.png)

- Pass/fail/ignored per test with durations; **Show Passed**, **Show Ignored**, **Sort
  Alphabetically**, **Sort by Duration**, **Expand/Collapse All**.
- **Previous/Next Failed Test** and **Rerun Failed Tests**.
- A failed assertion shows a **Comparison Failure** with Expected and Actual.
- Double-click a test to open it.

## Debugging

**Debug** on a PHP configuration (PHP Script, PHPUnit, Artisan, Composer) starts Forge's Xdebug
listener and runs the process with the Xdebug environment (`XDEBUG_MODE=debug`,
`XDEBUG_SESSION`, `XDEBUG_CONFIG`). See [Xdebug debugging](debugging.md).

> **Note:** Run/Debug configurations work in [remote projects](remote-development.md): the
> process runs on the remote host.

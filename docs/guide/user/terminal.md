---
title: Terminal
description: The integrated terminal - tabs, shells, clickable file links, copy and paste, and terminal settings.
order: 160
tags: [terminal, shell, pty]
---

# Terminal

The **Terminal** tool window (`Alt+F12`, bottom stripe) runs your shell on a real pseudo
terminal (xterm.js in the UI, a PTY in Rust), so interactive programs such as `vim`, `htop`,
`git rebase -i` or `php artisan tinker` work.

![Terminal tabs](../../screenshots/terminal-tabs.png)

## Tabs

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Show/hide the terminal | `Alt+F12` | `Alt+F12` |
| New tab | `Ctrl+Shift+T` | `Cmd+T` |
| Close tab | `Ctrl+Shift+F4` | `Cmd+Shift+F4` |

Tabs are named "Local", "Local (2)", …; double-click a tab or use **Rename Session** to rename
it. A tab's context menu has **New Tab**, **Rename Session**, **Close Tab** and **Close Other
Tabs**. The terminal area's context menu has **Copy** (`Ctrl+Shift+C`), **Paste**
(`Ctrl+Shift+V`), **Select All** and **Clear Buffer**.

## Keyboard

While the terminal has focus, plain `Ctrl`+letter, `Alt`+letter and `Escape` go to the shell
(so `Ctrl+C`, `Ctrl+R`, `Alt+B` behave as in any terminal). Function keys, `Alt`+digit,
`Ctrl+Shift` and `Ctrl+Alt` combinations still reach the IDE, so `Alt+1` or `Shift+Escape`
work from the terminal.

## Links

URLs and file references such as `app/Models/User.php:42` in the output are clickable; files
open in the editor at that line.

## Shell and start directory

The shell is, in this order: Settings | Tools | Terminal | **Shell path**
(`terminal.shellPath`), `$SHELL` on Linux/macOS (then bash, zsh, sh), on Windows PowerShell 7
(`pwsh.exe`), Windows PowerShell or `%ComSpec%`. bash, zsh and fish start as login shells.

New tabs start in the project root unless **Start directory** (`terminal.startDirectory`) is
set; **Environment variables** (`terminal.environment`, one `NAME=value` per entry) are added.

## Settings | Tools | Terminal

| Option | Setting | Default | Scope |
| --- | --- | --- | --- |
| Shell path | `terminal.shellPath` | system default | user |
| Default tab name | `terminal.tabName` | `Local` | user |
| Audible bell (a short tone when a program rings the bell) | `terminal.audibleBell` | off | user |
| Copy to clipboard on selection | `terminal.copyOnSelection` | off | user |
| Shell integration | `terminal.shellIntegration` | on | user |
| Start directory | `terminal.startDirectory` | project root | user, project |
| Environment variables | `terminal.environment` | empty | user, project |

## Shell integration

With **Shell integration** on, bash and zsh tell the terminal their working directory (shown
in the tab's tooltip) and where each command starts and ends: finished commands get a mark in
the scrollbar, green when they succeeded and red when they failed. Your own startup files
still run: bash starts with an init file that sources `~/.bashrc` (or the profile files for a
login shell), zsh with a `ZDOTDIR` whose files source yours. The scripts live in
`<cache dir>/forge/shell-integration` of the machine the shell runs on (also on remote hosts).
fish, sh and PowerShell start without it; it applies to new sessions.

## Other terminals

The Services tool window opens terminals into Docker containers and Kubernetes pods
(`docker exec`, `kubectl exec`), and in a [remote project](remote-development.md) the terminal
runs on the remote host.

![Remote terminal](../../screenshots/remote-terminal.png)

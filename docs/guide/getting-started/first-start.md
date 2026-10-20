---
title: First start and the Welcome screen
description: What Forge shows on its first launch, the Welcome screen and its recent projects list.
order: 20
tags: [welcome, start, recent projects]
---

# First start and the Welcome screen

## Starting Forge

Start Forge from your application menu, or from a terminal:

```sh
forge                       # Welcome screen
forge ~/Projects/shop       # open a folder as the project
forge src/App.tsx:12        # open a file at line 12 (in the project it belongs to)
```

All launcher forms are listed in [CLI arguments and forge:// links](../reference/cli-and-links.md).
If Forge is already running, a second `forge …` call is handed to the running window instead
of starting another instance.

Without a project argument Forge shows the **Welcome screen**. It does not reopen the last
project automatically yet.

> **Note:** the setting "Reopen projects on startup" (`system.reopenLastProject`) is listed
> under Settings | Appearance & Behavior | System Settings but is not applied yet.

## The Welcome screen

![Welcome screen with recent projects](../../screenshots/welcome.png)

The left side has four entries, as in PHPStorm:

| Entry | What it does |
| --- | --- |
| **Projects** | Recent projects, with a search field and the buttons **New Project**, **Open** and **Get from VCS** |
| **Remote Development** | SSH connections and remote projects, see [Remote development](../user/remote-development.md) |
| **Customize** | opens the Settings dialog |
| **Plugins** | opens the Settings dialog (Settings | Plugins) |

The gear icon at the bottom opens Settings as well.

### Recent projects

Every project you open is added to the list. Each row shows a colored badge with the project's
initials, the name, the path and, on the right, the detected project type ("Laravel 12",
"Symfony 7", "React 19", "LÖVE 11", "PHP 8.3", …). Remote projects carry an **SSH** badge and
reconnect when you click them.

![Project type badges on the Welcome screen](../../screenshots/welcome-project-badges.png)

Type in **Search projects** to filter by name or path. Click a row to open the project.

The list is stored in `recent-projects.json` in Forge's config folder (see
[File locations](../reference/file-locations.md)).

### Empty state

On the very first start there are no recent projects, and the screen shows three large
buttons instead: **New Project**, **Open** and **Get from VCS**.

> **Note:** **Get from VCS** (cloning a repository) is not available yet; the button is
> disabled.

## Window title bar

Forge draws its own title bar. Its buttons look like your desktop's by default (Windows,
macOS traffic lights, GNOME or KDE Plasma). You can change the style, or switch back to the
operating system's own title bar, with Settings | Appearance & Behavior | Appearance |
**Window controls**. See [Settings and themes](../user/settings-and-themes.md#window-controls).

## What next

- [Open or create a project](projects.md)
- [Learn the workbench](../user/workbench.md)

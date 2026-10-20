---
title: Installation
description: Install Forge as an AppImage, deb or rpm package on Linux, or as an installer on Windows and macOS.
order: 10
tags: [install, appimage, linux, windows, macos]
---

# Installation

Forge is a single native application (the binary is called `forge`). It is distributed as the
bundles Tauri produces for each operating system:

| System | Bundle | File name (version 0.1.0) |
| --- | --- | --- |
| Any x86_64 Linux | AppImage | `Forge_0.1.0_amd64.AppImage` |
| Debian, Ubuntu, Mint, Pop!_OS | deb | `Forge_0.1.0_amd64.deb` |
| Fedora, openSUSE, RHEL | rpm | `Forge-0.1.0-1.x86_64.rpm` |
| Windows 10/11 | MSI or NSIS installer | `Forge_0.1.0_x64_en-US.msi`, `Forge_0.1.0_x64-setup.exe` |
| macOS 11+ | disk image | `Forge_0.1.0_aarch64.dmg` (Apple silicon) or `_x64.dmg` (Intel) |

> **Note:** Forge is at version 0.1.0 and the project does not publish signed downloads yet.
> If there is no ready-made bundle for your system, build one yourself with
> [Release builds](../developer/release-builds.md) (about six minutes on a recent machine).
> Cross-compiling is not supported: build Windows bundles on Windows and macOS bundles on a Mac.

## Linux

Forge uses the system WebView (WebKitGTK 4.1), so that library must be installed on every
Linux system, whatever the bundle type.

### AppImage on Arch Linux and EndeavourOS

```sh
sudo pacman -S --needed webkit2gtk-4.1 fuse2
chmod +x Forge_0.1.0_amd64.AppImage
./Forge_0.1.0_amd64.AppImage
```

`fuse2` lets the AppImage mount itself. If you would rather not install FUSE, run it with
`APPIMAGE_EXTRACT_AND_RUN=1` (see below).

To start Forge from your application launcher, move the AppImage to a stable place such as
`~/Applications/` and create a desktop entry, or use a helper like AppImageLauncher or Gear Lever.

### AppImage on Ubuntu and Debian

```sh
# Ubuntu 24.04 and newer
sudo apt install libwebkit2gtk-4.1-0 libfuse2t64
# Ubuntu 22.04, Debian 12
sudo apt install libwebkit2gtk-4.1-0 libfuse2

chmod +x Forge_0.1.0_amd64.AppImage
./Forge_0.1.0_amd64.AppImage
```

### Running the AppImage without FUSE

An AppImage normally mounts itself with FUSE. In containers, on systems without `fuse2`, or
when you see `dlopen(): error loading libfuse.so.2`, let it extract itself to a temporary
folder instead:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./Forge_0.1.0_amd64.AppImage
```

The start takes a little longer because the image is unpacked on every launch. You can also
unpack it once with `./Forge_0.1.0_amd64.AppImage --appimage-extract` and run
`squashfs-root/AppRun`.

### deb package

```sh
sudo apt install ./Forge_0.1.0_amd64.deb
```

`apt` pulls in WebKitGTK automatically. The package installs the `forge` command, so you can
start it from a terminal (see [command-line arguments](../reference/cli-and-links.md)).

### rpm package

```sh
sudo dnf install ./Forge-0.1.0-1.x86_64.rpm      # Fedora, RHEL
sudo zypper install ./Forge-0.1.0-1.x86_64.rpm   # openSUSE
```

### What else Forge uses on Linux

Forge works without any of these, but features light up when they are installed:

| Tool | Used for |
| --- | --- |
| `git` | Git integration (commit, diff, blame, branches, log) |
| `php` | running PHP scripts, PHPUnit, artisan, the Symfony console, PHP scratches |
| `composer` | Composer scripts, new Laravel/Symfony projects |
| Node.js + `npm` | npm scripts, installing npm-based language servers, new web projects |
| language servers | completion, hover and diagnostics, see [Language servers](../user/language-servers.md) |
| `docker`, `kubectl` | the Services tool window |
| a Secret Service keyring (GNOME Keyring, KWallet) | storing API keys, tokens and passwords, see [Troubleshooting](../troubleshooting.md#keychain-on-linux) |

## Windows

Run the `.msi` or the `-setup.exe` installer and start Forge from the Start menu. Forge uses
Microsoft Edge WebView2, which is part of Windows 10 (April 2018 and later) and Windows 11; the
installer adds it on older systems.

The terminal starts PowerShell 7 (`pwsh.exe`) when it is installed, else Windows PowerShell,
else the shell named by `%ComSpec%`, unless you choose another one in Settings | Tools |
Terminal.

## macOS

Open the `.dmg` and drag Forge to Applications. Because the app is not signed and notarized
yet, Gatekeeper refuses the first start. Either right-click Forge in Finder and choose
**Open**, or remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Forge.app
```

On macOS the default keymap is PHPStorm's macOS keymap (`Cmd` instead of `Ctrl`), and Settings
opens with `Cmd+,`.

## Updating and uninstalling

There is no built-in updater yet. Replace the AppImage or install the newer package over the
old one. Uninstalling the app leaves your settings and caches in place; see
[File locations](../reference/file-locations.md) for what to delete for a clean slate.

## Next step

[First start and the Welcome screen](first-start.md)

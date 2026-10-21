---
title: Troubleshooting
description: Fixes for common problems - AppImage and FUSE, WebKitGTK, language servers not found, remote agent upload, the keychain on Linux, Wayland, Docker and Git.
order: 910
tags: [troubleshooting, appimage, fuse, webkit2gtk, wayland, keychain]
---

# Troubleshooting

## Starting Forge

### AppImage: `dlopen(): error loading libfuse.so.2`

The AppImage needs FUSE 2 to mount itself.

```sh
sudo pacman -S fuse2                 # Arch, EndeavourOS
sudo apt install libfuse2t64         # Ubuntu 24.04+
sudo apt install libfuse2            # Ubuntu 22.04, Debian 12
```

Or run it without FUSE (slower start):

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./Forge_0.1.0_amd64.AppImage
```

The same applies inside containers, where FUSE is usually unavailable.

### `error while loading shared libraries: libwebkit2gtk-4.1.so.0`

Forge uses the system's WebKitGTK 4.1 (also for the AppImage):

```sh
sudo pacman -S webkit2gtk-4.1                 # Arch
sudo apt install libwebkit2gtk-4.1-0          # Debian/Ubuntu
sudo dnf install webkit2gtk4.1                # Fedora
```

Note the version: `webkit2gtk-4.0` / `libwebkit2gtk-4.0` is a different library.

### Blank or white window, or a crash right after start (Linux)

This is usually WebKitGTK's GPU path (often with NVIDIA drivers or under Wayland). Try, one at a
time:

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 forge
WEBKIT_DISABLE_COMPOSITING_MODE=1 forge
```

These are WebKitGTK variables, not Forge settings; if one helps, put it in your desktop entry.

### Wayland

Forge runs as a native Wayland client through GTK. If something misbehaves (window placement of
dialog windows, title bar dragging, rendering), try the X11 backend:

```sh
GDK_BACKEND=x11 forge
```

Forge draws its own title bar; if window dragging or the buttons do not fit your compositor,
choose Settings | Appearance & Behavior | Appearance | **Window controls** → **Native title
bar**.

### macOS: "Forge is damaged" or "cannot be opened"

The app is not signed yet. Right-click → **Open**, or
`xattr -dr com.apple.quarantine /Applications/Forge.app`.

### `forge` command not found

The deb/rpm packages install it. For the AppImage, link it into your `PATH`
(`ln -s …/Forge_0.1.0_amd64.AppImage ~/.local/bin/forge`).

## Language servers

### Language server not found

1. Check Settings | Languages & Frameworks | Language Servers: is the server **Installed**, and
   where was it found? Click **Detect Again** after installing.
2. Forge started from a desktop launcher may have a shorter `PATH` than your terminal (nvm,
   volta, fnm, `~/.local/bin` and Composer paths are often only set in shell profiles). Forge
   searches the usual places itself, but if it still fails, set the server's **Executable** to
   the full path, or start Forge from a terminal.
3. Servers you installed yourself and that run on Node need `node` on Forge's `PATH`; servers installed by Forge use its own Node.js.
4. Install with the **Install…** button: npm servers go to Forge's own folder
   (`~/.local/share/forge/langservers` on Linux) and are found there.
5. The server must be **Enabled**, its plugin active (e.g. `forge.vue` for Volar), and for
   ESLint, Tailwind and Angular the project must use them.

### Intelephense starts but premium features are missing

Enter your licence key (Settings | Languages & Frameworks | Language Servers or PHP →
**Enter Licence Key…**). The status must read "Premium (key …1234)". If the keychain is not
available, the key cannot be stored (see below).

### No completion in PHP at all

Check Settings | Languages & Frameworks | PHP | **Language server**: with "None (built-in index
only)" no server runs. With "Auto", install Intelephense (**Install Intelephense** on the PHP page) or
Phpactor.

## Keychain on Linux

API keys, tokens, passwords and licence keys are stored through the Secret Service API. You need
a running provider: GNOME Keyring (GNOME, most desktops) or KWallet with its Secret Service
integration (KDE Plasma 6 provides it; on older Plasma install `kwallet-pam`/configure KWallet,
or use GNOME Keyring). On minimal window managers start `gnome-keyring-daemon` in your session,
for example:

```sh
sudo pacman -S gnome-keyring libsecret
# then make sure the daemon runs in your session (PAM or your WM's autostart)
```

Symptoms of a missing keychain: saving an AI provider key, a code host token, an SSH password or
a database password fails with an error. Fallbacks exist for some secrets: the environment
variables `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `GITHUB_TOKEN`/`GH_TOKEN`,
`GITLAB_TOKEN`, `FORGEJO_TOKEN`/`GITEA_TOKEN`; for SSH use ssh-agent authentication.

## Remote development

### Remote agent upload

Forge uploads its agent to `~/.cache/forge-agent/<version>/` on the host. If connecting stops
at this step:

- **No agent for the host's architecture**: the build must bundle the agents (`pnpm release`)
  or you must provide them in `FORGE_REMOTE_AGENT_DIR` or an `agents/` folder next to the `forge`
  binary (`forge-remote-agent-x86_64-linux.gz`, `forge-remote-agent-aarch64-linux.gz`). See
  [Release builds](developer/release-builds.md#remote-agents).
- **The host is not Linux** (macOS, Windows, BSD): not supported.
- **`~/.cache` is not writable**, the disk is full, or the home directory is read-only: fix the
  permissions or free space.
- **The login shell prints text or asks questions**: output before `FORGE-AGENT-READY` is
  skipped, but interactive prompts in `.bashrc`/`.profile` block the start. Guard them with
  `[[ $- == *i* ]]`.
- **`noexec` home or cache**: the agent cannot run from `~/.cache`; ask your administrator.
- For details set `FORGE_AGENT_LOG=debug` on the host and run Forge with `RUST_LOG=debug`.

### Host key changed

Forge refuses to connect when the host key differs from `~/.ssh/known_hosts` and shows both
fingerprints. Confirm only if you know why it changed (reinstalled server).

### Feature says "not available in remote projects yet"

Expected for WebForge's npm tool window and component navigation, StyLua, code hosts, HTTP
request environments and project compose files in Services. See
[Remote development](user/remote-development.md).

### Language server missing in a remote project

Catalog servers and Intelephense must be installed **on the host** (the settings page refuses to
install for remote projects). Install them there, e.g. `npm install -g intelephense`, then
reopen the project.

## Git

- **Push or fetch hangs or fails with an SSH remote**: Forge runs git with
  `ssh -o BatchMode=yes` and cannot answer passphrase prompts. Add your key to ssh-agent
  (`ssh-add`) or use a key without passphrase, or an HTTPS remote with a credential helper.
- **Wrong git binary**: set `FORGE_GIT=/path/to/git`.

## Services (Docker)

- **"Docker: not connected"**: check that the daemon runs and that your user may use the socket
  (`docker ps` in a terminal; on Linux add yourself to the `docker` group). For Docker Desktop
  or rootless Docker, Forge looks at `~/.docker/run/docker.sock` and
  `$XDG_RUNTIME_DIR/docker.sock`; otherwise set the socket path in Settings | Build, Execution,
  Deployment | Docker and use **Test Connection**.
- **Docker over SSH fails**: the host key must be in `~/.ssh/known_hosts` (connect once with `ssh` to accept it) and your key must be in ssh-agent or `~/.ssh`; the Docker socket on the host must be readable by that user.

## Xdebug

- No connection: check `xdebug.mode=debug`, `xdebug.client_port` (9003, same as
  `php.debug.port`), `xdebug.client_host` (for Docker: `host.docker.internal`) and that Forge is
  listening (toolbar icon). A firewall may block port 9003.
- Breakpoints not hit in Docker: add a server with path mappings (Settings | Languages &
  Frameworks | PHP | Servers) and set `PHP_IDE_CONFIG=serverName=<name>`.

## Browser Companion

- The extension cannot find Forge: **Accept open requests from paired browsers** must be on;
  the port (63342 or the next nine) must be free; Firefox needs the extra permission for
  `127.0.0.1`.
- Pairing code rejected: codes expire after five minutes and work once; start pairing again.

## Resetting Forge

Quit Forge and delete its folders (see [File locations](reference/file-locations.md#clean-slate)).
Project files in `.forge/` are not touched by this.

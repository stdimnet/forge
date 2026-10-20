---
title: Remote development (SSH)
description: Open a project on a Linux server over SSH - connections, authentication, host keys, the Forge agent, what runs on the host, reconnects and current limitations.
order: 200
tags: [remote, ssh, agent, gateway]
---

# Remote development

Forge can open a project that lives on another machine, like JetBrains Gateway. It connects over
SSH, installs a small headless **Forge agent** on the host and runs indexing, the PHP language
server, Git, terminals and run configurations **next to the files**. Only the UI runs on your
machine.

## Requirements

| Side | Needs |
| --- | --- |
| Remote host | **Linux** on x86_64 or aarch64, an SSH server, a POSIX shell, write access to `~/.cache`. PHP, Git, Intelephense etc. installed on the host as you would need them locally. |
| Forge | a build that bundles the agents (`pnpm release`), or agent binaries in `FORGE_REMOTE_AGENT_DIR` / an `agents/` folder next to the `forge` binary. See [Release builds](../developer/release-builds.md). |

Nothing needs to be installed on the host in advance: Forge uploads the agent itself.

## Connecting

Welcome screen → **Remote Development** → **New Connection** (or **New SSH Connection…** in
Find Action).

![Remote Development on the Welcome screen](../../screenshots/remote-welcome.png)

**Step 1 – Connect to SSH**

![New SSH Connection](../../screenshots/remote-ssh-dialog.png)

| Field | Notes |
| --- | --- |
| SSH configuration | a saved configuration, a host from `~/.ssh/config` (with `Include`s), or **New connection** |
| Host, Port, Username | |
| Authentication type | **Password**, **Key pair (OpenSSH or PuTTY)** with an optional passphrase, or **OpenSSH config and authentication agent** (ssh-agent plus the `IdentityFile`s of `~/.ssh/config`) |
| Save password / passphrase | stored in the OS keychain (service `forge-ssh`), never in a file |

**Test Connection** shows the host's system and the latency. **Check Connection and Continue**
moves to step 2.

**Step 2 – Choose Project**: browse the host's folders (home, parent, show hidden files) and
double-click the project folder, or type its path, then **Open Project**.

![Choose the project folder](../../screenshots/remote-folder-chooser.png)

![Project step](../../screenshots/remote-project-step.png)

### Host keys

Host keys are checked against `~/.ssh/known_hosts`. An **Unknown Host** shows the key type and
fingerprint ("Are you sure you want to continue connecting?"); **Accept New Key** adds it to
`known_hosts`. A **changed** key is reported prominently ("Host Key Has Changed") and must be
confirmed explicitly before the stored key is replaced.

![Fingerprint](../../screenshots/remote-fingerprint.png)

## The Forge agent

After connecting, Forge probes the host with one shell command (OS, architecture, home folder,
installed agent), uploads the matching agent to `~/.cache/forge-agent/<version>/` when it is
missing or differs (checked by SHA-256), and starts it in a login shell. The agent then serves
the project over the SSH channel.

Set `FORGE_AGENT_LOG=debug` in the host's environment for verbose agent logs (stderr).

## Working in a remote project

![Remote project](../../screenshots/remote-workbench.png)

The project's root is shown as `user@host:/path`. Recent Projects keeps remote projects with an
**SSH** badge; clicking one reconnects.

![Recent remote projects](../../screenshots/remote-open-recent.png)

| Works on the host | |
| --- | --- |
| Project view, editing, saving, file operations, file watching | yes |
| Go to File, Search Everywhere | yes |
| Git (status, commit, diff, blame, branches, log) | yes; output is parsed locally |
| PHP index, Structure, Generate, Intelephense/Phpactor | yes; Intelephense indexes the remote files itself |
| LuaForge index | yes |
| Terminal | yes, a shell on the host |
| Run/Debug configurations (run) | yes, processes run on the host |

![Remote terminal](../../screenshots/remote-terminal.png)

> **Note:** not available in remote projects yet: Laravel, Symfony and Doctrine analysis,
> WebForge (npm tool window, component navigation), catalog language servers other than PHP's,
> StyLua formatting, database tools, the Xdebug listener, AI agent CLIs (Claude Code, Gemini
> CLI), code hosts (pull requests) and the Browser Companion. They report "not available in
> remote projects yet". Full remote parity is being worked on.

## Connection status and reconnects

The status bar shows the host and latency (pinged every 5 seconds). Click it for the
connection popup.

![Status popup](../../screenshots/remote-status-popup.png)

When the connection drops, Forge reconnects automatically with a backoff from 1 to 30 seconds
and re-attaches the project ("Reconnecting…", with the next attempt). Host key and
authentication errors stop the retries. **Reconnect to Remote Host** (Find Action) retries now.

![Reconnecting](../../screenshots/remote-reconnecting.png)

## Saved configurations

Settings | Tools | **SSH Configurations** lists, adds, edits and removes saved configurations.
They are stored in `ssh.toml` in Forge's config folder; secrets stay in the keychain.

![SSH Configurations](../../screenshots/remote-ssh-settings.png)

## Troubleshooting

See [Troubleshooting: remote agent upload](../troubleshooting.md#remote-agent-upload).

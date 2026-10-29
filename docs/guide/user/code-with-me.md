---
title: Code With Me
description: Invite guests into the open project and edit it together - starting a session, the host's approval with the verification code, permission levels, changing permissions, removing guests, the session log, files guests never see, joining and leaving.
order: 200
tags: [code with me, collaboration, pair programming, session, guest, host, permissions]
---

# Code With Me

Code With Me lets you invite guests into the project that is open in your Forge. Everyone edits
the same files at the same time and sees the others' carets and selections. Guests use Forge
itself. Connections go straight from guest to host; no account and no Forge service is needed
(see [relay and NAT traversal](code-with-me-relay.md) for networks where that does not work).

## Hosting a session

1. Click the Code With Me icon in the toolbar (or **Tools | Code With Me | Start Session…**).
2. Pick what guests may do:

   | Level | Guests can |
   | --- | --- |
   | **Read-only** (default) | view files |
   | **Edit files** | edit files, but not use the terminal or run code |
   | **Full access** | edit files, use the terminal and run code on your machine |
   | **Custom…** | what you choose per area (see below) |

3. **Start Session** starts listening and copies the invitation link to the clipboard. Send the
   link to your guest. **Copy Session Link** in the popup copies it again.
4. **End Session** ends the session for everybody and the link stops working.

Nothing listens on your machine before you start a session.

### Custom permissions

**Custom…** opens the **Custom Permissions** dialog with one value per area:

| Area | Values |
| --- | --- |
| Files | Read-only · Edit |
| Terminal | No access · Read-only · Full access |
| Run | No access · Read-only (see output) · Full access (start, stop, rerun) |
| Version control | Read-only (status, diff, blame, log) · Full access (commit, push, checkout, …) |

Files that control what runs on your machine (run configurations, `composer.json`,
`package.json`, `Makefile`, …) can only be changed by guests with Files **Edit** and Run
**Full access**.

![Custom Permissions dialog](../../screenshots/code-with-me-custom-permissions.png)

## During a session

The popup lists the participants with their permission and offers:

- **Copy Session Link** copies the invitation link again.
- **Regenerate Link** creates a new link and copies it. The old link stops working at once;
  guests who are already in the session stay.
- **Permissions ▸** changes the permission of the whole session: guests who join later get it,
  and every guest in the session gets it at once.
- **End Session** ends the session for everybody.

The **⋮** button next to a guest (or a right click on the guest) opens the guest's menu:

- **Permissions ▸** changes only this guest's permission (Read-only, Edit files, Full access or
  Custom…).
- **Remove Guest** disconnects the guest at once. The guest is told that the host removed them.
  Removing does not ban: with the same link the guest can ask to join again, so use
  **Regenerate Link** if they should not.

A new permission counts from the guest's next request. When files become read-only for a
guest, their open editors turn read-only and they are told so; changes they send afterwards are
refused by your Forge.

![A guest's menu in the session popup](../../screenshots/code-with-me-guest-menu.png)

## Session log

**Session Log…** in the popup (also in Find Action) shows what happened in the session you
host, with the time: who joined, declined and unanswered requests, connections without a
valid link, permission changes, Regenerate Link, removed and leaving guests, files guests
saved, created, renamed or deleted, their git actions, and every request your Forge refused
(shown as warnings). Terminal keystrokes and file contents are never logged.

With **Keep a session log** on (the default, Settings | Tools | Code With Me), each session is
also written to a file in Forge's data directory, never into the project:

| OS | Folder |
| --- | --- |
| Linux | `~/.local/share/forge/code-with-me/` |
| macOS | `~/Library/Application Support/forge/code-with-me/` |
| Windows | `%APPDATA%\forge\code-with-me\` |

The file is `<date>-<session>.jsonl`, one JSON object per line. Logs older than 30 days are
deleted when you start a session.

![Session Log](../../screenshots/code-with-me-session-log.png)

## Accepting guests

Having the link is not enough. When a guest opens it, a balloon appears and stays until you
answer:

> **Alex wants to join the session**
> Make sure the code is 482 913. With Read-only permission Alex can view files.
> **Accept** · **Decline**

The guest sees the same **verification code** while waiting. Compare the two over a call or
chat before you accept: the code is derived from both sides' keys, so somebody who got hold of
the link shows a different code than the person you invited. If the codes differ, decline.

- A guest that is declined is told so. A request that is not answered within two minutes is
  dropped and the guest is told that the host did not answer.
- A guest can have one open request at a time, and at most five requests wait at once. Further
  ones are refused without a prompt.
- Ending the session refuses everyone who is still waiting.

If you close the balloon without answering, the request stays open until it times out.

## Joining a session

1. Click the link (`forge://cwm/join?…`), or choose **Tools | Code With Me | Join Session…**
   and paste it.
2. Forge shows **Waiting for <host> to accept…** with the verification code and **Cancel**.
3. When the host accepts, the host's project opens in your window. The title shows
   `<project> — <host> (Code With Me)`. **Leave Session** in the popup or the status item
   ends it for you.

Guests see the Project tree, Structure, Find, Problems and Git (read-only unless the host gave
Full access). AI Chat, Database, Services, Debug and the code host tool windows are not
available in a guest's window, because they would use the host's accounts and secrets.

## Files guests never see

Secret files are invisible to guests whatever their level, in the tree, in search, in Git and
in analysis results: `.env` and `.env.*` (but not `.env.example`), private keys, certificates
and the like. The list is **Settings | Tools | Code With Me | Files guests never see**. Files larger than
**Larger files open read-only (KB)** open read-only. Files that configure what runs on the host
(`composer.json`, `package.json`, `Makefile`, `.idea/`, run configurations) can be changed only
with Full access.

## Settings

Settings | Tools | Code With Me: your name in sessions, the default permission level,
whether ending a session asks for confirmation, whether a session log is kept, the files guests never see and the connection
settings of the [relay page](code-with-me-relay.md).

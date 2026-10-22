---
title: Code With Me
description: Invite guests into the open project and edit it together - starting a session, the host's approval with the verification code, permission levels, files guests never see, joining and leaving.
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

3. **Start Session** starts listening and copies the invitation link to the clipboard. Send the
   link to your guest. **Copy Session Link** in the popup copies it again.
4. **End Session** ends the session for everybody and the link stops working.

Nothing listens on your machine before you start a session.

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
whether ending a session asks for confirmation, the files guests never see and the connection
settings of the [relay page](code-with-me-relay.md).

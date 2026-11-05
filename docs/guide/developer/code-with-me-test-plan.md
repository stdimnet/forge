# Code With Me: manual desktop test plan

The automated tests (`crates/forge-collab/tests/`, the vitest suites and the browser-mode
screenshot scenarios) cover the protocol, permissions and UI states. This plan covers what only
real desktop builds show: windows, deep links, the clipboard, file associations, the network
and the platform webviews. Run it on **Linux, Windows and macOS** before a release that changes
Code With Me, with a release build of the same commit on every machine.

Copy the checklist into the pull request or release issue and tick it per platform.

## Setup

- Two machines on one LAN (A = host, B = guest). For the relay part: a VPS with a self-hosted
  `iroh-relay` (see [the relay guide](../user/code-with-me-relay.md)) and a host behind NAT.
- A test project on A: a git repository with a few PHP files, a `.env`, a `composer.json`,
  a text file larger than **Larger files open read-only** (default 5 MB, e.g. a generated
  `storage/logs/laravel.log`), a binary file (an image) and a file over 16 MB.
- Optional: an SSH remote project on A (Remote Development) to host from.

## Checklist

### Start and join

- [ ] A: Start Session (toolbar icon) with each level: Read-only, Edit files, Full access,
      Custom. The link is copied; "Copy Session Link" works.
- [ ] B: paste the link into **Join Session…** on the Welcome screen and from an open project.
- [ ] B: open the `forge://cwm/join?…` link from a browser or chat app (deep link), with Forge
      closed and with Forge running. The Join Session dialog opens filled in.
- [ ] Both sides show the same verification code; A accepts in the balloon. Decline and the
      110 s timeout are shown on B.
- [ ] B with unsaved changes in its own project: Join asks Save / Don't Save / Cancel.
- [ ] B's window title is `<project> — <host> (Code With Me)`; the project tree, Go to File,
      Search Everywhere and Find in Files show A's project without `.env`.

### Editing

- [ ] Typing on both sides converges; carets and selections show in the participant's color.
- [ ] Undo on each side reverts only its own edits.
- [ ] Save on either side writes A's file; both tabs show the same unsaved marker.
- [ ] A: the gutter marks the lines B changed until the file is saved.
- [ ] A changes a shared file on disk (`git checkout`, external editor): without unsaved
      changes everybody takes the new text; with unsaved changes A gets the File Cache
      Conflict dialog and B a balloon.
- [ ] B opens the large text file: read-only, with "This file is too large to edit together".
      A edits and saves it; B follows the new text.
- [ ] B opens the image: read-only, "Binary file (…)". The file over 16 MB is refused.
- [ ] Read-only guest: the editor is read-only; Edit files guest: `composer.json` is read-only.
- [ ] B: PHP hover, completion, Ctrl+B and diagnostics answered by A; git gutter and blame in
      B's tabs; the Commit window shows A's changes (Full access to version control only).

### Follow, terminal, run

- [ ] Follow the host, Jump To, Make Everyone Follow You; typing stops following.
- [ ] A shares a terminal: B sees it (Read-only) or types into it (Full access).
- [ ] A starts a run configuration; B sees the output. B starts a run with Full access and A is
      asked first.

### Connection

- [ ] Unplug B's network for 30 s: B shows "Reconnecting…", A shows B as reconnecting; after
      reconnecting B's tabs, carets and unsaved edits are still there.
- [ ] Stay connected for 10 minutes with a few files open and following on: no reconnects
      (check A's Session Log tab: no "connection lost" entries).
- [ ] Via the self-hosted relay with A behind NAT: join works, the status bar says "Relay".
- [ ] A ends the session: B gets "The host ended the session." and its own window state back.
- [ ] Permissions changed during the session (session and per guest), Remove Guest and
      Regenerate Link take effect at once; the Session Log records them.

### Platform specifics

- [ ] **Linux** (AppImage, deb or rpm; WebKitGTK): clipboard for the link, deep link via the
      `.desktop` `x-scheme-handler/forge`.
- [ ] **Windows** (NSIS or MSI; WebView2): the `forge://` URL protocol from a browser, the
      firewall prompt for the first session, a path with spaces and non-ASCII characters.
- [ ] **macOS** (dmg; WKWebView): `forge://` links via `CFBundleURLTypes`, the local network
      permission prompt, Cmd+Q and Cmd+W in the guest window.

## Results

| Date | Build | Platform | Result |
| --- | --- | --- | --- |
| 2026-11-05/06 | `dev/charming-ritchie-qqs4fk` release build, Xvfb + Openbox, host and guest on one machine | Linux | **Passed:** start and join, the same verification code on both sides, accept, co-editing with gutter marks and unsaved markers, undo, follow (both ways), the large file and the image read-only for the guest, PHP features and git on the guest's shared tabs, no reconnects for several minutes. **Found and fixed on the way:** guest panics from backend calls on a runtime worker; guest features sending `cwm://` tab paths to the host (~1,300 refusals a minute); the guest re-adopting the host's project on every status event; one oversized guest message (a call carrying a 6 MB editor text) ending the connection, which then looped (reconnect every ~1.5 s). **Open:** after End Session the guest window kept the host's project; the guest's status popup shows "Read-only" with Full access. **Not run:** deep links and the clipboard across machines, the relay. |
| | | Windows | not run |
| | | macOS | not run |

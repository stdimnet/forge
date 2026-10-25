---
title: forge-deploy
description: Deployment - SFTP, FTP/FTPS and local servers, path mappings and exclusions, upload, download, sync, delete and compare with the deployed files.
order: 68
tags: [crate, rust, deployment, sftp, ftp]
---

# forge-deploy

`crates/forge-deploy` — Deployment (Tools | Deployment): servers, path mappings, transfers and
comparing with the deployed files.

## Purpose

The logic behind [Deployment](../../user/deployment.md).

Pure modules (also built for WebAssembly with `default-features = false`):

- `config`: `DeployServer` (`kind`: `sftp | ftp | ftps | local`, the SSH configuration id,
  FTP host / port / user / anonymous / passive, `root_path`, `web_url`, `mappings`,
  `excluded`) and `DeploymentConfig { default_server, servers }`, stored as
  `.forge/deployment.toml` (`PROJECT_FILE`, `parse`, `to_toml`, `upsert` with ids from the
  name, `remove` moving the default). `validate()` lists the form's problems.
- `mapping`: `DeployServer::resolve(local)` → `Resolution::Mapped(deployment path) |
  NotMapped | Excluded` (the longest local path wins; no mappings = root to root; `..` is
  never mapped), `local_path(deployment)` back, `web_url_of`, `display_path` (root path
  included), `is_excluded(side, path)`, and `ExcludeNames` ("Exclude items by name" with `*`
  and `?`).
- `options`: `DeployOptions` (`AutoUpload::{Always, Explicit, Never}`, overwrite up-to-date
  files, preserve timestamps, delete target items, create empty directories,
  `WarnNewer`, `CompareBy`).
- `compare`: `Entry` (path, dir, size, modified, digest), `compare(local, remote, by)` on top
  of `forge_diff::compare_listings` (the digest carries content, nothing or the timestamp),
  `is_up_to_date` and `is_newer_on_target`.

With `client` (the default):

- `fs::ServerFs`: `stat`, `list`, `read`, `write`, `mkdir`, `remove_file`, `remove_dir`,
  `set_modified`, `describe`, plus `mkdir_all` / `remove_all`; paths are relative to the
  server's root path.
- `local::LocalFolder` (local servers and the project side of every transfer),
  `ftp::FtpFs` (suppaftp, MIT OR Apache-2.0; FTPS is explicit TLS with rustls and
  `webpki-roots`; `MLSD` with a `LIST` fallback, `MFMT` for timestamps, a listing cache per
  session) and `sftp::SftpFs` (russh-sftp, Apache-2.0, on a `forge_remote::session::SshSession`
  with its own one-thread runtime; `SshSession::open_subsystem("sftp")`).
- `transfer::Deployer` (`new(project, server_fs, server, options)`, `skip_local`,
  `cancel`, `on_progress`): `transfer(Upload | Download, paths, overwrite_newer)`,
  `delete_remote`, `read_remote`, `sync_compare(folder)`; results are `Summary` with a
  `FileResult` per file (`Transferred`, `UpToDate`, `NewerOnTarget`, `Excluded`,
  `NotMapped`, `Deleted`, `Failed`).
- `connect::open(server, secret, project)` opens any server type (`ssh_params` builds the
  SSH parameters of a saved configuration with its keychain secret); `connect::test` is Test
  Connection.
- `secrets`: FTP passwords in the keychain (service `forge-deploy`, account
  `ftp://user@host:port`).

## IPC

`app/src-tauri/src/commands/deployment.rs`: `deploy_config`, `deploy_server_save({ server,
secret? })`, `deploy_server_delete({ id })`, `deploy_set_default({ id })`,
`deploy_validate({ server })`, `deploy_test({ server, secret? })`, `deploy_transfer({ id,
serverId?, direction, paths, overwriteNewer? })`, `deploy_delete({ id, serverId?, paths })`,
`deploy_cancel({ id })`, `deploy_auto_upload({ id, paths, explicit })`,
`deploy_sync_compare({ serverId?, path })`, `deploy_read_remote({ serverId?, path })`; events
`deploy://transfer` (`started`, `file`, `finished`). Browser mode: `app/ui/src/mock/deploymentMock.ts`
with `crates/forge-wasm/src/deploy.rs` (`deploy_validate`, `deploy_resolve`,
`deploy_compare`).

Transfers never run project code, so they work in Safe Mode; `deploy_auto_upload` does
nothing in Safe Mode.

## Dependencies

`forge-diff`, `serde`, `toml`; with `client`: `forge-remote`, `russh-sftp`, `suppaftp`
(`rustls-ring`), `webpki-roots`, `tokio`, `keyring`, `filetime`, `dirs`.

## Tests

Unit tests for the configuration, mappings, exclusions, options, comparison, MFMT timestamps,
local folders and keychain accounts. `tests/servers.rs` runs the whole feature set (upload with
mappings and exclusions, up-to-date skips, newer files, Sync with Deployed, download,
Compare with Deployed Version, delete target items, delete on server, cancel) against a local
folder, an in-process FTP server (`tests/common/ftp.rs`, also without `MLSD`/`MFMT`, wrong
passwords, refused TLS) and an in-process SSH server with the SFTP subsystem
(`tests/common/sftp.rs`, host key confirmation). No network beyond 127.0.0.1.

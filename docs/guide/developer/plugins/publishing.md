---
title: Publishing plugins
description: The .forgeplugin package, forge plugin validate and pack for CI and marketplace repositories, the marketplace index format (version 1) with SHA-256 per artifact and optional Ed25519 signatures, update checks, Forge's marketplace (stdimnet/marketplace), hosting an index and private marketplaces with an access token.
order: 40
tags: [plugins, sdk, marketplace, publishing, index, sha256, signature, token, private, validate, cli]
---

# Publishing plugins

## The package

A `.forgeplugin` file is a zip archive with `forge-plugin.json` at its root and the files it
names. `pnpm plugin pack <folder>` (or `npm run pack` in a scaffolded project) writes
`<id>-<version>.forgeplugin` with the manifest, `dist/`, the folder of each icon theme, and
`README.md`, `LICENSE`, `CHANGELOG.md`, `THIRD_PARTY_NOTICES.md` and `NOTICE` when present (so
the attribution of bundled third-party material travels with the package), and prints its
SHA-256.

Forge refuses packages that are larger than 64 MB (packed or unpacked), have more than 4096
files, contain absolute paths, `..`, drive letters or symbolic links, lack the manifest or the
`main` file, or whose manifest does not validate for the running Forge version.
`forge plugin validate` runs the same checks before you publish (see below), and
`forge plugin pack` packs the same files in Rust.

## Validating and packing in CI

The Forge binary has both steps built in, with Forge's own rules, so a plugin or marketplace
repository does not need its own copy of them:

```sh
forge plugin validate plugins/acme-icons --strict --json > report.json
forge plugin pack plugins/acme-icons -o packages/acme.icons/acme.icons-1.0.0.forgeplugin
```

`validate` exits with 0 (valid), 1 (errors, or warnings with `--strict`) or 2 (wrong
arguments); `--json` prints a stable report with the package's size and SHA-256, ready for a
marketplace index. `--forge-version 0.1.0` checks `engines.forge` against another Forge
version than the one running. `pack` is reproducible and validates before it writes. Both
run headless, also on CI machines without a display. See
[CLI](../../reference/cli-and-links.md#forge-plugin-validate).

A marketplace repository that reimplements the rules in Node.js, like `stdimnet/marketplace`
(`scripts/lib/forge-rules.mjs`, `scripts/lib/zip.mjs`, the packing in `scripts/lib/plugins.mjs`),
can switch step by step without changing published packages:

1. Run `forge plugin validate <plugin folder> --strict --json` in `build.mjs` and `check.mjs`
   instead of `validateManifest`, `loadTheme` and `validatePackage`, and fail on a non-zero
   exit code. Validate the committed packages the same way (`forge plugin validate
   packages/<id>/<file>.forgeplugin`). This replaces `forge-rules.mjs` and the
   `forge-proof.sh` build of Forge's crates.
2. Write new versions with `forge plugin pack <plugin folder> -o packages/<id>/<id>-<version>.forgeplugin`
   and take `size` and `sha256` for `index.json` from `--json`. Rust's deflate output differs
   from Node's, so a repacked version gets a new SHA-256: keep already published packages
   as they are (they never change) and use the binary for new versions only.
3. Keep repository-specific checks (listing extras, changelog dates, resolver expectations)
   in Node.js; they are not part of Forge's install rules.

In CI, download a Forge release for the runner, or build it once with
`cargo build -p forge-app` from a Forge checkout, and point `FORGE_BIN` at it for the SDK's
`validate.mjs`.

## Forge's marketplace

The default URL is Forge's marketplace, the
[`stdimnet/marketplace`](https://github.com/stdimnet/marketplace) repository:
`https://raw.githubusercontent.com/stdimnet/marketplace/main/index.json`. Its README explains
how to add a plugin or a version: the plugin sources go into `plugins/<folder>/`, and its
`scripts/build.mjs` validates them against Forge's rules, packs them reproducibly into
`packages/` and writes the index.

## The marketplace index (format 1)

A marketplace is a static JSON file anyone can host (a web server, object storage, GitHub
Pages, a GitHub repository). Forge reads it from the URL in `plugins.marketplace.url`.

```json
{
  "formatVersion": 1,
  "name": "Acme plugins",
  "plugins": [
    {
      "id": "acme.todo-sync",
      "name": "TODO Sync",
      "publisher": "Acme",
      "description": "Turns TODO comments into issues.",
      "homepage": "https://acme.dev/todo-sync",
      "license": "MIT",
      "tags": ["todo", "issues"],
      "downloads": 12400,
      "versions": [
        {
          "version": "2.3.0",
          "engines": { "forge": ">=0.1.0" },
          "url": "files/acme.todo-sync-2.3.0.forgeplugin",
          "size": 4211,
          "sha256": "9f2c…(64 hex digits)",
          "signature": "base64 Ed25519 signature of the package bytes (optional)",
          "permissions": { "network": ["api.acme.dev"], "fs": { "read": ["${project}"] } },
          "released": "2026-10-28",
          "changes": "Links issues back into the comments."
        }
      ]
    }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `formatVersion` | `1`. An index with a higher number is refused with "update Forge"; new optional fields may be added within format 1 |
| `plugins[].id`, `name` | as in the manifest |
| `versions[].url` | the package, absolute or relative to the index URL (`http`/`https` only) |
| `versions[].sha256` | required; the download must match it |
| `versions[].size` | optional; when set, the download must have exactly this size |
| `versions[].signature` | optional; required when the user configured trusted keys |
| `versions[].permissions` | must equal the package manifest's permissions; shown in the install prompt |
| `versions[].engines` | the newest version whose range contains the running Forge is offered |

Entries with an invalid id, version, range or checksum are skipped (and reported), so one
broken entry does not hide the whole marketplace.

## Updates

The Marketplace tab marks installed plugins with a newer compatible version (**Update**), and
⚙ | Check for Updates lists them. Updating installs the new package next to the old one and
swaps it in, keeping the plugin's enablement and data.

## Signatures

Checksums protect against broken or swapped downloads when the index itself is trusted
(served over HTTPS). To also protect against a compromised download host, sign each package
with Ed25519 and publish your public key: users add it (base64, 32 bytes) to
`plugins.marketplace.trustedKeys`. With at least one trusted key, Forge installs only packages
whose `signature` verifies with one of them.

## Private marketplaces

A marketplace that needs authentication (a private GitHub repository, a server behind a
login) takes an access token: ⚙ | **Marketplace Settings…** in Settings | Plugins has an
**Access token** field. Forge keeps the token in the OS keychain (service `forge-plugins`,
one entry per marketplace host), never in the settings, and sends it as
`Authorization: Bearer <token>` with the index and package requests:

- only to the index's own host and port, over HTTPS (plain HTTP only to `localhost`);
- for an index on GitHub (`raw.githubusercontent.com`, `github.com`, `api.github.com`), to
  these three hosts;
- never to other hosts the index links to, and not after a redirect to another host.

For a private GitHub repository use a fine-grained personal access token with read access to
its contents. Without a token GitHub answers 404 for private files, so the Marketplace tab
shows that error with **Retry** and a pointer to Marketplace Settings. A 401 or 403 says
whether the token is missing or was rejected.

## Testing an index locally

Serve a folder with the index and the packages, for example
`python3 -m http.server 8000`, and set the index URL in ⚙ | **Marketplace Settings…** to
`http://localhost:8000/index.json`. Forge's own tests use a local mock server the same way
(also for the access token: which host receives it, 401 and 404); no public marketplace is
involved.

---
title: Publishing plugins
description: The .forgeplugin package, the marketplace index format (version 1) with SHA-256 per artifact and optional Ed25519 signatures, update checks, and how to host an index.
order: 40
tags: [plugins, sdk, marketplace, publishing, index, sha256, signature]
---

# Publishing plugins

## The package

A `.forgeplugin` file is a zip archive with `forge-plugin.json` at its root and the files it
names. `pnpm plugin pack <folder>` (or `npm run pack` in a scaffolded project) writes
`<id>-<version>.forgeplugin` with the manifest, `dist/`, and `README.md`, `LICENSE`,
`CHANGELOG.md` when present, and prints its SHA-256.

Forge refuses packages that are larger than 64 MB (packed or unpacked), have more than 4096
files, contain absolute paths, `..`, drive letters or symbolic links, lack the manifest or the
`main` file, or whose manifest does not validate for the running Forge version.

## The marketplace index (format 1)

A marketplace is a static JSON file anyone can host (a web server, object storage, GitHub
Pages). Forge reads it from the URL in `plugins.marketplace.url`.

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

## Testing an index locally

Serve a folder with the index and the packages, for example
`python3 -m http.server 8000`, and set the Marketplace URL to
`http://localhost:8000/index.json`. Forge's own tests use a local mock server the same way;
no public marketplace is involved.

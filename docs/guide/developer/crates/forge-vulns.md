---
title: forge-vulns
description: "Vulnerable dependency check: a shared vulnerability model, the JSON reports of each ecosystem's audit tool, what to audit in a project, where dependencies are declared and the Upgrade quick fix (pure, also built to WebAssembly)."
order: 257
tags: [crate, rust, security, vulnerabilities]
---

# forge-vulns

`crates/forge-vulns` — Vulnerable dependency check: a shared vulnerability model, the JSON
reports of each ecosystem's audit tool (composer audit, npm / pnpm / yarn audit, pip-audit,
cargo audit, govulncheck, OWASP dependency-check), what to audit in a project, where
dependencies are declared and the Upgrade quick fix.

## Purpose

Everything the [Vulnerable dependencies](../../user/vulnerable-dependencies.md) UI shows is
computed here. Forge has no advisory database of its own: each ecosystem's tool does the
lookup and this crate turns its JSON into one model. The crate is pure (no processes, files or
network): the desktop app runs the tools through the project's backend
(`app/src-tauri/src/commands/vulnerabilities.rs`) and browser mode feeds recorded reports of
the demo projects through `forge-wasm`.

The ecosystem parsers it builds on are reused, not copied: `phpforge-composer` reads
`composer audit` and composer.json / composer.lock, `pyforge-lang-python` the requirements
files and pyproject.toml, `goforge-lang-go` go.mod, `webforge-lang-web` the package manager
of package.json and `forge-jsonschema` the positions in package.json.

## Modules

| Module | Summary |
| --- | --- |
| `model.rs` | `Vulnerability` (ecosystem, package, installed version, advisory id and aliases, severity, title, affected range, fixed version, URL, `via` direct dependencies, `reachable`), `Notice` (abandoned, unmaintained, yanked), `Findings`, `Severity` with its editor severity, version comparison and the fixed version of an affected range or patched list. |
| `cvss.rs` | CVSS 3.0 / 3.1 base scores from a vector, for advisories without a rating (cargo audit). |
| `parse/` | One module per report: `composer` (through `phpforge_composer::audit`), `npm` (npm 7+ `auditReportVersion: 2`, the npm 6 format of pnpm and Yarn 2/3, Yarn 1 NDJSON, Yarn 4 trees), `pip` (pip-audit), `cargo` (cargo audit with warnings), `go` (the govulncheck message stream), `depcheck` (dependency-check JSON with Maven coordinates from package URLs). |
| `tool.rs` | `Tool` (id, label, executable, arguments, install hint), `detect` (which audits a project's root needs, with the lock file, the files to annotate and why one is blocked) and `cache_key` (FNV-1a over the tool and its input files). |
| `declare.rs` | Declarations with name and constraint positions in composer.json, composer.lock, package.json, requirements files, pyproject.toml (PEP 621, dependency groups, Poetry), Cargo.toml (tables, inline tables, renames, workspaces), go.mod (plus `go` / `toolchain` for the standard library), pom.xml and Gradle scripts; `upgraded_constraint` keeps the constraint's style. |
| `problems.rs` | The Vulnerable Dependencies problems of a file (direct packages with `Upgrade`, transitive ones on the dependency that pulls them in, notices) and `upgrade` (the edited text). |
| `report.rs` | `TargetResult` (status ok / blocked / missing / failed), `from_output` (a run's stdout and stderr, tolerant of the tools' error exit codes), `fill_installed` (installed versions from composer.lock / package-lock.json) and `CacheEntry`. |
| `text.rs` | Byte offsets ↔ 0-based lines and UTF-16 columns. |

## Used by

- IPC: `vulnerabilities_check`, `vulnerabilities_problems`, `vulnerabilities_upgrade`,
  `vulnerabilities_install_command` (`app/src-tauri/src/commands/vulnerabilities.rs`; the
  results are cached in `<cache dir>/forge/vulnerabilities/<project>/`)
- UI: `app/ui/src/features/vulnerabilities/` (and Composer | Audit in `features/composer`)
- Browser mode: built into `forge-wasm` (`vulnerabilities.rs`), used by
  `app/ui/src/mock/vulnerabilitiesMock.ts`

## Dependencies

- Workspace crates: forge-jsonschema, phpforge-composer, pyforge-lang-python, goforge-lang-go,
  webforge-lang-web (without its `fs` feature)
- External: `serde`, `serde_json`

## Tests

Unit tests for every module, plus `tests/reports.rs` with output recorded from each tool in
`tests/fixtures/` (composer audit, npm audit with and without a lock file, pnpm audit, Yarn 1
and Yarn 4, pip-audit, cargo audit, govulncheck, dependency-check). No test runs a tool or
needs network.

```sh
cargo test -p forge-vulns
```

---
title: forge-fuzzy
description: "PHPStorm-style fuzzy matching and Search Everywhere ranking (pure, WebAssembly-friendly)."
order: 110
tags: [crate, rust]
---

# forge-fuzzy

`crates/forge-fuzzy` — PHPStorm-style fuzzy matching and Search Everywhere ranking (pure, WebAssembly-friendly).

## Purpose

PHPStorm-style fuzzy matching, shared by the backend ("Go to File" over the file system)
and the UI (Search Everywhere, Recent Files, popups) through IPC and WebAssembly.

Query characters must appear in order (case-insensitive). Matches at the start, after
separators (`/ _ - . : $` and spaces) and on camel humps score higher, as do runs of
consecutive characters, so `OrCo` ranks `OrderController` first. The crate is pure (no
file system or threads) so it compiles to WebAssembly for browser mode.

## Modules

| Module | Summary |
| --- | --- |
| `everywhere.rs` | Search Everywhere ranking: merges the candidates of all providers of a tab, scores them against the query, dedupes the "All" tab and cuts each group to its page size. |

## Main public items

```rust
pub struct FuzzyMatch { … }
pub struct MatchOptions { … }
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> { … }
pub fn fuzzy_match_with(query: &str, candidate: &str, opts: MatchOptions) -> Option<FuzzyMatch> { … }
pub struct FileMatch { … }
pub fn match_file(query: &str, path: &str) -> Option<FileMatch> { … }
pub fn rank_files<'a>(
pub enum FilterCandidate { … }
pub struct FilterRequest { … }
pub struct FilterHit { … }
pub fn filter(req: &FilterRequest) -> Vec<FilterHit> { … }
```

## Used by

- IPC: `fuzzy_filter`, `search_rank`, `fs_find_files`
- UI: `app/ui/src/features/search`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `serde`

## Tests

2 source files, about 805 lines; 16 unit tests.

```sh
cargo test -p forge-fuzzy
```

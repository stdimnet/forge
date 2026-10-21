---
title: Search benchmark
description: Measure the search index with one command and compare a change against the recorded baseline.
order: 146
tags: [search, index, benchmark, performance]
---

# Search benchmark

`crates/forge-search/examples/bench.rs` runs the same scenarios against any project folder and
prints a Markdown table. Run it before and after a change to the index, the matchers or Find in
Files, and keep a change only when the numbers support it.

```sh
cargo run --release -p forge-search --example bench -- <project folder>
```

Each scenario reports the best of three runs. The first build of the index depends on the disk
cache: run it twice and compare the second result.

## Baseline

Measured on a source tree (272,700 files, Windows 11, release build, files in the OS cache).

| Scenario | Best of 3 | Result |
| --- | --- | --- |
| Cold index | 5458 ms | 272700 files, 272700 added |
| Load saved index | 1287 ms | 272700 files, cache 283 MB |
| Refresh, nothing changed | 336 ms | 0 changed |
| Go to File `usr` | 7.5 ms | 100 hits |
| Go to File `ordctl` | 4.5 ms | 100 hits |
| Go to File `index.php` | 4.1 ms | 100 hits |
| Go to File `index`, substring | 11.1 ms | 100 hits |
| Classes `order` | 1.5 ms | 50 hits |
| Classes `user` | 1.3 ms | 50 hits |
| Find `return` | 248 ms | 508159 matches in 36900 files |
| Find `->get(` | 16.5 ms | 3325 matches in 1029 files |
| Find `function \w+\(` regex | 137 ms | 138491 matches in 15722 files |
| Find `.*` regex | 1078 ms | 13462140 matches in 168727 files |
| Find rare word `zzqxv` | 6.5 ms | 23 matches in 6 files |
| Find `.*` regex, cancelled after 100 ms | 116 ms | Search cancelled |

The first cold index after a rebuild or a reboot took 77 s on the same folder, because the
files were not in the OS cache. That time is disk speed, not the index.

## Tried and rejected

Measured with the benchmark above on the same tree.

| Change | Result | Why not |
| --- | --- | --- |
| Rarest trigram first with early stop | `->get(` 16.5 to 19.8 ms, `return` 248 to 249 ms | Choosing candidates is not where the time goes; reading and scanning files is |
| Skip lockfiles, `.min.js`, `.min.css`, `.map` | About 20 MB of 272,700 files | Too little to gain, and Find in Files would stop matching them |
| Identifier-run trigrams instead of byte trigrams | Cache 283 to 169 MB, load 1287 to 861 ms, but `->get(` 16 to 274 ms | Queries with punctuation lose most of their narrowing; a hybrid needs both indexes |

## Against ripgrep

ripgrep 15.1.0 on the same folder, same excludes (`--hidden --no-require-git`, case-insensitive, `-c`, best of 3, nothing else running). It reads every file on each run; the index reads only the candidates.

| Query | ripgrep | Forge index |
| --- | --- | --- |
| `return` | 46,228 ms | 248 ms |
| `->get(` | 57,746 ms | 16.5 ms |
| `function \w+\(` regex | 11,914 ms | 137 ms |
| `.*` regex | 56,040 ms | 1,078 ms |
| `zzqxv` | 24,074 ms | 6.5 ms |

The ripgrep times are dominated by reading 272,700 files on this machine (virus scanning included) and vary a lot between runs, so read them as a gap in magnitude, not as exact ratios. No query was faster with ripgrep. The index costs a one-off build (5.5 s warm, 283 MB on disk) and a refresh (340 ms) that ripgrep does not need.

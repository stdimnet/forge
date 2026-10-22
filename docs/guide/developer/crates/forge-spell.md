---
title: forge-spell
description: "Spelling and grammar inspections for comments, strings, identifiers and text: Hunspell dictionaries, word extraction, suggestions and a small rule-based English grammar checker (pure, WebAssembly-friendly)."
order: 235
tags: [crate, rust, spelling, grammar]
---

# forge-spell

`crates/forge-spell` — Spelling and grammar inspections for comments, strings, identifiers and text: Hunspell dictionaries, word extraction, suggestions and a small rule-based English grammar checker (pure, WebAssembly-friendly).

## Purpose

The logic behind the "Typo" and "Grammar" inspections ([Spelling and
grammar](../../user/spelling-and-grammar.md)). The editor collects the comments, strings,
declared names and prose of the changed or visible part of a file from its syntax tree
(`app/ui/src/features/spelling/segments.ts`) and sends them as `Segment`s (`{ from, text,
kind }`, UTF-16 document offsets); this crate answers the problems. All offsets are UTF-16.

- `hunspell`: a minimal reader for Hunspell `.aff` / `.dic` files: `PFX` / `SFX` rules with
  conditions and cross products, twofold suffixes, `FLAG` (`char`, `long`, `num`, `UTF-8`),
  `AF` aliases, `NEEDAFFIX`, `ONLYINCOMPOUND`, `FORBIDDENWORD`, `NOSUGGEST`, `REP` pairs and
  `SET` (UTF-8 or single-byte encodings). Every stem is expanded into its word forms;
  compounding is not supported.
- `dictionary`: sorted, lowercase word lists with a by-length index for suggestions. The
  bundled en_US dictionary (`data/en_US.aff`, `data/en_US.dic.gz`, SCOWL 2020.12.07,
  licence in `data/en_US.LICENSE`) is parsed once on first use; `data/programming.txt` holds
  programming terms; custom dictionaries come from `.dic` (+ `.aff`) bytes or word lists.
- `words`: word extraction per `Context` (`comment`, `docComment`, `string`, `identifier`,
  `text`, `commitMessage`, `fileName`): identifiers are split at camelCase humps, `_`, `-`,
  ALLCAPS runs and digits; URLs, e-mail addresses, paths, hex numbers and colors, UUIDs, code
  references (`::`, `->`, `()`), `$variables`, doc tags, inline code, escape sequences,
  acronyms in text, words with digits and words under three letters are skipped.
- `suggest`: a weighted Damerau-Levenshtein distance (neighbouring QWERTY keys, swapped
  letters, double letters, dropped vowels and vowel confusions are cheap) over the
  dictionary words of similar length, `REP` candidates, names and acronyms ranked behind
  common words, and the case of the typed word applied to the suggestions.
- `grammar`: Forge's own English rules: `repeated-word`, `a-an` (vowel sounds with
  exceptions such as "an hour", "a user", acronyms read letter by letter), `sentence-start`
  (doc comments) and `double-space`, each with a replacement fix.
- `lib.rs`: `Speller` (the dictionaries), `inspect` (typos and grammar problems of the
  segments, honouring `Options` and the user's words) and `context_actions` (the Alt+Enter
  entries: "Typo: Change to...", "Typo: Rename to...", "Save 'word' to project-level /
  application-level dictionary", grammar fixes; they run UI actions `SpellCheck.*` with
  `data`).

## Main public items

```rust
pub struct Speller { … }
impl Speller {
    pub fn new(custom: Vec<Arc<Dictionary>>) -> Speller;
    pub fn knows(&self, word: &str, user: &UserWords) -> bool;
    pub fn suggest(&self, word: &str, user: &UserWords, limit: usize) -> Vec<String>;
}
pub fn inspect(speller: &Speller, request: &InspectRequest) -> Vec<Problem> { … }
pub fn context_actions(speller: &Speller, request: &ActionsRequest) -> Vec<ContextAction> { … }
impl Dictionary {
    pub fn english() -> Arc<Dictionary>;
    pub fn from_hunspell(name: &str, aff: Option<&[u8]>, dic: &[u8]) -> Dictionary;
}
```

## Used by

- IPC: `spell_inspect`, `spell_context_actions`, `spell_dictionaries`
  (`app/src-tauri/src/commands/spell.rs`; custom dictionaries from
  `spelling.customDictionaries`, loaded once per list of paths).
- UI: `app/ui/src/features/spelling` (the editor inspection, Alt+Enter handlers, the commit
  message, Settings | Editor | Natural Languages).
- Browser mode: built into `forge-wasm` (`spell.rs`).

## Dependencies

- Workspace crates: [`forge-codeintel`](forge-codeintel.md) (`ContextAction`)
- External: `serde`, `serde_json`, `flate2` (MIT OR Apache-2.0, pure Rust `miniz_oxide`
  backend) to unpack the bundled dictionary

## Tests

Unit tests for the Hunspell reader (affixes, conditions, cross products, flag types, aliases,
special flags, Latin-1), the bundled dictionary, word extraction per context (identifier
splitting, skipped tokens, UTF-16 offsets), the distance and ranking, every grammar rule
with its exceptions, `inspect` with options and user words, and the context actions.

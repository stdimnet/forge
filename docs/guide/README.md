---
title: About this guide
description: How the Forge guide is organized and the content contract a documentation site generator can rely on.
order: 999
tags: [meta, contributing]
---

# About this guide

> **For agents and LLM tools:** the guide is also published in the
> [llmstxt.org](https://llmstxt.org) format. [`docs/llms.txt`](../llms.txt) is a hand-curated
> index of the most important pages with one-line descriptions; edit it when you add or rename
> an important page. [`docs/llms-full.txt`](../llms-full.txt) is every guide page as plain
> Markdown in reading order, without frontmatter. It is generated: never edit it, run
> `pnpm docs:llms` in `app/ui` (or `node docs/scripts/build-llms.mjs`) after changing any page.
> The script also checks the links in `llms.txt`; `--check` only verifies that both files are
> up to date.

`docs/guide/` is the complete user and developer documentation of Forge, written as plain
Markdown so any static site generator (or GitHub's own Markdown view) can render it. This file
describes the structure and the rules every page follows. It is meant for the people and tools
that build the documentation site, and for contributors who add or change pages.

## Folder structure

```
docs/guide/
  index.md                 start page of the guide
  README.md                this file (structure and content contract)
  faq.md                   frequently asked questions
  troubleshooting.md       known problems and their fixes
  getting-started/         install, first start, projects, switching from PHPStorm
  user/                    one page per feature, written for IDE users
  reference/               generated-style tables: keymap, settings, file types, servers, …
  developer/               contributor documentation
    crates/                one page per Rust crate in crates/ and packs/
  user/shortcuts/          shortcuts by topic and the keymap reference card
  reference/main-menu/     one page per top-level main menu

docs/llms.txt              curated index for LLM tools (hand-written)
docs/llms-full.txt         all guide pages in one file (generated)
docs/scripts/build-llms.mjs  regenerates llms-full.txt and checks llms.txt
```

Every folder has an `index.md` that introduces the section and links to its pages. Pages
inside a folder are sorted by their `order` value (see below), not by file name.

## Page format

- **CommonMark** plus **GitHub Flavored Markdown tables**. Nothing else is required to render
  a page correctly.
- **One page per file.** File names are lowercase, words separated by `-`, extension `.md`.
- **Fenced code blocks always name their language** (`sh`, `toml`, `json`, `php`, `ts`,
  `rust`, `http`, `text`, …).
- **Headings:** every page starts with exactly one `#` heading that repeats the `title`.
  Sections use `##` and `###`.
- **Links between pages are relative** and point at the `.md` file, for example
  `[Editor](../user/editor.md)` or `[keymap](keymap.md#navigate)`. Anchors are GitHub-style
  heading slugs.
- **Images** are the existing screenshots in `docs/screenshots/`, linked relatively, e.g.
  `![Commit tool window](../../screenshots/vcs-commit.png)`. They are 1440×900 PNGs captured
  from browser mode. Do not hot-link external images.
- **Notes and warnings** are ordinary blockquotes that start with a bold label, so they work
  without any admonition plugin:

  ```markdown
  > **Note:** not available in remote projects yet.

  > **Warning:** this deletes the folder and everything in it.
  ```

  Generators may turn `> **Note:**`, `> **Tip:**` and `> **Warning:**` into styled callouts.
- **Keyboard shortcuts** are written in code spans with PHPStorm's notation:
  `Ctrl+Shift+A`, `Alt+Insert`, `Double Shift`, chords as `Ctrl+K Ctrl+C`. When Windows/Linux
  and macOS differ, both are given: `Ctrl+B` (macOS `Cmd+B`).
- **Settings paths** use PHPStorm's notation with `|`: Settings | Languages & Frameworks | PHP.
  Setting keys are written in code spans: `php.interpreter`.
- **No generator-specific syntax:** no MDX/JSX, no shortcodes, no `{% %}` or `{{ }}` template
  tags in prose, no custom container syntax (`:::`), no HTML except `<br>` inside table cells.

## Frontmatter contract

Every `.md` file starts with a YAML frontmatter block:

```yaml
---
title: Git
description: Commit, diff, blame, branches and AI commit messages.
order: 70
tags: [vcs, git]
---
```

| Field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `title` | yes | string | Page title for navigation, the browser tab and search results. Matches the page's `#` heading. |
| `description` | yes | string | One sentence. Use it for meta descriptions, link previews and section listings. |
| `order` | yes | integer | Sort key within the page's folder (ascending). `index.md` uses `0`. Gaps are intentional so pages can be inserted. |
| `tags` | no | list of strings | Lowercase keywords for search and tag pages. |

No other fields are used. Generators should ignore unknown fields rather than fail.

## Sections and audiences

| Folder | Audience | Voice |
| --- | --- | --- |
| `getting-started/` | new users | step by step |
| `user/` | people using Forge as their IDE | PHPStorm wording: tool windows, actions, Settings paths |
| `reference/` | everybody | complete tables, little prose |
| `developer/` | contributors | how the code is organized and how to extend it |

## Keeping the guide accurate

The guide describes what is on the branch it lives on. When a feature is not finished, the
page says so with a `> **Note:**` instead of describing planned behaviour. Features that are
being built on other branches can be listed under "Coming soon" on the [start page](index.md); new
features are announced there under "Recently added".

Every user-visible change updates the matching page in `docs/guide/` (this rule is also in
`AGENTS.md`). The reference tables are derived from the sources below; when you change a
source, update the table:

| Page | Source of truth |
| --- | --- |
| [reference/keymap.md](reference/keymap.md) | `crates/forge-keymap/data/phpstorm.json`, `ACTION_ALIASES` in `crates/forge-keymap/src/catalog.rs`, `registerKeymapActions` calls in `app/ui/src/features/` |
| [reference/settings.md](reference/settings.md) | `[[contributions.settings]]` in `app/src-tauri/plugins/*.toml`, page ids from `registerSettingsPage` |
| [reference/file-types.md](reference/file-types.md) | `crates/forge-filetypes/src/builtin.rs`, `crates/forge-filetypes/src/data.rs`, `fileTypes` in the manifests |
| [reference/language-servers.md](reference/language-servers.md) | `crates/forge-langservers/catalog.toml`, `crates/forge-langservers/pack_servers.toml` |
| [reference/cli-and-links.md](reference/cli-and-links.md) | `crates/forge-open/src/cli.rs`, `crates/forge-open/src/request.rs` |
| [reference/file-locations.md](reference/file-locations.md) | `dirs::config_dir()` / `cache_dir()` / `data_dir()` call sites in `crates/`, `packs/` and `app/src-tauri/` |
| [reference/environment-variables.md](reference/environment-variables.md) | `std::env::var` call sites |
| [reference/main-menu/](reference/main-menu/index.md) | `crates/forge-keymap/src/menu.rs` (structure, labels) and the commands the UI registers (enabled state) |
| [user/shortcuts/](user/shortcuts/index.md) | `crates/forge-keymap/data/phpstorm.json`, the registered commands and the menu paths from `menu.rs`; screenshots from `app/ui/scripts/scenarios/docs.mjs` |
| [developer/crates/](developer/crates/index.md) | each crate's `lib.rs` documentation and `Cargo.toml` |

The developer pages summarize [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md), which remains the
detailed contract for IPC commands and contribution points.

---
title: Forge documentation
description: The complete guide to Forge, the PHPStorm-style IDE built with Rust and Tauri - getting started, every feature, reference tables and contributor docs.
order: 0
tags: [start, overview]
---

# Forge documentation

Forge is a fast, memory-safe IDE in the spirit of PHPStorm for Windows, Linux and macOS, built
with Rust + Tauri 2 and a SolidJS UI. Layout, wording and the default keymap follow PHPStorm 1:1;
the look is Forge's own ("Ember" dark, "Anvil" light).

![Forge workbench](../screenshots/workbench-dark.png)

Languages and large feature sets ship as **packs**:

| Pack | Focus |
| --- | --- |
| **PHPForge** | PHP, Composer, PHPUnit, Xdebug, Laravel, Symfony, Doctrine (DQL), WordPress, Drupal / Joomla / Shopware basics |
| **WebForge** | HTML, CSS, JavaScript/TypeScript, npm/pnpm/yarn/bun, React, Vue, Angular |
| **LuaForge** | Lua 5.1–5.4, LuaJIT, LÖVE, Neovim |
| **PyForge** | Python: interpreters (venv, uv, Poetry, conda, Pipenv, Hatch, Docker, SSH, WSL), Python Packages, requirements inspections, basedpyright / ty / ruff, Black |
| **JavaForge** | Java with Maven and Gradle: Eclipse JDT LS, JDKs and Temurin downloads, JUnit/TestNG, the Java debugger; Groovy (scripts, Spock), Scala (sbt, Metals, ScalaTest/MUnit) and Android (Device Manager, Logcat, Android App runs); Spring Boot and Jakarta EE |
| **KotlinForge** | Kotlin/JVM: kotlin-lsp, Kotlin runs and scripts, JUnit 5 and Kotest, the Kotlin debugger with coroutines, ktlint and detekt |

The core adds Git and code review (GitHub, GitLab, Forgejo/Gitea), Run/Debug configurations, a
terminal, database tools, Scratches with an HTTP client, Docker/Compose/Kubernetes, remote
development over SSH, language servers for about 30 more languages, AI (your own providers,
Claude Code and Gemini CLI as a native chat) and a browser companion.

## Where to start

| Section | For |
| --- | --- |
| [Getting started](getting-started/index.md) | installing Forge, the first start, opening and creating projects, switching from PHPStorm |
| [User guide](user/index.md) | one page per feature |
| [Reference](reference/index.md) | keymap, settings, file types, language servers, CLI, file locations, environment variables, plugins |
| [Developer guide](developer/index.md) | architecture, extending Forge, quality gates, releases, one page per crate |
| [FAQ](faq.md) | short answers |
| [Troubleshooting](troubleshooting.md) | AppImage/FUSE, WebKitGTK, language servers, remote agent, keychain, Wayland |

## Popular pages

- [Keymap reference](reference/keymap.md) — every shortcut on Windows/Linux and macOS
- [Language servers](user/language-servers.md) — install Intelephense and friends, licence keys
- [Laravel](user/laravel.md) and [Symfony](user/symfony.md)
- [WordPress](user/wordpress.md) and [Drupal, Joomla and Shopware](user/cms.md)
- [Debugging](user/debugging.md)
- [Remote development](user/remote-development.md)
- [AI](user/ai.md)

## Recently added

- **CForge**: C and C++ in the style of CLion: clangd with clang-tidy (from the PATH, a
  toolchain or a verified download), Settings | Build, Execution, Deployment | Toolchains (GCC,
  Clang, MSVC through vswhere, MinGW, cross compilers) and CMake (profiles and
  `CMakePresets.json`), the CMake tool window with targets and the configure log, compilation
  database projects, Build / Rebuild / Clean (`Ctrl+F9`) with build problems, CMake Application
  run configurations, GoogleTest, Catch2 and CTest with gutter icons and gcov / llvm-cov
  coverage, and LLDB / GDB debugging with STL renderers ([CForge](user/cpp.md)).
- **Jupyter notebooks**: `.ipynb` files open in the notebook editor with code, Markdown and
  raw cells, Jupyter's command and edit mode (`Shift+Enter`, `A`, `B`, `D D`, `M`, `Y`),
  outputs with colors, images, sanitized HTML, errors and JSON, DataFrame tables with sorting,
  filter and pages, kernels in the project's interpreter (ipykernel over ZeroMQ, installed with
  your consent) or on a Jupyter server, the Variables view with View as DataFrame, and notebook
  diffs cell by cell ([Jupyter notebooks](user/jupyter.md)).
- **Spring Boot and Jakarta EE** (JavaForge): beans and injection points with gutter
  navigation, the Endpoints tool window (Spring MVC and JAX-RS) with Open in HTTP Client, JPA
  entities, `application.properties` / `application.yml` completion, documentation and
  inspections from the `spring-configuration-metadata.json` of the dependency jars, and the
  Spring Boot run configuration with its run dashboard in Services (port, profiles, actuator
  health, mappings and beans) ([Spring Boot and Jakarta EE](user/spring.md)).
- **RustForge**: Settings | Languages & Frameworks | Rust with rustup toolchains, the active
  toolchain and its `rust-toolchain.toml`, the sysroot and the standard library sources; the
  Cargo tool window with members, targets, feature checkboxes and dependencies; Cargo and
  Cargo Test run configurations with ▶ on `fn main`, tests, test modules and doctests
  (libtest on stable and nightly, cargo-nextest, coverage with cargo-llvm-cov); Clippy
  problems with quick fixes, rustfmt, rust-analyzer settings and LLDB / GDB debugging with
  rustc's pretty printers ([RustForge](user/rust.md)).
- **GoForge**: Settings | Languages & Frameworks | Go with GOROOT detection and verified SDK
  downloads from go.dev, GOPATH, GOFLAGS, build tags and environment; go.mod / go.work
  checks and version completion from the module proxy; gofmt / goimports, go vet and
  golangci-lint problems; go generate gutter icons; Go Build and Go Test run configurations
  with subtests, benchmarks, fuzzing, coverage and profiles; Delve debugging (launch, test,
  attach, remote) and the pprof Profiler with a flame graph ([GoForge](user/go.md)).
- **PyForge**: the Python interpreter in the status bar with a popup to switch,
  Settings | Project | Python Interpreter with detection of venv, uv, Poetry, conda, Pipenv,
  Hatch and pyenv environments and new environments, Docker / Compose / SSH / WSL
  interpreters, the Python Packages tool window (pip, uv, Poetry, Pipenv; PyPI search and
  latest versions), requirements inspections with "Install requirement" and "Add requirement",
  basedpyright, ruff and ty (preview) with verified downloads, Reformat Code with ruff format
  or Black, and the Python run configuration ([PyForge](user/python.md)).
- **Deployment**: Settings | Build, Execution, Deployment | Deployment configures SFTP, FTP,
  FTPS and local-folder servers with root path, web server URL, path mappings and excluded
  paths; Tools | Deployment and the Project view upload, download, compare with the deployed
  version and sync folders, files upload automatically on save, and the File Transfer tool
  window shows every transfer ([Deployment](user/deployment.md)).
- **Windows, modes and layouts**: several project windows (Open Project asks New Window or
  This Window, Window | Next / Previous Project Window `Ctrl+Alt+]` / `Ctrl+Alt+[`),
  Presentation, Distraction-Free, Zen and Full Screen modes that restore the layout when you
  leave them, Zoom IDE (`appearance.zoom`), tool window view modes (Dock Unpinned, Undock,
  Float, Window), editor tabs in their own window (`Shift+F4` or drag a tab out) and named
  layouts in Window | Layouts ([Windows, modes and layouts](user/windows-and-layouts.md)).
- **Keymaps and color schemes**: PHPStorm's VS Code, Eclipse, Sublime Text and Emacs keymaps,
  your own keymaps (Duplicate, Derive), mouse shortcuts such as `Ctrl+Button1 Click`, and
  Settings | Editor | Color Scheme with Ember, Anvil and your own schemes, per-attribute
  editing with a live preview and IntelliJ `.icls` import and export
  ([Keymap](user/keymap.md), [Color schemes](user/color-schemes.md))
- **Project trust and Safe Mode**: an untrusted project asks "Trust and Open Project?"
  (Trust Project, Preview in Safe Mode, Don't Open, "Trust projects in …"); in Safe Mode the
  backend refuses scripts, run configurations, tests, the debugger, build tools, project-local
  language servers and external tools on save, the editor area shows a Safe Mode banner with
  "Trust Project…", and Settings | Build, Execution, Deployment | Trusted Locations lists the
  trusted folders ([Project trust and Safe Mode](user/project-trust.md)).
- **Composer**: composer.json gets PHPStorm's Composer toolbar (Install, Update, Validate,
  Show Outdated, Audit), outdated packages are marked in the gutter with an "Update package"
  quick fix, package names and version constraints complete from Packagist with documentation
  on hover, Tools | Composer | Add Dependency… opens Manage Composer Dependencies, and
  `composer audit` lists security advisories in the Problems tool window
  ([Composer](user/composer.md)).
- **Scopes, file colors, Structural Search and Analyze Stack Trace**: named scopes in
  PHPStorm's scope language for Find in Files, Replace in Files, Find Usages and Structural
  Search, file colors in editor tabs and the Project view
  ([Scopes and file colors](user/scopes-and-file-colors.md)); Search and Replace
  Structurally with `$variables$` templates for PHP, JavaScript and TypeScript
  ([Structural Search and Replace](user/structural-search.md)); Code | Analyze Stack Trace
  with frames linked to project files and container paths mapped
  ([Analyze Stack Trace](user/run-debug.md#analyze-stack-trace)).
- **Vim emulation**: Tools | Vim turns on IdeaVim-style Vim emulation in the editor: Normal,
  Insert, Visual (character, line, block), Replace and command-line modes, motions, operators
  and text objects, registers (also the system clipboard), dot repeat, macros, marks, search,
  `:s`, `:g`, `:normal` and other ex commands, `~/.ideavimrc` with mappings to IDE actions, and
  Settings | Editor | Vim to choose Vim or the IDE per conflicting shortcut
  ([Vim emulation](user/vim-emulation.md)).
- **Markdown preview, image editor and Edit as Table**: Markdown files open with a live,
  GitHub-flavored preview (tables, task lists, footnotes, alerts, Mermaid diagrams, project
  images, front matter, links that open files in Forge) and synchronized scrolling; images open
  in an image editor with zoom, grid and chessboard, SVG with a live preview; CSV/TSV files
  can be edited as a table that keeps the file's quoting
  ([Markdown, images and CSV](user/markdown-images-csv.md)).
- **File and code templates**: New ▸ PHP Class / Interface / Trait / Enum with the PSR-4
  namespace from composer.json, PHPUnit Test, Laravel and Symfony classes, HTML, CSS,
  JavaScript, TypeScript, Vue, React and Lua files, the `Alt+Insert` New popup in the Project
  view, and Settings | Editor | File and Code Templates with Velocity templates and the code
  templates of the Generate actions ([File and code templates](user/file-templates.md)).
- **Split editors, pinned tabs and the tab context menu**, a tab limit, **Recent Locations**
  (`Ctrl+Shift+E`), **Bookmarks** (`F11`, `Ctrl+F11`, `Alt+2`), the **TODO** and
  **Notifications** tool windows and Find in Files results in the **Find** tool window
  ([Workbench](user/workbench.md#editor-tabs)).
- **PHP navigation like PHPStorm**: `Ctrl+Click` on every PHP symbol (also `\Throwable` and
  `array_map`, opened read-only in the PHP stubs), Show Usages on declarations, Find Usages
  (`Alt+F7`) in the Find tool window, Implementation / Super Method / Type Declaration and the
  implemented / overridden gutter icons ([PHP](user/php.md#navigation)).
- **Splash screen and updates**: the wordmark fills with molten metal while Forge loads, the
  last project reopens, and signed updates are offered on startup or with Help | Check for
  Updates… ([First start](getting-started/first-start.md#the-splash-screen)).
- **Main menu**: PHPStorm's menus behind ☰, merged into the toolbar or as a menu bar
  ([Workbench](user/workbench.md#main-menu)).
- **Find in Files** (`Ctrl+Shift+F`) and the **Choose Declaration** popup
  ([Search Everywhere](user/search-everywhere.md#find-in-files), [Editor](user/editor.md#go-to-declaration)).
- **One-click language server installs**, including Intelephense with its own Node.js
  ([Language servers](user/language-servers.md#installing-a-server)).
- **HTTP client**: environment picker, response handlers, body files, dynamic variables
  ([Scratches and HTTP client](user/scratches-and-http.md#http-client-http-files)).
- **Services**: Kubernetes port forwarding, container picker, deployment logs, Docker over SSH
  and Docker contexts ([Services](user/services.md)).
- **Remote parity**: frameworks, language servers, databases, Xdebug and AI agents in remote
  projects, `ProxyJump`/`ProxyCommand` ([Remote development](user/remote-development.md)).
- **Dialog windows and OS-styled window controls**
  ([Settings and themes](user/settings-and-themes.md#window-controls)).

## Not available yet

Refactorings other than Rename (Extract, Move, ...), Get from VCS, multiple
project windows and a plugin Marketplace. The pages mark smaller gaps with notes; the
[roadmap](../ROADMAP.md) lists what comes next.

## About this guide

The guide is plain Markdown with a small frontmatter contract so any documentation generator can
render it; see [About this guide](README.md). Every user-visible change to Forge updates the
matching page.

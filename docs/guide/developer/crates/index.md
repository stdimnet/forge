---
title: Crates
description: One page per Rust crate in crates/ and packs/ - purpose, modules, public items, IPC, dependencies and tests.
order: 200
tags: [crates, rust]
---

# Crates

Every Rust crate of the workspace. The pages are generated from each crate's `lib.rs` documentation, module headers and `Cargo.toml`, plus the IPC commands in `app/src-tauri/src/commands/` that use it. The desktop shell itself is the crate `forge-app` in `app/src-tauri` (see [Architecture overview](../architecture.md)).

## Core crates (`crates/`)

| Crate | Description |
| --- | --- |
| [forge-agent-cli](forge-agent-cli.md) | Runs AI coding agent CLIs (Claude Code, Gemini CLI) headless and normalizes their output |
| [forge-ai](forge-ai.md) | Forge IDE AI provider layer: Anthropic and OpenAI-compatible chat models |
| [forge-backend](forge-backend.md) | Forge IDE project backends: the Backend trait, the local backend and the remote agent protocol |
| [forge-bookmarks](forge-bookmarks.md) | Bookmarks: lists, mnemonics and line tracking through edits (pure, WebAssembly-friendly) |
| [forge-codeintel](forge-codeintel.md) | Code intelligence models: workspace edits with LSP positions and Find Usages grouping (pure, WebAssembly-friendly) |
| [forge-codestyle](forge-codestyle.md) | Code style schemes (Settings \| Editor \| Code Style): options per language, presets, scheme storage and resolution, import/export and the formatting passes and previews (pure, WebAssembly-friendly) |
| [forge-colors](forge-colors.md) | Color values in code: finding CSS colors (hex, rgb, hsl, hwb, named, Tailwind) in CSS, markup and script files, parsing and formatting them in their notation (pure, WebAssembly-friendly) |
| [forge-colorscheme](forge-colorscheme.md) | Editor color schemes (Settings \| Editor \| Color Scheme): the Ember and Anvil schemes, user schemes stored in the settings, per-attribute text attributes, CSS for the editor, and IntelliJ .icls import/export (pure, WebAssembly-friendly) |
| [forge-containers](forge-containers.md) | Forge IDE containers: Docker Engine API, Docker Compose and Kubernetes for the Services tool window |
| [forge-core](forge-core.md) | Forge IDE core: workspace, file system, settings and event bus |
| [forge-devenv](forge-devenv.md) | Forge IDE Dev Containers and WSL: devcontainer.json, docker command lines, WSL distributions and path mapping, and the Forge agent over a process's stdio |
| [forge-diff](forge-diff.md) | Forge IDE comparisons: folder diff (Compare Directories), pure and WebAssembly-friendly |
| [forge-deploy](forge-deploy.md) | Forge IDE deployment: SFTP, FTP/FTPS and local servers, path mappings, upload, download, sync and compare with the deployed files |
| [forge-dap](forge-dap.md) | Forge IDE shared debugging layer: Debug Adapter Protocol client, adapter registry and native debugger views |
| [forge-db](forge-db.md) | Forge IDE database tools: data sources (SSH tunnels, SSL), drivers for nine engines, the Database Explorer (object tree, schema filter, virtual foreign keys, full-text search, DDL data sources), consoles with sessions, the read-only / production guard and data editing |
| [forge-dbmodel](forge-dbmodel.md) | Forge database explorer model (pure, also built to WebAssembly): object tree, schema filter, virtual foreign keys and columns, DDL parsing, full-text search queries and Quick Documentation |
| [forge-debug](forge-debug.md) | Forge IDE debugger: DBGp (Xdebug 3) server, path mappings and breakpoints |
| [forge-editing](forge-editing.md) | Editor actions computed in Rust: postfix completion templates, Complete Current Statement and Cyclic Expand Word (pure, WebAssembly-friendly) |
| [forge-emmet](forge-emmet.md) | Emmet: abbreviation parser and expander for HTML, JSX and CSS, Surround with Emmet, edit points, Update/Remove Tag (pure, WebAssembly-friendly) |
| [forge-file-templates](forge-file-templates.md) | File and code templates (Settings \| Editor \| File and Code Templates): a Velocity subset renderer, the built-in templates, PSR-4 namespace mapping and New-file rendering (pure, WebAssembly-friendly) |
| [forge-filetypes](forge-filetypes.md) | File type recognition for Forge: names, extensions, shebangs and template data languages (pure, WebAssembly-friendly) |
| [forge-find](forge-find.md) | Find in Files and TODO scanning: query compilation, file masks, line matching and comment patterns (pure, WebAssembly-friendly) |
| [forge-forges](forge-forges.md) | Forge IDE code hosts: GitLab, Forgejo/Gitea and GitHub merge requests, issues and pipelines |
| [forge-fuzzy](forge-fuzzy.md) | PHPStorm-style fuzzy matching and Search Everywhere ranking (pure, WebAssembly-friendly) |
| [forge-injections](forge-injections.md) | Language injections: SQL, HTML, CSS, JSON and RegExp inside PHP and JavaScript/TypeScript strings, `// language=` comments, fragment escaping and the injection intentions (pure, WebAssembly-friendly) |
| [forge-inspections](forge-inspections.md) | Forge's inspection framework: registry, severities, profiles (PHPStorm XML), suppressions, duplicates, the batch runner, Code Cleanup and report export (pure, also built to WebAssembly) |
| [forge-jsonschema](forge-jsonschema.md) | JSON Schema for JSON, JSON5/JSONC and YAML files: position-keeping parsers, a draft 4–2020-12 validator with PHPStorm's messages, completion, hover, Go to Declaration, schema mappings, bundled schemas and remote schema downloads |
| [forge-keymap](forge-keymap.md) | PHPStorm-compatible keymap: shortcut model, default keymaps, overrides, conflicts and the key resolver |
| [forge-langservers](forge-langservers.md) | Forge IDE: catalog, detection, configuration and installation of language servers |
| [forge-livetemplates](forge-livetemplates.md) | Live templates: template sets, contexts, variables and expansion, Surround With and Unwrap/Remove (pure, WebAssembly-friendly) |
| [forge-localhistory](forge-localhistory.md) | Local History: file contents recorded on save, refactorings, VCS operations and external changes, with labels, revert and selection history |
| [forge-lsp](forge-lsp.md) | Forge IDE: generic Language Server Protocol client and server manager |
| [forge-maintenance](forge-maintenance.md) | Repairing and resetting the IDE: Invalidate Caches, Restore Default Settings, Export / Import Settings |
| [forge-markdown](forge-markdown.md) | Forge Markdown: a safe AST (no raw HTML) for chat messages and documentation popups, built with pulldown-cmark (pure, also built to WebAssembly) |
| [forge-viewers](forge-viewers.md) | Forge file viewers: image metadata for the image editor and CSV/TSV tables for Edit as Table (pure, also built to WebAssembly) |
| [forge-open](forge-open.md) | Forge IDE open requests: forge:// links, the CLI launcher and the Browser Companion local server |
| [forge-output](forge-output.md) | Forge process output: ANSI styling, console lines, file links, the test event protocol with its parsers (TeamCity, JUnit, TAP, Go, libtest JSON and text, Jest/Vitest, pytest) and test trees (pure, also built to WebAssembly) |
| [forge-plugin-api](forge-plugin-api.md) | Stable plugin API for Forge language packs and feature plugins |
| [forge-ports](forge-ports.md) | Free local ports for servers Forge starts (debug adapters, JDWP, debugpy, Jupyter kernels), probed without listening and outside the ephemeral range |
| [forge-process](forge-process.md) | Forge IDE processes: PTY terminal sessions, run configurations and the process runner |
| [forge-project](forge-project.md) | Forge IDE projects: project type detection and New Project generators |
| [forge-remote](forge-remote.md) | Forge IDE remote development: SSH connections, host keys, ~/.ssh/config and the Forge agent deployment |
| [forge-remote-agent](forge-remote-agent.md) | Headless Forge agent: serves a project's files, git, processes, language servers and PHP index over stdio for remote development |
| [forge-runtargets](forge-runtargets.md) | Run targets (Run on: local machine, Docker image, Docker Compose service, SSH, WSL, Dev Container): path mappings and the command lines that run, test and debug there (pure, also built to WebAssembly) |
| [forge-runtools](forge-runtools.md) | Run Anything, compound and before-launch plans, External Tools and their macros (pure, also built to WebAssembly) |
| [forge-save-actions](forge-save-actions.md) | Actions on Save: which actions run for a saved file, reformatting only changed lines and running Biome, ESLint, Stylelint, Prettier, Rector, PHP CS Fixer, phpcbf and Laravel Pint through the project's backend |
| [forge-scratches](forge-scratches.md) | Forge scratch files (PHPStorm's Scratches and Consoles): storage, naming, languages, run commands and the HTTP request client |
| [forge-scopes](forge-scopes.md) | Scopes (PHPStorm's scope language, predefined and named scopes) and file colors (pure, WebAssembly-friendly) |
| [forge-spell](forge-spell.md) | Spelling and grammar inspections for comments, strings, identifiers and text: Hunspell dictionaries, word extraction, suggestions and a small rule-based English grammar checker (pure, WebAssembly-friendly) |
| [forge-ssr](forge-ssr.md) | Structural Search and Replace: templates with `$variables$` matched on tree-sitter syntax trees (PHP, JavaScript, TypeScript) |
| [forge-sql](forge-sql.md) | Forge console text: dialects, statement splitting, the read-only / production guard, literals, row changes and export (pure, also built to WebAssembly) |
| [forge-sqlcode](forge-sqlcode.md) | The SQL editor (pure, also built to WebAssembly): dialects per file, resolve against the data source model, SQL inspections and quick fixes, the formatter, user parameters, query history and Run SQL Script planning |
| [forge-testing](forge-testing.md) | The shared test runner and coverage model of every pack: test framework contributions (detection, command lines, gutter discovery, Rerun Failed Tests, location rules) and coverage reports (LCOV, Cobertura, Clover, JaCoCo, Go coverprofile) (pure, also built to WebAssembly) |
| [forge-trust](forge-trust.md) | Project trust and Safe Mode: trusted projects and locations, and what may run |
| [forge-update](forge-update.md) | Forge update checks: when to check, which release is an update, the update feed, simulated updates and download speed |
| [forge-vcs](forge-vcs.md) | Forge IDE version control: drives the git CLI and parses its output |
| [forge-vim](forge-vim.md) | Vim emulation (IdeaVim-style): modes, motions, operators, text objects, registers, ex commands and ~/.ideavimrc parsing (pure, built to WebAssembly) |
| [forge-wasm](forge-wasm.md) | WebAssembly build of Forge's pure Rust logic for the browser-mode UI |
| [forge-window](forge-window.md) | Forge window chrome: desktop detection, title bar button layouts and dialog window geometry (pure, also built to WebAssembly) |

## Pack crates (`packs/`)

| Crate | Description |
| --- | --- |
| [javaforge-build](javaforge-build.md) | JavaForge: Maven and Gradle builds (detection from the build files, wrappers, modules, lifecycle goals and tasks, JaCoCo, command lines for goals, tasks, Build Project and tests; pure, also built to WebAssembly) |
| [javaforge-fw-spring](javaforge-fw-spring.md) | JavaForge: Spring Boot and Jakarta EE (Java / Kotlin declaration scanner, beans and injection points, Spring MVC and JAX-RS endpoints, JPA entities, application.properties / application.yml assistance from spring-configuration-metadata.json and the dependency jars, the Spring Boot run configuration, actuator client and run dashboard; pure, also built to WebAssembly) |
| [javaforge-jdk](javaforge-jdk.md) | JavaForge: JDK detection (JAVA_HOME, PATH, system folders, Windows registry, SDKMAN, asdf, jabba, Gradle toolchains, ~/.jdks) and Eclipse Temurin downloads with SHA-256 verification |
| [javaforge-android](javaforge-android.md) | JavaForge: Android basics (Gradle Android modules, AndroidManifest.xml, the Android SDK location, adb devices and emulator AVDs, Android App run command lines, Logcat parsing, filtering and stack trace links; pure, also built to WebAssembly) |
| [javaforge-lang-groovy](javaforge-lang-groovy.md) | JavaForge: Groovy basics (source scanner, scripts and main classes, Spock for the shared test runner, Groovy script command lines with groovy or Gradle, Groovy SDK lookup; pure, also built to WebAssembly) |
| [javaforge-lang-scala](javaforge-lang-scala.md) | JavaForge: Scala basics (source scanner, main objects, sbt builds from build.sbt and sbt output, sbt command lines, ScalaTest and MUnit for the shared test runner, Metals install with Coursier; pure, also built to WebAssembly) |
| [javaforge-lang-java](javaforge-lang-java.md) | JavaForge: Java language support (source scanner, main methods, JUnit 5/4/3 and TestNG for the shared test runner, application command lines, Eclipse JDT LS launch configuration; pure, also built to WebAssembly) |
| [kotlinforge-lang-kotlin](kotlinforge-lang-kotlin.md) | KotlinForge: Kotlin language support (source scanner, main functions, JUnit 5 and Kotest for the shared test runner, Kotlin/JVM and script command lines, kotlin-lsp launch, ktlint and detekt reports, coroutine dumps; pure, also built to WebAssembly) |
| [luaforge-lang-lua](luaforge-lang-lua.md) | LuaForge: Lua language support (lexer, structural parser, EmmyLua/LuaLS annotations, project index, require resolution, hover, navigation; pure, also built to WebAssembly) |
| [pyforge-lang-python](pyforge-lang-python.md) | PyForge: Python language support (PEP 440 versions and specifiers, PEP 508 requirements, requirements.txt and pyproject.toml dependencies, imports, requirements inspections and completion, run configurations; pure, also built to WebAssembly) |
| [pyforge-interpreters](pyforge-interpreters.md) | PyForge: Python interpreters (system, pyenv, venv/virtualenv, uv, Poetry, conda, Pipenv, Hatch, Docker, Docker Compose, SSH, WSL): model, detection, metadata parsing and new environments |
| [pyforge-packages](pyforge-packages.md) | PyForge: Python packages (installed packages of an environment, pip/uv/Poetry/Pipenv commands, PyPI metadata and search cached in the user cache dir) |
| [rustforge-lang-rust](rustforge-lang-rust.md) | RustForge: Rust and Cargo support (cargo metadata, toolchain files, run / test / doctest discovery, the Cargo Test provider, Cargo command lines, Clippy messages with fixes, rustfmt, rust-analyzer settings, debugger pretty printers; pure, also built to WebAssembly) |
| [rustforge-toolchain](rustforge-toolchain.md) | RustForge: rustup toolchains and tools on the user's machine, consented rustup installs, the pinned rust-analyzer download (SHA-256) and GDB with rustc's pretty printers as a debug adapter |
| [goforge-lang-go](goforge-lang-go.md) | GoForge: Go language support (go.mod / go.work, module versions, test / benchmark / fuzz / example discovery with subtests, the Go Test provider, Go Build command lines, go:generate, go vet and golangci-lint reports; pure, also built to WebAssembly) |
| [goforge-sdk](goforge-sdk.md) | GoForge: Go SDK detection and consented downloads from go.dev/dl (SHA-256), gopls / Delve / goimports locations and pinned go install commands, module proxy lookups cached in the user cache dir |
| [goforge-delve](goforge-delve.md) | GoForge: Delve (dlv dap) as the `go` debug adapter: launch, test, attach to a process, remote dlv --headless, core dumps |
| [goforge-pprof](goforge-pprof.md) | GoForge: pprof profiles (profile.proto, gzip) with top, call tree and flame graph views; pure, also built to WebAssembly |
| [cforge-toolchains](cforge-toolchains.md) | CForge: C/C++ toolchains on the user's machine (GCC, Clang, Apple Clang, MinGW, MSVC through vswhere, cross compilers) with CMake, Ninja, Make, GDB and LLDB; version outputs parsed in Rust |
| [cforge-cmake](cforge-cmake.md) | CForge: CMake profiles, CMakePresets.json / CMakeUserPresets.json, the cmake-file-api query and codemodel-v2 reply, configure logs, configure / build / clean / CTest command lines, CMakeLists completion (pure, also built to WebAssembly) |
| [cforge-lang-cpp](cforge-lang-cpp.md) | CForge: C/C++ build problems (GCC, Clang, MSVC, linkers), compilation databases, GoogleTest and Catch2 discovery, the GoogleTest / Catch2 / CTest providers with gcov and llvm-cov coverage, clangd / clang-format command lines and debugger setup (pure, also built to WebAssembly) |
| [cforge-analyzers](cforge-analyzers.md) | CForge: sanitizer reports (ASan, LSan, TSan, MSan, UBSan), Valgrind Memcheck XML and perf script output as Profiler views (pure, also built to WebAssembly) |
| [cforge-embedded](cforge-embedded.md) | CForge: platformio.ini and PlatformIO tasks, ESP-IDF (idf.py) and Zephyr (west) commands, OpenOCD flashing and GDB server, CMSIS-SVD files with register decoding (pure, also built to WebAssembly) |
| [pyforge-frameworks](pyforge-frameworks.md) | PyForge: Django, Flask, FastAPI and Celery read statically from the sources (settings, URL patterns, routes, models, tasks, templates, manage.py commands, run command lines; pure, also built to WebAssembly) |
| [pyforge-jupyter](pyforge-jupyter.md) | PyForge: Jupyter notebooks (nbformat round-trip, rendered outputs with sanitized HTML/SVG, DataFrame tables, notebook diffs, the messaging protocol; pure, also built to WebAssembly; kernels over ZeroMQ and Jupyter servers with `runtime`) |
| [pyforge-testing](pyforge-testing.md) | PyForge: pytest, unittest and tox on the shared test runner, debugpy and cProfile command lines, `.pstats` profiles and the Python Console protocol (pure, also built to WebAssembly) |
| [phpforge-cli-tools](phpforge-cli-tools.md) | PHPForge: Command Line Tool Support (tool model and detection, `list --format=json` and `wp cli cmd-dump` parsing, console input parsing and completion; pure, also built to WebAssembly) |
| [phpforge-quality-tools](phpforge-quality-tools.md) | PHPForge: PHP quality tools (PHPStan, Psalm, PHP CS Fixer, PHP_CodeSniffer, PHP Mess Detector, Laravel Pint, Rector): configurations, command lines, JSON report parsing with path mapping, unified diffs (pure) |
| [phpforge-test-frameworks](phpforge-test-frameworks.md) | PHPForge: PHP test frameworks (PHPUnit, Pest, Codeception, Behat, PHPSpec): settings and detection, run configuration command lines, test discovery for gutter icons, Gherkin and Behat step definitions, Rerun Failed Tests (pure, also built to WebAssembly) |
| [phpforge-composer](phpforge-composer.md) | PHPForge: Composer support (version constraints, composer.json/lock analysis, Packagist metadata, outdated packages, audit advisories, completion) |
| [phpforge-dql](phpforge-dql.md) | PHPForge: Doctrine DQL tokenizer, parser, highlighting, completion and diagnostics, and DQL sites in PHP source (pure, also built to WebAssembly) |
| [phpforge-fw-laravel](phpforge-fw-laravel.md) | PHPForge: Laravel support (routes, views, Blade components, config/env, Eloquent, translations, artisan) |
| [phpforge-fw-symfony](phpforge-fw-symfony.md) | PHPForge: Symfony support (service container, routes, Twig templates, console commands) |
| [phpforge-fw-wordpress](phpforge-fw-wordpress.md) | PHPForge: WordPress support (hook index, completion, navigation, gutter marks, usages, Yoda hints, detection, WP-CLI and stubs lookup) |
| [phpforge-fw-cms](phpforge-fw-cms.md) | PHPForge: Drupal, Joomla and Shopware basics (detection, Drupal hook completion, implementations and hook docs) |
| [phpforge-lang-php](phpforge-lang-php.md) | PHPForge: PHP language support (tree-sitter index, docblocks, hover, code generation, language server config) |
| [phpforge-orm-doctrine](phpforge-orm-doctrine.md) | PHPForge: Doctrine ORM support (entity mapping, repositories, DQL parsing and completion) |
| [webforge-fw-angular](webforge-fw-angular.md) | WebForge: Angular support (components, directives, pipes, templates and styles) |
| [webforge-fw-react](webforge-fw-react.md) | WebForge: React support (JSX components, props and usages) |
| [webforge-fw-vue](webforge-fw-vue.md) | WebForge: Vue.js support (single-file components, props, emits and usages) |
| [webforge-js-debug](webforge-js-debug.md) | WebForge: the JavaScript debugger (js-debug downloaded on consent, the pwa-node / pwa-chrome / pwa-msedge adapters, browser detection, launch requests) |
| [webforge-js-testing](webforge-js-testing.md) | WebForge: the Node.js test runners (Jest, Vitest, Mocha, Playwright, Cypress, node --test): discovery, command lines, Rerun Failed Tests, debug programs (pure, also built to WebAssembly) |
| [webforge-quality-tools](webforge-quality-tools.md) | WebForge: web code quality tools (ESLint, Prettier, Stylelint, Biome): configurations, automatic detection of packages and configuration files, command lines, JSON report parsing (pure) |
| [webforge-lang-web](webforge-lang-web.md) | WebForge: package.json, npm scripts, package managers and the shared JavaScript/TypeScript/HTML scanning used by the framework plugins |

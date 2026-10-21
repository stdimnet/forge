# JetBrains IDE gap analysis

Features of JetBrains' IDEs that Forge does not have yet (October 2026), collected from the
official help tables of contents (`https://www.jetbrains.com/help/<ide>/HelpTOC.json`) and the
2026.x release notes. JetBrains AI products, Fleet, Toolbox and the like are out of scope.
[ROADMAP.md](../ROADMAP.md) turns these lists into milestones; this file keeps the details.

Prefixes: `ps/` = https://www.jetbrains.com/help/phpstorm/, `ws/` = …/webstorm/, `ij/` = …/idea/,
`py/` = …/pycharm/, `go/` = …/go/, `rust/` = …/rust/, `clion/` = …/clion/, `rd/` = …/rider/,
`rb/` = …/ruby/, `dg/` = …/datagrip/. Priority: H = everyday feature users expect, M = power
users, L = niche.

## IntelliJ platform, PhpStorm and WebStorm

### Editor
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Postfix completion | `$x.if`, `.foreach`, `.return` turn an expression into a construct | H | ps/postfix-code-completion.html |
| Complete Current Statement (`Ctrl+Shift+Enter`) | adds missing `;`, braces, parentheses | H | ps/advanced-code-completion.html |
| Type-Matching Completion, Cyclic Expand Word (`Alt+/`) | completion by expected type, hippie completion | M | ps/advanced-code-completion.html |
| Clone Caret Above/Below, Column Selection Mode | keyboard multi-caret and block selection | H | ps/multicursor.html |
| Paste from History (`Ctrl+Shift+V`) | clipboard history | H | ps/using-code-editor.html |
| Quick Documentation, Documentation tool window, External Documentation | docs from the keyboard, pinnable; php.net/MDN | H | ws/viewing-inline-documentation.html, ps/documentation-tool-window.html |
| Quick Definition, Type Info, Context Info, Error Description | peek definition, inferred type, context, full problem text | H | ps/viewing-definition.html |
| Reader mode / Toggle Rendered View | rendered doc comments | M | ps/reader-mode.html |
| Sticky lines | enclosing headers stay at the top | M | ws/sticky-lines.html |
| Language injections (IntelliLang) | SQL/HTML/JSON/RegExp in strings, `// language=`, Edit Fragment | H | ps/intellilang.html, ps/language-injections.html |
| Run injected SQL | run an SQL string against a data source | M | ps/running-injected-sql-statements.html |
| Spellchecking, Grammar | typos in identifiers, comments, strings | M | ps/spellchecking.html, ps/grammar.html |
| Check RegExp | test a regex | L | ij/regular-expressions.html |
| File and code templates | New PHP Class/Interface/Enum/Trait with PSR-4 namespace, components | H | ps/using-file-and-code-templates.html |
| Create symbols from usage | create method/field/class/constant from Alt+Enter | H | ps/creating-symbols-from-usage.html |
| Rearrange Code | sort members by rules | L | ps/rearrange-code.html |
| Code style schemes and presets | PSR-12, PER-CS, Symfony, Laravel, WordPress; JS/TS/CSS/HTML/SQL pages; import/export | M | ps/configuring-code-style.html |
| Actions on Save | reformat, optimize imports, rearrange, cleanup, tools on save | H | ps/saving-and-reverting-changes.html |
| Macros | record and replay editor actions | L | ps/using-macros-in-the-editor.html |
| Sort/Reverse Lines, Fill Paragraph, Find Word at Caret | small editing actions | L | ps/using-code-editor.html |
| Custom folding regions, Fold Selection | `// region` folding | L | ps/using-code-editor.html |
| Active editor toggles, structure breadcrumbs | per-editor soft wrap/whitespace/line numbers; class › method breadcrumb | M | ps/using-code-editor.html |
| Color picker and previews | CSS/SCSS/Tailwind swatches | M | ps/color-picker.html |
| Emmet | abbreviation expansion in HTML/CSS/JSX/Blade | H | ps/using-zen-coding-support.html |
| Markdown preview | live preview, Mermaid, links | H | ps/markdown.html |
| Image viewer / SVG preview | view and zoom images | M | ws/view-images-in-ide.html |
| Edit CSV/TSV as table | table editor for delimited files | M | ij/editing-csv-and-tsv-files.html |
| JSON Schema | SchemaStore validation and completion | H | ps/json.html, ps/yaml.html |
| Copyright profiles | licence headers | L | ps/copyright.html |
| Code Vision: usages and author | lenses above declarations | M | ps/inlay-hints.html |
| UML class and module diagrams | diagrams of classes and dependencies | L | ps/class-diagram.html |
| Vim emulation (IdeaVim) | Vim keybindings | H | ps/using-product-as-the-vim-editor.html |
| LightEdit mode | single file without a project | L | ps/lightedit-mode.html |

### Navigation and search
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| File Structure popup (`Ctrl+F12`) | filterable member popup | H | ps/viewing-structure-of-a-source-file.html |
| Last/Next Edit Location | jump to the last edit | H | ps/navigating-through-the-source-code.html |
| Select In / autoscroll from source | reveal the file in the Project view | H | ps/project-tool-window.html |
| Navigation Bar (`Alt+Home`) | path bar with members | M | ps/file-navigation.html |
| Related Symbol (`Ctrl+Alt+Home`) | controller ↔ view, test ↔ class | M | ps/navigating-through-the-source-code.html |
| Next/Previous Method, Next/Previous Change | structural and VCS caret movement | M | ps/navigating-through-the-source-code.html |
| Highlight Usages in File, Find Usages in File, Find Usages settings | in-file usages, scoped search | H | ps/find-highlight-usages.html |
| Structural Search and Replace | template-based search/replace | M | ps/structural-search-and-replace.html |
| Scopes and file colors | named scopes, Project view scopes, tab colors | M | ps/configuring-scopes-and-file-colors.html |
| Analyze External Stack Trace | navigate a pasted stack trace | M | ps/analyzing-external-stacktraces.html |

### Refactoring
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Refactor This | popup of applicable refactorings | H | ps/refactoring-source-code.html |
| Introduce Field / Introduce Parameter | extract into a property or parameter | H | ps/refactoring-source-code.html |
| Extract Interface / Superclass | create interface or base class | M | ps/refactoring-source-code.html |
| Pull Members Up / Push Members Down | move along the hierarchy | M | ps/pull-members-up.html |
| Make Static, Copy (F5) | method to static; copy a class with namespace fixed | L/M | ps/make-method-static.html, ps/move-refactorings.html |
| JS/TS refactorings | extract component, arrow/async, destructuring, type alias | M | ps/specific-javascript-refactorings.html |
| Rename related symbols | rename tests, variables, file with the class | M | ps/rename-refactorings.html |

### Code analysis
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Locate duplicates | duplicated fragments project-wide | M | ps/analyzing-duplicates.html |
| Data flow to/from here | trace values | L | ps/analyzing-data-flow.html |
| Dependency analysis | file and package dependencies | L | ij/dependencies-analysis.html |
| Vulnerable dependency check | CVEs in composer.json/package.json | M | ij/package-analysis.html |
| PHP Mess Detector, Laravel Pint | quality tools | M | ps/using-php-mess-detector.html, ps/using-laravel-pint.html |
| Custom inspections (SSR), command-line inspector | user inspections, CI inspection | L | ps/creating-custom-inspections.html, ps/command-line-code-inspector.html |
| PHP type checking | type mismatch without paid Intelephense | M | ps/php-type-checking.html |

### Run, debug, test
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Run Anything | run commands, scripts, configurations from one popup | H | ps/running-anything.html |
| Compound configurations | start several together | M | ps/run-debug-multiple.html |
| PHP CLI interpreters: Docker, Compose, SSH, Vagrant, WSL | run tools inside containers with mappings | H | ps/configuring-remote-interpreters.html |
| Several local PHP interpreters | interpreter list and picker | M | ps/configuring-local-interpreter.html |
| PHP Built-in Web Server, PHP Web Page, HTTP request debugging | PHP run configurations | M | ps/php-built-in-web-server.html, ps/debugging-a-php-http-request.html |
| Xdebug validation; JIT, simultaneous sessions, DBGp proxy | debugger setup and modes | M/L | ps/validating-the-configuration-of-the-debugging-engine.html |
| Logpoints, temporary breakpoints, dependencies | breakpoint properties | M | ps/using-breakpoints.html, ij/logpoints.html |
| Set Value, inline watches, debugger console | change values, REPL | M | ps/examining-suspended-program.html |
| JavaScript debugger (Chrome, Node.js) | JS/TS breakpoints | H | ps/debugging-javascript-in-chrome.html |
| Codeception, Behat, PHPSpec, Pest | PHP test frameworks | M (Pest H) | ps/pest.html, ps/using-codeception-framework.html |
| Jest, Vitest, Mocha, Karma, Playwright, Cypress | JS test runners | H | ws/jest.html, ws/vitest.html, ws/playwright.html |
| Auto-test, test history, export results | runner extras | M | ps/viewing-and-exploring-test-results.html |

### Version control
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Merge conflict resolver | three-way merge, resolve simple conflicts | H | ps/resolve-conflicts.html |
| File/selection history, Compare with Revision/Branch | per-file history and diffs | H | ps/investigate-changes.html |
| Stash/Unstash; Pull/Merge/Rebase/Reset/Tag/Remotes dialogs | Git dialogs | H | ps/shelving-and-unshelving-changes.html, ps/sync-with-a-remote-repository.html |
| Log actions | revert, undo, reset, edit message, squash/fixup | H | ps/edit-project-history.html |
| Changelists, partial commits, commit checks | commit workflow | M | ps/managing-changelists.html, ps/commit-and-push-changes.html |
| Patches, worktrees, GPG/SSH signing | Git extras | M | ps/using-patches.html, ps/use-git-worktrees.html |
| Compare Files / Folders / Clipboard | general diffs | H | ps/comparing-files-and-folders.html |
| Issue links, tasks and contexts | `#123` links, task switching | L | ps/handling-issues.html |
| GitHub Actions / GitLab CI support | workflow file completion | M | ps/github-actions.html |
| Other VCS, `forge diff`/`forge merge` CLI | Mercurial, SVN; difftool/mergetool | L | ps/enabling-version-control.html |

### PHP, frameworks, web
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| `.phpstorm.meta.php` and attributes | factory return types, argument sets | H | ps/ide-advanced-metadata.html |
| Include paths, PSR-0/4 roots, content roots | project structure | H | ps/configuring-include-paths.html, ps/content-root.html |
| Command Line Tool Support | Composer/Artisan/Symfony/WP-CLI console | M | ps/php-specific-command-line-tools.html |
| Composer UI | add dependency, version completion, outdated markers | M | ps/using-the-composer-dependency-manager.html |
| WordPress, Drupal, Joomla, Shopware | CMS support | M/L | ps/using-wordpress-content-management-system.html |
| Symfony translations/forms/events | Symfony extras | M | ps/symfony-translations.html |
| Laravel dashboard, `ide.json` | 2026.2 Laravel features | M | ps/laravel.html |
| Live Edit, File Watchers, Sass/Less | web tooling | M | ps/live-edit.html, ps/using-file-watchers.html |
| Vite/Webpack aliases, Svelte, Astro, Nuxt, Nx, Prisma, Deno, Bun, Storybook | frameworks | M | ps/vite.html, ws/svelte.html |
| OpenAPI, GraphQL | spec editing, schema completion | M/L | ps/openapi.html, ws/graphql.html |
| HTTP client extras | OAuth 2.0, cURL, WebSocket/gRPC, CLI runner | M | ps/oauth-2-0-authorization.html |

### Remote, workbench, settings
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Deployment | SFTP/FTP, mappings, upload on save, compare | H | ps/deploying-applications.html |
| Remote Host tool window, SSH configurations, SSH terminal | browse servers from local projects | M | ps/remote-host-tool-window.html |
| Dev Containers, WSL, Vagrant | development environments | M | ps/dev-containers-starting-page.html |
| Layouts, floating tool windows, editor tabs in windows, IDE zoom | workbench | M | ps/tool-windows.html, ps/ide-zoom-level.html |
| Project view options | file nesting, Show Members, scopes | M | ps/project-tool-window.html |
| Project Structure (Mark Directory As) | sources/tests/excluded | H | ps/content-root.html |
| Invalidate Caches, Repair IDE, Settings for New Projects | recovery and defaults | M | ps/invalidate-caches.html |
| Other keymaps, mouse shortcuts, color scheme editor | personalisation | M | ps/configuring-keyboard-and-mouse-shortcuts.html |
| Project trust / safe mode | trust before running scripts | M | ps/project-security.html |
| External Tools, IDE Scripting Console, MCP server | extensibility | M/L | ps/configuring-third-party-tools.html, ps/mcp-server.html |

## PyCharm, GoLand, RustRover, CLion

### Shared
| Feature | What it does | Prio | Doc |
| --- | --- | --- | --- |
| Memory, Hex, Disassembly, Registers views | native debugger panels | H | clion/memory-view.html, clion/disassembly-view.html |
| Attach to process / to DAP | attach debuggers to running processes | H/M | py/attach-to-process.html |
| Run targets (WSL, Docker, SSH) | build, run, test, debug in targets | H | go/run-targets.html |
| Code coverage per language | coverage.py, go cover, llvm-cov, gcov | H | clion/code-coverage-clion.html |
| Core dumps, watchpoints, step filter, forked processes | debugger extras | M | clion/core-dump-debug.html |
| Endpoints tool window | HTTP routes of all frameworks | M | go/endpoints-tool-window.html |

### PyCharm
| Feature | Prio | Doc |
| --- | --- | --- |
| conda, pipenv, Hatch environments; WSL/SSH/Docker/Compose interpreters | H/M | py/conda-support-creating-conda-virtual-environment.html, py/using-docker-as-a-remote-interpreter.html |
| Python Packages tool window, requirements inspections, package repositories, workspaces | H/M | py/installing-uninstalling-and-upgrading-packages.html |
| Ruff, Pyright, basedpyright, Pyrefly, ty, Black | H | py/lsp-tools.html |
| Python console, debug console, remote debug server, profiler | H/M | py/using-consoles.html, py/profiler.html |
| Stubs, docstrings, Pydantic, create tests, unittest/doctest/tox, BDD | M/L | py/stubs.html, py/tox-support.html |
| Django (manage.py, console, server/tests configs, template navigation and debugging, structure, endpoints, DRF, i18n) | H/M | py/running-manage-py.html, py/navigating-between-templates-and-views.html |
| Flask, Celery | H/M | py/flask.html, py/run-celery-workers.html |
| Jupyter (editor, cells, outputs, servers, VCS, widgets, magics), plots, DataFrame views | H/M | py/editing-jupyter-notebook-files.html, py/scientific-features.html |

### GoLand
| Feature | Prio | Doc |
| --- | --- | --- |
| GOROOT/GOPATH, modules, workspaces, build constraints | H/M | go/configuring-goroot-and-gopath.html, go/go-workspaces.html |
| gofmt/goimports/generate/vet, golangci-lint, go fix, vulnerable dependencies | H/M | go/integration-with-go-tools.html, go/configuring-golangci-lint-in-the-go-linter-plugin.html |
| Go Build and Go Test (bench, fuzz), testify, create tests | H/M | go/go-test.html |
| Delve attach/remote, core dumps, pprof profiler, escape analysis | H/M | go/go-remote.html, go/profiler-intro.html |
| JSON to struct, struct tags, Go templates, Go refactorings | M | go/working-with-json.html, go/extract-interface.html |

### RustRover
| Feature | Prio | Doc |
| --- | --- | --- |
| Toolchains (rustup), Cargo tool window and configurations, profiles, Cargo.toml completion, cfg/features | H/M | rust/rust-toolchain.html, rust/rust-cargo-tool-window.html |
| Clippy, rustfmt, macro expansion, lifetime visualisation, code generation | H/M | rust/rust-external-linters.html, rust/rustfmt.html |
| Doctests, nextest, LLDB/GDB renderers, remote debug | H/L | rust/nextest.html, rust/rust-switch-debuggers-and-renderers.html |
| Axum, wasm-pack, REPL, templates | M/L | rust/axum.html |

### CLion (CForge pack)
| Feature | Prio | Doc |
| --- | --- | --- |
| CMake profiles, presets, tool window, reload; compilation database | H | clion/cmake-profile.html, clion/compilation-database.html |
| Makefile, Meson, Autotools, Bazel, custom build targets | M | clion/makefiles-support.html |
| Toolchains (GCC/Clang/MSVC/MinGW), Docker/WSL/remote toolchains, vcpkg/Conan | H/M | clion/how-to-create-toolchain-in-clion.html, clion/package-management.html |
| clangd settings, Clang-Tidy, ClangFormat, resolve context, Doxygen, data flow | H/M | clion/clang-tidy-checks-support.html |
| Run configurations, GDB/LLDB, debugger console, gdbserver | H/M | clion/configuring-debugger-options.html |
| GoogleTest, Catch2, Boost.Test, doctest, CTest | H | clion/ctest-support.html |
| Sanitizers, Valgrind, perf profiler | M | clion/google-sanitizers.html, clion/cpu-profiler.html |
| Embedded: OpenOCD, SVD peripherals, live watches, RTOS, PlatformIO, STM32CubeMX, ESP-IDF, Zephyr | M | clion/openocd-support.html, clion/peripheral-view.html |

## DataGrip, Rider, RubyMine

### DataGrip (database tools)
| Feature | Prio | Doc |
| --- | --- | --- |
| SSH tunnel and full SSL per data source | H | dg/configuring-ssh-and-ssl.html |
| SQL Server/Azure SQL; Redis, MongoDB, ClickHouse, CockroachDB, Redshift; long tail | H/M/L | dg/microsoft-sql-server.html, dg/redis.html, dg/mongodb.html |
| Connection by URL/socket, advanced auth, templates, cloud explorers, sessions, production colors | M/L | dg/configuring-database-connections.html, dg/managing-connection-sessions.html |
| DDL data sources | M | dg/ddl-data-sources.html |
| Full object tree, schema filter, introspection levels, virtual FKs/columns/views, full-text search, Find Usages | H/M | dg/database-explorer.html, dg/schemas.html |
| Create/Modify dialogs, Confirm Drop, edit object source, Database Changes window, SQL generator | H/M | dg/create-and-modify-dialogs.html |
| User parameters, Query History, execution options, Run SQL files, dumps with native tools | H/M | dg/run-a-query.html, dg/run-sql-files.html |
| SQL resolve and inspections, dialect per file, formatter, live templates, smarter completion, injection in PHP/JS | H | dg/datagrip-inspections.html, dg/settings-code-style-sql.html |
| Data editor: header sort/filter, value editor, FK navigation, transpose, typed editors, DML preview, mass edit, aggregates, compare data, charts | H/M/L | dg/tables-filter.html, dg/tables-view-data.html |
| Import (CSV/Excel/JSON), extractors, export objects | H/M | dg/import-data.html, dg/data-extractors.html |

### Rider (.NET pack)
Language servers: Roslyn LSP (MIT) first, csharp-ls as fallback, fsautocomplete for F#;
debugger netcoredbg (MIT). Never bundle vsdbg, C# Dev Kit, Visual Studio Tools for Unity,
dotTrace/dotMemory or ReSharper.

| Feature | Prio | Doc |
| --- | --- | --- |
| Solutions/projects, SDK and `global.json`, build window, configurations | H | rd/Supported_project_file_formats.html, rd/Building_Projects.html |
| Run configurations and launchSettings, debugger, unit tests, NuGet, `dotnet new`, cleanup, analyzers | H | rd/Run_Debug_Configuration_dotNet_Project.html, rd/Using_NuGet.html |
| Decompiler/SourceLink, namespaces import, Code Vision | M | rd/Exploring_Assemblies.html |
| ASP.NET Core (Razor/Blazor, endpoints, scaffolding), user secrets, Hot Reload, EF Core, Aspire, Docker, SSH debugging | H/M | rd/Web_Development__Index.html, rd/Visual_interface_for_EF_Core_commands.html |
| Coverage, continuous testing, profiling, hierarchies, project refactorings, IL viewer, REPLs, packaging, XAML, Azure DevOps | M/L | rd/Analyzing_Coverage_NET.html, rd/Azure-DevOps.html |
| Unity, Godot (C#); Unreal belongs to CForge | M/L | rd/Unity.html, rd/Godot.html |

### RubyMine (Ruby pack)
Language servers: ruby-lsp (+ ruby-lsp-rails), Solargraph; Sorbet/Steep; RuboCop/Standard;
Herb for ERB; debugger rdbg (DAP).

| Feature | Prio | Doc |
| --- | --- | --- |
| Interpreters and version managers, Bundler, Ruby/Rake configurations, RSpec/Minitest, rdbg, RuboCop, ERB, project templates, IRB | H/M | rb/ruby-version-managers.html, rb/using-the-bundler.html, rb/rspec.html |
| Rails: server configurations, navigation, routes, generators, console, ActiveRecord, i18n, Hotwire, diagrams | H/M | rb/run-rails-applications.html, rb/product-specific-navigation.html |
| Ruby refactorings, RBS/Sorbet, YARD, SimpleCov, Brakeman, profiler, Cucumber, Rack/Sinatra, Capistrano/Kamal | M/L | rb/rbs.html, rb/brakeman.html |

### DataSpell
Workspaces with attached directories, R and R Markdown, Data Wrangler (low priority; notebooks
belong to PyForge).

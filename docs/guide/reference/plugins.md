---
title: Plugins reference
description: Every built-in plugin with its id, pack, dependencies, the project types that activate it and what it contributes.
order: 80
tags: [plugins, packs, reference]
---

# Plugins reference

Generated from the manifests in `app/src-tauri/plugins/*.toml`. A plugin is active when it is
enabled (Settings | Plugins) and, if it lists project types, when the open project uses one of
them (a language, framework, pack or tool id from
[project detection](../getting-started/project-detection.md)). Disabling a plugin also disables
the plugins that depend on it.

Pack ids: `core`, `phpforge`, `webforge`, `luaforge`, `pyforge`, `javaforge`, `goforge`, `rustforge`, `rubyforge`, `kotlinforge`, `cforge`, `dotnetforge`, and `scriptforge` / `configforge` for the
script and config language plugins.

| Plugin | Id | Pack | Depends on | Active for (project types) | Contributes |
| --- | --- | --- | --- | --- | --- |
| Browser Companion | `forge.browser-companion` | core |  | always | 1 commands; 3 settings |
| Natural Languages | `forge.natural-languages` | core |  | always | 17 settings |
| GitLab, Forgejo, GitHub and Azure DevOps | `forge.code-hosts` | core | `forge.git` | always | tool windows: Pull Requests; 2 commands; 2 settings |
| Forge Platform (required) | `forge.core` | core |  | always | tool windows: Project; 2 commands; 49 settings; 3 keybindings |
| Database Tools and SQL | `forge.database` | core |  | always | tool windows: Database; 2 commands; 3 settings |
| Git | `forge.git` | core |  | always | tool windows: Commit, Git; 2 commands; 8 settings; 1 keybindings |
| Bash | `forge.bash` | scriptforge |  | always | file types: Shell Script |
| YAML | `forge.yaml` | configforge |  | always | file types: YAML |
| JSON | `forge.json` | configforge |  | always | file types: JSON |
| .env | `forge.dotenv` | configforge |  | always | file types: Dotenv |
| Ignore Files | `forge.ignore` | configforge |  | always | file types: Ignore File |
| Docker | `forge.docker` | configforge |  | always | file types: Dockerfile, Docker Compose |
| Postfix | `forge.postfix` | configforge |  | always | file types: Postfix main.cf, Postfix master.cf |
| E-Mail | `forge.eml` | configforge |  | always | file types: E-Mail |
| INI | `forge.ini` | configforge |  | always | file types: INI |
| Config Files | `forge.conf` | configforge |  | always | file types: Config |
| Language Servers | `forge.langservers` | core | `forge.core` | always | 1 commands; 9 settings |
| Lua | `forge.lua` | luaforge |  | `luaforge` | 1 commands; 6 settings; file types: Lua; 1 LSP servers; run types: Lua Script |
| Python | `forge.python` | pyforge |  | always | 1 commands; 4 settings; file types: Python, pip Requirements |
| Python Interpreters and Packages | `forge.python-env` | pyforge | `forge.python` | `pyforge` | tool windows: Python Packages; 4 commands; 8 settings; run types: Python |
| Go | `forge.go` | goforge |  | `goforge` | tool windows: Profiler; 9 commands; 14 settings; file types: Go Module, Go Workspace, Go Checksums; run types: Go Build, Go Remote; test frameworks: Go Test |
| Ruby | `forge.ruby` | rubyforge |  | `rubyforge` | tool windows (UI): Rake; 9 commands; 13 settings; file types: Ruby, Gemfile.lock; run types: Ruby, Rake; test frameworks: RSpec, Minitest; 1 debug adapter (rdbg, in Rust); inspections: RuboCop |
| Ruby on Rails | `forge.rails` | rubyforge | `forge.ruby` | `rails` | tool windows (UI): Endpoints (Rails view); 6 commands; 7 settings; run types: Rails; inspections: Missing translation, Unresolved partial or template, Unknown Stimulus controller, action or target |
| Rust | `forge.rust` | rustforge |  | `rustforge` | tool windows (UI): Cargo; 8 commands; 18 settings; file types: TOML (Cargo.lock, rust-toolchain); run types: Cargo; test frameworks: Cargo Test; 1 debug adapter (rust-gdb, in Rust) |
| Python Tests, Debugger, Console and Profiler | `forge.python-tools` | pyforge | `forge.python-env` | `pyforge` | tool windows: Python Console, Profiler; 5 commands; 9 settings; run types: pytest, Unittests, tox; 1 debug adapters |
| Python Web Frameworks | `forge.python-web` | pyforge | `forge.python-env` | `django`, `flask`, `fastapi` | tool windows: Endpoints; 1 commands |
| Django | `forge.django` | pyforge | `forge.python-web` | `django` | tool windows: Django Structure; 1 commands; 4 settings; file types: Django Template; run types: Django Server, Django tests |
| Flask | `forge.flask` | pyforge | `forge.python-web` | `flask` | 1 settings; file types: Jinja2; run types: Flask Server |
| FastAPI | `forge.fastapi` | pyforge | `forge.python-web` | `fastapi` | 1 settings; run types: FastAPI |
| Celery | `forge.celery` | pyforge | `forge.python-env` | `celery` | 1 settings; run types: Celery worker, Celery beat |
| Jupyter | `forge.jupyter` | pyforge | `forge.python` | always | 9 commands; 6 settings; the notebook editor and notebook diffs for `.ipynb` |
| Java | `forge.java` | javaforge |  | `javaforge` | 3 commands; 21 settings; 1 debug adapters (java-debug); 2 test frameworks (JUnit, TestNG); run types (UI): Application, Gradle, Maven, JUnit, TestNG, Remote JVM Debug |
| C/C++ | `forge.cpp` | cforge | `forge.debugger` | `cforge` | tool windows (UI): CMake, PlatformIO; 18 commands; 41 settings; file types: Clang Configuration; run types: OpenOCD Download & Run, Embedded GDB Server, CMake Application; test frameworks: Google Test, Catch2, CTest |
| .NET | `forge.dotnet` | dotnetforge | `forge.debugger` | `dotnetforge` | tool windows (UI): Build, NuGet, EF Core, the ASP.NET Core view of Endpoints; 19 commands; 21 settings; file types: MSBuild, Solution File, Razor; run types: .NET Project, .NET Aspire, .NET Docker; test frameworks: .NET Tests; 1 debug adapter (coreclr, netcoredbg) |
| Unity | `forge.unity` | dotnetforge | `forge.dotnet` | `unity` | tool window (UI): Unity; 9 commands; 5 settings; file types: ShaderLab, HLSL; run types: Unity Tests; 1 debug adapter (unity, a Mono debug adapter you configure) |
| Godot | `forge.godot` | dotnetforge | `forge.debugger` | `godot` | 3 commands; 6 settings; file types: GDScript, Godot Resource, Godot Shader; run types: Godot, GdUnit4; the `godot` debug adapter (the Godot editor, registered in Rust); the Godot editor's GDScript language server |
| Spring Boot and Jakarta EE | `forge.spring` | javaforge | `forge.java` | `spring-boot`, `jakarta-ee` | tool windows: Endpoints, Spring; 3 commands; 7 settings; run types (UI): Spring Boot; Services provider: Spring Boot run dashboard |
| Groovy | `forge.groovy` | javaforge | `forge.java` | `groovy` | 2 settings; 1 test frameworks (Spock); run types (UI): Groovy Script, Spock |
| Scala | `forge.scala` | javaforge | `forge.java` | `scala` | tool windows (UI): sbt; 3 commands; 6 settings; 2 test frameworks (ScalaTest, MUnit); run types (UI): sbt Task, ScalaTest, MUnit |
| Android | `forge.android` | javaforge | `forge.java` | `android` | tool windows (UI): Device Manager, Logcat; 2 commands; 4 settings; run types (UI): Android App |
| Kotlin | `forge.kotlin` | kotlinforge | `forge.java` | `kotlinforge` | 4 commands; 16 settings; 1 debug adapters (kotlin-lsp); 1 test frameworks (Kotest); run types (UI): Kotlin Script, Kotest |
| AI Assistant | `forge.ai` | core |  | always | tool windows: AI Chat; 3 AI providers; 1 chat agents; 2 code generators |
| Claude Code | `forge.claude-code` | core | `forge.ai` | always | 3 settings; 1 AI providers; 1 chat agents |
| Gemini CLI | `forge.gemini-cli` | core | `forge.ai` | always | 3 settings; 1 AI providers; 1 chat agents |
| PHP | `forge.php` | phpforge |  | `php` | 1 commands; 18 settings; 1 LSP servers; run types: PHP Script, PHP Built-in Web Server; 4 code generators |
| Composer | `forge.composer` | phpforge | `forge.php` | `composer` | 6 commands; 6 settings; run types: Composer Script; 1 main menu entries |
| Command Line Tool Support | `forge.command-line-tools` | phpforge | `forge.php` | `php`, `composer` | tool windows: Command Line Tools Console; 1 commands; 2 settings; 1 keybindings; 1 main menu entries |
| PHPUnit | `forge.phpunit` | phpforge | `forge.php` | `phpunit`, `pest` | run types: PHPUnit; 1 code generators |
| Xdebug | `forge.xdebug` | phpforge | `forge.php` | `php` | tool windows: Debug; 1 commands; 8 settings; run types: PHP Remote Debug; 1 main menu entries |
| Laravel | `forge.laravel` | phpforge | `forge.php`, `forge.composer` | `laravel` | tool windows: Routes; 1 commands; 6 settings; run types: Artisan; 1 code generators; 1 main menu entries |
| Symfony | `forge.symfony` | phpforge | `forge.php`, `forge.composer` | `symfony` | tool windows: Symfony; 2 commands; 7 settings; 1 main menu entries |
| Symfony Services | `forge.symfony-services` | phpforge | `forge.yaml` | `symfony` | file types: Symfony Services |
| Doctrine | `forge.doctrine` | phpforge | `forge.php` | `doctrine` | tool windows: Doctrine; 1 commands; 6 settings |
| Run/Debug | `forge.run` | core |  | always | tool windows: Run; 3 commands; 2 settings; run types: Shell Script |
| Debugger | `forge.debugger` | core |  | always | 6 settings; debug adapters: LLDB, GDB |
| Deployment | `forge.deployment` | core |  | always | tool windows: File Transfer; 8 commands; 8 settings; 1 main menu entries |
| Docker and Kubernetes | `forge.services` | core |  | always | tool windows: Services; 2 commands; 15 settings; 1 main menu entries |
| Terminal | `forge.terminal` | core |  | always | tool windows: Terminal; 1 commands; 7 settings; 1 keybindings |
| Vim | `forge.vim` | core | `forge.core` | always | 2 commands; 3 settings; 1 main menu entries |
| HTML, CSS & JavaScript | `forge.web` | webforge |  | `webforge` | tool windows: npm; 2 commands; 4 settings; 3 LSP servers; run types: npm |
| React | `forge.react` | webforge | `forge.web` | `react`, `next` |  |
| Vue.js | `forge.vue` | webforge | `forge.web` | `vue`, `nuxt` | 1 LSP servers |
| Angular | `forge.angular` | webforge | `forge.web` | `angular` | 1 settings; 1 LSP servers |

> **Note:** some contributions are declared ahead of their implementation: the "PHP Built-in
> Web Server" and "PHP Remote Debug" run configuration types, the "Google Gemini" AI provider
> kind, the "Assistant" chat agent and the AI code generators ("Tests with AI", "Documentation
> with AI"), the PHPUnit "Test Method" and Laravel "Eloquent Model" generators, and the commands
> `composer.install`, `composer.update` and `php.runFile` have no implementation on this
> version.

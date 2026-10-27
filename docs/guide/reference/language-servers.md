---
title: Language servers reference
description: The catalog of language servers Forge knows - languages, commands, install options, licences and start conditions.
order: 40
tags: [lsp, language servers, reference]
---

# Language servers reference

Generated from `crates/forge-langservers/catalog.toml` and `pack_servers.toml`. Servers are
grouped as on Settings | Languages & Frameworks | Language Servers. See
[Language servers](../user/language-servers.md) for how detection, installation and per-server
settings work.

- **Install**: npm commands (`npm install -g …`) become managed installs into Forge's server
  folder, never global ones; Forge downloads Node.js itself when none (>= 18) is installed.
  "Download … from PyPI" fetches the official wheel of the platform (ruff, ty), checks its
  SHA-256 against PyPI and copies the executable to `<data dir>/forge/langservers/bin`.
  Commands with `sudo` are only shown for you to run. Links are download pages.
- **Alternative group**: only the first enabled and installed server of a group starts.
- **Needs plugin**: the server starts only while that plugin is enabled and active.
- The PHP servers are started by the PHP pack (Settings | Languages & Frameworks | PHP), not by
  the catalog.

## Servers

### JavaScript & TypeScript

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [TypeScript Language Server](https://github.com/typescript-language-server/typescript-language-server) | `typescript-language-server` | TypeScript, JavaScript | `typescript-language-server --stdio` | `npm install -g typescript-language-server typescript`<br>`brew install typescript-language-server` (macos, linux)<br>`sudo pacman -S typescript-language-server` (linux) | MIT | alternative group `typescript`; needs plugin `forge.web`; Uses the project's TypeScript (node_modules) or a global one. Serves .vue files too when the Vue language server is installed (hybrid mode). |
| [vtsls](https://github.com/yioneko/vtsls) | `vtsls` | TypeScript, JavaScript | `vtsls --stdio` | `npm install -g @vtsls/language-server` | MIT | alternative group `typescript`; needs plugin `forge.web`; Alternative to typescript-language-server built on VS Code's TypeScript extension. Starts only when typescript-language-server is missing or disabled. |
| [ESLint](https://github.com/hrsh7th/vscode-langservers-extracted) | `vscode-eslint-language-server` | JavaScript, TypeScript, Vue, Svelte, Astro | `vscode-eslint-language-server --stdio` | `npm install -g vscode-langservers-extracted`<br>`brew install vscode-langservers-extracted` (macos, linux) | MIT | needs plugin `forge.web`; only when the project has files `eslint.config.*`, `.eslintrc*` or packages `eslint`; Starts in projects with an ESLint configuration or an eslint dependency. |

### Vue

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Vue Language Server (Volar)](https://github.com/vuejs/language-tools) | `vue-language-server` | Vue | `vue-language-server --stdio` | `npm install -g @vue/language-server @vue/typescript-plugin typescript` | MIT | needs plugin `forge.vue`; Hybrid mode: templates and styles here, script blocks through the TypeScript server with @vue/typescript-plugin. |

### Angular

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Angular Language Service](https://github.com/angular/vscode-ng-language-service) | `ngserver` | TypeScript, HTML | `ngserver --stdio` | `npm install -g @angular/language-server typescript` | MIT | needs plugin `forge.angular`; only when the project has packages `@angular/core`; Starts in projects that depend on @angular/core. |

### Svelte

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Svelte Language Server](https://github.com/sveltejs/language-tools) | `svelteserver` | Svelte | `svelteserver --stdio` | `npm install -g svelte-language-server` | MIT | needs plugin `forge.web` |

### Astro

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Astro Language Server](https://github.com/withastro/language-tools) | `astro-ls` | Astro | `astro-ls --stdio` | `npm install -g @astrojs/language-server typescript` | MIT | needs plugin `forge.web` |
| [Deno Language Server](https://docs.deno.com/runtime/reference/lsp_integration/) | `deno` | TypeScript, JavaScript | `deno lsp` | `npm install -g deno`, `brew install deno`, `cargo install deno --locked` | MIT | needs plugin `forge.deno`; only with a root `deno.json` / `deno.jsonc`, where it replaces the TypeScript language server |
| [Prisma Language Server](https://github.com/prisma/language-tools) | `prisma-language-server` | Prisma | `prisma-language-server --stdio` | `npm install -g @prisma/language-server` | Apache-2.0 | needs plugin `forge.prisma` |

### HTML & CSS

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [HTML Language Server](https://github.com/hrsh7th/vscode-langservers-extracted) | `vscode-html-language-server` | HTML | `vscode-html-language-server --stdio` | `npm install -g vscode-langservers-extracted`<br>`brew install vscode-langservers-extracted` (macos, linux)<br>`sudo pacman -S vscode-html-languageserver` (linux) | MIT | needs plugin `forge.web` |
| [CSS Language Server](https://github.com/hrsh7th/vscode-langservers-extracted) | `vscode-css-language-server` | CSS, SCSS, Less | `vscode-css-language-server --stdio` | `npm install -g vscode-langservers-extracted`<br>`brew install vscode-langservers-extracted` (macos, linux)<br>`sudo pacman -S vscode-css-languageserver` (linux) | MIT | needs plugin `forge.web` |
| [Tailwind CSS IntelliSense](https://github.com/tailwindlabs/tailwindcss-intellisense) | `tailwindcss-language-server` | HTML, CSS, SCSS, JavaScript, TypeScript, Vue, Svelte, Astro, PHP, Twig | `tailwindcss-language-server --stdio` | `npm install -g @tailwindcss/language-server` | MIT | needs plugin `forge.web`; only when the project has files `tailwind.config.*` or packages `tailwindcss`, `@tailwindcss/vite`, `@tailwindcss/postcss`; Starts in projects with a Tailwind config or a tailwindcss dependency. Class completion in Blade and Twig templates too. |

### JSON

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [JSON Language Server](https://github.com/hrsh7th/vscode-langservers-extracted) | `vscode-json-language-server` | JSON | `vscode-json-language-server --stdio` | `npm install -g vscode-langservers-extracted`<br>`brew install vscode-langservers-extracted` (macos, linux) | MIT | Validates composer.json, package.json, tsconfig.json and more with schemas from SchemaStore (downloaded by the server). |

### YAML

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [YAML Language Server](https://github.com/redhat-developer/yaml-language-server) | `yaml-language-server` | YAML, Docker Compose | `yaml-language-server --stdio` | `npm install -g yaml-language-server`<br>`brew install yaml-language-server` (macos, linux)<br>`sudo pacman -S yaml-language-server` (linux) | MIT | Schemas for GitHub Actions, docker-compose and Kubernetes manifests (in k8s/, kubernetes/, deploy/ or manifests/). |

### TOML

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Taplo (TOML)](https://taplo.tamasfe.dev) | `taplo` | TOML | `taplo lsp stdio` | `cargo install --locked taplo-cli`<br>`npm install -g @taplo/cli`<br>`brew install taplo` (macos, linux)<br>`sudo pacman -S taplo-cli` (linux) | MIT |  |

### XML

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [LemMinX (XML)](https://github.com/eclipse/lemminx) | `lemminx` | XML | `lemminx` | [download](https://github.com/redhat-developer/vscode-xml/releases)<br>`brew install lemminx` (macos, linux) | EPL-2.0 | Validates phpunit.xml, Maven POMs and any XML with an XSD or DTD. Needs Java unless you use the native binary. |

### Markdown

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Marksman (Markdown)](https://github.com/artempyanykh/marksman) | `marksman` | Markdown | `marksman server` | `brew install marksman` (macos, linux)<br>`sudo pacman -S marksman` (linux)<br>[download](https://github.com/artempyanykh/marksman/releases) | MIT |  |

### Docker

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Dockerfile Language Server](https://github.com/rcjsuen/dockerfile-language-server) | `docker-langserver` | Dockerfile | `docker-langserver --stdio` | `npm install -g dockerfile-language-server-nodejs` | MIT |  |
| [Docker Compose Language Service](https://github.com/microsoft/compose-language-service) | `docker-compose-langserver` | Docker Compose | `docker-compose-langserver --stdio` | `npm install -g @microsoft/compose-language-service` | MIT |  |

### Shell Script

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Bash Language Server](https://github.com/bash-lsp/bash-language-server) | `bash-language-server` | Shell Script | `bash-language-server start` | `npm install -g bash-language-server`<br>`brew install bash-language-server` (macos, linux)<br>`sudo pacman -S bash-language-server` (linux) | MIT | Uses shellcheck for diagnostics and shfmt for formatting when they are installed. |

### SQL

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [sqls](https://github.com/sqls-server/sqls) | `sqls` | SQL | `sqls` | `go install github.com/sqls-server/sqls@latest` | MIT | Completion and formatting for SQL. Configure connections in ~/.config/sqls/config.yml. |

### LaTeX

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [TexLab](https://github.com/latex-lsp/texlab) | `texlab` | LaTeX | `texlab` | `cargo install --locked texlab`<br>`brew install texlab` (macos, linux)<br>`sudo pacman -S texlab` (linux) | GPL-3.0 |  |

### Twig

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Twiggy (Twig)](https://github.com/moetelo/twiggy) | `twiggy-language-server` | Twig | `twiggy-language-server --stdio` | `npm install -g twiggy-language-server` | MPL-2.0 |  |

### Lua

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Lua Language Server](https://luals.github.io) | `lua-language-server` | Lua | `lua-language-server` | `brew install lua-language-server` (macos, linux)<br>`sudo pacman -S lua-language-server` (linux)<br>[download](https://github.com/LuaLS/lua-language-server/releases) | MIT | needs plugin `forge.lua`; Pick LÖVE or Neovim under Lua environment to get their APIs and globals. |

### Python

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [basedpyright](https://docs.basedpyright.com) | `basedpyright` | Python | `basedpyright-langserver --stdio` | `npm install -g basedpyright` (managed: Forge installs it)<br>`pipx install basedpyright`<br>`python3 -m pip install --user basedpyright` | MIT | needs plugin `forge.python`; alternative group `python`; gets the project's interpreter as `python.pythonPath` |
| [Pyright](https://github.com/microsoft/pyright) | `pyright` | Python | `pyright-langserver --stdio` | `npm install -g pyright`<br>`pipx install pyright`<br>`sudo pacman -S pyright` (linux) | MIT | alternative group `python`; Starts only when basedpyright is missing or disabled. |
| [ty (preview)](https://docs.astral.sh/ty/) | `ty` | Python | `ty server` | Download ty from PyPI (SHA-256 verified)<br>`uv tool install ty`<br>`pipx install ty` | MIT | needs plugin `forge.python`; off by default; alternative group `python`; Preview: Astral's type checker. Enable it and disable basedpyright and Pyright to use it. |
| [Ruff](https://docs.astral.sh/ruff/) | `ruff` | Python | `ruff server` | Download ruff from PyPI (SHA-256 verified)<br>`uv tool install ruff`<br>`pipx install ruff`<br>`brew install ruff` (macos, linux) | MIT | needs plugin `forge.python`; Linting and quick fixes next to the type checker; ruff format is the default formatter. |

### Go

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [gopls](https://go.dev/gopls) | `gopls` | Go | `gopls` | `go install golang.org/x/tools/gopls@latest`<br>`sudo pacman -S gopls` (linux)<br>`sudo apt install gopls` (linux) | BSD-3-Clause |  |

### Rust

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [rust-analyzer](https://rust-analyzer.github.io) | `rust-analyzer` | Rust | `rust-analyzer` | `rustup component add rust-analyzer`<br>`brew install rust-analyzer` (macos, linux)<br>`sudo pacman -S rust-analyzer` (linux) | MIT OR Apache-2.0 |  |

### C/C++

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [clangd](https://clangd.llvm.org) | `clangd` | C, C++ | started by CForge (`forge.cpp`) | Download from Settings \| Languages & Frameworks \| C/C++ (GitHub releases, SHA-256 checked)<br>`brew install llvm` (macos)<br>`sudo pacman -S clang` (linux)<br>`sudo apt install clangd` (linux)<br>[download](https://github.com/clangd/clangd/releases) | Apache-2.0 WITH LLVM-exception | A pack server: CForge starts it with the compilation database of the active CMake profile, clang-tidy and the toolchain's compilers as query drivers. See [CForge](../user/cpp.md#clangd). |

### Zig

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [ZLS](https://github.com/zigtools/zls) | `zls` | Zig | `zls` | `brew install zls` (macos, linux)<br>`sudo pacman -S zls` (linux)<br>[download](https://github.com/zigtools/zls/releases) | MIT |  |

### Java

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Eclipse JDT Language Server](https://github.com/eclipse-jdtls/eclipse.jdt.ls) | `jdtls` | Java | started by JavaForge (`forge.java`) | Install from Settings \| Languages & Frameworks \| Java (download.eclipse.org, SHA-256 checked)<br>[download](https://download.eclipse.org/jdtls/milestones/) | EPL-2.0 | A pack server: JavaForge starts it with a JDK 21+ (25 for newer builds) and keeps its data in the user cache directory. See [JavaForge](../user/java.md). |

### Kotlin

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Kotlin LSP](https://github.com/Kotlin/kotlin-lsp) | `kotlin-lsp` | Kotlin | started by KotlinForge (`forge.kotlin`) | Install from Settings \| Languages & Frameworks \| Kotlin (download.jetbrains.com, pinned SHA-256)<br>`brew install JetBrains/utils/kotlin-lsp` | Apache-2.0 (the server build includes closed JetBrains components) | A pack server: KotlinForge starts `bin/intellij-server --stdio` with the project JDK as `JAVA_HOME` and pulls its diagnostics. See [KotlinForge](../user/kotlin.md). |
| [Kotlin Language Server](https://github.com/fwcd/kotlin-language-server) | `kotlin-language-server` | Kotlin | started by KotlinForge when chosen | `brew install kotlin-language-server` (macos, linux)<br>[download](https://github.com/fwcd/kotlin-language-server/releases) | MIT | A pack server, the alternative to kotlin-lsp. |

### Scala

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Metals](https://scalameta.org/metals/) | `metals` | Scala | started by JavaForge's Scala plugin (`forge.scala`) | Install from Settings \| Languages & Frameworks \| Scala (your Coursier: `cs bootstrap`, pinned 1.5.3)<br>`cs install metals` | Apache-2.0 | A pack server: started with the project JDK as `JAVA_HOME`; it imports sbt builds (Bloop), so it does not start in Safe Mode. See [Groovy and Scala](../user/groovy-scala.md#metals). |

### C#

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [csharp-ls](https://github.com/razzmatazz/csharp-language-server) | `csharp-ls` | C# | `csharp-ls` | `dotnet tool install --global csharp-ls` | MIT | Needs the .NET SDK. |

### Ruby

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Ruby LSP](https://shopify.github.io/ruby-lsp) | `ruby-lsp` | Ruby | `ruby-lsp` | `gem install --user-install ruby-lsp` | MIT |  |

### PHP

| Server | Id | Languages | Command | Install | Licence | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| [Intelephense](https://intelephense.com) | `intelephense` | PHP | `intelephense --stdio` | `npm install -g intelephense` | Proprietary (free core, premium licence key optional) | needs plugin `forge.php`; Free by default. Enter your own licence key to unlock premium features such as rename, code actions and type hierarchy. |
| [Phpactor](https://phpactor.readthedocs.io) | `phpactor` | PHP | `phpactor language-server` | `composer global require phpactor/phpactor`<br>[download](https://github.com/phpactor/phpactor/releases) | MIT | needs plugin `forge.php`; Used when Intelephense is not installed, or when chosen in the PHP settings. |


## Languages

Languages the catalog detects in a project (the project scan reads up to
`langservers.scanLimit` files, skipping ignored files, `node_modules` and `vendor`).

| Language id | Name | Extensions | File names |
| --- | --- | --- | --- |
| `javascript` | JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` |  |
| `typescript` | TypeScript | `.ts`, `.mts`, `.cts`, `.tsx` |  |
| `vue` | Vue | `.vue` |  |
| `svelte` | Svelte | `.svelte` |  |
| `astro` | Astro | `.astro` |  |
| `html` | HTML | `.html`, `.htm`, `.xhtml` |  |
| `css` | CSS | `.css` |  |
| `scss` | SCSS | `.scss` |  |
| `less` | Less | `.less` |  |
| `json` | JSON | `.json`, `.jsonc`, `.json5` | `.babelrc`, `.eslintrc`, `.prettierrc` |
| `yaml` | YAML | `.yml`, `.yaml` |  |
| `toml` | TOML | `.toml` | `Cargo.lock`, `Pipfile` |
| `xml` | XML | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.wsdl`, `.xliff`, `.xlf`, `.svg` | `phpunit.xml.dist`, `phpcs.xml.dist`, `psalm.xml.dist` |
| `markdown` | Markdown | `.md`, `.markdown`, `.mdx` |  |
| `dockerfile` | Dockerfile | `.dockerfile` | `Dockerfile`, `Containerfile` |
| `dockercompose` | Docker Compose |  | `docker-compose.yml`, `docker-compose.yaml`, `compose.yml`, `compose.yaml`, `docker-compose.override.yml`, `docker-compose.override.yaml` |
| `shellscript` | Shell Script | `.sh`, `.bash`, `.ksh`, `.zsh` | `.bashrc`, `.bash_profile`, `.profile` |
| `sql` | SQL | `.sql` |  |
| `latex` | LaTeX | `.tex`, `.sty`, `.cls`, `.ltx`, `.bib` |  |
| `twig` | Twig | `.twig` |  |
| `lua` | Lua | `.lua`, `.rockspec` | `.luacheckrc`, `.luarc.json` |
| `python` | Python | `.py`, `.pyi`, `.pyw` |  |
| `go` | Go | `.go` | `go.mod`, `go.work` |
| `rust` | Rust | `.rs` |  |
| `c` | C | `.c`, `.h` |  |
| `cpp` | C++ | `.cpp`, `.cc`, `.cxx`, `.c++`, `.hpp`, `.hh`, `.hxx`, `.ipp`, `.inl` |  |
| `java` | Java | `.java` |  |
| `csharp` | C# | `.cs`, `.csx` |  |
| `ruby` | Ruby | `.rb`, `.rake`, `.gemspec`, `.ru`, `.erb` | `Gemfile`, `Rakefile`, `Guardfile` |
| `kotlin` | Kotlin | `.kt`, `.kts` |  |
| `scala` | Scala | `.scala`, `.sc`, `.sbt` |  |
| `zig` | Zig | `.zig`, `.zon` |  |
| `php` | PHP | `.php`, `.phtml` |  |
| `php` | PHP | `.php`, `.phtml` |  |

---
title: File types reference
description: Every file type Forge recognizes - bundled types and plugin types with their extensions, file names, shebang interpreters, indentation and template data languages.
order: 30
tags: [file types, reference, syntax]
---

# File types reference

Detection order: file names and path patterns, then the longest matching extension, then the
interpreter of a `#!` line. File names may contain `*` anywhere and `/` to match a path suffix
(`config/services*.yaml`). Bundled types indent with 4 spaces unless a plugin type says
otherwise. See [File types and syntax highlighting](../user/file-types.md).

## Bundled file types

| Id | Name (status bar) | Extensions | File names | Template |
| --- | --- | --- | --- | --- |
| `php` | PHP | `.php`, `.phtml`, `.inc` |  |  |
| `blade` | Blade | `.blade.php` |  |  |
| `json` | JSON | `.json`, `.jsonc`, `.json5`, `.lock`, `.webmanifest`, `.geojson`, `.har`, `.asmdef`, `.asmref`, `.inputactions` | `.prettierrc`, `.babelrc`, `.eslintrc`, `composer.lock`, `.php-cs-fixer.cache` |  |
| `yaml` | YAML | `.yml`, `.yaml`, `.neon` |  |  |
| `sql` | SQL | `.sql`, `.ddl` |  |  |
| `markdown` | Markdown | `.md`, `.markdown` |  |  |
| `javascript` | JavaScript | `.js`, `.mjs`, `.cjs`, `.es6` |  |  |
| `jsx` | JavaScript React | `.jsx` |  |  |
| `typescript` | TypeScript | `.ts`, `.mts`, `.cts` |  |  |
| `tsx` | TypeScript React | `.tsx` |  |  |
| `css` | CSS | `.css`, `.pcss`, `.postcss`, `.uss` |  |  |
| `scss` | SCSS | `.scss` |  |  |
| `sass` | Sass | `.sass` |  |  |
| `less` | Less | `.less` |  |  |
| `stylus` | Stylus | `.styl`, `.stylus` |  |  |
| `html` | HTML | `.html`, `.htm`, `.xhtml` |  |  |
| `svelte` | Svelte | `.svelte` |  |  |
| `astro` | Astro | `.astro` |  |  |
| `graphql` | GraphQL | `.graphql`, `.gql`, `.graphqls` |  |  |
| `prisma` | Prisma | `.prisma` |  |  |
| `vue` | Vue | `.vue` |  |  |
| `twig` | Twig | `.twig`, `.html.twig` |  | yes |
| `smarty` | Smarty | `.tpl`, `.smarty` |  | yes |
| `latte` | Latte | `.latte` |  | yes |
| `gotemplate` | Go Template | `.tmpl`, `.gotmpl`, `.gohtml` |  | yes |
| `jinja` | Jinja2 | `.j2`, `.jinja`, `.jinja2`, `.njk`, `.nunjucks` |  | yes |
| `django` | Django Template | `.djhtml` |  | yes |
| `handlebars` | Handlebars | `.hbs`, `.handlebars`, `.mustache` |  | yes |
| `erb` | ERB | `.erb`, `.rhtml` |  | yes |
| `velocity` | Velocity | `.vm`, `.vtl` |  |  |
| `pug` | Pug | `.pug`, `.jade` |  |  |
| `xml` | XML | `.xml`, `.xsd`, `.xsl`, `.svg`, `.dist`, `.xliff`, `.xlf`, `.uxml` |  |  |
| `dotenv` | Dotenv | `.env` | `.env*`, `*.env` |  |
| `properties` | Properties | `.properties`, `.ini`, `.cfg`, `.conf`, `.editorconfig` | `.editorconfig`, `.gitattributes` |  |
| `shell` | Shell Script | `.sh`, `.bash`, `.zsh` | `.bashrc`, `.zshrc`, `.profile`, `artisan.sh` |  |
| `dockerfile` | Dockerfile | `.dockerfile` | `dockerfile`, `containerfile`, `dockerfile.*` |  |
| `toml` | TOML | `.toml` |  |  |
| `nginx` | Nginx | `.nginx` | `nginx.conf` |  |
| `lua` | Lua | `.lua` |  |  |
| `diff` | Diff | `.diff`, `.patch` |  |  |
| `python` | Python | `.py`, `.pyw`, `.pyi` |  |  |
| `go` | Go | `.go` |  |  |
| `rust` | Rust | `.rs` |  |  |
| `java` | Java | `.java` |  |  |
| `cpp` | C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx`, `.ino` |  |  |
| `c` | C | `.c`, `.h` |  |  |
| `csharp` | C# | `.cs`, `.csx` |  |  |
| `kotlin` | Kotlin | `.kt`, `.kts` |  |  |
| `scala` | Scala | `.scala`, `.sc`, `.sbt` |  |  |
| `dart` | Dart | `.dart` |  |  |
| `objc` | Objective-C | `.m`, `.mm` |  |  |
| `swift` | Swift | `.swift` |  |  |
| `ruby` | Ruby | `.rb`, `.rake`, `.gemspec`, `.ru` | `gemfile`, `rakefile`, `vagrantfile` |  |
| `perl` | Perl | `.pl`, `.pm`, `.t` |  |  |
| `powershell` | PowerShell | `.ps1`, `.psm1`, `.psd1` |  |  |
| `batch` | Batch | `.bat`, `.cmd` |  |  |
| `groovy` | Groovy | `.groovy`, `.gradle`, `.gvy`, `.gy` | `jenkinsfile` |  |
| `haskell` | Haskell | `.hs`, `.lhs` |  |  |
| `erlang` | Erlang | `.erl`, `.hrl` |  |  |
| `elixir` | Elixir | `.ex`, `.exs` |  |  |
| `clojure` | Clojure | `.clj`, `.cljs`, `.cljc`, `.edn` |  |  |
| `r` | R | `.r` |  |  |
| `julia` | Julia | `.jl` |  |  |
| `elm` | Elm | `.elm` |  |  |
| `crystal` | Crystal | `.cr` |  |  |
| `ocaml` | OCaml | `.ml`, `.mli` |  |  |
| `fsharp` | F# | `.fs`, `.fsi`, `.fsx` |  |  |
| `coffeescript` | CoffeeScript | `.coffee` |  |  |
| `lisp` | Common Lisp | `.lisp`, `.lsp`, `.cl` |  |  |
| `scheme` | Scheme | `.scm`, `.ss`, `.rkt` |  |  |
| `tcl` | Tcl | `.tcl` |  |  |
| `vb` | Visual Basic | `.vb`, `.vbs` |  |  |
| `pascal` | Pascal | `.pas`, `.pp` |  |  |
| `fortran` | Fortran | `.f`, `.f90`, `.f95`, `.for` |  |  |
| `verilog` | Verilog | `.v`, `.sv`, `.svh` |  |  |
| `vhdl` | VHDL | `.vhd`, `.vhdl` |  |  |
| `octave` | MATLAB/Octave | `.matlab`, `.oct` |  |  |
| `asm` | Assembly | `.asm`, `.s` |  |  |
| `latex` | LaTeX | `.tex`, `.sty`, `.cls`, `.bib` |  |  |
| `textile` | Textile | `.textile` |  |  |
| `protobuf` | Protocol Buffers | `.proto` |  |  |
| `cmake` | CMake | `.cmake` | `cmakelists.txt` |  |
| `gherkin` | Gherkin | `.feature` |  |  |
| `http` | HTTP Request | `.http`, `.rest` |  |  |
| `puppet` | Puppet | `.pp.puppet` |  |  |
| `makefile` | Makefile | `.mk`, `.make` | `makefile`, `gnumakefile` |  |
| `htaccess` | Apache Config |  | `.htaccess`, `httpd.conf`, `apache2.conf` |  |
| `log` | Log | `.log` |  |  |
| `ignore` | Ignore File |  | `.gitignore`, `.dockerignore`, `.npmignore` |  |

## File types of plugins

Active only while their plugin is active; a plugin type with the id of a bundled type replaces it.

| Id | Name | Plugin | Extensions | File names | Shebang interpreters | Indent |
| --- | --- | --- | --- | --- | --- | --- |
| `shell` | Shell Script | `forge.bash` | `.sh`, `.bash`, `.zsh`, `.ksh`, `.bats` | `.bashrc`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.bash_aliases`, `.profile`, `.zshrc`, `.zprofile`, `.envrc`, `PKGBUILD`, `.xinitrc`, `.xprofile` | `sh`, `bash`, `zsh`, `ksh`, `dash`, `ash` | 2 |
| `python` | Python | `forge.python` | `.py`, `.pyw`, `.pyi`, `.pyx`, `.pxd`, `.gyp` | `SConstruct`, `SConscript`, `wscript`, `.pythonstartup`, `.pythonrc` | `python`, `python3`, `pypy`, `pypy3` | 4 |
| `pip-requirements` | pip Requirements | `forge.python` |  | `requirements*.txt`, `constraints*.txt`, `requirements*.in`, `requirements/*.txt` |  | 4 |
| `django` | Django Template | `forge.django` | `.djhtml` | `templates/*.html`, `templates/*.htm`, `templates/*.txt` |  | 4 |
| `jinja` | Jinja2 | `forge.flask` | `.j2`, `.jinja`, `.jinja2`, `.njk`, `.nunjucks` | `templates/*.html`, `templates/*.htm` |  | 4 |
| `yaml` | YAML | `forge.yaml` | `.yml`, `.yaml`, `.neon` | `.clang-format`, `.clang-tidy`, `.yamllint`, `.gemrc`, `CITATION.cff`, `.sops.yaml` |  | 2 |
| `json` | JSON | `forge.json` | `.json`, `.jsonc`, `.json5`, `.lock`, `.webmanifest`, `.geojson`, `.har`, `.code-workspace`, `.ipynb`, `.avsc`, `.mcmeta`, `.asmdef`, `.asmref`, `.inputactions` | `.babelrc`, `.eslintrc`, `.prettierrc`, `.stylelintrc`, `.jshintrc`, `.swcrc`, `.watchmanconfig`, `.lintstagedrc`, `.htmlhintrc`, `.remarkrc`, `composer.lock`, `.php-cs-fixer.cache`, `.php-cs-fixer.json`, `symfony.lock` |  | 2 |
| `dotenv` | Dotenv | `forge.dotenv` | `.env` | `.env`, `.env.local`, `.env.*.local`, `.env.dist`, `.env.*.dist`, `.env.example`, `.env.sample`, `.env.template`, `.env.defaults`, `.env.dev`, `.env.development`, `.env.prod`, `.env.production`, `.env.stage`, `.env.staging`, `.env.test`, `.env.testing` |  | 2 |
| `ignore` | Ignore File | `forge.ignore` | `.gitignore`, `.dockerignore`, `.npmignore`, `.ignore` | `.*ignore`, `.gitignore_global`, `.git/info/exclude` |  | 2 |
| `dockerfile` | Dockerfile | `forge.docker` | `.dockerfile`, `.containerfile` | `Dockerfile`, `Dockerfile.*`, `Containerfile`, `Containerfile.*` |  | 4 |
| `docker-compose` | Docker Compose | `forge.docker` |  | `docker-compose.yml`, `docker-compose.yaml`, `docker-compose.*.yml`, `docker-compose.*.yaml`, `compose.yml`, `compose.yaml`, `compose.*.yml`, `compose.*.yaml` |  | 2 |
| `postfix` | Postfix main.cf | `forge.postfix` | `.cf` | `main.cf`, `main.cf.proto`, `dynamicmaps.cf` |  | 4 |
| `postfix-master` | Postfix master.cf | `forge.postfix` |  | `master.cf`, `master.cf.proto` |  | 4 |
| `eml` | E-Mail | `forge.eml` | `.eml` |  |  | 4 |
| `ini` | INI | `forge.ini` | `.ini`, `.cfg`, `.inf`, `.desktop`, `.service`, `.socket`, `.timer`, `.mount`, `.target` | `php.ini`, `php.ini-*`, `.editorconfig`, `.gitconfig`, `.gitmodules`, `.npmrc`, `.pylintrc`, `.flake8`, `tox.ini`, `setup.cfg`, `pytest.ini`, `my.cnf`, `*.cnf`, `supervisord.conf`, `*.ini.*`, `pip.conf` |  | 4 |
| `conf` | Config | `forge.conf` | `.conf`, `.config`, `.cnf` | `*.conf.*` |  | 4 |
| `ruby` | Ruby | `forge.ruby` | `.rb`, `.rake`, `.gemspec`, `.ru`, `.rbw`, `.jbuilder`, `.builder` | `Gemfile`, `gems.rb`, `Rakefile`, `rakefile`, `Guardfile`, `Capfile`, `Vagrantfile`, `Brewfile`, `Podfile`, `Fastfile`, `Appfile`, `Steepfile`, `.irbrc`, `.pryrc` | `ruby`, `jruby`, `truffleruby` | 2 |
| `gemfile-lock` | Gemfile.lock | `forge.ruby` |  | `Gemfile.lock`, `gems.locked` |  | 2 |
| `lua` | Lua | `forge.lua` | `.lua`, `.rockspec` | `.luacheckrc`, `.busted` | `lua`, `luajit`, `lua5.1`, `lua5.2`, `lua5.3`, `lua5.4`, `tarantool` | 4 |
| `symfony-services` | Symfony Services | `forge.symfony-services` |  | `config/services*.yaml`, `config/services*.yml`, `config/services/*.yaml`, `config/services/*.yml` |  | 4 |
| `shaderlab` | ShaderLab | `forge.unity` | `.shader` |  |  | 4 |
| `hlsl` | HLSL | `forge.unity` | `.hlsl`, `.cginc`, `.hlslinc`, `.compute`, `.raytrace`, `.cg` |  |  | 4 |
| `gdscript` | GDScript | `forge.godot` | `.gd` |  |  | 4 |
| `godot-resource` | Godot Resource | `forge.godot` | `.tscn`, `.tres`, `.godot`, `.import`, `.gdextension` | `export_presets.cfg`, `override.cfg`, `plugin.cfg` |  | 4 |
| `gdshader` | Godot Shader | `forge.godot` | `.gdshader`, `.gdshaderinc` |  |  | 4 |

## Template data languages

Template types (marked *Template* above) highlight the content between their tags in a data
language: `html` (default), `xml`, `latex`, `css`, `scss`, `less`, `javascript`, `typescript`,
`json`, `sql`, `yaml`, `ini`, `shell`, `go`, `text`.

Chosen from the setting `templates.dataLanguages` (`pattern=language` entries), then from a
compound file name, then HTML. Compound inner extensions:

| Inner extension | Data language |
| --- | --- |
| `html`, `htm`, `xhtml` | HTML |
| `xml`, `svg`, `rss`, `atom` | XML |
| `tex`, `latex`, `sty`, `cls` | LaTeX |
| `css` / `scss` / `less` | CSS / SCSS / Less |
| `js`, `mjs` | JavaScript |
| `ts` | TypeScript |
| `json` | JSON |
| `sql` | SQL |
| `yml`, `yaml` | YAML |
| `ini`, `env`, `conf` | INI / Properties |
| `sh` | Shell Script |
| `go` | Go |
| `txt`, `text`, `md`, `csv`, `eml` | Plain text |

Example: `invoice.tex.twig` → LaTeX, `report.sql.twig` → SQL, `style.css.tpl` → CSS,
`welcome.text.erb` → plain text.

## Soft wraps

Files matching `editor.softWrapFiles` (default `*.md; *.txt; *.rst; *.adoc`) are soft-wrapped
when `editor.softWraps` is on.

---
title: PHP
description: PHP support in PHPForge - the PHP index, Structure tool window, hover documentation, class and symbol search, Generate (Alt+Insert) for constructors, getters and setters, and the PHP settings.
order: 60
tags: [php, phpforge, generate, structure, index]
---

# PHP

PHP support is the **PHPForge** pack (plugin `forge.php`). It combines two engines:

- **Forge's PHP index**, built with tree-sitter in Rust: classes, interfaces, traits, enums,
  functions, constants, members, PHPDoc and attributes of every PHP file in the project. It
  powers the Structure tool window, Go to Class/Symbol, index-based hover and the Generate
  actions, and works without any language server.
- **A PHP language server**, Intelephense or Phpactor, for completion, diagnostics,
  Go to Declaration, hover and formatting. See
  [Language servers](language-servers.md#php-intelephense-and-phpactor).

The pack activates in projects with a `composer.json` or `.php` files.

## The PHP index

Indexing starts when a PHP project opens. The status bar shows **Indexing… 45%** with a
progress bar; afterwards the index follows file changes on its own.

- Files in `vendor/` are not indexed up front. When a class from a dependency is needed (as an
  ancestor, in hover), Forge loads it on demand through Composer's PSR-4 autoload map.
- The index is cached in `<cache dir>/forge/php-index/` (keyed by the project path), so the
  next start only re-reads changed files. Nothing is written into the project.
- **Reindex PHP Files** (Find Action) rebuilds it.
- Extra library folders can be added under Settings | Languages & Frameworks | PHP |
  **Include Path** (`php.includePaths`); they are also passed to the language server.

## Structure

The **Structure** tool window (`Alt+7`, macOS `Cmd+7`) lists the classes of the active file
with their constants, properties and methods, including signatures and visibility. Click an
entry to jump to it.

- **Show Inherited** adds members of parent classes, traits and interfaces, grayed and with
  the declaring class.
- **Sort Alphabetically** sorts within each kind.

![Structure tool window](../../screenshots/php-structure.png)

![Inherited members](../../screenshots/php-structure-inherited.png)

## Go to Class and Go to Symbol

**Go to Class** (`Ctrl+N`, macOS `Cmd+O`) and **Go to Symbol** (`Ctrl+Alt+Shift+N`, macOS
`Cmd+Alt+O`) search the PHP index with camel-hump matching: `OrCo` finds
`OrderController`, `OrderController::sto` finds its `store` method. See
[Search Everywhere](search-everywhere.md).

## Hover documentation

Hovering a class, function, method, property or constant shows its signature, attributes
(for example `#[Route('/orders', name: 'orders')]`) and its PHPDoc (description, `@param`,
`@return`, `@throws`, `@deprecated`, links) as a PHPStorm-style popup. The language server's
answer is merged with the index; when no server runs, or it has nothing for a symbol, the
index answers alone.

![PHP hover](../../screenshots/php-hover.png)

![Hover from the index](../../screenshots/php-hover-index-fallback.png)

## Generate

**Generate…** (`Alt+Insert`, macOS `Cmd+N`) in a PHP file opens a popup at the caret with
what can be generated there:

| Entry | Generates |
| --- | --- |
| Constructor | a constructor with a parameter per chosen field that assigns it (`$this->x = $x`) |
| Getters | `getX()` / `isX()` for booleans |
| Setters | `setX()`; tick **Fluent setters** to return `$this` (`: static`) |
| Getters and Setters | both |
| Override Methods… (`Ctrl+O`) | stubs for inherited methods that are not overridden yet |
| Implement Methods… (`Ctrl+I`) | stubs for abstract and interface methods |
| toString() | a `__toString()` method |
| PHPDoc Comment | a PHPDoc block for the function, method or property at the caret, with `@param`, `@return` and `@throws` |

![Generate popup](../../screenshots/php-generate-popup.png)

Entries that do not apply are disabled (for example Constructor when the class already has
one). Type the underlined letter to pick an entry. For fields and methods a chooser dialog
opens ("Choose Fields to Generate Getters and Setters", "Select Methods to Override") with
**Select All** / **Select None**:

![Choose fields](../../screenshots/php-generate-chooser.png)

The generated code follows the class's indentation, inserts properties after the last
property and methods at the end of the class, and adds the `use` imports that inherited
signatures need.

![Generated getters and setters](../../screenshots/php-generate-result.png)

`Ctrl+O` and `Ctrl+I` work directly in PHP editors as shortcuts for Override/Implement
Methods.

## Settings | Languages & Frameworks | PHP

![PHP settings](../../screenshots/settings-php.png)

| Option | Setting | Default |
| --- | --- | --- |
| PHP language level (7.4 – 8.4) | `php.languageLevel` | 8.3 |
| CLI Interpreter (path to `php`) | `php.interpreter` | `php` |
| Language server (Auto, Intelephense, phpactor, None) | `php.lsp.server` | Auto |
| Max indexed file size for the server (KB) | `php.lsp.maxFileSize` | 5000 |
| When Intelephense is not installed (`ask`, `always`, `never`), see [Installing Intelephense](language-servers.md#installing-intelephense) | `php.intelephense.autoInstall` | `ask` |
| I accept the Intelephense licence | `php.intelephense.licenceAccepted` | off |
| Include Path | `php.includePaths` | empty |

The language level and the interpreter can be stored per project ("For current project" in
the page header). The status bar shows the language level ("PHP 8.3"); click it to open this
page. The page also shows the [Intelephense licence](language-servers.md#intelephense-licence-key-premium).

Sub-pages: **Composer**, **Debug**, **Servers** (see [Xdebug debugging](debugging.md)),
**Laravel**, **Symfony**, **Doctrine**.

### Composer

Settings | Languages & Frameworks | PHP | Composer:

| Option | Setting | Default | Status |
| --- | --- | --- | --- |
| Path to composer | `php.composer.executable` | `composer` | used by Composer run configurations |
| Path to composer.json | `php.composer.json` | `composer.json` | not applied yet |
| Synchronize IDE settings with composer.json | `php.composer.syncWithComposerJson` | on | not applied yet |
| Add packages as libraries | `php.composer.addPackagesAsLibraries` | on | not applied yet |
| Check for package updates on project open | `php.composer.checkUpdates` | off | not applied yet |

Run Composer scripts with a **Composer Script** run configuration, see
[Run/Debug configurations](run-debug.md).

### Code style

Settings | Editor | Code Style | PHP lists tabs and indents, brace placement and blank lines.

> **Note:** these options are not applied yet. Reformat Code uses the language server's
> formatter with its own defaults.

## Running and testing PHP

- **PHP Script** run configurations run a file with the CLI interpreter.
- **PHPUnit** configurations run a class, method, directory or `phpunit.xml`; test classes
  and methods get ▶ icons in the gutter and results appear as a test tree.
- **PHP scratch files** run with the project's autoloader.

See [Run/Debug configurations](run-debug.md), [Xdebug debugging](debugging.md) and
[Scratches](scratches-and-http.md).

## Remote projects

In a [remote project](remote-development.md) the PHP index and the language server run on the
remote host through the Forge agent; Structure, search, hover and Generate work as locally.

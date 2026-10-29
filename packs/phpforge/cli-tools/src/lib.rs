//! Command Line Tool Support of PHPForge, like PHPStorm's (Settings | Tools | Command Line
//! Tool Support, Tools | Run Command… / Ctrl+Shift+X):
//!
//! - [`model`]: the tools (Composer, Laravel Artisan, Symfony Console, WP-CLI, Drush, tools
//!   based on Symfony Console or on a PHP script), their settings codec and auto-detection;
//! - [`definitions`]: commands, arguments and options parsed from `list --format=json`
//!   (Symfony Console, shared by Artisan, Symfony, Composer and Drush) and `wp cli cmd-dump`;
//! - [`cmdline`]: splitting the console input (`alias command args`) with offsets, quoting;
//! - [`complete`]: completion of aliases, commands, options, option values and arguments,
//!   with the help of the command at the cursor;
//! - [`invoke`]: the command line that runs a tool (through PHP or as its own program);
//! - [`cache`]: fingerprints and the file format of the per-tool command cache.
//!
//! Everything is pure (no processes, files or network) and also builds to WebAssembly for
//! browser mode; the app runs the tools through the project backend and the PHP interpreter.

pub mod cache;
pub mod cmdline;
pub mod complete;
pub mod definitions;
pub mod invoke;
pub mod model;

pub use complete::{complete, Completion, CompletionItem, ItemKind};
pub use definitions::{ArgumentDef, CommandDef, OptionDef};
pub use model::{ShowConsoleIn, Tool, ToolKind};

/// Setting keys (see `app/src-tauri/plugins/phpforge.toml`).
pub const TOOLS_KEY: &str = "commandLineTools.tools";
pub const AUTO_DETECT_KEY: &str = "commandLineTools.autoDetect";

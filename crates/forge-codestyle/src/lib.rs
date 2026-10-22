//! Code style schemes (Settings | Editor | Code Style), as in PHPStorm.
//!
//! - [`model`]: the options of the General page and of every language page (PHP, HTML,
//!   CSS/SCSS/Less, JavaScript, TypeScript, JSON, YAML, Lua, Blade, Twig, SQL, XML,
//!   Markdown), grouped into PHPStorm's tabs, with the "Set from…" presets (PSR-12, PER
//!   Coding Style 2.0, Symfony, Laravel, WordPress, Drupal; JavaScript Standard, Airbnb-like,
//!   Prettier; …).
//! - [`scheme`]: the Default (IDE) and Project schemes and further IDE schemes, stored in
//!   the settings layers under the code style keys every page always used; the selected
//!   scheme's values; the settings changes that save edited schemes.
//! - [`io`]: export and import as Forge JSON or as PHPStorm code style XML.
//! - [`format`]: the live preview of each page and the passes Reformat Code runs after the
//!   language server (PHP spaces, braces and conversions, quotes, hex colors, SQL keyword
//!   case, Markdown, Blade/Twig delimiters, blank lines, formatter markers).
//! - [`lsp`]: the options handed to language servers and StyLua.
//!
//! The crate is pure (no file system), so the desktop backend and the browser-mode
//! WebAssembly build share it. EditorConfig overrides the indentation the scheme sets, see
//! `forge_text::resolve_format`.

pub mod format;
pub mod io;
pub mod lsp;
pub mod model;
pub mod scheme;

pub use format::{post_format, preview, PostFormatRequest};
pub use io::{export, import, ExportFormat, ImportedScheme};
pub use model::{model, CodeStyleModel, LanguageSpec, OptionSpec, Values};
pub use scheme::{
    apply_preset, effective_settings, load, plan_save, selected_values, Layers, Scheme,
    SchemeLevel, SchemesState, SettingWrite, WriteScope,
};

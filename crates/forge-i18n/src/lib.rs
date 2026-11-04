//! Localization of Forge's UI.
//!
//! - Messages use ICU MessageFormat ([`message`]): arguments, plurals, select, numbers and
//!   dates.
//! - Catalogs are flat JSON files `locales/<locale>/<area>.json` with dotted keys
//!   (`toolwindow.project.title`), bundled into the binary ([`catalog`]). English is the
//!   source; a key missing in another language falls back to English, never to the key.
//! - [`Localizer`] looks keys up along the fallback chain (`de-AT` → `de` → `en`); the
//!   process-wide one ([`set_locale`], [`t`], [`t_or`]) localizes Rust-originated text such as
//!   the main menu, action titles, plugin manifests and errors.
//!
//! The UI calls the same code through WebAssembly (`crates/forge-wasm`, `app/ui/src/i18n`).

pub mod catalog;
pub mod format;
pub mod localizer;
pub mod message;
pub mod plural;

pub use catalog::{
    builtin_messages, check_translation, coverage, locales, slug, Coverage, LocaleInfo, Problem,
    SOURCE_LOCALE,
};
pub use localizer::{
    add_messages, current, normalize_locale, set_locale, set_timezone_offset, t, t_args, t_or,
    Localizer,
};
pub use message::{Args, Message, ParseError};

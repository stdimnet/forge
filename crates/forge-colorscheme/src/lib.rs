//! Editor color schemes, PHPStorm's Settings | Editor | Color Scheme.
//!
//! - [`model`]: [`TextAttributes`] (foreground, background, bold, italic, effect), the
//!   attribute list and the bundled schemes Ember (dark), Anvil (light), High Contrast and
//!   High Contrast Light, which match the theme CSS.
//! - [`scheme`]: user schemes stored in the settings, resolution, the page snapshot and its
//!   changes, and the CSS variables that make the editor show a scheme.
//! - [`icls`]: import and export of IntelliJ `.icls` files.
//! - [`preview`]: the sample code of the page.
//!
//! The crate is pure (no file system), so browser mode runs it as WebAssembly.

pub mod icls;
pub mod model;
pub mod preview;
pub mod scheme;

pub use icls::{export_icls, import_icls, Imported};
pub use model::{Bundled, CssStyle, Effect, TextAttributes, ATTRIBUTES};
pub use scheme::{
    SchemeChange, SchemeSettings, SchemesSnapshot, SettingWrite, UserScheme, SETTING_KEYS,
};

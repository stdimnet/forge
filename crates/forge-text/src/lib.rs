//! Everything Forge needs to know about a text file besides its language: how its lines end,
//! how it is encoded, how it is indented and what happens to it on save.
//!
//! - [`eol`]: line separators (LF, CRLF, CR): detection, normalization to `\n` and back.
//! - [`encoding`]: decoding and encoding of UTF-8 (with or without BOM), UTF-16 LE/BE,
//!   ISO-8859-1 and Windows-1252, with detection from the bytes.
//! - [`indent`]: detecting the indentation a file uses ("4 spaces", "Tab").
//! - [`save`]: "Remove trailing spaces on" (none / modified lines / all lines) and "Ensure
//!   every saved file ends with a line break".
//! - [`editorconfig`]: `.editorconfig` parsing and section matching.
//! - [`format`]: the effective format of a file (indent, separators, save options, guides)
//!   from the code style settings, EditorConfig and the file's content.
//! - [`commit`]: commit message inspections (subject length, blank line, body margin).
//!
//! The crate is pure (no file system), so the desktop backend and the browser-mode
//! WebAssembly build share it.

pub mod commit;
pub mod editorconfig;
pub mod encoding;
pub mod eol;
pub mod file;
pub mod format;
pub mod indent;
pub mod save;

pub use encoding::{decode, encode, DecodedText, Encoding};
pub use eol::LineSeparator;
pub use format::{resolve_format, FileFormat, FormatRequest};
pub use indent::{detect_indent, IndentStyle};
pub use save::{prepare_save, SaveOptions, StripMode};

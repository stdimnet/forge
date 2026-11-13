//! Data behind Forge's non-text file editors, computed in Rust so the desktop app and the
//! browser mode (WebAssembly) share it:
//!
//! - [`image`]: format, dimensions and colour depth of PNG, JPEG, GIF, WebP, ICO, BMP and
//!   SVG files for the image editor, and their bytes as a data URL payload.
//! - [`table`]: CSV/TSV parsing and writing for "Edit as Table" that keeps the delimiter,
//!   the quoting of every cell and the final line break, plus the table edits and sorting.

pub mod image;
pub mod table;

pub use image::{image_data, image_info, is_image_path, ImageData, ImageInfo};
pub use table::{apply_edits, detect_delimiter, parse_table, sort_order, CsvEdit, CsvTable};

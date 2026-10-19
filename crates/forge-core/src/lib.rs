//! Forge core: the language-agnostic foundation every pack builds on.

pub mod error;
pub mod workspace;

pub use error::{ForgeError, Result};
pub use workspace::{FileEntry, FileKind, Workspace};

/// Product name shown in the UI and used for config directories.
pub const APP_NAME: &str = "Forge";

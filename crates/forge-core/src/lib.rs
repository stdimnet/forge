//! Forge core: the language-agnostic foundation every pack builds on.

pub mod error;
pub mod search;
pub mod settings;
pub mod watch;
pub mod workspace;

pub use error::{ForgeError, Result};
pub use search::{fuzzy_match, FileMatch, FuzzyMatch};
pub use settings::{Settings, SettingsChange, SettingsScope};
pub use watch::{FsChanges, ProjectWatcher};
pub use workspace::{FileEntry, FileKind, Workspace};

/// Product name shown in the UI and used for config directories.
pub const APP_NAME: &str = "Forge";

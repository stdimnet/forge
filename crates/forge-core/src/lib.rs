//! Forge core: the language-agnostic foundation every pack builds on.

pub mod error;
pub mod excludes;
pub(crate) mod gitignore;
pub mod paths;
pub mod project_files;
pub mod project_state;
pub mod search;
pub mod settings;
pub mod watch;
pub mod workspace;

pub use error::{ForgeError, Result};
pub use excludes::Excludes;
pub use project_files::{LocalFiles, MemoryFiles, ProjectFiles, SharedFiles};
pub use search::{
    fuzzy_match, fuzzy_match_with, CollectRequest, FileMatch, FuzzyMatch, MatchOptions, SourceFile,
};
pub use settings::{
    parse_settings_toml, settings_to_toml, Settings, SettingsChange, SettingsScope,
};
pub use watch::{FsChanges, ProjectWatcher};
pub use workspace::{FileEntry, FileKind, Workspace};

/// Product name shown in the UI and used for config directories.
pub const APP_NAME: &str = "Forge";

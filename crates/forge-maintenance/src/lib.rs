//! Repairing and resetting the IDE, as PHPStorm's File menu offers it:
//!
//! - **Invalidate Caches…** ([`caches`]): the caches Forge keeps in the user cache and data
//!   directories, the dialog's options and the cleanup plan. The cleanup runs on the next
//!   start ([`pending`]), before anything opens the files, so locked files are no problem.
//! - **Manage IDE Settings** ([`groups`], [`archive`], [`restore`]): the settings groups of
//!   Export / Import Settings, the settings archive (a zip file) and Restore Default
//!   Settings, which backs up the current settings and starts with the defaults.
//! - **Settings Sync** ([`sync`]): the same groups kept in step through a folder (a cloud
//!   drive) or a git repository the user provides, last writer wins per group, with backups.
//!
//! The models (dialog options, settings groups) are pure so browser mode runs them as
//! WebAssembly; everything that touches the disk sits behind the `disk` feature.

pub mod caches;
pub mod groups;

#[cfg(feature = "disk")]
pub mod archive;
#[cfg(feature = "disk")]
pub mod dirs;
#[cfg(feature = "disk")]
pub mod pending;
#[cfg(feature = "disk")]
pub mod restore;
#[cfg(feature = "disk")]
pub mod sync;

pub use caches::{invalidate_model, CacheOption, InvalidateModel};
pub use groups::{group_of_key, settings_groups, SettingsGroup};

//! Comparisons that are not git diffs: the folder diff of Compare Directories.
//!
//! [`compare_listings`] is pure (the browser mode runs it as WebAssembly on the demo
//! files); [`compare_trees`] walks two folders on this machine first. Text diffs of two
//! files are computed by the diff viewer itself.

mod dirs;

pub use dirs::{
    compare_listings, compare_trees, digest, list_tree, DiffStatus, DirCompare, DirDiffEntry,
    ListedEntry, Listing, MAX_ENTRIES,
};

//! IPC commands exposed to the UI. One module per feature area; register new
//! commands in `lib.rs`.

pub mod agent;
pub mod ai;
pub mod doctrine;
pub mod git;
pub mod lsp;
pub mod php;
pub mod project;
pub mod settings;
pub mod symfony;
pub mod workspace;

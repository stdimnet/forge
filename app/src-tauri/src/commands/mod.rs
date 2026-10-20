//! IPC commands exposed to the UI. One module per feature area; register new
//! commands in `lib.rs`.

pub mod agent;
pub mod ai;
pub mod companion;
pub mod db;
pub mod debug;
pub mod doctrine;
pub mod forges;
pub mod git;
pub mod keymap;
pub mod langservers;
pub mod laravel;
pub mod lsp;
pub mod php;
pub mod project;
pub mod run;
pub mod settings;
pub mod symfony;
pub mod terminal;
pub mod workspace;

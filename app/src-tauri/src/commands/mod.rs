//! IPC commands exposed to the UI. One module per feature area; register new
//! commands in `lib.rs`.

pub mod agent;
pub mod ai;
pub mod codeintel;
pub mod companion;
pub mod db;
pub mod debug;
pub mod doctrine;
pub mod editor;
pub mod forges;
pub mod git;
pub mod keymap;
pub mod langservers;
pub mod laravel;
pub mod library;
pub mod live_templates;
pub mod lsp;
pub mod lua;
pub mod markdown;
pub mod output;
pub mod php;
pub mod project;
pub mod project_state;
pub mod remote;
pub mod run;
pub mod scratches;
pub mod search;
pub mod services;
pub mod settings;
pub mod symfony;
pub mod terminal;
pub mod updates;
pub mod web;
pub mod window;
pub mod workbench;
pub mod workspace;

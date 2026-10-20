//! Forge projects: detect what a project is made of (languages, frameworks, tools) so the
//! matching packs activate.

pub mod detect;

pub use detect::{detect, language_name, Framework, ProjectInfo};

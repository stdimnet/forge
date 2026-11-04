//! Forge projects: detect what a project is made of (languages, frameworks, tools) so the
//! matching packs activate, and create new projects from generators (File | New Project).

pub mod create;
pub mod detect;
pub mod generators;
pub mod templates;

pub use create::{create, find_executable, has_tool, plan_for_system, CreateEvent};
pub use detect::{detect, language_name, Framework, ProjectInfo};
pub use generators::{
    find_generator, generators, generators_with, plan, validate_name, CreateRequest, Generator, GeneratorOption,
    Plan, Step,
};

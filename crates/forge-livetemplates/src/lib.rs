//! Live templates as in PHPStorm: abbreviations that expand into code with variables
//! (`$NAME$`, `$END$`, `$SELECTION$`), expressions (`className()`, `camelCase(NAME)`,
//! `date("yyyy-MM-dd")`, …), tab stops and applicable contexts, plus Surround With
//! (Ctrl+Alt+T) and Unwrap/Remove (Ctrl+Shift+Delete).
//!
//! The crate is pure (no file system, clock or clipboard access) so it also runs as
//! WebAssembly in the browser-mode UI. Everything the UI sends and receives uses UTF-16
//! offsets, like CodeMirror; conversion happens at the edges of the public functions.
//!
//! - [`sets`]: the built-in template sets (PHP, JavaScript, Lua, Twig, Laravel, Blade, Symfony)
//!   and how the user's settings (`editor.liveTemplates.*`) change them.
//! - [`context`]: which contexts apply at a position (PHP statement, class member, HTML, …) and
//!   what the code around it knows (class, function, namespace, variables).
//! - [`template`] and [`expr`]: template text and variable expressions.
//! - [`expand`]: expansion by abbreviation, from the Insert Live Template list and Surround With.
//! - [`unwrap`]: the statements around the caret that Unwrap/Remove can take apart.

pub mod context;
pub mod expand;
pub mod expr;
mod lex;
pub mod sets;
pub mod surround;
pub mod template;
mod text;
pub mod unwrap;

pub use context::{analyze, context_tree, CaretInfo, ContextNode, Lang};
pub use expand::{
    applicable, expand, surround_apply, surround_items, ApplicableItem, ApplicableRequest,
    ApplicableResult, Env, ExpandRequest, Expansion, Field, SurroundItem, SurroundRequest,
};
pub use sets::{resolve, Group, Template, TemplateSettings, Variable};
pub use template::variable_names;
pub use unwrap::{unwrap_options, UnwrapOption, UnwrapRequest};

use serde::Serialize;

/// Everything the Live Templates settings page shows: the groups with their templates and
/// the context tree for "Applicable in".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub groups: Vec<Group>,
    pub contexts: Vec<ContextNode>,
    /// Expression functions with a description, for the variables editor.
    pub functions: Vec<(&'static str, &'static str)>,
}

/// The groups after the user's changes, and the context tree.
pub fn catalog(settings: &TemplateSettings) -> Catalog {
    Catalog {
        groups: resolve(settings),
        contexts: context_tree(),
        functions: expr::FUNCTIONS.to_vec(),
    }
}

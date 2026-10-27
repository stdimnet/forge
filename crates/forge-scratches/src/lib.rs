//! Scratch files, PHPStorm's "Scratches and Consoles".
//!
//! Scratches are global: they live in the user config dir (`<config>/forge/scratches/`), not
//! in a project, and are listed at the bottom of the Project tool window.
//!
//! - [`languages`]: the language list of the "New Scratch File" popup (file types from
//!   `forge-filetypes`, ranked with `forge-fuzzy`) and the language → extension mapping.
//! - [`naming`]: auto-numbered names (`scratch.php`, `scratch_1.php`, `buffer1.txt`) and name
//!   validation.
//! - [`store`]: create, list, read, write, rename and delete scratches on disk, plus the data
//!   source attached to SQL scratches.
//! - [`run`]: what runs a scratch (PHP with the project's autoloader, Node, ...) and the
//!   gutter run markers.
//! - [`http`]: the `.http` request parser and the console rendering of responses (PHPStorm's
//!   HTTP Client, lite); [`handler`] runs response handler scripts (`> {% … %}`) as a safe
//!   subset without a JavaScript engine; [`client`] sends the requests (feature `client`).
//!
//! Everything except [`store`] and [`client`] is pure, so browser mode runs it as WebAssembly.

pub mod curl;
pub mod handler;
pub mod http;
pub mod oauth;
pub mod languages;
pub mod naming;
pub mod run;
pub mod store;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client")]
pub mod grpc;
#[cfg(feature = "client")]
pub mod oauth_client;
#[cfg(feature = "client")]
pub mod ws;

pub use languages::{LanguageItem, LanguageRequest, ScratchLanguage};
pub use run::{Marker, MarkerKind, RunCommand, RunContext, Runner};
pub use store::{ScratchFile, ScratchStore};

/// Errors of scratch operations; their messages are shown to the user as they are.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("scratch '{0}' does not exist")]
    NotFound(String),
    #[error("a scratch named '{0}' already exists")]
    Exists(String),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

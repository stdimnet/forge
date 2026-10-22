//! WebForge core (`forge.web`): `package.json` and npm scripts, package manager detection,
//! and the lexical scanning of JavaScript/TypeScript modules and HTML/JSX tags that the
//! React, Vue and Angular plugins (`webforge-fw-*`) build their component indexes on.
//!
//! Everything except [`fs`] is pure, so the browser-mode UI runs the same code as
//! WebAssembly (`crates/forge-wasm/src/web.rs`).

pub mod component;
pub mod extract;
#[cfg(feature = "fs")]
pub mod fs;
pub mod imports;
pub mod js;
pub mod json;
pub mod markup;
pub mod npm;
pub mod package_json;
pub mod pm;
pub mod scope;
pub mod source;
pub mod text;

pub use component::{Component, ComponentKind, Framework, GutterKind, GutterMark, Member};
pub use npm::NpmPackage;
pub use package_json::{PackageJson, Script};
pub use pm::{Detection, PackageManager};
pub use source::{SourceFile, Sources};
pub use text::Location;

/// Navigation targets for a position in a file, found by one of the framework plugins.
pub trait FrameworkIndex {
    /// Components, directives and pipes of the project, with usage counts.
    fn components(&self) -> Vec<Component>;
    /// Go to Declaration at UTF-16 `offset` of `content` (the file at `path`). On a
    /// component's own name it answers its usages, like Ctrl+B in WebStorm.
    fn goto(&self, path: &str, content: &str, offset: usize) -> Vec<Location>;
    /// Line markers for a file.
    fn gutter(&self, path: &str, content: &str) -> Vec<GutterMark>;
}

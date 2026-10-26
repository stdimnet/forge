//! PyForge: Python language support that does not need a language server.
//!
//! - [`pep440`]: versions and version specifiers, ordered and matched like pip.
//! - [`pep508`]: dependency specifications and PEP 503 name normalization.
//! - [`deps`]: dependencies declared in `requirements*.txt` and `pyproject.toml` (PEP 621,
//!   PEP 735 groups, Poetry) with their positions, and "Add requirement" edits.
//! - [`imports`]: import statements, the standard library and module → distribution names.
//! - [`inspections`]: unsatisfied requirements and undeclared imports with quick fixes.
//! - [`completion`]: package name / version completion contexts in dependency files.
//! - [`run`]: the "Python" run configuration's arguments and `__main__` run markers.
//!
//! Everything is pure and also builds to WebAssembly (`crates/forge-wasm/src/python.rs`).

pub mod completion;
pub mod deps;
pub mod imports;
pub mod inspections;
pub mod pep440;
pub mod pep508;
pub mod run;

pub use deps::{Declared, DeclaredIn, DepFile};
pub use inspections::{Fix, ImportContext, InstalledPackage, Problem};
pub use pep440::{Specifier, SpecifierSet, Version};
pub use pep508::{normalize, Requirement};

/// Problems of a file for the editor: dependency files are checked against `installed`,
/// Python files for undeclared imports. `None` for other files.
pub fn inspect(
    path: &str,
    text: &str,
    installed: Option<&[InstalledPackage]>,
    imports: &ImportContext,
) -> Option<Vec<Problem>> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if name == "pyproject.toml" {
        return Some(inspections::check_dep_file(
            &deps::pyproject(text),
            installed,
            &imports.ignored,
        ));
    }
    if deps::is_requirements_file(path) {
        return Some(inspections::check_dep_file(
            &deps::requirements_txt(text),
            installed,
            &imports.ignored,
        ));
    }
    if name.ends_with(".py") || name.ends_with(".pyw") {
        return Some(inspections::check_imports(text, imports));
    }
    None
}

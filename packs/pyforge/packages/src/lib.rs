//! PyForge: Python packages (the Python Packages tool window, "Install requirement" quick
//! fixes and completion in dependency files).
//!
//! - [`listing`]: the installed packages of an environment (`importlib.metadata` script).
//! - [`manager`]: pip, uv, Poetry or Pipenv command lines for install / uninstall / upgrade.
//! - [`pypi`]: PyPI's JSON API and search page.
//! - `net` (feature `net`): PyPI requests cached in `<cache dir>/forge/pypi`.

pub mod listing;
pub mod manager;
#[cfg(feature = "net")]
pub mod net;
pub mod pypi;
#[cfg(all(test, feature = "net"))]
mod testserver;

pub use manager::{Action, Manager};
pub use pypi::{PackageInfo, SearchHit};

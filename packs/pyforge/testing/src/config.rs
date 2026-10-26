//! The run configuration types of the Python test runners and their options:
//!
//! - `pytest`: `scope` = `all` | `directory` | `file` | `test` | `keyword` | `nodes`, with
//!   `directory`, `file`, `test` (the node id after the file: `TestCart::test_add[one]`),
//!   `keyword` (`-k`) and `nodes` (node ids, one per line: Rerun Failed Tests).
//! - `unittest`: `scope` = `all` | `directory` | `file` | `test` | `nodes`, with
//!   `directory` and `pattern` (discovery), `file`, `test` (`TestCart.test_add`) and `nodes`
//!   (dotted test ids, one per line).
//! - `tox`: `environments` (comma separated; empty: tox's `env_list`).
//!
//! Common fields: the interpreter, interpreter options, the working directory,
//! environment variables and `arguments` (extra arguments of the runner).

use forge_runtools::Configuration;

pub const PYTEST: &str = "pytest";
pub const UNITTEST: &str = "unittest";
pub const TOX: &str = "tox";

/// Every Python test configuration type.
pub const TYPES: [&str; 3] = [PYTEST, UNITTEST, TOX];

pub fn pytest_file(path: &str, name: &str) -> Configuration {
    Configuration::new(name, PYTEST)
        .with_option("scope", "file")
        .with_option("file", path)
}

/// A test, class or parametrized case: `test` is the node id after the file.
pub fn pytest_test(path: &str, test: &str, name: &str) -> Configuration {
    Configuration::new(name, PYTEST)
        .with_option("scope", "test")
        .with_option("file", path)
        .with_option("test", test)
}

pub fn unittest_file(path: &str, name: &str) -> Configuration {
    Configuration::new(name, UNITTEST)
        .with_option("scope", "file")
        .with_option("file", path)
}

/// A test case class or method: `test` is `Class` or `Class.method`.
pub fn unittest_test(path: &str, test: &str, name: &str) -> Configuration {
    Configuration::new(name, UNITTEST)
        .with_option("scope", "test")
        .with_option("file", path)
        .with_option("test", test)
}

/// The scope of a configuration, `all` when none is set.
pub fn scope(cfg: &Configuration) -> &str {
    match cfg.option("scope") {
        "" => "all",
        s => s,
    }
}

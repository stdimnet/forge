//! RubyForge's Ruby language support, in the style of RubyMine. Pure Rust (no I/O), shared
//! by the desktop backend and the WebAssembly build of browser mode.
//!
//! - [`version`]: the requested Ruby (`.ruby-version`, `.tool-versions`, `mise.toml`, the
//!   Gemfile's `ruby`), version matching and ordering.
//! - [`gemfile`]: Gemfile and Gemfile.lock, merged into the [`Bundle`] of the Bundler tool
//!   window and External Libraries.
//! - [`rake`]: tasks from `rake --tasks --all` / `rake --where` and from Rakefiles.
//! - [`discovery`]: RSpec examples and groups, Minitest classes, methods and specs.
//! - [`provider`]: RSpec and Minitest for the shared test runner (commands, reruns).
//! - [`command`]: Ruby script, Rake, Bundler, gem, IRB and rdbg command lines.
//! - [`rubocop`]: RuboCop's JSON report as problems, autocorrect arguments.
//! - [`context`]: the project's [`RubyContext`] (interpreter, Bundler, gem executables).

pub mod command;
pub mod context;
pub mod discovery;
pub mod gemfile;
pub mod provider;
pub mod rake;
pub mod rubocop;
pub mod version;

pub use command::{
    bundler_action, gem_install, irb_command, rake_command, rake_tasks_command, rdbg_command,
    script_command, RAKE, RUBY,
};
pub use context::{RubyCommand, RubyContext, CONTEXT_KEY};
pub use discovery::{discover_minitest, discover_rspec, RubyTest, RubyTestKind};
pub use gemfile::{bundle, parse_gemfile, parse_lockfile, Bundle, BundleGem, Gemfile, Lockfile};
pub use provider::{RubyTests, MINITEST, RSPEC};
pub use rake::{parse_tasks, scan_rakefile, RakeTask};
pub use rubocop::{parse_report as parse_rubocop, RubyProblem};
pub use version::{requested_version, VersionRequest};

/// Whether a project file is Ruby (sources, Rake and gem files, Bundler's files).
pub fn is_ruby_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or(&p);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    matches!(ext, "rb" | "rake" | "gemspec" | "ru" | "rbs")
        || matches!(
            name,
            "Gemfile" | "Gemfile.lock" | "Rakefile" | "Guardfile" | "Capfile" | ".irbrc" | ".pryrc"
        )
}

/// A Ruby `def main`-like entry: whether a file is a script worth a ▶ (a shebang naming
/// Ruby, or `if __FILE__ == $0` / `$PROGRAM_NAME`). Returns the 0-based line for the
/// marker.
pub fn script_entry(text: &str) -> Option<u32> {
    let first = text.lines().next().unwrap_or("");
    if first.starts_with("#!") && first.contains("ruby") {
        return Some(0);
    }
    text.lines()
        .position(|l| {
            let t = l.trim();
            t.starts_with("if __FILE__ == $0")
                || t.starts_with("if __FILE__ == $PROGRAM_NAME")
                || t.starts_with("if $PROGRAM_NAME == __FILE__")
                || t.starts_with("if $0 == __FILE__")
        })
        .map(|l| l as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_paths_and_scripts() {
        assert!(
            is_ruby_path("lib/shop.rb")
                && is_ruby_path("Gemfile")
                && is_ruby_path("lib/tasks/db.rake")
        );
        assert!(is_ruby_path("shop.gemspec") && is_ruby_path("config.ru"));
        assert!(!is_ruby_path("README.md"));
        assert_eq!(script_entry("#!/usr/bin/env ruby\nputs 1\n"), Some(0));
        assert_eq!(
            script_entry("def x; end\n\nif __FILE__ == $PROGRAM_NAME\n  x\nend\n"),
            Some(2)
        );
        assert_eq!(script_entry("class A; end\n"), None);
    }
}

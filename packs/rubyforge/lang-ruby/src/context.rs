//! The project's Ruby context: the interpreter, Bundler and the gem executables Forge found,
//! the environment of its commands, and whether commands go through `bundle exec`. The
//! desktop backend builds it (`commands::ruby::context`) and hands it to the test
//! provider through `CommandContext.settings[CONTEXT_KEY]`; command lines are built from
//! it in pure Rust.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The key of the serialized context in `forge_testing::CommandContext::settings`.
pub const CONTEXT_KEY: &str = "ruby.context";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RubyContext {
    /// The Ruby interpreter (empty: `ruby` on the PATH).
    pub ruby: String,
    /// Bundler's `bundle` (empty: `bundle` on the PATH).
    pub bundle: String,
    /// Commands run through `bundle exec` (the project has a Gemfile and the setting
    /// `ruby.bundler.useBundleExec` allows it).
    pub use_bundler: bool,
    /// Gem executables of the interpreter by name (`rspec`, `rake`, `rubocop`, `rdbg`,
    /// `irb`, `ruby-lsp`).
    pub tools: BTreeMap<String, String>,
    /// Environment of every Ruby command (`PATH` with the interpreter's `bin` first,
    /// `GEM_HOME` / `GEM_PATH` of version managers, `ruby.environment`).
    pub env: BTreeMap<String, String>,
    /// Gems of the bundle (`Gemfile.lock`), to tell RSpec from Minitest projects.
    pub gems: Vec<String>,
    pub windows: bool,
}

/// A program with its arguments, a working directory (project relative, empty: the
/// root) and environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyCommand {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl RubyContext {
    pub fn ruby(&self) -> String {
        if self.ruby.trim().is_empty() {
            "ruby".into()
        } else {
            self.ruby.clone()
        }
    }

    pub fn bundle(&self) -> String {
        match self.tools.get("bundle") {
            _ if !self.bundle.trim().is_empty() => self.bundle.clone(),
            Some(b) if !b.is_empty() => b.clone(),
            _ => "bundle".into(),
        }
    }

    /// The executable of a gem's command (`rspec`), or its name for the PATH.
    pub fn tool(&self, name: &str) -> String {
        self.tools
            .get(name)
            .filter(|p| !p.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    pub fn has_gem(&self, name: &str) -> bool {
        self.gems.iter().any(|g| g == name)
    }

    /// A gem command (`rspec`, `rake`, `rubocop`), through `bundle exec` when the project
    /// uses Bundler; `executable` (a configuration's field) wins.
    pub fn gem_command(&self, tool: &str, executable: &str, args: Vec<String>) -> RubyCommand {
        let executable = executable.trim();
        let (program, mut all) = if !executable.is_empty() {
            (executable.to_string(), Vec::new())
        } else if self.use_bundler {
            (self.bundle(), vec!["exec".to_string(), tool.to_string()])
        } else {
            (self.tool(tool), Vec::new())
        };
        all.extend(args);
        RubyCommand {
            program,
            args: all,
            cwd: String::new(),
            env: self.env.clone(),
        }
    }

    /// `ruby <args>`, through `bundle exec` when the project uses Bundler and `bundler`
    /// allows it.
    pub fn ruby_command(&self, args: Vec<String>, bundler: bool) -> RubyCommand {
        let (program, mut all) = if bundler && self.use_bundler {
            (self.bundle(), vec!["exec".to_string(), "ruby".to_string()])
        } else {
            (self.ruby(), Vec::new())
        };
        all.extend(args);
        RubyCommand {
            program,
            args: all,
            cwd: String::new(),
            env: self.env.clone(),
        }
    }

    /// A Bundler command (`bundle install`), never through `bundle exec`.
    pub fn bundler_command(&self, args: &[&str]) -> RubyCommand {
        RubyCommand {
            program: self.bundle(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: String::new(),
            env: self.env.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_go_through_bundler_when_the_project_uses_it() {
        let mut ctx = RubyContext {
            ruby: "/home/anna/.rbenv/versions/3.3.6/bin/ruby".into(),
            ..RubyContext::default()
        };
        ctx.tools.insert(
            "rspec".into(),
            "/home/anna/.rbenv/versions/3.3.6/bin/rspec".into(),
        );
        let c = ctx.gem_command("rspec", "", vec!["spec".into()]);
        assert_eq!(c.program, "/home/anna/.rbenv/versions/3.3.6/bin/rspec");
        assert_eq!(c.args, ["spec"]);
        ctx.use_bundler = true;
        let c = ctx.gem_command("rspec", "", vec!["spec".into()]);
        assert_eq!(
            (c.program.as_str(), c.args.join(" ")),
            ("bundle", "exec rspec spec".into())
        );
        let c = ctx.gem_command("rspec", "bin/rspec", vec![]);
        assert_eq!(c.program, "bin/rspec");
        assert_eq!(
            ctx.ruby_command(vec!["x.rb".into()], false).program,
            ctx.ruby
        );
        assert_eq!(
            ctx.ruby_command(vec!["x.rb".into()], true).args,
            ["exec", "ruby", "x.rb"]
        );
        assert_eq!(ctx.bundler_command(&["install"]).args, ["install"]);
    }
}

//! Rake tasks: the output of `rake --tasks --all` (`rake -AT`, every task with its
//! description, if any) and, without running Rake (Safe Mode, no Ruby), a scan of the
//! Rakefile and `lib/tasks/**/*.rake` / `rakelib/*.rake` for `desc`, `task`, `namespace`
//! and the tasks of `Rake::TestTask`, `RSpec::Core::RakeTask`, `RuboCop::RakeTask` and
//! Bundler's gem tasks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RakeTask {
    /// Full name with namespaces (`db:migrate`).
    pub name: String,
    /// Arguments (`rake release[remote]` → `["remote"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Where it is defined (project-relative), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 0-based line in `file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl RakeTask {
    /// The namespace (`db` of `db:migrate`; empty at the top).
    pub fn namespace(&self) -> &str {
        self.name.rsplit_once(':').map(|(n, _)| n).unwrap_or("")
    }
}

/// `name[a,b]` → (name, args).
fn name_and_args(s: &str) -> (String, Vec<String>) {
    match s.split_once('[') {
        Some((n, a)) => (
            n.to_string(),
            a.trim_end_matches(']')
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        ),
        None => (s.to_string(), Vec::new()),
    }
}

/// Parse `rake --tasks --all` (`rake -AT`) output: `rake build  # Build shop.gem`; tasks
/// without a description have no comment.
pub fn parse_tasks(output: &str) -> Vec<RakeTask> {
    let mut out: Vec<RakeTask> = Vec::new();
    for line in output.lines() {
        let Some(rest) = line.trim_end().strip_prefix("rake ") else {
            continue;
        };
        // Tasks without a description end in `#` (`rake -AT`) or have no comment.
        let rest = rest.strip_suffix(" #").unwrap_or(rest);
        let (head, description) = match rest.split_once(" # ") {
            Some((h, d)) => (h.trim(), d.trim()),
            None => (rest.trim(), ""),
        };
        if head.is_empty() || head.contains(' ') {
            continue;
        }
        let (name, args) = name_and_args(head);
        if out.iter().any(|t| t.name == name) {
            continue;
        }
        out.push(RakeTask {
            name,
            args,
            description: description.to_string(),
            file: None,
            line: None,
        });
    }
    out
}

/// `rake -W` output (`rake build  /p/Rakefile:12:in 'block in <top>'`): where tasks are
/// defined, merged into `tasks` (paths made relative to `root`).
pub fn merge_locations(tasks: &mut [RakeTask], where_output: &str, root: &str) {
    let root = root.trim_end_matches(['/', '\\']).replace('\\', "/");
    for line in where_output.lines() {
        let Some(rest) = line.trim().strip_prefix("rake ") else {
            continue;
        };
        let Some((head, spot)) = rest.split_once(char::is_whitespace) else {
            continue;
        };
        let (name, _) = name_and_args(head.trim());
        let spot = spot.trim().replace('\\', "/");
        let Some((file, after)) = spot.split_once(':') else {
            continue;
        };
        let line_no: Option<u32> = after
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|n| n.parse().ok());
        let rel = file
            .strip_prefix(&format!("{root}/"))
            .unwrap_or(file)
            .to_string();
        if let Some(t) = tasks
            .iter_mut()
            .find(|t| t.name == name && t.file.is_none())
        {
            t.file = Some(rel);
            t.line = line_no.map(|l| l.saturating_sub(1));
        }
    }
}

fn first_string(s: &str) -> Option<String> {
    let s = s.trim();
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = s[1..].find(q)?;
    Some(s[1..1 + end].to_string())
}

/// `:name`, `"name"`, `name: [:deps]`, `:name, [:arg] => :deps` → name.
fn task_name(rest: &str) -> Option<String> {
    let r = rest.trim().trim_start_matches('(').trim();
    if let Some(s) = r.strip_prefix(':') {
        let n: String = s
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '?' || *c == '!')
            .collect();
        return (!n.is_empty()).then_some(n);
    }
    if let Some(s) = first_string(r) {
        return Some(s);
    }
    // `name: :deps` / `name: [:a]`
    let n: String = r
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!n.is_empty() && r[n.len()..].starts_with(':')).then_some(n)
}

/// Tasks of a Rakefile or `.rake` file, found without running Rake.
pub fn scan_rakefile(path: &str, text: &str) -> Vec<RakeTask> {
    let mut out: Vec<RakeTask> = Vec::new();
    // (namespace, indent) of open namespaces.
    let mut namespaces: Vec<(String, usize)> = Vec::new();
    let mut desc = String::new();
    let full = |ns: &[(String, usize)], name: &str| {
        let mut parts: Vec<&str> = ns.iter().map(|(n, _)| n.as_str()).collect();
        parts.push(name);
        parts.join(":")
    };
    let add = |out: &mut Vec<RakeTask>, name: String, description: String, line: usize| {
        if !out.iter().any(|t| t.name == name) {
            out.push(RakeTask {
                name,
                args: Vec::new(),
                description,
                file: Some(path.to_string()),
                line: Some(line as u32),
            });
        }
    };
    for (i, raw) in text.lines().enumerate() {
        let indent = raw.len() - raw.trim_start().len();
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        while namespaces
            .last()
            .is_some_and(|(_, ind)| t == "end" && *ind == indent)
        {
            namespaces.pop();
        }
        if let Some(rest) = t
            .strip_prefix("namespace ")
            .or_else(|| t.strip_prefix("namespace("))
        {
            if let Some(n) = task_name(rest) {
                namespaces.push((n, indent));
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("desc ").or_else(|| t.strip_prefix("desc(")) {
            desc = first_string(rest).unwrap_or_default();
            continue;
        }
        let task_kw = ["task ", "task(", "file ", "multitask "]
            .iter()
            .find_map(|k| t.strip_prefix(k));
        if let Some(rest) = task_kw {
            if let Some(n) = task_name(rest) {
                let (n, args) = match rest.split_once(", [") {
                    Some((_, a)) => (n, a.split(']').next().unwrap_or("").to_string()),
                    None => (n, String::new()),
                };
                let name = full(&namespaces, &n);
                add(&mut out, name.clone(), std::mem::take(&mut desc), i);
                if !args.is_empty() {
                    if let Some(task) = out.iter_mut().find(|t| t.name == name) {
                        task.args = args
                            .split(',')
                            .map(|a| a.trim().trim_start_matches(':').to_string())
                            .filter(|a| !a.is_empty())
                            .collect();
                    }
                }
            }
            continue;
        }
        // Task libraries.
        let lib = [
            ("Rake::TestTask.new", "test", "Run tests"),
            (
                "RSpec::Core::RakeTask.new",
                "spec",
                "Run RSpec code examples",
            ),
            ("RuboCop::RakeTask.new", "rubocop", "Run RuboCop"),
            ("Minitest::TestTask.create", "test", "Run the test suite"),
        ];
        if let Some((prefix, default, d)) = lib.iter().find(|(p, _, _)| t.starts_with(p)) {
            let rest = &t[prefix.len()..];
            let name = task_name(rest).unwrap_or_else(|| default.to_string());
            let description = if desc.is_empty() {
                d.to_string()
            } else {
                std::mem::take(&mut desc)
            };
            add(&mut out, full(&namespaces, &name), description.clone(), i);
            if *default == "rubocop" {
                add(
                    &mut out,
                    full(&namespaces, &format!("{name}:autocorrect")),
                    "Autocorrect RuboCop offenses (only when it's safe)".into(),
                    i,
                );
            }
            continue;
        }
        if t.starts_with("require \"bundler/gem_tasks\"")
            || t.starts_with("require 'bundler/gem_tasks'")
        {
            for (n, d) in [
                ("build", "Build the gem into the pkg directory"),
                ("install", "Build and install the gem into system gems"),
                (
                    "release",
                    "Create a tag, build and push the gem to rubygems.org",
                ),
            ] {
                add(&mut out, n.into(), d.into(), i);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rake_tasks_output() {
        let out = "rake build                # Build shop-0.1.0.gem into the pkg directory\nrake clobber\nrake release[remote]      # Create tag v0.1.0 and build and push shop-0.1.0.gem to rubygems.org\nrake spec                 # Run RSpec code examples\nrake test                 # Run tests\n(in /home/anna/shop)\n";
        let tasks = parse_tasks(out);
        let names: Vec<_> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["build", "clobber", "release", "spec", "test"]);
        assert_eq!(tasks[2].args, ["remote"]);
        assert_eq!(tasks[1].description, "");
        assert_eq!(tasks[3].description, "Run RSpec code examples");
        let mut tasks = tasks;
        merge_locations(
            &mut tasks,
            "rake spec  /home/anna/shop/Rakefile:7:in `<top (required)>'\nrake test  /home/anna/shop/lib/tasks/test.rake:3:in `block in <top>'\n",
            "/home/anna/shop",
        );
        assert_eq!(tasks[3].file.as_deref(), Some("Rakefile"));
        assert_eq!(tasks[3].line, Some(6));
        assert_eq!(tasks[4].file.as_deref(), Some("lib/tasks/test.rake"));
    }

    #[test]
    fn rake_all_tasks_of_a_gem() {
        let text = std::fs::read_to_string(format!(
            "{}/tests/fixtures/rake-tasks-all.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let tasks = parse_tasks(&text);
        assert_eq!(tasks.len(), 15);
        let guard = tasks
            .iter()
            .find(|t| t.name == "release:guard_clean")
            .unwrap();
        assert_eq!(guard.description, "");
        let push = tasks
            .iter()
            .find(|t| t.name == "release:source_control_push")
            .unwrap();
        assert_eq!(push.args, ["remote"]);
        assert_eq!(push.namespace(), "release");
        assert_eq!(
            tasks.iter().find(|t| t.name == "test").unwrap().description,
            "Run the test suite"
        );
    }

    #[test]
    fn rakefile_scan() {
        let text = r#"require "bundler/gem_tasks"
require "rspec/core/rake_task"

RSpec::Core::RakeTask.new(:spec)

desc "Print the cart report"
task :report, [:month] => :environment do |_t, args|
  puts args[:month]
end

namespace :db do
  desc "Load the seed data"
  task seed: :environment do
  end

  namespace :cache do
    task "clear"
  end
end

task default: %i[spec]
"#;
        let tasks = scan_rakefile("Rakefile", text);
        let rows: Vec<_> = tasks
            .iter()
            .map(|t| (t.name.as_str(), t.description.as_str(), t.line))
            .collect();
        assert_eq!(
            rows,
            [
                ("build", "Build the gem into the pkg directory", Some(0)),
                (
                    "install",
                    "Build and install the gem into system gems",
                    Some(0)
                ),
                (
                    "release",
                    "Create a tag, build and push the gem to rubygems.org",
                    Some(0)
                ),
                ("spec", "Run RSpec code examples", Some(3)),
                ("report", "Print the cart report", Some(6)),
                ("db:seed", "Load the seed data", Some(12)),
                ("db:cache:clear", "", Some(16)),
                ("default", "", Some(20)),
            ]
        );
        assert_eq!(tasks[4].args, ["month"]);
        assert_eq!(tasks[5].namespace(), "db");
    }
}

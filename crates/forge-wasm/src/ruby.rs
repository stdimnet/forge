//! RubyForge (`rubyforge-lang-ruby`), mirroring the `ruby_*` IPC commands for the
//! browser-mode mock: the bundle from Gemfile and Gemfile.lock, Rake tasks from `rake
//! --tasks` output or Rakefiles, Ruby and Rake command lines, script ▶ markers, RuboCop
//! reports, Bundler and IRB command lines and the project's requested Ruby version.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use rubyforge_lang_ruby::RubyContext;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct BundleRequest {
    gemfile: String,
    #[serde(default)]
    lock: Option<String>,
}

/// `{ gemfile, lock? }` → `Bundle` (`ruby_bundle`).
#[wasm_bindgen]
pub fn ruby_bundle(request_json: &str) -> String {
    match parse::<BundleRequest>(request_json) {
        Ok(r) => to_json(&rubyforge_lang_ruby::bundle(&r.gemfile, r.lock.as_deref())),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RakeRequest {
    /// `rake --tasks --all` output.
    #[serde(default)]
    output: Option<String>,
    /// Rakefiles and `.rake` files: path → text.
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ output? , files? }` → `RakeTask[]` (`ruby_rake_tasks`).
#[wasm_bindgen]
pub fn ruby_rake_tasks(request_json: &str) -> String {
    match parse::<RakeRequest>(request_json) {
        Ok(r) => {
            let tasks = match r.output {
                Some(o) => rubyforge_lang_ruby::parse_tasks(&o),
                None => {
                    let mut out: Vec<rubyforge_lang_ruby::RakeTask> = Vec::new();
                    for (path, text) in &r.files {
                        for t in rubyforge_lang_ruby::scan_rakefile(path, text) {
                            if !out.iter().any(|x| x.name == t.name) {
                                out.push(t);
                            }
                        }
                    }
                    out.sort_by(|a, b| a.name.cmp(&b.name));
                    out
                }
            };
            to_json(&tasks)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct CommandRequest {
    configuration: Configuration,
    #[serde(default)]
    context: RubyContext,
}

/// `{ configuration, context }` → `RubyCommand` of a Ruby or Rake configuration.
#[wasm_bindgen]
pub fn ruby_command_line(request_json: &str) -> String {
    match parse::<CommandRequest>(request_json).and_then(|r| {
        if r.configuration.type_id == rubyforge_lang_ruby::RAKE {
            rubyforge_lang_ruby::rake_command(&r.configuration, &r.context)
        } else {
            rubyforge_lang_ruby::script_command(&r.configuration, &r.context)
        }
    }) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ActionRequest {
    /// `bundle:<action>`, `rake:<task>`, `irb` or `gem:<name>`.
    action: String,
    #[serde(default)]
    arg: Option<String>,
    #[serde(default)]
    context: RubyContext,
}

/// `{ action, arg?, context }` → `RubyCommand` of a Bundler action, Rake task, gem install
/// or the IRB console.
#[wasm_bindgen]
pub fn ruby_action_command(request_json: &str) -> String {
    let built = parse::<ActionRequest>(request_json).and_then(|r| {
        let ctx = &r.context;
        match r.action.split_once(':') {
            Some(("bundle", a)) => rubyforge_lang_ruby::bundler_action(a, r.arg.as_deref(), ctx),
            Some(("rake", task)) => {
                let cfg = Configuration::new(task, rubyforge_lang_ruby::RAKE)
                    .with_option("task", task)
                    .with_option("taskArgs", r.arg.clone().unwrap_or_default());
                rubyforge_lang_ruby::rake_command(&cfg, ctx)
            }
            Some(("gem", name)) => rubyforge_lang_ruby::gem_install(name, ctx, false),
            None if r.action == "irb" => Ok(rubyforge_lang_ruby::irb_command(ctx, true)),
            _ => Err(format!("unknown action '{}'", r.action)),
        }
    });
    match built {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct TextRequest {
    text: String,
}

/// `{ text }` → the script ▶ line (`ruby_file_targets`), or `null`.
#[wasm_bindgen]
pub fn ruby_script_entry(request_json: &str) -> String {
    match parse::<TextRequest>(request_json) {
        Ok(r) => to_json(&rubyforge_lang_ruby::script_entry(&r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RubocopRequest {
    json: String,
    #[serde(default)]
    root: String,
    #[serde(default)]
    path: Option<String>,
}

/// `{ json, root, path? }` → `RubyProblem[]` (`ruby_rubocop`).
#[wasm_bindgen]
pub fn ruby_rubocop_report(request_json: &str) -> String {
    match parse::<RubocopRequest>(request_json)
        .and_then(|r| rubyforge_lang_ruby::parse_rubocop(&r.json, &r.root, r.path.as_deref()))
    {
        Ok(p) => to_json(&p),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct VersionRequest {
    files: BTreeMap<String, String>,
}

/// `{ files }` → the requested Ruby version (`.ruby-version`, `.tool-versions`, Gemfile).
#[wasm_bindgen]
pub fn ruby_requested_version(request_json: &str) -> String {
    match parse::<VersionRequest>(request_json) {
        Ok(r) => to_json(&rubyforge_lang_ruby::requested_version(&|p| {
            r.files.get(p).cloned()
        })),
        Err(e) => error(e),
    }
}

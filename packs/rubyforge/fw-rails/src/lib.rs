//! RubyForge's Rails plugin (RubyMine's Ruby on Rails support), read statically from the
//! project's files — nothing of the project runs unless the caller asks for `bin/rails
//! routes` / `bin/rails generate` output (trusted projects only):
//!
//! - [`routes`]: `config/routes.rb` (and drawn files) as `bin/rails routes` prints it, or
//!   that output itself;
//! - [`app`]: controllers and mailers with their actions, views and partials, `render`
//!   references, models with tables, associations and scopes;
//! - [`schema`]: tables and columns of `db/schema.rb` and the migrations;
//! - [`database`]: the environments of `config/database.yml` (ERB-lite, `url:`);
//! - [`i18n`]: locale files, the default locale and `t(…)` calls with lazy lookup;
//! - [`hotwire`]: Stimulus controllers and attributes, Turbo frame ids;
//! - [`generators`], [`command`]: generator lists and Rails command lines;
//! - [`editor`]: Go to Declaration, completion, gutter marks, inspections and folds.
//!
//! Pure (files in, data out); also built to WebAssembly (`crates/forge-wasm/src/rails.rs`).

pub mod app;
pub mod command;
pub mod database;
pub mod editor;
pub mod generators;
pub mod hotwire;
pub mod i18n;
pub mod inflect;
pub mod routes;
pub mod schema;
pub mod source;
pub mod yaml;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use app::{Controller, Model, View};
pub use database::DatabaseConfig;
pub use hotwire::StimulusController;
pub use routes::Route;
pub use schema::Table;

/// A position in a project file (0-based line, UTF-16 column).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

impl Location {
    pub fn file(path: &str) -> Self {
        Location {
            path: path.to_string(),
            line: 0,
            character: 0,
        }
    }
}

/// What the caller adds to the files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// The output of `bin/rails routes`, when the project is trusted and the setting asks
    /// for it.
    pub routes_output: Option<String>,
    /// Environment variables for database.yml's ERB (the project's `.env` is read too).
    pub env: BTreeMap<String, String>,
}

/// What the Endpoints and Rails tool windows show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RailsInfo {
    /// A Rails application: `rails` in the Gemfile and `config/application.rb`.
    pub rails: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The application's module (`Blog` of `module Blog` in application.rb).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    /// `bin/rails` exists.
    pub binstub: bool,
    pub routes: Vec<Route>,
    /// `routes.rb` (read statically) or `rails routes` (the command's output).
    pub routes_source: String,
    pub controllers: Vec<Controller>,
    pub models: Vec<Model>,
    pub tables: Vec<Table>,
    pub default_locale: String,
    pub locales: Vec<String>,
    /// Number of translation keys.
    pub translations: usize,
    pub stimulus: Vec<StimulusController>,
    pub databases: Vec<DatabaseConfig>,
    pub views: usize,
}

/// File extensions the analysis reads.
pub const EXTENSIONS: &[&str] = &["rb", "erb", "haml", "slim", "jbuilder", "builder", "yml", "yaml", "js", "ts", "html"];

/// Files without those extensions that the analysis reads.
pub const FILE_NAMES: &[&str] = &["Gemfile", "Gemfile.lock", "bin/rails", ".env"];

const SKIPPED: &[&str] = &[
    "node_modules/",
    "vendor/",
    "tmp/",
    "log/",
    "public/",
    "storage/",
    "coverage/",
    ".git/",
    ".bundle/",
];

/// Whether the analysis needs a file (the caller may skip the others before reading).
pub fn wanted(path: &str) -> bool {
    if SKIPPED.iter().any(|d| path.starts_with(d) || path.contains(&format!("/{d}"))) {
        return false;
    }
    if FILE_NAMES.contains(&path) {
        return true;
    }
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "js" | "ts" => hotwire::identifier(path).is_some(),
        "html" => path.starts_with("app/views/"),
        "yml" | "yaml" => path.starts_with("config/"),
        _ => {
            path.starts_with("app/")
                || path.starts_with("config/")
                || path == "db/schema.rb"
                || path.starts_with("db/migrate/")
                || path.starts_with("lib/generators/")
        }
    }
}

/// `gem "rails"` (or `railties`) in a Gemfile.
pub fn gemfile_has_rails(gemfile: &str) -> bool {
    gemfile.lines().any(|l| {
        let t = source::code(l).trim();
        let Some(rest) = t.strip_prefix("gem ").or_else(|| t.strip_prefix("gem(")) else {
            return false;
        };
        let rest = rest.trim_start();
        ["\"rails\"", "'rails'", "\"railties\"", "'railties'"]
            .iter()
            .any(|g| rest.starts_with(g))
    })
}

/// A Rails application: `rails` in the Gemfile and a `config/application.rb`.
pub fn is_rails_app(read: &dyn Fn(&str) -> Option<String>) -> bool {
    read("config/application.rb").is_some() && read("Gemfile").is_some_and(|g| gemfile_has_rails(&g))
}

/// The locked Rails version (`    rails (7.2.1)` in Gemfile.lock), else the Gemfile's.
pub fn rails_version(gemfile: Option<&str>, lock: Option<&str>) -> Option<String> {
    if let Some(lock) = lock {
        for l in lock.lines() {
            if let Some(v) = l.strip_prefix("    rails (").and_then(|r| r.strip_suffix(')')) {
                return Some(v.to_string());
            }
        }
    }
    let g = gemfile?;
    for l in g.lines() {
        let toks = source::tokens(source::code(l));
        if !toks.first().is_some_and(|t| t.is_ident("gem")) {
            continue;
        }
        let strs: Vec<String> = toks
            .iter()
            .filter_map(|t| match &t.tok {
                source::Tok::Str { value, .. } => Some(value.clone()),
                _ => None,
            })
            .collect();
        if strs.first().is_some_and(|s| s == "rails") {
            if let Some(req) = strs.get(1) {
                let v: String = req
                    .trim_start_matches(|c: char| !c.is_ascii_digit())
                    .to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// The project's files with their analysis.
#[derive(Debug, Clone, Default)]
pub struct Project {
    files: BTreeMap<String, String>,
    pub views: Vec<View>,
    pub catalog: i18n::Catalog,
    pub info: RailsInfo,
}

fn dotenv(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        let t = t.strip_prefix("export ").unwrap_or(t);
        if let Some((k, v)) = t.split_once('=') {
            out.insert(
                k.trim().to_string(),
                v.trim().trim_matches(['"', '\'']).to_string(),
            );
        }
    }
    out
}

impl Project {
    /// Analyse `files` (workspace-relative path → text).
    pub fn new(files: BTreeMap<String, String>, options: Options) -> Self {
        let read = |p: &str| files.get(p).cloned();
        let mut info = RailsInfo {
            rails: is_rails_app(&read),
            version: rails_version(files.get("Gemfile").map(String::as_str), files.get("Gemfile.lock").map(String::as_str)),
            binstub: files.contains_key("bin/rails"),
            ..RailsInfo::default()
        };
        info.app_name = files.get("config/application.rb").and_then(|t| {
            t.lines()
                .find_map(|l| l.trim().strip_prefix("module ").map(|m| m.trim().to_string()))
        });
        let views: Vec<View> = files.keys().filter_map(|p| app::view(p)).collect();
        info.views = views.len();
        info.tables = schema::tables(files.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        let mut models: Vec<Model> = files
            .iter()
            .filter_map(|(p, t)| app::model(p, t))
            .collect();
        let prefix_of = |ns: &str| -> Option<String> {
            let path = format!("app/models/{}.rb", inflect::underscore(ns));
            let text = files.get(&path)?;
            let i = text.find("table_name_prefix")?;
            let rest = &text[i..];
            let q = rest.find(['"', '\''])?;
            let quote = rest[q..].chars().next()?;
            let end = rest[q + 1..].find(quote)?;
            Some(rest[q + 1..q + 1 + end].to_string())
        };
        app::link_models(&mut models, &info.tables, &prefix_of);
        models.sort_by(|a, b| a.name.cmp(&b.name));
        info.models = models;
        info.controllers = files
            .iter()
            .filter_map(|(p, t)| app::controller(p, t))
            .collect();
        if let Some(text) = files.get("config/routes.rb") {
            info.routes = routes::parse_routes("config/routes.rb", text, &read);
            info.routes_source = "routes.rb".into();
        }
        if let Some(out) = options.routes_output.as_deref().filter(|o| !o.trim().is_empty()) {
            let parsed = routes::parse_rails_routes(out, &info.routes);
            if !parsed.is_empty() {
                info.routes = parsed;
                info.routes_source = "rails routes".into();
            }
        }
        for r in &mut info.routes {
            if let Some(c) = info.controllers.iter().find(|c| c.name == r.controller) {
                r.action_location = Some(
                    c.action(&r.action)
                        .map(|a| a.location.clone())
                        .unwrap_or_else(|| c.location.clone()),
                );
            }
        }
        info.default_locale = i18n::default_locale(files.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        let catalog = i18n::Catalog::new(
            files
                .iter()
                .filter(|(p, _)| p.starts_with("config/locales/") && (p.ends_with(".yml") || p.ends_with(".yaml")))
                .map(|(p, t)| (p.as_str(), t.as_str())),
            &info.default_locale,
        );
        info.locales = catalog.locales.clone();
        info.translations = catalog.entries.len();
        info.stimulus = files
            .iter()
            .filter_map(|(p, t)| hotwire::controller(p, t))
            .collect();
        let mut env = files.get(".env").map(|t| dotenv(t)).unwrap_or_default();
        env.extend(options.env.clone());
        if let Some(text) = files.get("config/database.yml") {
            info.databases = database::parse("config/database.yml", text, &|k| env.get(k).cloned());
        }
        Project {
            files,
            views,
            catalog,
            info,
        }
    }

    pub fn file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files.iter().map(|(p, t)| (p.as_str(), t.as_str()))
    }

    pub fn controller(&self, name: &str) -> Option<&Controller> {
        self.info.controllers.iter().find(|c| c.name == name)
    }

    pub fn model(&self, name: &str) -> Option<&Model> {
        let name = name.trim_start_matches("::");
        self.info.models.iter().find(|m| m.name == name)
    }

    pub fn model_at(&self, path: &str) -> Option<&Model> {
        self.info.models.iter().find(|m| m.path == path)
    }

    pub fn stimulus(&self, identifier: &str) -> Option<&StimulusController> {
        self.info.stimulus.iter().find(|s| s.identifier == identifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_and_versions() {
        assert!(gemfile_has_rails("source \"https://rubygems.org\"\ngem \"rails\", \"~> 7.2.1\"\n"));
        assert!(gemfile_has_rails("gem 'railties', '>= 7'\n"));
        assert!(!gemfile_has_rails("gem \"rails-html-sanitizer\"\n# gem \"rails\"\n"));
        assert_eq!(rails_version(Some("gem \"rails\", \"~> 7.2.1\""), None).as_deref(), Some("7.2.1"));
        assert_eq!(rails_version(None, Some("GEM\n  specs:\n    rails (8.0.1)\n      actioncable (= 8.0.1)\n")).as_deref(), Some("8.0.1"));
        assert!(wanted("app/models/article.rb") && wanted("config/locales/en.yml") && wanted("Gemfile"));
        assert!(wanted("app/javascript/controllers/hello_controller.js"));
        assert!(!wanted("app/javascript/application.js") && !wanted("node_modules/x/app/y.rb"));
        assert!(!wanted("test/models/article_test.rb") && !wanted("public/index.html"));
    }
}

//! PHPForge Laravel support, built on the PHP pack's tree-sitter parser.
//!
//! [`Laravel::load`] analyses a project statically:
//! - [`routes`]: `routes/*.php` (groups, prefixes, names, middleware, resources) or the
//!   output of `php artisan route:list --json`;
//! - [`views`]: view names, Blade components (anonymous, class based, Livewire) and custom
//!   Blade directives;
//! - [`config`]: `config()` and `env()` keys;
//! - [`eloquent`]: models with columns from migrations, `$fillable`, casts, relations,
//!   scopes and accessors;
//! - [`translations`]: `__()` keys from PHP and JSON language files.
//!
//! On top of that, [`complete`] offers completion, Go to Declaration and gutter markers
//! for an editor buffer, and [`artisan`] lists and builds artisan commands.

pub mod artisan;
pub mod complete;
pub mod config;
pub mod context;
pub mod eloquent;
pub mod php;
pub mod project;
pub mod routes;
pub mod translations;
pub mod views;

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use complete::{CompletionItem, Completions, GutterMark};
pub use config::KeyEntry;
pub use eloquent::{Model, ModelAttribute, Relation, Scope};
pub use project::{Location, Psr4};
pub use routes::Route;
pub use translations::Translation;
pub use views::{Component, CustomDirective, View};

/// Everything Forge knows about a Laravel application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Laravel {
    #[serde(skip)]
    pub root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub routes: Vec<Route>,
    /// Where the routes come from: `static` or `artisan`.
    pub routes_source: String,
    pub views: Vec<View>,
    pub components: Vec<Component>,
    pub directives: Vec<CustomDirective>,
    pub config: Vec<KeyEntry>,
    pub env: Vec<KeyEntry>,
    pub models: Vec<Model>,
    pub translations: Vec<Translation>,
    #[serde(skip)]
    pub psr4: Psr4,
    #[serde(skip)]
    pub fingerprint: u64,
}

/// Summary for the status bar and the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaravelInfo {
    pub detected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub routes: usize,
    pub views: usize,
    pub components: usize,
    pub models: usize,
    pub config_keys: usize,
    pub translations: usize,
    /// Custom Blade directives (`@money`, and `@admin`/`@elseadmin`/`@endadmin` for `Blade::if`).
    #[serde(default)]
    pub directives: Vec<String>,
}

impl Laravel {
    /// Analyse the project with statically parsed routes.
    pub fn load(root: &Path) -> Self {
        let psr4 = Psr4::load(root);
        Self {
            root: root.to_path_buf(),
            version: project::laravel_version(root),
            routes: routes::parse_routes(root, &psr4),
            routes_source: "static".into(),
            views: views::list_views(root),
            components: views::list_components(root, &psr4),
            directives: views::custom_directives(root),
            config: config::list_config(root),
            env: config::list_env(root),
            models: eloquent::list_models(root),
            translations: translations::list_translations(root),
            fingerprint: fingerprint(root),
            psr4,
        }
    }

    /// Replace the routes with `php artisan route:list --json` output. Returns `false` (and
    /// keeps the static routes) when the JSON cannot be read.
    pub fn use_route_list_json(&mut self, json: &str) -> bool {
        match routes::parse_route_list_json(json, &self.root, &self.psr4, &self.routes) {
            Some(r) => {
                self.routes = r;
                self.routes_source = "artisan".into();
                true
            }
            None => false,
        }
    }

    pub fn info(&self) -> LaravelInfo {
        LaravelInfo {
            detected: true,
            version: self.version.clone(),
            routes: self.routes.len(),
            views: self.views.len(),
            components: self.components.len(),
            models: self.models.len(),
            config_keys: self.config.len(),
            translations: self.translations.len(),
            directives: self.directive_names(),
        }
    }

    /// Names of the custom directives, including the variants `Blade::if` defines.
    pub fn directive_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for d in &self.directives {
            out.push(d.name.clone());
            if d.kind == "if" {
                out.extend(["else", "unless", "end"].map(|p| format!("{p}{}", d.name)));
            }
        }
        out
    }

    /// Model by fully qualified or short class name.
    pub fn model(&self, name: &str) -> Option<&Model> {
        let name = name.trim_start_matches('\\');
        self.models
            .iter()
            .find(|m| m.class.eq_ignore_ascii_case(name))
            .or_else(|| {
                let short = name.rsplit('\\').next().unwrap_or(name);
                self.models
                    .iter()
                    .find(|m| m.name.eq_ignore_ascii_case(short))
            })
    }
}

pub use project::is_laravel;

/// Cheap change detector over the files the analysis reads (paths, sizes, mtimes).
pub fn fingerprint(root: &Path) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in [
        "composer.json",
        "composer.lock",
        ".env",
        ".env.example",
        "bootstrap/app.php",
    ] {
        stamp(&root.join(f), &mut h);
    }
    for dir in [
        "routes",
        "config",
        "app",
        "database/migrations",
        "resources/views",
        "lang",
        "resources/lang",
    ] {
        let base = root.join(dir);
        if !base.is_dir() {
            continue;
        }
        for e in walkdir::WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if e.file_type().is_file() {
                e.path().hash(&mut h);
                stamp(e.path(), &mut h);
            }
        }
    }
    h.finish()
}

fn stamp(path: &Path, h: &mut impl Hasher) {
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(h);
        if let Ok(m) = meta.modified() {
            m.hash(h);
        }
    }
}

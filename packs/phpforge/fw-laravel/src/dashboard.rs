//! The Laravel tool window's overview: version, environment, routes, models, the
//! application's artisan commands, log files and what `ide.json` contributes.

use serde::{Deserialize, Serialize};

use crate::ide_json::CodeGeneration;
use crate::logs::{log_files, LogFile};
use crate::project::{files_under, read, Location};
use crate::Laravel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub name: String,
    pub class: String,
    pub table: String,
    pub attributes: usize,
    pub relations: usize,
    pub location: Location,
}

/// An artisan command defined by the application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCommand {
    pub name: String,
    /// The whole signature (`orders:prune {--days=30}`).
    pub signature: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeJsonSummary {
    pub files: Vec<String>,
    pub completions: usize,
    pub code_generations: Vec<CodeGeneration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaravelOverview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The `php` requirement of composer.json.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub php: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub routes: usize,
    pub views: usize,
    pub components: usize,
    pub translations: usize,
    pub config_keys: usize,
    pub models: Vec<ModelSummary>,
    pub commands: Vec<AppCommand>,
    pub log_files: Vec<LogFile>,
    pub ide_json: IdeJsonSummary,
}

/// The first quoted string after `needle` in `src`, with its offset.
fn quoted_after(src: &str, needle: &str) -> Option<(String, usize)> {
    let at = src.find(needle)? + needle.len();
    let rest = &src[at..];
    let q = rest.find(['\'', '"'])?;
    if rest[..q].contains(';') {
        return None;
    }
    let quote = rest.as_bytes()[q] as char;
    let body = &rest[q + 1..];
    let end = body.find(quote)?;
    Some((body[..end].to_string(), at + q + 1))
}

/// Commands in `app/Console/Commands` (`$signature`, `$description`) and closures in
/// `routes/console.php` (`Artisan::command('name', …)->purpose('…')`).
pub fn app_commands(laravel: &Laravel) -> Vec<AppCommand> {
    let root = &laravel.root;
    let mut out = Vec::new();
    for path in files_under(root, "app/Console/Commands", ".php") {
        let Some(src) = read(root, &path) else { continue };
        let Some((signature, at)) = quoted_after(&src, "$signature") else {
            continue;
        };
        let name = signature.split_whitespace().next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let description = quoted_after(&src, "$description").map(|(d, _)| d).unwrap_or_default();
        out.push(AppCommand {
            name,
            signature: signature.split_whitespace().collect::<Vec<_>>().join(" "),
            description,
            location: Location::at(&path, &src, at),
        });
    }
    if let Some(src) = read(root, "routes/console.php") {
        let mut from = 0;
        while let Some(p) = src[from..].find("Artisan::command(") {
            let at = from + p;
            from = at + 17;
            let Some((signature, off)) = quoted_after(&src[at..], "Artisan::command(") else {
                continue;
            };
            let stmt_end = src[at..].find(";\n").map(|e| at + e).unwrap_or(src.len());
            let description = quoted_after(&src[at..stmt_end], "->purpose(")
                .or_else(|| quoted_after(&src[at..stmt_end], "->describe("))
                .map(|(d, _)| d)
                .unwrap_or_default();
            out.push(AppCommand {
                name: signature.split_whitespace().next().unwrap_or("").to_string(),
                signature,
                description,
                location: Location::at("routes/console.php", &src, at + off),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn overview(laravel: &Laravel) -> LaravelOverview {
    let env = |k: &str| laravel.env.iter().find(|e| e.key == k).map(|e| e.value.trim_matches('"').to_string());
    let php = read(&laravel.root, "composer.json")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|j| j["require"]["php"].as_str().map(String::from));
    let ide = &laravel.ide;
    LaravelOverview {
        version: laravel.version.clone(),
        php,
        app_name: env("APP_NAME"),
        environment: env("APP_ENV"),
        debug: env("APP_DEBUG").map(|d| d == "true" || d == "1"),
        url: env("APP_URL"),
        routes: laravel.routes.len(),
        views: laravel.views.len(),
        components: laravel.components.len(),
        translations: laravel.translations.len(),
        config_keys: laravel.config.len(),
        models: laravel
            .models
            .iter()
            .map(|m| ModelSummary {
                name: m.name.clone(),
                class: m.class.clone(),
                table: m.table.clone(),
                attributes: m.attributes.len(),
                relations: m.relations.len(),
                location: m.location.clone(),
            })
            .collect(),
        commands: app_commands(laravel),
        log_files: log_files(&laravel.root),
        ide_json: IdeJsonSummary {
            files: ide.files.clone(),
            completions: ide.completions.len(),
            code_generations: ide.code_generations.clone(),
            errors: ide.errors.clone(),
        },
    }
}

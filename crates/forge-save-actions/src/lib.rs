//! Actions on Save (Settings | Tools | Actions on Save), like PHPStorm.
//!
//! - [`plan`] decides which actions run when a file is saved: Reformat code (whole file or
//!   changed lines, limited to file types), Optimize imports, Rearrange code, Run code
//!   cleanup and the external tools (Biome, `eslint --fix`, Stylelint, Prettier, Rector, PHP
//!   CS Fixer, phpcbf, Laravel Pint). The web tools run for files that match their "Run for
//!   files" patterns and not when they are disabled.
//!   They run on explicit save (Ctrl+S); with "Also run on autosave" the built-in actions run
//!   on autosave too, the external tools never do.
//! - [`changed::restrict_to_changed_lines`] keeps only the reformatting of lines changed
//!   since the last commit ("Changed lines").
//! - `tools` (feature `backend`) finds the tools in the project (`vendor/bin`,
//!   `node_modules/.bin`) and runs them on the saved file through the project's backend, so
//!   remote projects work.
//!
//! The editor applies the result as one edit, so a single undo reverts every save action.

pub mod changed;
#[cfg(feature = "backend")]
pub mod tools;

use forge_filetypes::glob::matches_glob_list;
use forge_filetypes::FileTypeRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use changed::restrict_to_changed_lines;

pub const REFORMAT_KEY: &str = "actionsOnSave.reformat.enabled";
pub const REFORMAT_SCOPE_KEY: &str = "actionsOnSave.reformat.scope";
pub const REFORMAT_FILE_TYPES_KEY: &str = "actionsOnSave.reformat.fileTypes";
pub const OPTIMIZE_IMPORTS_KEY: &str = "actionsOnSave.optimizeImports";
pub const REARRANGE_KEY: &str = "actionsOnSave.rearrange";
pub const CODE_CLEANUP_KEY: &str = "actionsOnSave.codeCleanup";
pub const RUN_ON_AUTOSAVE_KEY: &str = "actionsOnSave.runOnAutosave";
pub const ESLINT_KEY: &str = "actionsOnSave.eslint";
pub const ESLINT_FILES_KEY: &str = "web.eslint.files";
pub const PRETTIER_KEY: &str = "actionsOnSave.prettier.enabled";
pub const PRETTIER_FILES_KEY: &str = "actionsOnSave.prettier.files";
pub const STYLELINT_KEY: &str = "actionsOnSave.stylelint";
pub const STYLELINT_FILES_KEY: &str = "web.stylelint.files";
pub const BIOME_KEY: &str = "actionsOnSave.biome";
pub const BIOME_FILES_KEY: &str = "web.biome.files";
pub const PHP_CS_FIXER_KEY: &str = "actionsOnSave.phpCsFixer";
pub const PINT_KEY: &str = "actionsOnSave.pint";
pub const PHPCBF_KEY: &str = "actionsOnSave.phpcbf";
pub const RECTOR_KEY: &str = "actionsOnSave.rector";

/// PHPStorm's default of "Run for files" (Prettier).
pub const DEFAULT_PRETTIER_FILES: &str =
    "*.js; *.ts; *.jsx; *.tsx; *.cjs; *.cts; *.mjs; *.mts; *.vue; *.astro";

/// PHPStorm's default "Run for files" of ESLint, Stylelint and Biome.
pub const DEFAULT_ESLINT_FILES: &str = "{**/*,*}.{js,ts,jsx,tsx,cjs,mjs,vue,html}";
pub const DEFAULT_STYLELINT_FILES: &str = "{**/*,*}.{css,scss,less,vue}";
pub const DEFAULT_BIOME_FILES: &str = "{**/*,*}.{js,ts,jsx,tsx,cjs,cts,mjs,mts,json,jsonc,css}";

/// "Reformat code": the whole file or only the lines changed since the last commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReformatScope {
    #[default]
    Whole,
    Changed,
}

/// What started the save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// Ctrl+S (Save All) or File | Save.
    #[default]
    Explicit,
    /// Autosave (frame deactivation, idle time, before running or refactoring).
    Autosave,
}

/// An external tool run on the saved file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    Eslint,
    Prettier,
    Stylelint,
    Biome,
    PhpCsFixer,
    Pint,
    /// PHP_CodeSniffer's fixer.
    Phpcbf,
    Rector,
}

impl Tool {
    /// In the order the tools run (and the Actions on Save page lists them).
    /// The linters fix before Prettier formats; Rector refactors before the code style
    /// fixers format.
    pub const ALL: [Tool; 8] = [
        Tool::Biome,
        Tool::Eslint,
        Tool::Stylelint,
        Tool::Prettier,
        Tool::Rector,
        Tool::PhpCsFixer,
        Tool::Phpcbf,
        Tool::Pint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Eslint => "ESLint",
            Tool::Prettier => "Prettier",
            Tool::Stylelint => "Stylelint",
            Tool::Biome => "Biome",
            Tool::PhpCsFixer => "PHP CS Fixer",
            Tool::Pint => "Laravel Pint",
            Tool::Phpcbf => "phpcbf",
            Tool::Rector => "Rector",
        }
    }

    /// The setting with a custom path of the tool (empty: found in the project).
    pub fn path_key(self) -> &'static str {
        match self {
            Tool::Eslint => "web.eslint.path",
            Tool::Prettier => "web.prettier.path",
            Tool::Stylelint => "web.stylelint.path",
            Tool::Biome => "web.biome.path",
            Tool::PhpCsFixer => "php.tools.phpCsFixer.path",
            Tool::Pint => "php.tools.pint.path",
            Tool::Phpcbf => "php.tools.phpcs.phpcbfPath",
            Tool::Rector => "php.tools.rector.path",
        }
    }

    /// A PHP quality tool (configured on Languages & Frameworks | PHP | Quality Tools).
    pub fn is_php(self) -> bool {
        matches!(
            self,
            Tool::PhpCsFixer | Tool::Pint | Tool::Phpcbf | Tool::Rector
        )
    }

    /// A web code quality tool (Languages & Frameworks | Code Quality Tools).
    pub fn is_web(self) -> bool {
        !self.is_php()
    }
}

/// The Actions on Save settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActionsOnSave {
    pub reformat: bool,
    pub reformat_scope: ReformatScope,
    /// File type ids reformatted on save; empty = all file types.
    pub reformat_file_types: Vec<String>,
    pub optimize_imports: bool,
    pub rearrange: bool,
    pub code_cleanup: bool,
    pub run_on_autosave: bool,
    pub eslint: bool,
    pub eslint_files: String,
    pub prettier: bool,
    pub prettier_files: String,
    pub stylelint: bool,
    pub stylelint_files: String,
    pub biome: bool,
    pub biome_files: String,
    pub php_cs_fixer: bool,
    pub pint: bool,
    pub phpcbf: bool,
    pub rector: bool,
}

impl ActionsOnSave {
    /// Read the settings through `get` (the effective value of a key).
    pub fn from_settings(get: impl Fn(&str) -> Option<Value>) -> Self {
        let flag = |k: &str| get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let text = |k: &str, default: &str| {
            get(k)
                .and_then(|v| v.as_str().map(str::to_string))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        // A disabled web tool never runs on save.
        let web = |key: &str, tool: &str| {
            flag(key)
                && get(&format!("web.{tool}.mode"))
                    .and_then(|v| v.as_str().map(str::to_string))
                    .as_deref()
                    != Some("disabled")
        };
        Self {
            reformat: flag(REFORMAT_KEY),
            reformat_scope: get(REFORMAT_SCOPE_KEY)
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            reformat_file_types: get(REFORMAT_FILE_TYPES_KEY)
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            optimize_imports: flag(OPTIMIZE_IMPORTS_KEY),
            rearrange: flag(REARRANGE_KEY),
            code_cleanup: flag(CODE_CLEANUP_KEY),
            run_on_autosave: flag(RUN_ON_AUTOSAVE_KEY),
            eslint: web(ESLINT_KEY, "eslint"),
            eslint_files: text(ESLINT_FILES_KEY, DEFAULT_ESLINT_FILES),
            prettier: web(PRETTIER_KEY, "prettier"),
            prettier_files: text(PRETTIER_FILES_KEY, DEFAULT_PRETTIER_FILES),
            stylelint: web(STYLELINT_KEY, "stylelint"),
            stylelint_files: text(STYLELINT_FILES_KEY, DEFAULT_STYLELINT_FILES),
            biome: web(BIOME_KEY, "biome"),
            biome_files: text(BIOME_FILES_KEY, DEFAULT_BIOME_FILES),
            php_cs_fixer: flag(PHP_CS_FIXER_KEY),
            pint: flag(PINT_KEY),
            phpcbf: flag(PHPCBF_KEY),
            rector: flag(RECTOR_KEY),
        }
    }
}

/// The actions to run for one save.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlan {
    /// Reformat the file (or only its changed lines).
    pub reformat: Option<ReformatScope>,
    pub optimize_imports: bool,
    pub rearrange: bool,
    pub code_cleanup: bool,
    /// External tools, in the order they run.
    pub tools: Vec<Tool>,
}

impl SavePlan {
    pub fn is_empty(&self) -> bool {
        self.reformat.is_none()
            && !self.optimize_imports
            && !self.rearrange
            && !self.code_cleanup
            && self.tools.is_empty()
    }
}

/// The file type id of `path` among the bundled file types (`php`, `javascript`, ...).
pub fn file_type_of(path: &str) -> Option<String> {
    FileTypeRegistry::with_builtins()
        .detect(path, None, &[])
        .map(|t| t.id.clone())
}

/// Which actions run when `path` is saved.
pub fn plan(path: &str, trigger: Trigger, settings: &ActionsOnSave) -> SavePlan {
    let auto = trigger == Trigger::Autosave;
    if auto && !settings.run_on_autosave {
        return SavePlan::default();
    }
    let file_type = file_type_of(path);
    let php = file_type.as_deref() == Some("php");
    let reformat = settings.reformat
        && file_type.is_some()
        && (settings.reformat_file_types.is_empty()
            || file_type
                .as_ref()
                .is_some_and(|t| settings.reformat_file_types.contains(t)));
    let mut tools = Vec::new();
    // External tools are slow and rewrite the file: only on explicit save. Whether Biome
    // takes over ESLint and Prettier depends on the project's files (decided where they
    // run).
    if !auto {
        let web = [
            (Tool::Biome, settings.biome, &settings.biome_files),
            (Tool::Eslint, settings.eslint, &settings.eslint_files),
            (
                Tool::Stylelint,
                settings.stylelint,
                &settings.stylelint_files,
            ),
            (Tool::Prettier, settings.prettier, &settings.prettier_files),
        ];
        for (tool, on, files) in web {
            if on && matches_glob_list(path, files) {
                tools.push(tool);
            }
        }
        if settings.rector && php {
            tools.push(Tool::Rector);
        }
        if settings.php_cs_fixer && php {
            tools.push(Tool::PhpCsFixer);
        }
        if settings.phpcbf && php {
            tools.push(Tool::Phpcbf);
        }
        if settings.pint && php {
            tools.push(Tool::Pint);
        }
    }
    SavePlan {
        reformat: reformat.then_some(settings.reformat_scope),
        optimize_imports: settings.optimize_imports && php,
        rearrange: settings.rearrange && php,
        code_cleanup: settings.code_cleanup && php,
        tools,
    }
}

/// A file type the "Reformat code" filter offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeChoice {
    pub id: String,
    pub name: String,
}

/// Every bundled file type, sorted by name.
pub fn file_type_choices() -> Vec<FileTypeChoice> {
    let mut out: Vec<FileTypeChoice> = FileTypeRegistry::with_builtins()
        .specs()
        .map(|s| FileTypeChoice {
            id: s.id.clone(),
            name: s.name.clone(),
        })
        .collect();
    out.sort_by_key(|c| c.name.to_lowercase());
    out.dedup_by(|a, b| a.id == b.id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn all_on() -> ActionsOnSave {
        ActionsOnSave {
            reformat: true,
            optimize_imports: true,
            rearrange: true,
            code_cleanup: true,
            eslint: true,
            eslint_files: DEFAULT_ESLINT_FILES.into(),
            prettier: true,
            prettier_files: DEFAULT_PRETTIER_FILES.into(),
            stylelint: true,
            stylelint_files: DEFAULT_STYLELINT_FILES.into(),
            php_cs_fixer: true,
            pint: true,
            phpcbf: true,
            rector: true,
            ..Default::default()
        }
    }

    #[test]
    fn php_files_get_the_php_actions_and_tools() {
        let p = plan("app/Models/User.php", Trigger::Explicit, &all_on());
        assert_eq!(p.reformat, Some(ReformatScope::Whole));
        assert!(p.optimize_imports && p.rearrange && p.code_cleanup);
        assert_eq!(
            p.tools,
            vec![Tool::Rector, Tool::PhpCsFixer, Tool::Phpcbf, Tool::Pint]
        );
    }

    #[test]
    fn blade_templates_are_not_php_code() {
        let p = plan("resources/views/a.blade.php", Trigger::Explicit, &all_on());
        assert!(!p.optimize_imports && p.tools.is_empty());
        assert_eq!(p.reformat, Some(ReformatScope::Whole));
    }

    #[test]
    fn javascript_runs_eslint_then_prettier() {
        let p = plan("src/app.ts", Trigger::Explicit, &all_on());
        assert_eq!(p.tools, vec![Tool::Eslint, Tool::Prettier]);
        assert!(!p.optimize_imports);
        let css = plan("src/app.css", Trigger::Explicit, &all_on());
        assert_eq!(css.tools, vec![Tool::Stylelint]);
        let biome = ActionsOnSave {
            biome: true,
            biome_files: DEFAULT_BIOME_FILES.into(),
            ..all_on()
        };
        assert_eq!(
            plan("src/app.ts", Trigger::Explicit, &biome).tools,
            vec![Tool::Biome, Tool::Eslint, Tool::Prettier]
        );
        assert!(plan("README.md", Trigger::Explicit, &biome)
            .tools
            .is_empty());
    }

    #[test]
    fn disabled_web_tools_do_not_run_on_save() {
        let s = ActionsOnSave::from_settings(|k| match k {
            ESLINT_KEY | STYLELINT_KEY => Some(json!(true)),
            "web.eslint.mode" => Some(json!("disabled")),
            STYLELINT_FILES_KEY => Some(json!("*.pcss")),
            _ => None,
        });
        assert!(!s.eslint && s.stylelint);
        assert_eq!(s.eslint_files, DEFAULT_ESLINT_FILES);
        assert_eq!(
            plan("a.pcss", Trigger::Explicit, &s).tools,
            vec![Tool::Stylelint]
        );
        assert!(plan("a.css", Trigger::Explicit, &s).tools.is_empty());
    }

    #[test]
    fn autosave_runs_only_when_enabled_and_never_tools() {
        assert!(plan("a.php", Trigger::Autosave, &all_on()).is_empty());
        let s = ActionsOnSave {
            run_on_autosave: true,
            ..all_on()
        };
        let p = plan("a.php", Trigger::Autosave, &s);
        assert!(p.optimize_imports && p.tools.is_empty());
    }

    #[test]
    fn reformat_respects_the_file_type_filter() {
        let s = ActionsOnSave {
            reformat_file_types: vec!["javascript".into()],
            ..all_on()
        };
        assert_eq!(plan("a.php", Trigger::Explicit, &s).reformat, None);
        assert!(plan("a.js", Trigger::Explicit, &s).reformat.is_some());
        // Plain text files are never reformatted.
        assert_eq!(
            plan("notes.unknown", Trigger::Explicit, &all_on()).reformat,
            None
        );
    }

    #[test]
    fn nothing_is_on_by_default() {
        let s = ActionsOnSave::from_settings(|_| None);
        assert!(plan("a.php", Trigger::Explicit, &s).is_empty());
        assert_eq!(s.prettier_files, DEFAULT_PRETTIER_FILES);
    }

    #[test]
    fn reads_the_settings() {
        let s = ActionsOnSave::from_settings(|k| match k {
            REFORMAT_KEY | PINT_KEY => Some(json!(true)),
            REFORMAT_SCOPE_KEY => Some(json!("changed")),
            REFORMAT_FILE_TYPES_KEY => Some(json!(["php"])),
            _ => None,
        });
        assert!(s.reformat && s.pint && !s.eslint);
        assert_eq!(s.reformat_scope, ReformatScope::Changed);
        assert_eq!(s.reformat_file_types, vec!["php".to_string()]);
    }

    #[test]
    fn file_type_choices_are_sorted_and_unique() {
        let c = file_type_choices();
        assert!(c.iter().any(|t| t.id == "php" && t.name == "PHP"));
        assert!(c
            .windows(2)
            .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }
}

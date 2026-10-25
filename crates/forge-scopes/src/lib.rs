//! Scopes and file colors, like PHPStorm's (Settings | Appearance & Behavior | Scopes and
//! File Colors).
//!
//! - [`pattern`]: the scope language (`file:app//*.php&&!test:*..*`).
//! - [`Predefined`] scopes (Project Files, Project Production Files, Project Test Files,
//!   Open Files, Changed Files, Current File, …) and [`NamedScope`]s from the settings
//!   (`scopes.shared` in the project, `scopes.local` in the user settings).
//! - [`ScopeFilter`] is what a search carries (Find in Files, Find Usages, Structural Search):
//!   the scope plus the [`ScopeContext`] it is evaluated against; [`Scope`] is it compiled.
//! - [`edit`]: the Include / Exclude buttons of the Scopes page, [`tree`]: its preview,
//!   [`colors`]: file colors per scope.
//! - [`structure`]: excluded folders, resource roots and "Exclude files" patterns
//!   (Settings | Directories, "Mark Directory as"); [`view`]: the Project view's rows
//!   (File Nesting, Compact Middle Packages, sorting).
//!
//! The crate is pure so the desktop app, the remote agent and the browser-mode WebAssembly
//! build share it.

pub mod colors;
pub mod context;
pub mod edit;
pub mod pattern;
pub mod structure;
pub mod tree;
pub mod view;

use serde::{Deserialize, Serialize};

pub use context::{composer_test_roots, is_library_path, is_non_project_path, ScopeContext};
pub use pattern::{parse, Expr, ParseError};

/// PHPStorm's predefined scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Predefined {
    /// Project files and libraries.
    AllPlaces,
    ProjectFiles,
    ProjectProductionFiles,
    ProjectTestFiles,
    OpenFiles,
    ChangedFiles,
    CurrentFile,
    /// Files outside the project (library files, scratches); used by file colors.
    NonProjectFiles,
}

impl Predefined {
    pub const ALL: [Predefined; 8] = [
        Predefined::AllPlaces,
        Predefined::ProjectFiles,
        Predefined::ProjectProductionFiles,
        Predefined::ProjectTestFiles,
        Predefined::OpenFiles,
        Predefined::ChangedFiles,
        Predefined::CurrentFile,
        Predefined::NonProjectFiles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Predefined::AllPlaces => "All Places",
            Predefined::ProjectFiles => "Project Files",
            Predefined::ProjectProductionFiles => "Project Production Files",
            Predefined::ProjectTestFiles => "Project Test Files",
            Predefined::OpenFiles => "Open Files",
            Predefined::ChangedFiles => "Changed Files",
            Predefined::CurrentFile => "Current File",
            Predefined::NonProjectFiles => "Non-Project Files",
        }
    }

    /// The predefined scope a name refers to; "Tests" is the file colors' name for
    /// Project Test Files.
    pub fn from_label(label: &str) -> Option<Predefined> {
        if label == "Tests" {
            return Some(Predefined::ProjectTestFiles);
        }
        Predefined::ALL.into_iter().find(|p| p.label() == label)
    }

    pub fn contains(self, path: &str, ctx: &ScopeContext) -> bool {
        let project = !is_non_project_path(path);
        match self {
            Predefined::AllPlaces => project,
            Predefined::ProjectFiles => project && !is_library_path(path),
            Predefined::ProjectProductionFiles => {
                project && !is_library_path(path) && !ctx.is_test(path)
            }
            Predefined::ProjectTestFiles => project && !is_library_path(path) && ctx.is_test(path),
            Predefined::OpenFiles => ctx.open_files.iter().any(|p| p == path),
            Predefined::ChangedFiles => ctx.changed_files.iter().any(|p| p == path),
            Predefined::CurrentFile => !path.is_empty() && ctx.current_file == path,
            Predefined::NonProjectFiles => !project,
        }
    }

    fn contains_dir(self, dir: &str, ctx: &ScopeContext) -> bool {
        match self {
            // Folders of test roots and conventional test folders.
            Predefined::ProjectTestFiles => {
                !dir.is_empty() && !is_library_path(&format!("{dir}/")) && {
                    let probe = format!("{dir}/\u{1}");
                    ctx.is_test(&probe)
                }
            }
            Predefined::NonProjectFiles => is_non_project_path(dir),
            _ => false,
        }
    }
}

/// A scope defined on the Scopes page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NamedScope {
    pub name: String,
    pub pattern: String,
}

/// Which scope a search uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScopeSpec {
    Predefined(Predefined),
    /// A scope pattern (a named scope resolved by the UI, or typed).
    Pattern(String),
}

impl Default for ScopeSpec {
    fn default() -> Self {
        ScopeSpec::Predefined(Predefined::ProjectFiles)
    }
}

/// A scope and what it is evaluated against; searches carry it to the backend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScopeFilter {
    pub scope: ScopeSpec,
    pub context: ScopeContext,
}

/// A compiled [`ScopeFilter`].
#[derive(Debug, Clone)]
pub struct Scope {
    kind: Kind,
    ctx: ScopeContext,
}

#[derive(Debug, Clone)]
enum Kind {
    Predefined(Predefined),
    Pattern(Option<Expr>),
}

impl Scope {
    pub fn new(filter: &ScopeFilter) -> Result<Self, ParseError> {
        let kind = match &filter.scope {
            ScopeSpec::Predefined(p) => Kind::Predefined(*p),
            ScopeSpec::Pattern(text) => Kind::Pattern(parse(text)?),
        };
        Ok(Scope {
            kind,
            ctx: filter.context.clone(),
        })
    }

    pub fn predefined(p: Predefined, ctx: ScopeContext) -> Self {
        Scope {
            kind: Kind::Predefined(p),
            ctx,
        }
    }

    /// A pattern scope; an invalid pattern is an error.
    pub fn pattern(text: &str, ctx: ScopeContext) -> Result<Self, ParseError> {
        Ok(Scope {
            kind: Kind::Pattern(parse(text)?),
            ctx,
        })
    }

    /// Whether a file (project-relative, or a non-project path) is in the scope.
    pub fn contains(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./");
        match &self.kind {
            Kind::Predefined(p) => p.contains(path, &self.ctx),
            Kind::Pattern(None) => false,
            Kind::Pattern(Some(e)) => !is_non_project_path(path) && e.matches(path, &self.ctx),
        }
    }

    /// Whether a folder belongs to the scope as a whole (file colors of folders).
    pub fn contains_dir(&self, dir: &str) -> bool {
        let dir = dir.trim_matches('/');
        match &self.kind {
            Kind::Predefined(p) => p.contains_dir(dir, &self.ctx),
            Kind::Pattern(None) => false,
            Kind::Pattern(Some(e)) => !is_non_project_path(dir) && e.matches_dir(dir, &self.ctx),
        }
    }
}

/// The scopes the UI offers: predefined ones and the named scopes of the settings, shared
/// (project) ones first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScopeSet {
    pub shared: Vec<NamedScope>,
    pub local: Vec<NamedScope>,
}

/// A scope in a chooser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeEntry {
    /// The name the scope is stored and chosen by.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predefined: Option<Predefined>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// `shared` or `local` for named scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
}

impl ScopeSet {
    /// The named scope or predefined scope called `name`; named scopes win.
    pub fn resolve(&self, name: &str) -> Option<ScopeSpec> {
        self.shared
            .iter()
            .chain(&self.local)
            .find(|s| s.name == name)
            .map(|s| ScopeSpec::Pattern(s.pattern.clone()))
            .or_else(|| Predefined::from_label(name).map(ScopeSpec::Predefined))
    }

    /// Every scope for a chooser: the predefined search scopes, then the named ones.
    pub fn entries(&self) -> Vec<ScopeEntry> {
        let mut out: Vec<ScopeEntry> = Predefined::ALL
            .into_iter()
            .filter(|p| *p != Predefined::NonProjectFiles)
            .map(|p| ScopeEntry {
                name: p.label().into(),
                predefined: Some(p),
                pattern: None,
                storage: None,
            })
            .collect();
        for (storage, list) in [("shared", &self.shared), ("local", &self.local)] {
            for s in list {
                if s.name.trim().is_empty() || out.iter().any(|e| e.name == s.name) {
                    continue;
                }
                out.push(ScopeEntry {
                    name: s.name.clone(),
                    predefined: None,
                    pattern: Some(s.pattern.clone()),
                    storage: Some(storage.into()),
                });
            }
        }
        out
    }

    /// Problems of the named scopes for the Scopes page: empty or duplicate names and
    /// patterns that do not parse, as `(name, message)`.
    pub fn problems(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for s in self.shared.iter().chain(&self.local) {
            let name = s.name.trim();
            if name.is_empty() {
                out.push((s.name.clone(), "The scope name is empty".into()));
            } else if seen.contains(&name) || Predefined::from_label(name).is_some() {
                out.push((
                    s.name.clone(),
                    format!("A scope named '{name}' already exists"),
                ));
            }
            seen.push(name);
            if let Err(e) = parse(&s.pattern) {
                out.push((s.name.clone(), e.to_string()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ScopeContext {
        ScopeContext {
            test_roots: vec!["tests".into()],
            open_files: vec!["app/Models/User.php".into(), "routes/web.php".into()],
            changed_files: vec!["routes/web.php".into()],
            current_file: "app/Models/User.php".into(),
        }
    }

    #[test]
    fn predefined_scopes() {
        let c = ctx();
        let has = |p: Predefined, path: &str| Scope::predefined(p, c.clone()).contains(path);
        assert!(has(Predefined::ProjectFiles, "app/Models/User.php"));
        assert!(!has(Predefined::ProjectFiles, "vendor/a/b.php"));
        assert!(has(Predefined::AllPlaces, "vendor/a/b.php"));
        assert!(has(
            Predefined::ProjectProductionFiles,
            "app/Models/User.php"
        ));
        assert!(!has(
            Predefined::ProjectProductionFiles,
            "tests/Unit/UserTest.php"
        ));
        assert!(has(Predefined::ProjectTestFiles, "tests/Unit/UserTest.php"));
        assert!(has(Predefined::OpenFiles, "routes/web.php"));
        assert!(!has(Predefined::OpenFiles, "app/Http/Kernel.php"));
        assert!(has(Predefined::ChangedFiles, "routes/web.php"));
        assert!(!has(Predefined::ChangedFiles, "app/Models/User.php"));
        assert!(has(Predefined::CurrentFile, "app/Models/User.php"));
        assert!(has(
            Predefined::NonProjectFiles,
            "library://php-stubs/Core/Core_c.php"
        ));
        assert!(!has(Predefined::NonProjectFiles, "app/Models/User.php"));
        let tests = Scope::predefined(Predefined::ProjectTestFiles, c.clone());
        assert!(tests.contains_dir("tests") && tests.contains_dir("tests/Unit"));
        assert!(!tests.contains_dir("app"));
    }

    #[test]
    fn named_scopes_resolve_and_validate() {
        let set = ScopeSet {
            shared: vec![NamedScope {
                name: "Models".into(),
                pattern: "file:app/Models//*".into(),
            }],
            local: vec![
                NamedScope {
                    name: "Broken".into(),
                    pattern: "file:a&&".into(),
                },
                NamedScope {
                    name: "Models".into(),
                    pattern: "file:x".into(),
                },
            ],
        };
        assert_eq!(
            set.resolve("Models"),
            Some(ScopeSpec::Pattern("file:app/Models//*".into()))
        );
        assert_eq!(
            set.resolve("Tests"),
            Some(ScopeSpec::Predefined(Predefined::ProjectTestFiles))
        );
        assert_eq!(set.resolve("Nope"), None);
        let names: Vec<String> = set.entries().into_iter().map(|e| e.name).collect();
        assert_eq!(names[1], "Project Files");
        assert_eq!(&names[names.len() - 2..], ["Models", "Broken"]);
        let problems = set.problems();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[1].1.contains("already exists"));
        let filter: ScopeFilter = serde_json::from_value(serde_json::json!({
            "scope": { "pattern": "file:app//*&&!file:app/Legacy//*" },
            "context": { "testRoots": ["tests"] }
        }))
        .unwrap();
        let scope = Scope::new(&filter).unwrap();
        assert!(scope.contains("app/Models/User.php"));
        assert!(!scope.contains("app/Legacy/Old.php"));
        assert!(!scope.contains("library://file/app/x.php"));
        let filter: ScopeFilter =
            serde_json::from_value(serde_json::json!({ "scope": { "predefined": "openFiles" } }))
                .unwrap();
        assert!(!Scope::new(&filter).unwrap().contains("a.php"));
    }
}

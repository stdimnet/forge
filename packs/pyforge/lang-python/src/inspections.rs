//! Requirements inspections, PyCharm style:
//!
//! - "Package requirement 'X' is not satisfied": a dependency of a requirements file or of
//!   pyproject.toml is not installed in the project's interpreter, or its installed version
//!   does not match (quick fixes "Install requirement" / "Install all missing requirements").
//! - "Package containing module 'x' is not listed in the project requirements": an import of
//!   a third-party package that no dependency file declares (quick fix "Add requirement 'x'
//!   to requirements.txt / pyproject.toml").

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::deps::{Declared, DepFile};
use crate::imports::{self, Import};
use crate::pep440::Version;
use crate::pep508::normalize;

/// A package installed in the interpreter's environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    /// Top-level modules it provides (`top_level.txt` or its files).
    pub modules: Vec<String>,
}

/// A quick fix of a [`Problem`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Fix {
    /// Install these requirements (`pip install 'X>=1'`).
    InstallRequirements {
        title: String,
        requirements: Vec<String>,
    },
    /// Add a requirement to a dependency file (`requirements.txt` or `pyproject.toml`).
    AddRequirement {
        title: String,
        requirement: String,
        target: String,
    },
    /// Stop reporting this package (setting `python.inspections.ignoredPackages`).
    Ignore { title: String, package: String },
}

/// One problem for the editor (0-based line, UTF-16 columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    /// `error`, `warning` or `weak`.
    pub severity: String,
    pub message: String,
    pub code: String,
    pub fixes: Vec<Fix>,
}

fn find<'a>(installed: &'a [InstalledPackage], normalized: &str) -> Option<&'a InstalledPackage> {
    installed.iter().find(|p| normalize(&p.name) == normalized)
}

/// The requirement text to install for a declared dependency (`Django>=4.2,<5`).
fn install_spec(d: &Declared) -> String {
    format!("{}{}", d.name, d.specifier)
}

/// Whether a dependency is satisfied by what is installed.
pub fn satisfied(d: &Declared, installed: &[InstalledPackage]) -> bool {
    let Some(p) = find(installed, &d.normalized) else {
        return false;
    };
    if d.direct || d.specifier.is_empty() {
        return true;
    }
    match (d.specifiers(), Version::parse(&p.version)) {
        (Some(set), Some(v)) => set.contains(&v),
        _ => true,
    }
}

/// Problems of a dependency file: its syntax errors, and dependencies that are not
/// satisfied by `installed` (unknown environment: `None`, only syntax errors). Dependencies
/// with environment markers are not checked, nor `ignored` packages.
pub fn check_dep_file(
    file: &DepFile,
    installed: Option<&[InstalledPackage]>,
    ignored: &[String],
) -> Vec<Problem> {
    let mut out: Vec<Problem> = file
        .errors
        .iter()
        .map(|e| Problem {
            line: e.line,
            start: e.start,
            end: e.end.max(e.start + 1),
            severity: "error".into(),
            message: e.message.clone(),
            code: "python.requirementSyntax".into(),
            fixes: Vec::new(),
        })
        .collect();
    let Some(installed) = installed else {
        return out;
    };
    let ignored: HashSet<String> = ignored.iter().map(|n| normalize(n)).collect();
    let unsatisfied: Vec<&Declared> = file
        .deps
        .iter()
        .filter(|d| d.marker.is_none() && !ignored.contains(&d.normalized))
        .filter(|d| !satisfied(d, installed))
        .collect();
    let all: Vec<String> = unsatisfied.iter().map(|d| install_spec(d)).collect();
    for d in &unsatisfied {
        let shown = if d.written.is_empty() {
            d.name.clone()
        } else {
            format!("{}{}", d.name, d.written.replace(' ', ""))
        };
        let mut message = format!("Package requirement '{shown}' is not satisfied");
        if let Some(p) = find(installed, &d.normalized) {
            message.push_str(&format!(" (installed: {})", p.version));
        }
        let mut fixes = vec![Fix::InstallRequirements {
            title: "Install requirement".into(),
            requirements: vec![install_spec(d)],
        }];
        if all.len() > 1 {
            fixes.push(Fix::InstallRequirements {
                title: "Install all missing requirements".into(),
                requirements: all.clone(),
            });
        }
        fixes.push(Fix::Ignore {
            title: format!("Ignore requirement '{}'", d.name),
            package: d.name.clone(),
        });
        out.push(Problem {
            line: d.line,
            start: d.start,
            end: d.end,
            severity: "warning".into(),
            message,
            code: "python.requirementNotSatisfied".into(),
            fixes,
        });
    }
    out
}

/// What the import inspection needs to know about the project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportContext {
    /// Normalized names of every declared dependency (all files, all groups).
    pub declared: Vec<String>,
    /// The environment's packages (`None`: unknown, the import is still reported when the
    /// name is clearly third-party).
    pub installed: Option<Vec<InstalledPackage>>,
    /// Top-level modules and packages of the project itself (`app`, `tests`, `conftest`).
    pub local_modules: Vec<String>,
    /// The project's own distribution name.
    pub project_name: Option<String>,
    /// The file "Add requirement" writes to (`requirements.txt`, `pyproject.toml`); `None`
    /// when the project has no dependency file: then nothing is reported.
    pub target: Option<String>,
    pub ignored: Vec<String>,
}

/// The distribution providing `import`'s module: an installed package that ships it, else
/// a well-known name, else the module's top-level name.
fn providing<'a>(
    import: &Import,
    installed: &'a [InstalledPackage],
) -> (String, Option<&'a InstalledPackage>) {
    if let Some(p) = installed
        .iter()
        .find(|p| p.modules.iter().any(|m| m == &import.top))
    {
        return (p.name.clone(), Some(p));
    }
    let dist = imports::distribution_for(&import.module);
    let p = find(installed, &normalize(&dist));
    (dist, p)
}

/// "Package containing module 'x' is not listed in the project requirements".
pub fn check_imports(text: &str, ctx: &ImportContext) -> Vec<Problem> {
    let Some(target) = ctx.target.as_deref() else {
        return Vec::new();
    };
    let declared: HashSet<&str> = ctx.declared.iter().map(String::as_str).collect();
    let ignored: HashSet<String> = ctx.ignored.iter().map(|n| normalize(n)).collect();
    let own = ctx.project_name.as_deref().map(normalize);
    let installed = ctx.installed.as_deref().unwrap_or(&[]);
    let mut out = Vec::new();
    for import in imports::scan(text) {
        if import.guarded
            || imports::is_stdlib(&import.top)
            || ctx.local_modules.iter().any(|m| m == &import.top)
        {
            continue;
        }
        let (dist, package) = providing(&import, installed);
        let normalized = normalize(&dist);
        if declared.contains(normalized.as_str())
            || ignored.contains(&normalized)
            || own.as_deref() == Some(normalized.as_str())
            || own.as_deref() == Some(normalize(&import.top).as_str())
        {
            continue;
        }
        // A module nobody installed that is not a known package name may be a local one
        // the scan did not see; stay quiet unless the environment is unknown.
        if package.is_none() && ctx.installed.is_some() && !looks_like_package(&dist) {
            continue;
        }
        let file = target.rsplit('/').next().unwrap_or(target);
        let mut fixes = vec![Fix::AddRequirement {
            title: format!("Add requirement '{dist}' to {file}"),
            requirement: dist.clone(),
            target: target.to_string(),
        }];
        if package.is_none() {
            fixes.push(Fix::InstallRequirements {
                title: format!("Install package {dist}"),
                requirements: vec![dist.clone()],
            });
        }
        fixes.push(Fix::Ignore {
            title: format!("Ignore requirement '{dist}'"),
            package: dist.clone(),
        });
        out.push(Problem {
            line: import.line,
            start: import.start,
            end: import.start + import.top.encode_utf16().count() as u32,
            severity: "weak".into(),
            message: format!(
                "Package containing module '{}' is not listed in the project requirements",
                import.top
            ),
            code: "python.undeclaredImport".into(),
            fixes,
        });
    }
    out
}

/// A name that is a well-known distribution (so worth reporting when not installed).
fn looks_like_package(dist: &str) -> bool {
    imports::KNOWN_DISTRIBUTIONS.iter().any(|(_, d)| *d == dist)
        || POPULAR.contains(&normalize(dist).as_str())
}

/// Frequently used distributions whose module has the same name.
pub const POPULAR: &[&str] = &[
    "requests",
    "numpy",
    "pandas",
    "flask",
    "django",
    "fastapi",
    "pydantic",
    "httpx",
    "pytest",
    "click",
    "typer",
    "rich",
    "uvicorn",
    "celery",
    "redis",
    "boto3",
    "matplotlib",
    "scipy",
    "torch",
    "tensorflow",
    "aiohttp",
    "sqlalchemy",
    "alembic",
    "jinja2",
    "marshmallow",
    "starlette",
    "gunicorn",
    "psutil",
    "tqdm",
    "loguru",
    "arrow",
    "pendulum",
    "orjson",
    "ujson",
    "toml",
    "tomli",
    "black",
    "ruff",
    "mypy",
    "lxml",
    "selenium",
    "scrapy",
    "openai",
    "anthropic",
    "transformers",
    "polars",
    "pyarrow",
    "kafka",
    "pika",
    "paramiko",
    "fabric",
    "invoke",
    "jsonschema",
    "cryptography",
    "bcrypt",
    "passlib",
    "stripe",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::{pyproject, requirements_txt};

    fn pkg(name: &str, version: &str, modules: &[&str]) -> InstalledPackage {
        InstalledPackage {
            name: name.into(),
            version: version.into(),
            modules: modules.iter().map(|m| m.to_string()).collect(),
        }
    }

    #[test]
    fn reports_missing_and_mismatched_requirements() {
        let file = requirements_txt(
            "Django>=4.2,<5\nrequests==2.32.3\nrich\nnumpy; sys_platform == 'linux'\nbad >=\n",
        );
        let installed = vec![
            pkg("django", "5.0.1", &["django"]),
            pkg("Requests", "2.32.3", &[]),
        ];
        let problems = check_dep_file(&file, Some(&installed), &[]);
        let msgs: Vec<(&str, u32)> = problems
            .iter()
            .map(|p| (p.message.as_str(), p.line))
            .collect();
        assert_eq!(
            msgs,
            vec![
                ("'>=' has no version", 4),
                (
                    "Package requirement 'Django>=4.2,<5' is not satisfied (installed: 5.0.1)",
                    0
                ),
                ("Package requirement 'rich' is not satisfied", 2),
            ]
        );
        assert_eq!(problems[0].severity, "error");
        match &problems[1].fixes[1] {
            Fix::InstallRequirements {
                title,
                requirements,
            } => {
                assert_eq!(title, "Install all missing requirements");
                assert_eq!(
                    requirements,
                    &vec!["Django>=4.2,<5".to_string(), "rich".to_string()]
                );
            }
            f => panic!("unexpected {f:?}"),
        }
        assert!(
            check_dep_file(&file, Some(&installed), &["Rich".into(), "django".into()])
                .iter()
                .all(|p| p.code != "python.requirementNotSatisfied")
        );
        assert_eq!(check_dep_file(&file, None, &[]).len(), 1);
    }

    #[test]
    fn poetry_constraints_are_checked() {
        let file =
            pyproject("[tool.poetry.dependencies]\npython = \"^3.11\"\nrequests = \"^2.31\"\n");
        let ok = vec![pkg("requests", "2.32.3", &[])];
        assert!(check_dep_file(&file, Some(&ok), &[]).is_empty());
        let old = vec![pkg("requests", "2.28.0", &[])];
        let p = check_dep_file(&file, Some(&old), &[]);
        assert_eq!(
            p[0].message,
            "Package requirement 'requests^2.31' is not satisfied (installed: 2.28.0)"
        );
    }

    #[test]
    fn reports_undeclared_imports() {
        let src = "import os\nimport requests\nimport yaml\nfrom app.models import User\nimport rich.console\nimport mystery\nimport bs4\n";
        let ctx = ImportContext {
            declared: vec!["requests".into()],
            installed: Some(vec![
                pkg("PyYAML", "6.0.2", &["yaml", "_yaml"]),
                pkg("requests", "2.32.3", &["requests"]),
            ]),
            local_modules: vec!["app".into()],
            project_name: Some("shop".into()),
            target: Some("requirements.txt".into()),
            ignored: vec![],
        };
        let problems = check_imports(src, &ctx);
        let got: Vec<(&str, u32)> = problems
            .iter()
            .map(|p| (p.message.as_str(), p.line))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "Package containing module 'yaml' is not listed in the project requirements",
                    2
                ),
                (
                    "Package containing module 'rich' is not listed in the project requirements",
                    4
                ),
                (
                    "Package containing module 'bs4' is not listed in the project requirements",
                    6
                ),
            ]
        );
        match &problems[0].fixes[0] {
            Fix::AddRequirement {
                title, requirement, ..
            } => {
                assert_eq!(title, "Add requirement 'PyYAML' to requirements.txt");
                assert_eq!(requirement, "PyYAML");
            }
            f => panic!("unexpected {f:?}"),
        }
        // Not installed: also offers the install.
        assert!(matches!(
            &problems[1].fixes[1],
            Fix::InstallRequirements { .. }
        ));
        assert_eq!((problems[0].start, problems[0].end), (7, 11));
        // No dependency file: nothing to add to.
        let none = ImportContext {
            target: None,
            ..ctx
        };
        assert!(check_imports(src, &none).is_empty());
    }
}

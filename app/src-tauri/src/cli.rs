//! The command-line inspector, like PHPStorm's `inspect.sh`:
//!
//! ```text
//! forge inspect <project> <profile.xml|profile name> <output dir> [-format xml|json|html]
//!               [-d <directory>] [-fail-on <severity>] [-v0|-v1|-v2]
//! ```
//!
//! It opens the project headless (no window), builds the PHP index, runs every enabled
//! inspection of the profile that works without the IDE (the PHP index and data-flow
//! inspections, duplicated code) and writes the results: one XML file per inspection plus
//! `.descriptions.xml` (PHPStorm's format), `inspections.json` or `index.html`. The exit
//! code is 0, 1 when `-fail-on` is given and a problem of that severity (or worse) was found,
//! and 2 for wrong arguments or a project that cannot be opened.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_backend::{Backend, LocalBackend};
use forge_core::excludes::StructureConfig;
use forge_core::Settings;
use forge_inspections::report as reports;
use forge_inspections::{severity, Report, RunOptions};
use phpforge_lang_php::project::PhpProjectSettings;

use crate::inspections::{self as engine, InspectScope};

pub const USAGE: &str = "Usage: forge inspect <project> <profile.xml|profile name> <output dir> [-format xml|json|html] [-d <directory>] [-fail-on <severity>] [-v0|-v1|-v2]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub project: PathBuf,
    pub profile: String,
    pub output: PathBuf,
    pub format: String,
    pub directory: Option<String>,
    pub fail_on: Option<String>,
    pub verbosity: u8,
}

/// Parse the arguments after `inspect`.
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut format = "xml".to_string();
    let mut directory = None;
    let mut fail_on = None;
    let mut verbosity = 1;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.trim_start_matches('-') {
            "format" if a.starts_with('-') => {
                format = it
                    .next()
                    .ok_or("-format needs a value")?
                    .to_ascii_lowercase();
                if !matches!(format.as_str(), "xml" | "json" | "html") {
                    return Err(format!("unknown format '{format}' (xml, json or html)"));
                }
            }
            "d" if a.starts_with('-') => {
                directory = Some(it.next().ok_or("-d needs a directory")?.clone())
            }
            "fail-on" if a.starts_with('-') => {
                let level = severity::normalize(it.next().ok_or("-fail-on needs a severity")?);
                fail_on = Some(level);
            }
            "v0" | "v1" | "v2" if a.starts_with('-') => {
                verbosity = a.trim_start_matches("-v").parse().unwrap_or(1)
            }
            "h" | "help" if a.starts_with('-') => return Err(USAGE.into()),
            _ if a.starts_with('-') => return Err(format!("unknown option '{a}'\n{USAGE}")),
            _ => positional.push(a.clone()),
        }
    }
    let [project, profile, output] = positional.as_slice() else {
        return Err(USAGE.into());
    };
    Ok(Args {
        project: PathBuf::from(project),
        profile: profile.clone(),
        output: PathBuf::from(output),
        format,
        directory,
        fail_on,
        verbosity,
    })
}

/// `forge inspect …`: `None` when the arguments are not a command-line inspector call.
pub fn main(argv: &[String]) -> Option<i32> {
    if argv.get(1).map(String::as_str) != Some("inspect") {
        return None;
    }
    let args = match parse(&argv[2..]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return Some(2);
        }
    };
    Some(match inspect(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("forge inspect: {e}");
            2
        }
    })
}

fn log(args: &Args, level: u8, msg: &str) {
    if args.verbosity >= level {
        eprintln!("{msg}");
    }
}

/// Run the inspector; returns the exit code.
pub fn inspect(args: &Args) -> Result<i32, String> {
    let root = forge_core::paths::canonicalize(&args.project)
        .map_err(|e| format!("cannot open {}: {e}", args.project.display()))?;
    let plugins = crate::plugins::builtin_registry().map_err(|e| e.to_string())?;
    let mut settings = Settings::new();
    settings.set_defaults(
        plugins
            .all_settings()
            .map(|s| (s.key.clone(), s.default.clone())),
    );
    if let Some(path) = Settings::default_user_path() {
        let _ = settings.load_user(path);
    }
    settings
        .load_project(&root)
        .map_err(|e| format!("cannot read the project settings: {e}"))?;
    let mut plugins = plugins;
    let disabled: Vec<String> = settings
        .get_as(crate::plugins::DISABLED_PLUGINS_KEY)
        .unwrap_or_default();
    for id in disabled {
        let _ = plugins.set_enabled(&id, false);
    }
    let registry = plugins.inspection_registry();

    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).map_err(|e| e.to_string())?);
    let _ = backend.set_structure(StructureConfig::from_settings(|k| settings.get(k).cloned()));
    let mut profiles = engine::load_profiles(&settings, Some(backend.as_ref()), &registry);

    // A profile file, or a profile's name.
    let profile_path = Path::new(&args.profile);
    let profile_name = if profile_path.is_file() {
        let text = std::fs::read_to_string(profile_path)
            .map_err(|e| format!("cannot read {}: {e}", args.profile))?;
        let profile = forge_inspections::io::import(&text, &registry)?;
        let name = profile.name.clone();
        profiles.profiles.retain(|p| p.name != name);
        profiles.profiles.push(profile);
        name
    } else if profiles.get(&args.profile).is_some() {
        args.profile.clone()
    } else {
        let names: Vec<&str> = profiles.profiles.iter().map(|p| p.name.as_str()).collect();
        return Err(format!(
            "no profile '{}' (profiles: {})",
            args.profile,
            names.join(", ")
        ));
    };
    log(
        args,
        1,
        &format!(
            "Inspecting {} with profile '{profile_name}'",
            root.display()
        ),
    );

    // The PHP index, for the index-based inspections.
    let php_settings = PhpProjectSettings::from_settings(|k| settings.get(k).cloned());
    if backend.php().configure(php_settings).is_ok() {
        let verbose = args.verbosity >= 2;
        let sink: forge_backend::php::ProgressSink = Arc::new(move |done, total| {
            if verbose && (done % 500 == 0 || done == total) {
                eprintln!("Indexing PHP files: {done}/{total}");
            }
        });
        if let Err(e) = backend.php().index_workspace(sink) {
            log(args, 1, &format!("PHP index not built: {e}"));
        }
    }

    let test_roots = backend
        .read_optional("composer.json")
        .ok()
        .flatten()
        .map(|t| forge_scopes::composer_test_roots(&t))
        .unwrap_or_default();
    let scope = match &args.directory {
        Some(d) => InspectScope {
            kind: "directory".into(),
            path: Some(d.trim_matches('/').into()),
            label: format!("Directory '{d}'"),
            ..InspectScope::default()
        },
        None => InspectScope {
            kind: "project".into(),
            label: "Whole project".into(),
            ..InspectScope::default()
        },
    };
    let paths =
        engine::scope_files(backend.as_ref(), &scope, &test_roots).map_err(|e| e.to_string())?;
    let verbose = args.verbosity >= 2;
    let mut progress = |p: forge_inspections::RunProgress| {
        if verbose && !p.text.is_empty() {
            eprintln!("[{}/{}] {}", p.done, p.total, p.text);
        }
    };
    let files = engine::read_files(backend.as_ref(), &paths, &mut progress, &|| false);
    let mut batch = registry.clone();
    batch.inspections.retain(|s| s.batch);
    let scopes = engine::scope_set(&settings);
    let mut report = engine::inspect(
        backend.clone(),
        batch,
        &profiles,
        Some(&profile_name),
        &scopes,
        &test_roots,
        &files,
        &RunOptions::default(),
        &mut progress,
        &|| false,
    );
    report.scope = scope.label.clone();
    write_output(&report, &args.output, &args.format)?;
    log(
        args,
        1,
        &format!(
            "{} problem{} in {} file{}; results in {}",
            report.problems.len(),
            if report.problems.len() == 1 { "" } else { "s" },
            report.files,
            if report.files == 1 { "" } else { "s" },
            args.output.display()
        ),
    );
    let failed = args.fail_on.as_ref().is_some_and(|level| {
        report
            .problems
            .iter()
            .any(|p| severity::rank(&p.severity) <= severity::rank(level))
    });
    Ok(i32::from(failed))
}

/// Write the results into `dir` in `format`.
pub fn write_output(report: &Report, dir: &Path, format: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let files: Vec<(String, String)> = match format {
        "json" => vec![("inspections.json".into(), reports::to_json(report))],
        "html" => vec![("index.html".into(), reports::to_html(report))],
        _ => reports::to_xml_files(report),
    };
    for (name, text) in files {
        let path = dir.join(&name);
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_arguments() {
        let a = parse(&s(&[
            ".",
            "Project Default",
            "out",
            "-format",
            "JSON",
            "-d",
            "src",
            "-fail-on",
            "warning",
            "-v2",
        ]))
        .unwrap();
        assert_eq!(a.profile, "Project Default");
        assert_eq!(a.format, "json");
        assert_eq!(a.directory.as_deref(), Some("src"));
        assert_eq!(a.fail_on.as_deref(), Some("WARNING"));
        assert_eq!(a.verbosity, 2);
        assert!(parse(&s(&[".", "p"])).is_err());
        assert!(parse(&s(&[".", "p", "o", "-format", "pdf"])).is_err());
        assert!(parse(&s(&[".", "p", "o", "-x"])).is_err());
        assert_eq!(main(&s(&["forge", "/some/project"])), None);
        assert_eq!(main(&s(&["forge", "inspect"])), Some(2));
    }

    #[test]
    fn inspects_a_project_headless() {
        let project = tempfile::tempdir().unwrap();
        let src = project.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("Price.php"),
            "<?php\n\nfunction price($net)\n{\n    $rate = 19;\n    $rate = 7;\n    if (false) {\n        return 0;\n    }\n    return $net * (1 + $rate / 100);\n}\n",
        )
        .unwrap();
        let out = tempfile::tempdir().unwrap();
        let args = Args {
            project: project.path().to_path_buf(),
            profile: "Project Default".into(),
            output: out.path().to_path_buf(),
            format: "xml".into(),
            directory: None,
            fail_on: Some("WARNING".into()),
            verbosity: 0,
        };
        assert_eq!(inspect(&args).unwrap(), 1, "a warning was found");
        let unused = std::fs::read_to_string(out.path().join("PhpUnusedAssignment.xml")).unwrap();
        assert!(
            unused.contains("<file>file://$PROJECT_DIR$/src/Price.php</file>"),
            "{unused}"
        );
        assert!(unused.contains("<line>5</line>"), "{unused}");
        assert!(out.path().join("PhpConstantCondition.xml").exists());
        assert!(out.path().join(".descriptions.xml").exists());

        // A profile file that turns the data-flow inspections off.
        let profile = project.path().join("ci.xml");
        std::fs::write(
            &profile,
            "<component name=\"InspectionProjectProfileManager\"><profile version=\"1.0\"><option name=\"myName\" value=\"CI\" />\
             <inspection_tool class=\"PhpUnusedAssignment\" enabled=\"false\" level=\"WARNING\" enabled_by_default=\"false\" />\
             <inspection_tool class=\"PhpConstantCondition\" enabled=\"false\" level=\"WARNING\" enabled_by_default=\"false\" />\
             </profile></component>",
        )
        .unwrap();
        let json = tempfile::tempdir().unwrap();
        let args = Args {
            profile: profile.display().to_string(),
            output: json.path().to_path_buf(),
            format: "json".into(),
            ..args
        };
        assert_eq!(inspect(&args).unwrap(), 0);
        let report: Report = serde_json::from_str(
            &std::fs::read_to_string(json.path().join("inspections.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report.profile, "CI");
        assert!(
            report
                .problems
                .iter()
                .all(|p| p.inspection != "PhpUnusedAssignment"),
            "{report:?}"
        );
        let bad = Args {
            profile: "Nope".into(),
            ..args
        };
        assert!(inspect(&bad).is_err());
    }
}

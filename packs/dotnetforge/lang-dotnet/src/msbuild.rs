//! MSBuild project files (`.csproj`, `.fsproj`, `.vbproj`) and the files the SDK imports
//! next to them (`Directory.Build.props`, `Directory.Packages.props`), read statically the
//! way Rider shows them before a design-time build: properties with `$(Property)`
//! references, simple conditions, package references with central package management,
//! project and framework references, and F# compile order.
//!
//! Conditions are evaluated when both sides are known (`'$(Configuration)' == 'Debug'`
//! with the active configuration); unknown ones count as true, so every target
//! framework's package references are listed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths::{file_name, join, normalize, parent};
use crate::solution::ProjectLanguage;
use crate::xml::{self, Element};

/// A property as written (`<Nullable Condition="…">enable</Nullable>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condition: String,
}

/// An item as written (`<PackageReference Include="xunit" Version="2.7.0" />`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// `PackageReference`, `ProjectReference`, `Compile`, ...
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub include: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub update: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remove: String,
    /// Attributes and child elements other than Include / Update / Remove / Condition.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condition: String,
    /// 0-based line of the element.
    pub line: u32,
}

impl Item {
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.trim().is_empty())
    }
}

/// An MSBuild file read without evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsBuildFile {
    /// `Microsoft.NET.Sdk`, `Microsoft.NET.Sdk.Web` (attribute or `<Sdk Name=…>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<String>,
    pub properties: Vec<Property>,
    pub items: Vec<Item>,
    /// `<Import Project="…">` paths as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
}

fn group_condition(group: &Element, el: &Element) -> String {
    let a = group.attr("Condition").unwrap_or("").trim();
    let b = el.attr("Condition").unwrap_or("").trim();
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("({a}) and ({b})"),
    }
}

/// Read an MSBuild file.
pub fn parse_msbuild(text: &str) -> Result<MsBuildFile, String> {
    let root = xml::parse(text)?;
    if !root.name.eq_ignore_ascii_case("Project") {
        return Err(format!(
            "not an MSBuild file (the root element is <{}>)",
            root.name
        ));
    }
    let mut file = MsBuildFile {
        sdk: root.attr_nonempty("Sdk").map(str::to_string),
        ..MsBuildFile::default()
    };
    for el in &root.children {
        match el.name.as_str() {
            n if n.eq_ignore_ascii_case("Sdk") => {
                if file.sdk.is_none() {
                    file.sdk = el.attr_nonempty("Name").map(str::to_string);
                }
            }
            n if n.eq_ignore_ascii_case("PropertyGroup") => {
                for p in &el.children {
                    file.properties.push(Property {
                        name: p.name.clone(),
                        value: p.deep_text().trim().to_string(),
                        condition: group_condition(el, p),
                    });
                }
            }
            n if n.eq_ignore_ascii_case("ItemGroup") => {
                for i in &el.children {
                    let mut metadata = BTreeMap::new();
                    for (k, v) in &i.attrs {
                        if !["include", "update", "remove", "condition", "exclude"]
                            .contains(&k.to_ascii_lowercase().as_str())
                        {
                            metadata.insert(k.clone(), v.clone());
                        }
                    }
                    for c in &i.children {
                        metadata.insert(c.name.clone(), c.deep_text().trim().to_string());
                    }
                    file.items.push(Item {
                        kind: i.name.clone(),
                        include: i.attr("Include").unwrap_or("").trim().to_string(),
                        update: i.attr("Update").unwrap_or("").trim().to_string(),
                        remove: i.attr("Remove").unwrap_or("").trim().to_string(),
                        metadata,
                        condition: group_condition(el, i),
                        line: i.line,
                    });
                }
            }
            n if n.eq_ignore_ascii_case("Import") => {
                if let Some(p) = el.attr_nonempty("Project") {
                    file.imports.push(p.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(file)
}

// ---- Evaluation --------------------------------------------------------------------------

/// Properties during evaluation (names are case-insensitive).
#[derive(Debug, Clone, Default)]
pub struct Properties {
    values: BTreeMap<String, (String, String)>,
}

impl Properties {
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        self.values
            .insert(name.to_ascii_lowercase(), (name.to_string(), value.into()));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(&name.to_ascii_lowercase())
            .map(|(_, v)| v.as_str())
    }

    /// Replace `$(Name)` references; unknown properties expand to "" like MSBuild, except
    /// in [`Properties::expand_known`].
    pub fn expand(&self, text: &str) -> String {
        self.expand_with(text, true).0
    }

    /// The expansion and whether every reference was known.
    pub fn expand_known(&self, text: &str) -> (String, bool) {
        self.expand_with(text, false)
    }

    fn expand_with(&self, text: &str, empty_unknown: bool) -> (String, bool) {
        let mut out = String::with_capacity(text.len());
        let mut known = true;
        let mut rest = text;
        while let Some(i) = rest.find("$(") {
            out.push_str(&rest[..i]);
            let after = &rest[i + 2..];
            let Some(end) = after.find(')') else {
                out.push_str(&rest[i..]);
                rest = "";
                break;
            };
            let name = after[..end].trim();
            match self.get(name) {
                Some(v) => out.push_str(v),
                None => {
                    known = false;
                    if !empty_unknown {
                        out.push_str(&rest[i..i + 2 + end + 1]);
                    }
                }
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        (out, known)
    }

    pub fn names(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.values().map(|(n, v)| (n.as_str(), v.as_str()))
    }
}

/// Evaluate a condition: `'$(A)' == 'b'`, `!=`, `and`, `or`, `!`, `Exists(...)` (unknown).
/// `None` when it cannot be decided statically.
pub fn eval_condition(cond: &str, props: &Properties) -> Option<bool> {
    let c = cond.trim();
    if c.is_empty() {
        return Some(true);
    }
    let lower = c.to_ascii_lowercase();
    // Split on the outermost ` or ` / ` and `.
    for (op, is_or) in [(" or ", true), (" and ", false)] {
        let mut depth = 0i32;
        let mut quote = false;
        let bytes = lower.as_bytes();
        let mut parts = Vec::new();
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' => quote = !quote,
                b'(' if !quote => depth += 1,
                b')' if !quote => depth -= 1,
                _ => {}
            }
            if depth == 0 && !quote && lower[i..].starts_with(op) {
                parts.push(&c[start..i]);
                start = i + op.len();
                i = start;
                continue;
            }
            i += 1;
        }
        if !parts.is_empty() {
            parts.push(&c[start..]);
            let values: Vec<Option<bool>> =
                parts.iter().map(|p| eval_condition(p, props)).collect();
            return if is_or {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if values.contains(&Some(false)) {
                Some(false)
            } else if values.iter().all(|v| *v == Some(true)) {
                Some(true)
            } else {
                None
            };
        }
    }
    if c.starts_with('(') && c.ends_with(')') {
        return eval_condition(&c[1..c.len() - 1], props);
    }
    if let Some(inner) = c.strip_prefix('!') {
        return eval_condition(inner, props).map(|v| !v);
    }
    let (op, eq) = if let Some(i) = c.find("==") {
        (i, true)
    } else if let Some(i) = c.find("!=") {
        (i, false)
    } else {
        return match lower.as_str() {
            "true" | "'true'" => Some(true),
            "false" | "'false'" => Some(false),
            _ => None,
        };
    };
    let side = |s: &str| -> Option<String> {
        let s = s.trim().trim_matches('\'');
        let (v, known) = props.expand_known(s);
        known.then(|| v.trim().to_ascii_lowercase())
    };
    let left = side(&c[..op])?;
    let right = side(&c[op + 2..])?;
    Some((left == right) == eq)
}

/// A NuGet package reference after evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageReference {
    pub id: String,
    /// The version in effect (empty: unknown).
    pub version: String,
    /// Where the version comes from: `project`, `central` (Directory.Packages.props),
    /// `override` (VersionOverride), `props` (Directory.Build.props) or `implicit`.
    pub source: String,
    /// `PrivateAssets="all"` (analyzers, test adapters).
    #[serde(default)]
    pub private_assets: bool,
    /// The condition it is listed under (`'$(TargetFramework)' == 'netstandard2.0'`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condition: String,
    /// 0-based line in the project file (`None` when it comes from another file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// The test framework a project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestFramework {
    Xunit,
    Nunit,
    Mstest,
}

impl TestFramework {
    pub fn label(self) -> &'static str {
        match self {
            TestFramework::Xunit => "xUnit",
            TestFramework::Nunit => "NUnit",
            TestFramework::Mstest => "MSTest",
        }
    }

    /// By a package id.
    pub fn of_package(id: &str) -> Option<TestFramework> {
        let id = id.to_ascii_lowercase();
        if id == "xunit" || id.starts_with("xunit.v3") || id == "xunit.core" {
            Some(TestFramework::Xunit)
        } else if id == "nunit" {
            Some(TestFramework::Nunit)
        } else if id == "mstest" || id == "mstest.testframework" || id == "mstest.sdk" {
            Some(TestFramework::Mstest)
        } else {
            None
        }
    }
}

/// A project as the solution view, the NuGet tool window and run configurations see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetProject {
    pub name: String,
    /// The project file relative to the project root.
    pub path: String,
    pub language: ProjectLanguage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<String>,
    /// `Exe`, `WinExe`, `Library`.
    pub output_type: String,
    pub target_frameworks: Vec<String>,
    pub assembly_name: String,
    pub root_namespace: String,
    /// ASP.NET Core (`Microsoft.NET.Sdk.Web`), Blazor WebAssembly, Razor, worker.
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub is_test_project: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_framework: Option<TestFramework>,
    #[serde(default)]
    pub central_package_management: bool,
    pub packages: Vec<PackageReference>,
    /// Referenced project files relative to the project root.
    pub project_references: Vec<String>,
    /// `Microsoft.AspNetCore.App`, ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub framework_references: Vec<String>,
    /// F# compile order (`<Compile Include>`), relative to the project root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compile_order: Vec<String>,
    /// Interesting properties (Nullable, LangVersion, Version, UserSecretsId, ...).
    pub properties: BTreeMap<String, String>,
    /// `Properties/launchSettings.json` profiles (filled by the workspace loader).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launch_profiles: Vec<crate::launch::LaunchProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl DotnetProject {
    pub fn dir(&self) -> String {
        parent(&self.path)
    }

    /// Whether `dotnet run` can start it.
    pub fn is_runnable(&self) -> bool {
        !self.is_test_project
            && (self.web
                || self.output_type.eq_ignore_ascii_case("exe")
                || self.output_type.eq_ignore_ascii_case("winexe"))
    }

    /// The default target framework (the first one).
    pub fn framework(&self) -> &str {
        self.target_frameworks
            .first()
            .map(String::as_str)
            .unwrap_or("")
    }

    /// `bin/<configuration>/<framework>/<assembly>.dll` relative to the project root.
    pub fn output_dll(&self, configuration: &str, framework: &str) -> String {
        let tfm = if framework.is_empty() {
            self.framework()
        } else {
            framework
        };
        let mut p = format!("bin/{configuration}/");
        if !tfm.is_empty() {
            p.push_str(tfm);
            p.push('/');
        }
        join(&self.dir(), &format!("{p}{}.dll", self.assembly_name))
    }

    pub fn package(&self, id: &str) -> Option<&PackageReference> {
        self.packages.iter().find(|p| p.id.eq_ignore_ascii_case(id))
    }
}

/// Files imported implicitly by the SDK, read by the workspace loader.
#[derive(Debug, Clone, Default)]
pub struct Imports<'a> {
    /// The nearest `Directory.Build.props` (path, text).
    pub build_props: Option<(&'a str, &'a str)>,
    /// The nearest `Directory.Packages.props` (path, text).
    pub packages_props: Option<(&'a str, &'a str)>,
}

const SHOWN_PROPERTIES: &[&str] = &[
    "Nullable",
    "LangVersion",
    "ImplicitUsings",
    "Version",
    "UserSecretsId",
    "Description",
    "Company",
    "Authors",
    "PackageId",
    "IsPackable",
    "InvariantGlobalization",
    "TreatWarningsAsErrors",
    "RuntimeIdentifier",
    "PublishAot",
];

fn apply_properties(file: &MsBuildFile, props: &mut Properties, problems: &mut Vec<String>) {
    for p in &file.properties {
        match eval_condition(&p.condition, props) {
            Some(false) => continue,
            None if !p.condition.is_empty() => {
                // Undecidable: the first value wins (TFM-specific values do not replace
                // the general one).
                if props.get(&p.name).is_some() {
                    continue;
                }
            }
            _ => {}
        }
        let value = props.expand(&p.value);
        props.set(&p.name, value);
    }
    let _ = problems;
}

/// Evaluate a project file with the SDK's implicit imports. `configuration` is the active
/// build configuration (`Debug`).
pub fn evaluate_project(
    path: &str,
    text: &str,
    imports: &Imports<'_>,
    configuration: &str,
) -> DotnetProject {
    let path = normalize(path);
    let dir = parent(&path);
    let name = file_name(&path)
        .rsplit_once('.')
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| file_name(&path).to_string());
    let language = ProjectLanguage::of_path(&path);
    let mut problems = Vec::new();
    let mut props = Properties::default();
    props.set("Configuration", configuration);
    props.set("Platform", "AnyCPU");
    props.set("MSBuildProjectName", &name);
    props.set("MSBuildProjectDirectory", &dir);
    props.set("MSBuildThisFileDirectory", format!("{dir}/"));
    props.set("AssemblyName", &name);
    props.set("RootNamespace", name.replace([' ', '-'], "_"));
    props.set("OutputType", "Library");
    let build_props = imports
        .build_props
        .and_then(|(p, t)| match parse_msbuild(t) {
            Ok(f) => Some(f),
            Err(e) => {
                problems.push(format!("{p}: {e}"));
                None
            }
        });
    let packages_props = imports
        .packages_props
        .and_then(|(p, t)| match parse_msbuild(t) {
            Ok(f) => Some(f),
            Err(e) => {
                problems.push(format!("{p}: {e}"));
                None
            }
        });
    if let Some(f) = &build_props {
        apply_properties(f, &mut props, &mut problems);
    }
    let file = match parse_msbuild(text) {
        Ok(f) => f,
        Err(e) => {
            problems.push(e);
            MsBuildFile::default()
        }
    };
    if let Some(f) = &packages_props {
        apply_properties(f, &mut props, &mut problems);
    }
    apply_properties(&file, &mut props, &mut problems);
    // Directory.Build.targets is imported after the project: not read (rarely sets these).
    let mut frameworks: Vec<String> = props
        .get("TargetFrameworks")
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if frameworks.is_empty() {
        if let Some(tf) = props
            .get("TargetFramework")
            .filter(|t| !t.trim().is_empty())
        {
            frameworks.push(tf.trim().to_string());
        }
    }
    let sdk = file.sdk.clone();
    let sdk_lower = sdk.as_deref().unwrap_or("").to_ascii_lowercase();
    let web = sdk_lower.starts_with("microsoft.net.sdk.web")
        || sdk_lower.starts_with("microsoft.net.sdk.blazorwebassembly")
        || sdk_lower.starts_with("microsoft.net.sdk.worker")
        || sdk_lower.starts_with("microsoft.net.sdk.razor")
            && props
                .get("OutputType")
                .unwrap_or("")
                .eq_ignore_ascii_case("exe");
    let mut output_type = props.get("OutputType").unwrap_or("Library").to_string();
    if web
        && output_type.eq_ignore_ascii_case("library")
        && !sdk_lower.starts_with("microsoft.net.sdk.razor")
    {
        output_type = "Exe".into();
    }
    let central = props
        .get("ManagePackageVersionsCentrally")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));

    // Central versions (Directory.Packages.props).
    let mut central_versions: BTreeMap<String, String> = BTreeMap::new();
    if let Some(f) = &packages_props {
        for i in f
            .items
            .iter()
            .filter(|i| i.kind.eq_ignore_ascii_case("PackageVersion"))
        {
            if eval_condition(&i.condition, &props) == Some(false) {
                continue;
            }
            let id = if i.include.is_empty() {
                &i.update
            } else {
                &i.include
            };
            if let Some(v) = i.meta("Version") {
                central_versions.insert(id.to_ascii_lowercase(), props.expand(v));
            }
        }
    }

    let mut packages: Vec<PackageReference> = Vec::new();
    let mut project_references = Vec::new();
    let mut framework_references = Vec::new();
    let mut compile_order = Vec::new();
    let mut add_package = |i: &Item, from_props: bool, props: &Properties| {
        let id = props.expand(&i.include);
        if id.is_empty() {
            return;
        }
        let (version, source) = if let Some(v) = i.meta("VersionOverride") {
            (props.expand(v), "override")
        } else if let Some(v) = i.meta("Version") {
            (
                props.expand(v),
                if from_props { "props" } else { "project" },
            )
        } else if let Some(v) = central_versions.get(&id.to_ascii_lowercase()) {
            (v.clone(), "central")
        } else {
            (String::new(), "implicit")
        };
        let private_assets = i
            .meta("PrivateAssets")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("all"));
        let pr = PackageReference {
            id: id.clone(),
            version: version.trim().to_string(),
            source: source.into(),
            private_assets,
            condition: i.condition.clone(),
            line: (!from_props).then_some(i.line),
        };
        match packages.iter_mut().find(|p| p.id.eq_ignore_ascii_case(&id)) {
            Some(existing) if !from_props && existing.line.is_none() => *existing = pr,
            Some(_) => {}
            None => packages.push(pr),
        }
    };
    let sources: Vec<(&MsBuildFile, bool)> = build_props
        .iter()
        .map(|f| (f, true))
        .chain(std::iter::once((&file, false)))
        .collect();
    for (f, from_props) in sources {
        for i in &f.items {
            if eval_condition(&i.condition, &props) == Some(false) {
                continue;
            }
            let kind = i.kind.to_ascii_lowercase();
            match kind.as_str() {
                "packagereference" if !i.include.is_empty() => add_package(i, from_props, &props),
                "packagereference" if !i.update.is_empty() => {
                    // `Update` changes an implicit reference (FSharp.Core): listed with its version.
                    let mut u = i.clone();
                    u.include = i.update.clone();
                    add_package(&u, from_props, &props);
                }
                "projectreference" if !i.include.is_empty() => {
                    let base = if from_props {
                        String::new()
                    } else {
                        dir.clone()
                    };
                    let p = join(&base, &props.expand(&i.include));
                    if !project_references.contains(&p) {
                        project_references.push(p);
                    }
                }
                "frameworkreference" if !i.include.is_empty() => {
                    framework_references.push(props.expand(&i.include))
                }
                "compile" if language == ProjectLanguage::FSharp && !i.include.is_empty() => {
                    for inc in props
                        .expand(&i.include)
                        .split(';')
                        .filter(|s| !s.trim().is_empty())
                    {
                        compile_order.push(join(&dir, inc.trim()));
                    }
                }
                _ => {}
            }
        }
    }
    if web
        && sdk_lower.starts_with("microsoft.net.sdk.web")
        && !framework_references
            .iter()
            .any(|f| f == "Microsoft.AspNetCore.App")
    {
        framework_references.insert(0, "Microsoft.AspNetCore.App".into());
    }
    if language == ProjectLanguage::FSharp
        && !packages
            .iter()
            .any(|p| p.id.eq_ignore_ascii_case("FSharp.Core"))
    {
        packages.push(PackageReference {
            id: "FSharp.Core".into(),
            version: String::new(),
            source: "implicit".into(),
            private_assets: false,
            condition: String::new(),
            line: None,
        });
    }
    let test_framework = packages
        .iter()
        .find_map(|p| TestFramework::of_package(&p.id));
    let is_test_project = props
        .get("IsTestProject")
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        || sdk_lower.starts_with("mstest.sdk")
        || packages
            .iter()
            .any(|p| p.id.eq_ignore_ascii_case("Microsoft.NET.Test.Sdk"))
            && test_framework.is_some();
    let test_framework = if sdk_lower.starts_with("mstest.sdk") {
        Some(TestFramework::Mstest)
    } else {
        test_framework
    };
    let properties = SHOWN_PROPERTIES
        .iter()
        .filter_map(|k| props.get(k).map(|v| (k.to_string(), v.trim().to_string())))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    DotnetProject {
        assembly_name: props
            .get("AssemblyName")
            .unwrap_or(&name)
            .trim()
            .to_string(),
        root_namespace: props
            .get("RootNamespace")
            .unwrap_or(&name)
            .trim()
            .to_string(),
        name,
        path,
        language,
        sdk,
        output_type,
        target_frameworks: frameworks,
        web,
        is_test_project,
        test_framework,
        central_package_management: central,
        packages,
        project_references,
        framework_references,
        compile_order,
        properties,
        launch_profiles: Vec::new(),
        problems,
    }
}

/// The `Directory.Build.props` / `Directory.Packages.props` candidates of a project file,
/// nearest first (the folders from the project's up to the project root).
pub fn import_candidates(project_path: &str, file: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut dir = parent(&normalize(project_path));
    loop {
        out.push(if dir.is_empty() {
            file.to_string()
        } else {
            format!("{dir}/{file}")
        });
        if dir.is_empty() {
            break;
        }
        dir = parent(&dir);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    fn props(pairs: &[(&str, &str)]) -> Properties {
        let mut p = Properties::default();
        for (k, v) in pairs {
            p.set(k, *v);
        }
        p
    }

    #[test]
    fn conditions() {
        let p = props(&[("Configuration", "Debug"), ("TargetFramework", "net8.0")]);
        assert_eq!(
            eval_condition("'$(Configuration)' == 'Debug'", &p),
            Some(true)
        );
        assert_eq!(
            eval_condition("'$(configuration)'=='Release'", &p),
            Some(false)
        );
        assert_eq!(
            eval_condition("'$(Configuration)' != 'Release'", &p),
            Some(true)
        );
        assert_eq!(
            eval_condition(
                "'$(Configuration)' == 'Debug' and '$(TargetFramework)' == 'net8.0'",
                &p
            ),
            Some(true)
        );
        assert_eq!(
            eval_condition("'$(Configuration)' == 'Release' OR '$(Unknown)' == 'x'", &p),
            None
        );
        assert_eq!(
            eval_condition("!('$(Configuration)' == 'Debug')", &p),
            Some(false)
        );
        assert_eq!(eval_condition("Exists('foo.props')", &p), None);
        assert_eq!(eval_condition("", &p), Some(true));
        assert_eq!(p.expand("$(Configuration)/$(Nope)/x"), "Debug//x");
        assert_eq!(p.expand_known("a$(Nope)").1, false);
    }

    #[test]
    fn reads_raw_files() {
        let f = parse_msbuild(&fixture(
            "bookstore/tests/Bookstore.Tests/Bookstore.Tests.csproj",
        ))
        .unwrap();
        assert_eq!(f.sdk.as_deref(), Some("Microsoft.NET.Sdk"));
        let runner = f
            .items
            .iter()
            .find(|i| i.include == "xunit.runner.visualstudio")
            .unwrap();
        assert_eq!(runner.meta("PrivateAssets"), Some("all"));
        assert!(parse_msbuild("<Solution/>").is_err());
        let sdk_element =
            parse_msbuild("<Project><Sdk Name=\"MSTest.Sdk\" Version=\"3.6.0\" /></Project>")
                .unwrap();
        assert_eq!(sdk_element.sdk.as_deref(), Some("MSTest.Sdk"));
    }

    fn evaluate(path: &str) -> DotnetProject {
        let build = fixture("bookstore/Directory.Build.props");
        let packages = fixture("bookstore/Directory.Packages.props");
        evaluate_project(
            path,
            &fixture(&format!("bookstore/{path}")),
            &Imports {
                build_props: Some(("Directory.Build.props", &build)),
                packages_props: Some(("Directory.Packages.props", &packages)),
            },
            "Debug",
        )
    }

    #[test]
    fn web_project_with_central_versions() {
        let api = evaluate("src/Bookstore.Api/Bookstore.Api.csproj");
        assert_eq!(api.name, "Bookstore.Api");
        assert!(api.web && api.is_runnable() && !api.is_test_project);
        assert_eq!(api.output_type, "Exe");
        assert_eq!(api.target_frameworks, vec!["net8.0"]);
        assert!(api.central_package_management);
        let swagger = api.package("swashbuckle.aspnetcore").unwrap();
        assert_eq!(
            (swagger.version.as_str(), swagger.source.as_str()),
            ("6.5.0", "central")
        );
        assert_eq!(swagger.line, Some(10));
        let serilog = api.package("Serilog.AspNetCore").unwrap();
        assert_eq!(
            (serilog.version.as_str(), serilog.source.as_str()),
            ("8.0.1", "override")
        );
        assert_eq!(
            api.project_references,
            vec!["src/Bookstore.Core/Bookstore.Core.csproj"]
        );
        assert_eq!(api.framework_references, vec!["Microsoft.AspNetCore.App"]);
        assert_eq!(
            api.properties.get("Nullable").map(String::as_str),
            Some("enable")
        );
        assert_eq!(
            api.properties.get("LangVersion").map(String::as_str),
            Some("12.0")
        );
        // Release-only property is not in effect in Debug.
        assert!(!api.properties.contains_key("TreatWarningsAsErrors"));
        assert_eq!(
            api.output_dll("Debug", ""),
            "src/Bookstore.Api/bin/Debug/net8.0/Bookstore.Api.dll"
        );
    }

    #[test]
    fn library_with_several_frameworks() {
        let core = evaluate("src/Bookstore.Core/Bookstore.Core.csproj");
        assert_eq!(core.target_frameworks, vec!["net8.0", "netstandard2.0"]);
        assert_eq!(core.assembly_name, "Acme.Bookstore.Core");
        assert!(!core.is_runnable());
        assert_eq!(
            core.properties.get("Description").map(String::as_str),
            Some("Catalog and cart of Acme Books")
        );
        // A TFM-conditioned reference is listed with its condition.
        let json = core.package("System.Text.Json").unwrap();
        assert_eq!(json.version, "8.0.4");
        assert!(json.condition.contains("netstandard2.0"));
        assert_eq!(
            core.output_dll("Release", "netstandard2.0"),
            "src/Bookstore.Core/bin/Release/netstandard2.0/Acme.Bookstore.Core.dll"
        );
    }

    #[test]
    fn test_and_fsharp_projects() {
        let t = evaluate("tests/Bookstore.Tests/Bookstore.Tests.csproj");
        assert!(t.is_test_project);
        assert_eq!(t.test_framework, Some(TestFramework::Xunit));
        assert!(
            t.package("xunit.runner.visualstudio")
                .unwrap()
                .private_assets
        );
        assert_eq!(t.package("xunit").unwrap().version, "2.7.0");
        assert!(!t.is_runnable());
        let fs = evaluate("tools/Reports/Reports.fsproj");
        assert_eq!(fs.language, ProjectLanguage::FSharp);
        assert!(fs.is_runnable());
        assert_eq!(
            fs.compile_order,
            vec!["tools/Reports/Sales.fs", "tools/Reports/Program.fs"]
        );
        let core = fs.package("FSharp.Core").unwrap();
        assert_eq!(core.version, "8.0.300");
        // Without central management a missing version is "implicit".
        let plain = evaluate_project(
            "A/A.csproj",
            "<Project Sdk=\"MSTest.Sdk/3.6.0\"><ItemGroup><PackageReference Include=\"Moq\" /></ItemGroup></Project>",
            &Imports::default(),
            "Debug",
        );
        assert_eq!(plain.package("Moq").unwrap().source, "implicit");
        assert!(plain.is_test_project);
        assert_eq!(plain.test_framework, Some(TestFramework::Mstest));
        let broken = evaluate_project("B.csproj", "<Project>", &Imports::default(), "Debug");
        assert_eq!(broken.problems.len(), 1);
    }

    #[test]
    fn import_folders() {
        assert_eq!(
            import_candidates("src/App/App.csproj", "Directory.Build.props"),
            vec![
                "src/App/Directory.Build.props",
                "src/Directory.Build.props",
                "Directory.Build.props"
            ]
        );
        assert_eq!(import_candidates("App.csproj", "x"), vec!["x"]);
    }
}

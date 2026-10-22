//! Navigate | Test / Test Subject (`GotoTest`, Ctrl+Shift+T) and PHPStorm's Create New Test
//! dialog:
//!
//! - [`test_targets`]: a class's or module's tests, or a test's subjects, best match first
//!   (PHPUnit and Pest `*Test.php`, JS/TS `*.test.*`, `*.spec.*` and `__tests__/`).
//! - [`new_test_plan`]: what the Create New Test dialog starts with: the test frameworks the
//!   project uses (PHPUnit / Pest from composer.json, Vitest / Jest from package.json), the
//!   test class, its namespace and folder from composer's `autoload-dev` PSR-4 rules (Laravel:
//!   `tests/Unit` or `tests/Feature` mirroring the source path, Symfony: `tests/` with
//!   `App\Tests`), the superclass, the file name and the subject's public methods; JS/TS
//!   tests go next to the subject, into `__tests__` or a `tests/` folder like the project's
//!   existing tests.
//! - [`test_source`]: the new test file (PHPUnit class with `setUp()`/`tearDown()` and
//!   `test<Method>()` stubs, Pest `beforeEach`/`test()` blocks, Vitest/Jest `describe`/`it`).
//!
//! Pure: the caller hands over the project's file list and a reader, so the backend runs it
//! next to the files (local or the remote agent) and browser mode runs it as WebAssembly.

use forge_file_templates::psr4::{self, Psr4Root};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::related::{related, RelatedItem, RelatedKind, RelatedRequest};
use crate::structure::{editor_structure, ScopeKind};

/// The file Go to Test / Create New Test starts from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TestNavRequest {
    /// Workspace-relative, `/`-separated.
    pub path: String,
    /// The editor's text of the file.
    pub text: String,
    /// The project's composer.json (Settings | PHP | Composer); `composer.json` when empty.
    pub composer_path: String,
}

/// What Go to Test found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestTargets {
    /// The file is a test: the targets are its subjects.
    pub is_test: bool,
    /// The class, component or module (`Order`, `OrderTest`) for the chooser's title.
    pub name: String,
    /// Best match first.
    pub targets: Vec<RelatedItem>,
    /// Create New Test… is offered (a PHP or JS/TS source file that is not a test).
    pub can_create: bool,
}

/// A test framework the Create New Test dialog offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestFramework {
    PHPUnit,
    Pest,
    Vitest,
    Jest,
}

impl TestFramework {
    pub fn is_php(self) -> bool {
        matches!(self, Self::PHPUnit | Self::Pest)
    }
}

/// A public method (PHP) or export (JS/TS) of the subject a test can be generated for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestMember {
    pub name: String,
    /// JS/TS: the default export (imported without braces).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default_export: bool,
}

/// What the Create New Test dialog starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTestPlan {
    /// The frameworks the project uses (the default first).
    pub frameworks: Vec<TestFramework>,
    pub framework: TestFramework,
    /// `Order`, `Button`, `format`.
    pub subject: String,
    /// `App\Models\Order` (PHP; empty for the global namespace or JS/TS).
    pub subject_fqn: String,
    pub subject_path: String,
    /// `OrderTest` (PHP).
    pub class_name: String,
    /// `Tests\Unit\Models` (PHPUnit).
    pub namespace: String,
    /// Project-relative folder of the new test.
    pub directory: String,
    /// The test's parent class (PHPUnit).
    pub superclass: String,
    /// Parent classes to choose from (PHPUnit).
    pub superclasses: Vec<String>,
    /// `OrderTest.php`, `format.test.ts`.
    pub file_name: String,
    pub members: Vec<TestMember>,
}

/// The values of the Create New Test dialog.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TestSourceRequest {
    pub framework: Option<TestFramework>,
    pub subject: String,
    pub subject_fqn: String,
    pub subject_path: String,
    pub class_name: String,
    pub namespace: String,
    pub directory: String,
    pub superclass: String,
    pub file_name: String,
    /// The members to generate test stubs for.
    pub members: Vec<TestMember>,
    /// `setUp()` (PHPUnit), `beforeEach()` (Pest, Vitest, Jest).
    pub set_up: bool,
    /// `tearDown()` (PHPUnit), `afterEach()` (Pest, Vitest, Jest).
    pub tear_down: bool,
}

/// The new test file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSource {
    /// Project-relative path.
    pub path: String,
    pub text: String,
    /// 0-based line for the caret (the first test's body).
    pub line: u32,
}

// ---- helpers

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

fn join(dir: &str, name: &str) -> String {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn is_test_dir(path: &str) -> bool {
    dir_of(path)
        .split('/')
        .any(|s| matches!(s, "tests" | "test" | "__tests__" | "spec" | "specs"))
}

fn is_php(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".php") && !lower.ends_with(".blade.php")
}

const JS_EXTS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "vue", "svelte",
];

/// `Button.test.tsx` → (`Button`, [`test`], `tsx`).
fn js_parts(path: &str) -> Option<(&str, Vec<&str>, &str)> {
    let name = file_name(path);
    let mut parts = name.split('.');
    let stem = parts.next().filter(|s| !s.is_empty())?;
    let rest: Vec<&str> = parts.collect();
    let ext = *rest.last()?;
    JS_EXTS
        .contains(&ext.to_ascii_lowercase().as_str())
        .then(|| (stem, rest[..rest.len() - 1].to_vec(), ext))
}

fn is_js_test(path: &str) -> bool {
    js_parts(path).is_some_and(|(_, quals, _)| {
        quals
            .iter()
            .any(|q| matches!(*q, "test" | "spec" | "e2e" | "cy"))
            || dir_of(path).split('/').any(|s| s == "__tests__")
    })
}

fn is_php_test(path: &str, text: &str) -> bool {
    let stem = file_name(path).trim_end_matches(".php");
    stem.len() > 4
        && stem.ends_with("Test")
        && (is_test_dir(path)
            || text.contains("extends TestCase")
            || text.contains("PHPUnit\\")
            || pest_calls(text))
}

fn pest_calls(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("it(") || t.starts_with("test(") || t.starts_with("describe(")
    })
}

/// How many trailing folder names two paths share (`app/Models` ~ `tests/Unit/Models`: 1),
/// ignoring the test folders themselves.
fn shared_tail(a: &str, b: &str) -> usize {
    let segs = |p: &str| -> Vec<String> {
        dir_of(p)
            .split('/')
            .filter(|s| {
                !s.is_empty()
                    && !matches!(
                        s.to_ascii_lowercase().as_str(),
                        "tests" | "test" | "__tests__" | "unit" | "feature" | "integration"
                    )
            })
            .map(str::to_ascii_lowercase)
            .collect()
    };
    let (a, b) = (segs(a), segs(b));
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

// ---- Go to Test

/// Go to Test (Ctrl+Shift+T): the tests of `req.path`, or the subjects of a test.
pub fn test_targets(
    req: &TestNavRequest,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> TestTargets {
    let path = req.path.as_str();
    let php = is_php(path);
    let js = js_parts(path).is_some();
    let is_test = if php {
        is_php_test(path, &req.text)
    } else {
        is_js_test(path)
    };
    let name = if php {
        file_name(path).trim_end_matches(".php").to_string()
    } else {
        js_parts(path).map_or_else(|| file_name(path).to_string(), |(s, _, _)| s.to_string())
    };
    let wanted = if is_test {
        RelatedKind::TestSubject
    } else {
        RelatedKind::Test
    };
    let rel = RelatedRequest {
        path: req.path.clone(),
        text: req.text.clone(),
    };
    let mut targets: Vec<RelatedItem> = if php || js {
        related(&rel, files, read)
            .into_iter()
            .filter(|i| i.kind == wanted)
            .collect()
    } else {
        Vec::new()
    };
    // Best match first: the most shared folder names (`app/Models` ~ `tests/Unit/Models`).
    targets.sort_by_key(|t| std::cmp::Reverse(shared_tail(path, &t.path)));
    TestTargets {
        is_test,
        name,
        targets,
        can_create: !is_test && (php || js),
    }
}

// ---- Create New Test: PHP

fn composer(read: &dyn Fn(&str) -> Option<String>, composer_path: &str) -> (Value, String) {
    let path = if composer_path.trim().is_empty() {
        "composer.json"
    } else {
        composer_path.trim()
    };
    let json = read(path)
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    (json, path.to_string())
}

fn requires(json: &Value, package: &str) -> bool {
    ["require", "require-dev"]
        .iter()
        .any(|s| json[s].get(package).is_some())
}

/// `namespace App\Models;` of a PHP file.
fn php_namespace(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let t = l.trim();
        let rest = t.strip_prefix("namespace ")?;
        let ns = rest.trim_end_matches(['{', ';', ' ']).trim();
        (!ns.is_empty()).then(|| ns.to_string())
    })
}

/// The class declared in a PHP file (the one named like the file, else the first).
fn php_class_name(text: &str, stem: &str) -> String {
    let structure = editor_structure(text, "php");
    let classes: Vec<&str> = structure
        .scopes
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                ScopeKind::Class | ScopeKind::Enum | ScopeKind::Trait
            )
        })
        .map(|s| s.name.as_str())
        .collect();
    classes
        .iter()
        .find(|c| **c == stem)
        .or(classes.first())
        .map_or(stem, |c| c)
        .to_string()
}

/// The public methods of the class `class` (without magic methods).
fn php_public_methods(text: &str, class: &str) -> Vec<TestMember> {
    let structure = editor_structure(text, "php");
    let lines: Vec<&str> = text.lines().collect();
    let Some(ci) = structure
        .scopes
        .iter()
        .position(|s| s.name == class && s.kind != ScopeKind::Method)
    else {
        return Vec::new();
    };
    let mut out: Vec<TestMember> = Vec::new();
    for s in &structure.scopes {
        if s.kind != ScopeKind::Method || s.parent != Some(ci as u32) {
            continue;
        }
        let name = s.name.trim_end_matches("()");
        if name.starts_with("__") || out.iter().any(|m| m.name == name) {
            continue;
        }
        let header: String = lines
            .get(s.line as usize..=(s.open_line as usize).min(lines.len().saturating_sub(1)))
            .map(|l| l.join(" "))
            .unwrap_or_default();
        let head = header.split("function").next().unwrap_or("");
        let words: Vec<&str> = head.split_whitespace().collect();
        if words
            .iter()
            .any(|w| matches!(*w, "private" | "protected" | "abstract"))
        {
            continue;
        }
        out.push(TestMember {
            name: name.to_string(),
            default_export: false,
        });
    }
    out
}

fn segments(ns: &str) -> Vec<&str> {
    ns.split('\\').filter(|s| !s.is_empty()).collect()
}

/// The subject's namespace below its PSR-4 root (`App\Http\Controllers` → `Http\Controllers`).
fn relative_namespace(roots: &[Psr4Root], subject_ns: &str, subject_dir: &str) -> Vec<String> {
    let root = roots
        .iter()
        .filter(|r| !r.dev)
        .filter(|r| {
            r.dir.is_empty()
                || subject_dir == r.dir
                || subject_dir.starts_with(&format!("{}/", r.dir))
        })
        .max_by_key(|r| r.dir.len());
    let ns = segments(subject_ns);
    if let Some(root) = root {
        let prefix = segments(&root.prefix);
        let starts = prefix.len() <= ns.len()
            && prefix
                .iter()
                .zip(&ns)
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
        if starts {
            return ns[prefix.len()..].iter().map(|s| s.to_string()).collect();
        }
        // No namespace statement: the folders below the root.
        let rest = subject_dir
            .strip_prefix(root.dir.as_str())
            .unwrap_or(subject_dir)
            .trim_matches('/');
        return rest
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    // Without rules: drop the vendor-ish first segment (`App`, `Acme`).
    ns.iter().skip(1).map(|s| s.to_string()).collect()
}

/// The PSR-4 root for tests: an `autoload-dev` rule for a `tests` folder, else any dev rule.
fn test_root(roots: &[Psr4Root], files: &[String]) -> (String, String) {
    let dev: Vec<&Psr4Root> = roots.iter().filter(|r| r.dev).collect();
    let testy = |r: &&&Psr4Root| {
        let first = r.dir.split('/').next_back().unwrap_or("");
        matches!(first, "tests" | "test" | "Tests")
            || r.prefix.ends_with("Tests")
            || r.prefix.ends_with("Test")
    };
    if let Some(r) = dev.iter().find(testy).or(dev.first()) {
        return (r.prefix.clone(), r.dir.clone());
    }
    let dir = if files.iter().any(|f| f.starts_with("test/"))
        && !files.iter().any(|f| f.starts_with("tests/"))
    {
        "test"
    } else {
        "tests"
    };
    ("Tests".to_string(), dir.to_string())
}

fn php_plan(
    req: &TestNavRequest,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> NewTestPlan {
    let path = req.path.as_str();
    let stem = file_name(path).trim_end_matches(".php");
    let (json, composer_path) = composer(read, &req.composer_path);
    let roots = psr4::roots(&json.to_string(), &psr4::composer_dir(&composer_path));
    let subject_dir = dir_of(path);
    let subject = php_class_name(&req.text, stem);
    let subject_ns = php_namespace(&req.text)
        .or_else(|| psr4::namespace_for_dir(&roots, subject_dir))
        .unwrap_or_default();
    let subject_fqn = if subject_ns.is_empty() {
        subject.clone()
    } else {
        format!("{subject_ns}\\{subject}")
    };
    let laravel = requires(&json, "laravel/framework");
    let symfony = requires(&json, "symfony/framework-bundle");
    let pest =
        requires(&json, "pestphp/pest") || files.iter().any(|f| f.ends_with("tests/Pest.php"));

    let rel = relative_namespace(&roots, &subject_ns, subject_dir);
    let (test_prefix, test_dir) = test_root(&roots, files);
    let mut sub: Vec<String> = Vec::new();
    let has = |d: &str| {
        let d = join(&test_dir, d);
        files.iter().any(|f| f.starts_with(&format!("{d}/")))
    };
    let feature = rel
        .first()
        .is_some_and(|s| matches!(s.as_str(), "Http" | "Livewire" | "Console"))
        || subject.ends_with("Controller");
    let laravel_layout = laravel || has("Unit") || has("Feature");
    if laravel_layout {
        sub.push(if feature { "Feature" } else { "Unit" }.to_string());
    }
    sub.extend(rel);
    let namespace = segments(&test_prefix)
        .into_iter()
        .map(str::to_string)
        .chain(sub.iter().cloned())
        .collect::<Vec<_>>()
        .join("\\");
    let directory = sub.iter().fold(test_dir.clone(), |d, s| join(&d, s));

    // Superclasses: the project's own TestCase, framework test cases, PHPUnit's.
    let mut superclasses = Vec::new();
    let own_case = join(&test_dir, "TestCase.php");
    let own = files.contains(&own_case).then(|| {
        let ns = read(&own_case)
            .and_then(|t| php_namespace(&t))
            .unwrap_or_else(|| test_prefix.clone());
        if ns.is_empty() {
            "TestCase".to_string()
        } else {
            format!("{ns}\\TestCase")
        }
    });
    let phpunit = "PHPUnit\\Framework\\TestCase".to_string();
    let web_case = "Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase".to_string();
    let kernel_case = "Symfony\\Bundle\\FrameworkBundle\\Test\\KernelTestCase".to_string();
    // Laravel's Unit tests extend PHPUnit's TestCase unless the project's do not.
    let unit_uses_own = || {
        files
            .iter()
            .filter(|f| {
                f.starts_with(&format!("{}/", join(&test_dir, "Unit"))) && f.ends_with(".php")
            })
            .take(20)
            .filter_map(|f| read(f))
            .any(|t| {
                own.as_ref()
                    .is_some_and(|o| t.contains(&format!("use {o};")))
            })
    };
    let superclass = match &own {
        Some(o) if !laravel_layout || feature || unit_uses_own() => o.clone(),
        _ if symfony && subject.ends_with("Controller") => web_case.clone(),
        _ => phpunit.clone(),
    };
    superclasses.extend(own);
    if symfony {
        superclasses.push(web_case);
        superclasses.push(kernel_case);
    }
    superclasses.push(phpunit);

    let mut frameworks = vec![TestFramework::PHPUnit];
    if pest {
        frameworks.insert(0, TestFramework::Pest);
    }
    let class_name = format!("{subject}Test");
    NewTestPlan {
        framework: frameworks[0],
        frameworks,
        file_name: format!("{class_name}.php"),
        members: php_public_methods(&req.text, &subject),
        subject,
        subject_fqn,
        subject_path: path.to_string(),
        class_name,
        namespace,
        directory,
        superclass,
        superclasses,
    }
}

// ---- Create New Test: JavaScript / TypeScript

fn package_deps(read: &dyn Fn(&str) -> Option<String>) -> Value {
    read("package.json")
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null)
}

fn has_dep(json: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|s| json[s].get(name).is_some())
}

fn ident(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(s.len());
    let id = &s[..end];
    (!id.is_empty() && !id.starts_with(|c: char| c.is_ascii_digit())).then_some(id)
}

/// The exports of a JS/TS module (`export function x`, `export const x`, `export class X`,
/// `export { a, b }`, `export default`); components (`.vue`, `.svelte`) export themselves.
fn js_exports(path: &str, text: &str, stem: &str) -> Vec<TestMember> {
    let mut out: Vec<TestMember> = Vec::new();
    let mut add = |name: &str, default_export: bool| {
        if !out.iter().any(|m| m.name == name) {
            out.push(TestMember {
                name: name.to_string(),
                default_export,
            });
        }
    };
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".vue") || lower.ends_with(".svelte") {
        add(stem, true);
        return out;
    }
    for line in text.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("export ") else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(d) = rest.strip_prefix("default ") {
            let d = d.trim_start();
            let d = d.strip_prefix("async ").unwrap_or(d).trim_start();
            let named = ["function", "class"]
                .iter()
                .find_map(|k| d.strip_prefix(k))
                .map(|r| r.trim_start().trim_start_matches('*').trim_start());
            let name = named
                .and_then(ident)
                .or_else(|| ident(d).filter(|_| named.is_none()));
            let name = name
                .filter(|n| !matches!(*n, "function" | "class" | "async"))
                .unwrap_or(stem);
            add(name, true);
            continue;
        }
        if let Some(list) = rest.strip_prefix('{') {
            if rest.contains(" from ") {
                continue; // re-exports
            }
            for part in list.split('}').next().unwrap_or("").split(',') {
                let part = part.trim();
                let name = part.rsplit(" as ").next().unwrap_or(part).trim();
                if let Some(n) = ident(name).filter(|n| *n != "default") {
                    add(n, false);
                }
            }
            continue;
        }
        let rest = rest.strip_prefix("async ").unwrap_or(rest).trim_start();
        for kw in [
            "function*",
            "function",
            "class",
            "const",
            "let",
            "var",
            "abstract class",
        ] {
            if let Some(r) = rest.strip_prefix(kw) {
                if let Some(n) = r
                    .starts_with(char::is_whitespace)
                    .then(|| ident(r.trim_start()))
                    .flatten()
                {
                    add(n, false);
                }
                break;
            }
        }
    }
    out
}

/// Where the project keeps its JS/TS tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsLayout {
    /// `Button.test.ts` next to `Button.ts`.
    Colocated,
    /// `__tests__/Button.test.ts` next to `Button.ts`.
    TestsFolder,
    /// `tests/…/Button.test.ts` (mirroring the source folders below `src/`).
    Separate,
}

fn js_convention(files: &[String]) -> (JsLayout, &'static str, Option<String>) {
    let (mut colocated, mut folder, mut separate, mut spec, mut test) = (0, 0, 0, 0, 0);
    let mut root: Option<String> = None;
    for f in files
        .iter()
        .filter(|f| !f.contains("node_modules/") && is_js_test(f))
    {
        let (_, quals, _) = js_parts(f).expect("a JS test");
        if quals.contains(&"spec") {
            spec += 1;
        } else if quals.contains(&"test") {
            test += 1;
        }
        let segs: Vec<&str> = dir_of(f).split('/').collect();
        if segs.contains(&"__tests__") {
            folder += 1;
        } else if matches!(segs.first(), Some(&"tests") | Some(&"test")) {
            separate += 1;
            root.get_or_insert_with(|| segs[0].to_string());
        } else {
            colocated += 1;
        }
    }
    let layout = if separate > colocated && separate > folder {
        JsLayout::Separate
    } else if folder > colocated {
        JsLayout::TestsFolder
    } else {
        JsLayout::Colocated
    };
    (layout, if spec > test { "spec" } else { "test" }, root)
}

fn js_plan(
    req: &TestNavRequest,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<NewTestPlan> {
    let path = req.path.as_str();
    let (stem, _, ext) = js_parts(path)?;
    let pkg = package_deps(read);
    let config = |name: &str| {
        files
            .iter()
            .any(|f| file_name(f).starts_with(&format!("{name}.config.")))
    };
    let vitest = has_dep(&pkg, "vitest") || config("vitest");
    let jest = has_dep(&pkg, "jest") || config("jest");
    let frameworks = match (vitest, jest) {
        (false, true) => vec![TestFramework::Jest],
        (true, false) => vec![TestFramework::Vitest],
        _ => vec![TestFramework::Vitest, TestFramework::Jest],
    };
    let (layout, suffix, root) = js_convention(files);
    let dir = dir_of(path);
    let directory = match layout {
        JsLayout::Colocated => dir.to_string(),
        JsLayout::TestsFolder => join(dir, "__tests__"),
        JsLayout::Separate => {
            let root = root.unwrap_or_else(|| "tests".into());
            let below = dir
                .split_once('/')
                .filter(|(first, _)| matches!(*first, "src" | "lib" | "app"))
                .map_or(
                    if matches!(dir, "src" | "lib" | "app") {
                        ""
                    } else {
                        dir
                    },
                    |(_, rest)| rest,
                );
            join(&root, below)
        }
    };
    let typescript = files.iter().any(|f| f == "tsconfig.json") || has_dep(&pkg, "typescript");
    let test_ext = match ext.to_ascii_lowercase().as_str() {
        "vue" | "svelte" => if typescript { "ts" } else { "js" }.to_string(),
        _ => ext.to_string(),
    };
    Some(NewTestPlan {
        framework: frameworks[0],
        frameworks,
        subject: stem.to_string(),
        subject_fqn: String::new(),
        subject_path: path.to_string(),
        class_name: String::new(),
        namespace: String::new(),
        directory,
        superclass: String::new(),
        superclasses: Vec::new(),
        file_name: format!("{stem}.{suffix}.{test_ext}"),
        members: js_exports(path, &req.text, stem),
    })
}

/// Create New Test: the dialog's initial values for the subject `req.path`; `None` for a
/// test or an unsupported file.
pub fn new_test_plan(
    req: &TestNavRequest,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<NewTestPlan> {
    let path = req.path.as_str();
    if is_php(path) {
        (!is_php_test(path, &req.text)).then(|| php_plan(req, files, read))
    } else if is_js_test(path) {
        None
    } else {
        js_plan(req, files, read)
    }
}

// ---- Test source

fn ucfirst(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn php_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn check_file_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Specify the file name".into());
    }
    if name.contains(['/', '\\', ':', '\0']) || name == "." || name == ".." {
        return Err(format!("'{name}' is not a valid file name"));
    }
    Ok(())
}

fn check_directory(dir: &str) -> Result<(), String> {
    if dir
        .split('/')
        .any(|s| s == ".." || s.contains(['\\', ':', '\0']))
    {
        return Err(format!("'{dir}' is not a valid directory"));
    }
    Ok(())
}

/// `'ship order'` → a quoted string for PHP (`'`) or JS (`"`).
fn quote(s: &str, q: char) -> String {
    let escaped = s.replace('\\', "\\\\").replace(q, &format!("\\{q}"));
    format!("{q}{escaped}{q}")
}

/// `shipOrder` → `ship order`.
fn words(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            out.push(' ');
        } else if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn phpunit_source(req: &TestSourceRequest) -> Result<(String, u32), String> {
    let class = req.class_name.trim();
    if !php_identifier(class) {
        return Err(if class.is_empty() {
            "Specify the test class name".into()
        } else {
            format!("'{class}' is not a valid class name")
        });
    }
    let ns = req.namespace.trim().trim_matches('\\');
    if !ns.is_empty() && !ns.split('\\').all(php_identifier) {
        return Err(format!("'{ns}' is not a valid namespace"));
    }
    let parent = req.superclass.trim().trim_start_matches('\\');
    if !parent.is_empty() && !parent.split('\\').all(php_identifier) {
        return Err(format!("'{parent}' is not a valid class name"));
    }
    let mut out = String::from("<?php\n\n");
    if !ns.is_empty() {
        out.push_str(&format!("namespace {ns};\n\n"));
    }
    let mut extends = String::new();
    if !parent.is_empty() {
        let (pns, short) = parent.rsplit_once('\\').unwrap_or(("", parent));
        if pns.eq_ignore_ascii_case(ns) || (pns.is_empty() && ns.is_empty()) {
            extends = format!(" extends {short}");
        } else if short == class || ns.is_empty() {
            extends = format!(" extends \\{parent}");
        } else {
            out.push_str(&format!("use {parent};\n\n"));
            extends = format!(" extends {short}");
        }
    }
    out.push_str(&format!("class {class}{extends}\n{{\n"));
    let mut blocks: Vec<String> = Vec::new();
    let call_parent = !parent.is_empty();
    let mut caret = None;
    if req.set_up {
        blocks.push(format!(
            "    protected function setUp(): void\n    {{\n{}    }}\n",
            if call_parent {
                "        parent::setUp();\n"
            } else {
                "\n"
            }
        ));
    }
    if req.tear_down {
        blocks.push(format!(
            "    protected function tearDown(): void\n    {{\n{}    }}\n",
            if call_parent {
                "        parent::tearDown();\n"
            } else {
                "\n"
            }
        ));
    }
    let mut seen: Vec<String> = Vec::new();
    for m in &req.members {
        let name = format!("test{}", ucfirst(m.name.trim()));
        if !php_identifier(&name) || seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        caret.get_or_insert(blocks.len());
        blocks.push(format!(
            "    public function {name}(): void\n    {{\n\n    }}\n"
        ));
    }
    let mut line = None;
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if Some(i) == caret {
            line = Some(out.matches('\n').count() as u32 + 2);
        }
        out.push_str(b);
    }
    if blocks.is_empty() {
        out.push('\n');
    }
    let line = line.unwrap_or(out.matches('\n').count() as u32 - 1);
    out.push_str("}\n");
    Ok((out, line))
}

fn pest_source(req: &TestSourceRequest) -> (String, u32) {
    let mut out = String::from("<?php\n\n");
    let mut line = None;
    if req.set_up {
        out.push_str("beforeEach(function () {\n    //\n});\n\n");
    }
    if req.tear_down {
        out.push_str("afterEach(function () {\n    //\n});\n\n");
    }
    let mut any = false;
    for m in &req.members {
        let name = words(m.name.trim());
        if name.is_empty() {
            continue;
        }
        any = true;
        line.get_or_insert(out.matches('\n').count() as u32 + 1);
        out.push_str(&format!(
            "test({}, function () {{\n    //\n}});\n\n",
            quote(&name, '\'')
        ));
    }
    if !any {
        line = Some(out.matches('\n').count() as u32 + 1);
        out.push_str("test('example', function () {\n    expect(true)->toBeTrue();\n});\n\n");
    }
    out.pop();
    (out, line.unwrap_or(0))
}

/// The import path of `subject` from a file in `from_dir` (`../utils/format`).
fn relative_import(from_dir: &str, subject: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = subject.split('/').filter(|s| !s.is_empty()).collect();
    let (to_dirs, file) = to.split_at(to.len().saturating_sub(1));
    let common = from.iter().zip(to_dirs).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); from.len() - common];
    parts.extend(to_dirs[common..].iter().map(|s| s.to_string()));
    let file = file.first().copied().unwrap_or("");
    let lower = file.to_ascii_lowercase();
    let keep_ext = lower.ends_with(".vue") || lower.ends_with(".svelte");
    let name = if keep_ext {
        file.to_string()
    } else {
        file.rsplit_once('.').map_or(file, |(s, _)| s).to_string()
    };
    parts.push(name);
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

fn js_source(req: &TestSourceRequest, framework: TestFramework) -> (String, u32) {
    let members: Vec<&TestMember> = req
        .members
        .iter()
        .filter(|m| ident(m.name.trim()) == Some(m.name.trim()))
        .collect();
    let mut globals = vec!["describe", "it"];
    if !members.is_empty() {
        globals.push("expect");
    }
    if req.set_up {
        globals.push("beforeEach");
    }
    if req.tear_down {
        globals.push("afterEach");
    }
    globals.sort_unstable();
    let mut out = String::new();
    if framework == TestFramework::Vitest {
        out.push_str(&format!(
            "import {{ {} }} from \"vitest\";\n",
            globals.join(", ")
        ));
    }
    let default = members.iter().find(|m| m.default_export);
    let named: Vec<&str> = members
        .iter()
        .filter(|m| !m.default_export)
        .map(|m| m.name.trim())
        .collect();
    if default.is_some() || !named.is_empty() {
        let mut spec = Vec::new();
        if let Some(d) = default {
            spec.push(d.name.trim().to_string());
        }
        if !named.is_empty() {
            spec.push(format!("{{ {} }}", named.join(", ")));
        }
        out.push_str(&format!(
            "import {} from \"{}\";\n",
            spec.join(", "),
            relative_import(&req.directory, &req.subject_path)
        ));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "describe({}, () => {{\n",
        quote(&req.subject, '"')
    ));
    let mut blocks: Vec<String> = Vec::new();
    if req.set_up {
        blocks.push("  beforeEach(() => {\n  });\n".into());
    }
    if req.tear_down {
        blocks.push("  afterEach(() => {\n  });\n".into());
    }
    let first_test = blocks.len();
    for m in &members {
        let name = m.name.trim();
        blocks.push(format!(
            "  it({}, () => {{\n    expect({name}).toBeDefined();\n  }});\n",
            quote(name, '"')
        ));
    }
    if members.is_empty() {
        blocks.push("  it(\"works\", () => {\n  });\n".into());
    }
    let mut line = 0;
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i == first_test {
            line = out.matches('\n').count() as u32 + 1;
        }
        out.push_str(b);
    }
    out.push_str("});\n");
    (out, line)
}

/// Create New Test: the new file's path and text.
pub fn test_source(req: &TestSourceRequest) -> Result<TestSource, String> {
    let framework = req.framework.ok_or("Choose a test framework")?;
    check_file_name(&req.file_name)?;
    let directory = req.directory.trim().trim_matches('/');
    check_directory(directory)?;
    let (text, line) = match framework {
        TestFramework::PHPUnit => phpunit_source(req)?,
        TestFramework::Pest => pest_source(req),
        TestFramework::Vitest | TestFramework::Jest => js_source(
            &TestSourceRequest {
                directory: directory.to_string(),
                ..req.clone()
            },
            framework,
        ),
    };
    Ok(TestSource {
        path: join(directory, req.file_name.trim()),
        text,
        line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn project(files: &[(&str, &str)]) -> (Vec<String>, HashMap<String, String>) {
        (
            files.iter().map(|(p, _)| p.to_string()).collect(),
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        )
    }

    fn targets(files: &[(&str, &str)], path: &str) -> TestTargets {
        let (list, map) = project(files);
        let read = |p: &str| map.get(p).cloned();
        test_targets(
            &TestNavRequest {
                path: path.into(),
                text: map.get(path).cloned().unwrap_or_default(),
                composer_path: String::new(),
            },
            &list,
            &read,
        )
    }

    fn plan(files: &[(&str, &str)], path: &str) -> NewTestPlan {
        let (list, map) = project(files);
        let read = |p: &str| map.get(p).cloned();
        new_test_plan(
            &TestNavRequest {
                path: path.into(),
                text: map.get(path).cloned().unwrap_or_default(),
                composer_path: String::new(),
            },
            &list,
            &read,
        )
        .expect("a plan")
    }

    fn paths(t: &TestTargets) -> Vec<&str> {
        t.targets.iter().map(|i| i.path.as_str()).collect()
    }

    const LARAVEL_COMPOSER: &str = r#"{
        "require": {"laravel/framework": "^12.0"},
        "require-dev": {"phpunit/phpunit": "^11.0"},
        "autoload": {"psr-4": {"App\\": "app/"}},
        "autoload-dev": {"psr-4": {"Tests\\": "tests/"}}
    }"#;

    const ORDER: &str = "<?php\n\nnamespace App\\Models;\n\nclass Order extends Model\n{\n    public function __construct() {}\n\n    public function ship(): void\n    {\n    }\n\n    public static function findByNumber(string $n): ?self\n    {\n        return null;\n    }\n\n    protected function secret()\n    {\n    }\n\n    private function hidden() {}\n\n    function legacy()\n    {\n    }\n}\n";

    fn laravel() -> Vec<(&'static str, &'static str)> {
        vec![
            ("composer.json", LARAVEL_COMPOSER),
            ("app/Models/Order.php", ORDER),
            (
                "app/Http/Controllers/OrderController.php",
                "<?php\n\nnamespace App\\Http\\Controllers;\n\nclass OrderController extends Controller\n{\n    public function index()\n    {\n    }\n}\n",
            ),
            ("tests/TestCase.php", "<?php\n\nnamespace Tests;\n\nabstract class TestCase extends BaseTestCase\n{\n}\n"),
            ("tests/Unit/ExampleTest.php", "<?php\n\nnamespace Tests\\Unit;\n\nuse PHPUnit\\Framework\\TestCase;\n\nclass ExampleTest extends TestCase {}\n"),
            ("tests/Feature/ExampleTest.php", "<?php\n\nnamespace Tests\\Feature;\n\nuse Tests\\TestCase;\n\nclass ExampleTest extends TestCase {}\n"),
        ]
    }

    #[test]
    fn php_class_goes_to_its_tests_best_match_first() {
        let mut files = laravel();
        files.push((
            "tests/Feature/OrderTest.php",
            "<?php\nclass OrderTest extends TestCase {}\n",
        ));
        files.push((
            "tests/Unit/Models/OrderTest.php",
            "<?php\nclass OrderTest extends TestCase {}\n",
        ));
        let t = targets(&files, "app/Models/Order.php");
        assert!(!t.is_test);
        assert!(t.can_create);
        assert_eq!(t.name, "Order");
        assert_eq!(
            paths(&t),
            [
                "tests/Unit/Models/OrderTest.php",
                "tests/Feature/OrderTest.php"
            ]
        );
        assert!(t.targets.iter().all(|i| i.kind == RelatedKind::Test));
        // And back: the test's subject.
        let back = targets(&files, "tests/Unit/Models/OrderTest.php");
        assert!(back.is_test);
        assert!(!back.can_create);
        assert_eq!(back.name, "OrderTest");
        assert_eq!(paths(&back), ["app/Models/Order.php"]);
    }

    #[test]
    fn a_class_without_tests_offers_create() {
        let t = targets(&laravel(), "app/Models/Order.php");
        assert!(t.targets.is_empty());
        assert!(t.can_create);
        // Other files have no tests.
        let none = targets(&[("README.md", "")], "README.md");
        assert!(!none.can_create);
    }

    #[test]
    fn pest_tests_are_tests() {
        let files = [
            (
                "composer.json",
                r#"{"require-dev":{"pestphp/pest":"^3.0"}}"#,
            ),
            ("src/Cart.php", "<?php\nclass Cart {}\n"),
            (
                "tests/Unit/CartTest.php",
                "<?php\n\ntest('adds items', function () {\n    expect(true)->toBeTrue();\n});\n",
            ),
        ];
        assert_eq!(
            paths(&targets(&files, "src/Cart.php")),
            ["tests/Unit/CartTest.php"]
        );
        let back = targets(&files, "tests/Unit/CartTest.php");
        assert!(back.is_test);
        assert_eq!(paths(&back), ["src/Cart.php"]);
    }

    #[test]
    fn js_modules_and_their_tests() {
        let files = [
            ("src/components/Button.vue", ""),
            ("src/components/__tests__/Button.spec.ts", ""),
            ("src/utils/format.ts", ""),
            ("src/utils/format.test.ts", ""),
        ];
        assert_eq!(
            paths(&targets(&files, "src/components/Button.vue")),
            ["src/components/__tests__/Button.spec.ts"]
        );
        let back = targets(&files, "src/utils/format.test.ts");
        assert!(back.is_test);
        assert_eq!(back.name, "format");
        assert_eq!(paths(&back), ["src/utils/format.ts"]);
    }

    #[test]
    fn laravel_unit_test_plan_mirrors_the_source_path() {
        let p = plan(&laravel(), "app/Models/Order.php");
        assert_eq!(p.frameworks, [TestFramework::PHPUnit]);
        assert_eq!(p.subject, "Order");
        assert_eq!(p.subject_fqn, "App\\Models\\Order");
        assert_eq!(p.class_name, "OrderTest");
        assert_eq!(p.namespace, "Tests\\Unit\\Models");
        assert_eq!(p.directory, "tests/Unit/Models");
        assert_eq!(p.file_name, "OrderTest.php");
        // Laravel's Unit tests extend PHPUnit's TestCase like the skeleton's.
        assert_eq!(p.superclass, "PHPUnit\\Framework\\TestCase");
        assert_eq!(
            p.superclasses,
            ["Tests\\TestCase", "PHPUnit\\Framework\\TestCase"]
        );
        let names: Vec<&str> = p.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["ship", "findByNumber", "legacy"]);
    }

    #[test]
    fn laravel_controllers_get_feature_tests() {
        let p = plan(&laravel(), "app/Http/Controllers/OrderController.php");
        assert_eq!(p.namespace, "Tests\\Feature\\Http\\Controllers");
        assert_eq!(p.directory, "tests/Feature/Http/Controllers");
        assert_eq!(p.superclass, "Tests\\TestCase");
        assert_eq!(
            p.members,
            [TestMember {
                name: "index".into(),
                default_export: false
            }]
        );
    }

    #[test]
    fn symfony_tests_mirror_src_under_app_tests() {
        let files = [
            (
                "composer.json",
                r#"{"require":{"symfony/framework-bundle":"^7.0"},"autoload":{"psr-4":{"App\\":"src/"}},"autoload-dev":{"psr-4":{"App\\Tests\\":"tests/"}}}"#,
            ),
            ("src/Service/Mailer.php", "<?php\nnamespace App\\Service;\n\nfinal class Mailer\n{\n    public function send(): void\n    {\n    }\n}\n"),
            ("src/Controller/HomeController.php", "<?php\nnamespace App\\Controller;\n\nclass HomeController\n{\n}\n"),
        ];
        let p = plan(&files, "src/Service/Mailer.php");
        assert_eq!(p.namespace, "App\\Tests\\Service");
        assert_eq!(p.directory, "tests/Service");
        assert_eq!(p.superclass, "PHPUnit\\Framework\\TestCase");
        assert!(p
            .superclasses
            .contains(&"Symfony\\Bundle\\FrameworkBundle\\Test\\KernelTestCase".to_string()));
        let c = plan(&files, "src/Controller/HomeController.php");
        assert_eq!(
            c.superclass,
            "Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase"
        );
        assert_eq!(c.directory, "tests/Controller");
    }

    #[test]
    fn pest_projects_default_to_pest_and_plain_ones_to_tests() {
        let files = [
            (
                "composer.json",
                r#"{"require-dev":{"pestphp/pest":"^3.0"},"autoload":{"psr-4":{"Acme\\":"src/"}}}"#,
            ),
            (
                "src/Billing/Invoice.php",
                "<?php\nnamespace Acme\\Billing;\n\nclass Invoice {}\n",
            ),
        ];
        let p = plan(&files, "src/Billing/Invoice.php");
        assert_eq!(p.frameworks, [TestFramework::Pest, TestFramework::PHPUnit]);
        assert_eq!(p.framework, TestFramework::Pest);
        assert_eq!(p.namespace, "Tests\\Billing");
        assert_eq!(p.directory, "tests/Billing");
        // A test is not a subject.
        let (list, map) = project(&[(
            "tests/FooTest.php",
            "<?php class FooTest extends TestCase {}",
        )]);
        let read = |p: &str| map.get(p).cloned();
        let req = TestNavRequest {
            path: "tests/FooTest.php".into(),
            text: map["tests/FooTest.php"].clone(),
            composer_path: String::new(),
        };
        assert!(new_test_plan(&req, &list, &read).is_none());
    }

    #[test]
    fn js_plans_follow_the_project_convention() {
        let colocated = [
            ("package.json", r#"{"devDependencies":{"vitest":"^2.0","typescript":"^5.0"}}"#),
            ("src/utils/format.ts", "export function format(n: number) {}\nexport const PI = 3.14;\nexport default class Formatter {}\nexport { helper as other };\n"),
            ("src/utils/date.ts", ""),
            ("src/utils/date.spec.ts", ""),
        ];
        let p = plan(&colocated, "src/utils/format.ts");
        assert_eq!(p.frameworks, [TestFramework::Vitest]);
        assert_eq!(p.directory, "src/utils");
        assert_eq!(p.file_name, "format.spec.ts");
        let names: Vec<(&str, bool)> = p
            .members
            .iter()
            .map(|m| (m.name.as_str(), m.default_export))
            .collect();
        assert_eq!(
            names,
            [
                ("format", false),
                ("PI", false),
                ("Formatter", true),
                ("other", false)
            ]
        );

        let folder = [
            ("package.json", r#"{"devDependencies":{"jest":"^29.0"}}"#),
            ("src/components/Button.vue", "<template></template>"),
            ("src/components/__tests__/Card.test.js", ""),
        ];
        let p = plan(&folder, "src/components/Button.vue");
        assert_eq!(p.frameworks, [TestFramework::Jest]);
        assert_eq!(p.directory, "src/components/__tests__");
        assert_eq!(p.file_name, "Button.test.js");
        assert_eq!(
            p.members,
            [TestMember {
                name: "Button".into(),
                default_export: true
            }]
        );

        let separate = [
            ("src/lib/api/client.ts", "export async function get() {}\n"),
            ("tests/lib/other.test.ts", ""),
            ("tests/api.test.ts", ""),
        ];
        let p = plan(&separate, "src/lib/api/client.ts");
        assert_eq!(p.directory, "tests/lib/api");
        assert_eq!(p.frameworks, [TestFramework::Vitest, TestFramework::Jest]);
    }

    fn source_req(framework: TestFramework) -> TestSourceRequest {
        TestSourceRequest {
            framework: Some(framework),
            subject: "Order".into(),
            subject_fqn: "App\\Models\\Order".into(),
            subject_path: "app/Models/Order.php".into(),
            class_name: "OrderTest".into(),
            namespace: "Tests\\Unit\\Models".into(),
            directory: "tests/Unit/Models/".into(),
            superclass: "PHPUnit\\Framework\\TestCase".into(),
            file_name: "OrderTest.php".into(),
            members: vec![
                TestMember {
                    name: "ship".into(),
                    default_export: false,
                },
                TestMember {
                    name: "findByNumber".into(),
                    default_export: false,
                },
            ],
            set_up: true,
            tear_down: false,
        }
    }

    #[test]
    fn phpunit_source_with_set_up_and_test_methods() {
        let s = test_source(&source_req(TestFramework::PHPUnit)).unwrap();
        assert_eq!(s.path, "tests/Unit/Models/OrderTest.php");
        assert_eq!(
            s.text,
            "<?php\n\nnamespace Tests\\Unit\\Models;\n\nuse PHPUnit\\Framework\\TestCase;\n\nclass OrderTest extends TestCase\n{\n    protected function setUp(): void\n    {\n        parent::setUp();\n    }\n\n    public function testShip(): void\n    {\n\n    }\n\n    public function testFindByNumber(): void\n    {\n\n    }\n}\n"
        );
        assert_eq!(s.text.lines().nth(s.line as usize), Some(""));
        assert_eq!(
            s.text.lines().nth(s.line as usize - 2).map(str::trim),
            Some("public function testShip(): void")
        );
        // Same namespace as the superclass: no import.
        let mut r = source_req(TestFramework::PHPUnit);
        r.superclass = "Tests\\Unit\\Models\\BaseCase".into();
        r.members.clear();
        r.set_up = false;
        let s = test_source(&r).unwrap();
        assert!(
            s.text
                .contains("class OrderTest extends BaseCase\n{\n\n}\n"),
            "{}",
            s.text
        );
        assert!(!s.text.contains("use "));
        r.class_name = "1Bad".into();
        assert_eq!(
            test_source(&r).unwrap_err(),
            "'1Bad' is not a valid class name"
        );
        r.class_name = "OrderTest".into();
        r.directory = "../outside".into();
        assert!(test_source(&r).is_err());
    }

    #[test]
    fn pest_source_with_hooks_and_tests() {
        let mut r = source_req(TestFramework::Pest);
        r.tear_down = true;
        let s = test_source(&r).unwrap();
        assert_eq!(
            s.text,
            "<?php\n\nbeforeEach(function () {\n    //\n});\n\nafterEach(function () {\n    //\n});\n\ntest('ship', function () {\n    //\n});\n\ntest('find by number', function () {\n    //\n});\n"
        );
        assert_eq!(
            s.text.lines().nth(s.line as usize).map(str::trim),
            Some("//")
        );
        r.members.clear();
        r.set_up = false;
        r.tear_down = false;
        assert!(test_source(&r).unwrap().text.contains("test('example'"));
    }

    #[test]
    fn vitest_and_jest_sources_import_the_subject() {
        let r = TestSourceRequest {
            framework: Some(TestFramework::Vitest),
            subject: "format".into(),
            subject_path: "src/utils/format.ts".into(),
            directory: "tests/utils".into(),
            file_name: "format.test.ts".into(),
            members: vec![
                TestMember {
                    name: "format".into(),
                    default_export: false,
                },
                TestMember {
                    name: "Formatter".into(),
                    default_export: true,
                },
            ],
            set_up: true,
            ..Default::default()
        };
        let s = test_source(&r).unwrap();
        assert_eq!(s.path, "tests/utils/format.test.ts");
        assert_eq!(
            s.text,
            "import { beforeEach, describe, expect, it } from \"vitest\";\nimport Formatter, { format } from \"../../src/utils/format\";\n\ndescribe(\"format\", () => {\n  beforeEach(() => {\n  });\n\n  it(\"format\", () => {\n    expect(format).toBeDefined();\n  });\n\n  it(\"Formatter\", () => {\n    expect(Formatter).toBeDefined();\n  });\n});\n"
        );
        assert_eq!(
            s.text.lines().nth(s.line as usize).map(str::trim),
            Some("expect(format).toBeDefined();")
        );
        let jest = test_source(&TestSourceRequest {
            framework: Some(TestFramework::Jest),
            subject: "Button".into(),
            subject_path: "src/components/Button.vue".into(),
            directory: "src/components/__tests__".into(),
            file_name: "Button.test.js".into(),
            members: vec![TestMember {
                name: "Button".into(),
                default_export: true,
            }],
            ..Default::default()
        })
        .unwrap();
        assert!(
            jest.text
                .starts_with("import Button from \"../Button.vue\";\n\ndescribe(\"Button\""),
            "{}",
            jest.text
        );
        assert_eq!(
            relative_import("src/utils", "src/utils/format.ts"),
            "./format"
        );
        assert_eq!(relative_import("", "src/a.tsx"), "./src/a");
    }
}

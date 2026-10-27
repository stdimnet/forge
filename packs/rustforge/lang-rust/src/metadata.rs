//! A Cargo workspace from `cargo metadata --format-version 1`: the members with their
//! targets (bin, lib, test, bench, example, build script, proc macro), features and direct
//! dependencies, and each member's dependency tree from the `resolve` graph (like
//! `cargo tree`: packages seen before are marked `repeated` and not expanded again).
//!
//! Paths become project relative (`/` separated); `--no-deps` output (no `resolve`) gives
//! trees of the direct dependencies only.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// What a target builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Lib,
    Bin,
    Test,
    Bench,
    Example,
    BuildScript,
    ProcMacro,
}

impl TargetKind {
    /// Cargo's `kind` list (`["lib"]`, `["cdylib", "rlib"]`, `["custom-build"]`, …).
    pub fn from_kinds(kinds: &[String]) -> Self {
        let has = |k: &str| kinds.iter().any(|x| x == k);
        if has("bin") {
            TargetKind::Bin
        } else if has("test") {
            TargetKind::Test
        } else if has("bench") {
            TargetKind::Bench
        } else if has("example") {
            TargetKind::Example
        } else if has("custom-build") {
            TargetKind::BuildScript
        } else if has("proc-macro") {
            TargetKind::ProcMacro
        } else {
            TargetKind::Lib
        }
    }

    /// The cargo option that selects a target of this kind (`--bin`, `--test`, …).
    pub fn flag(self) -> Option<&'static str> {
        match self {
            TargetKind::Lib | TargetKind::ProcMacro => Some("--lib"),
            TargetKind::Bin => Some("--bin"),
            TargetKind::Test => Some("--test"),
            TargetKind::Bench => Some("--bench"),
            TargetKind::Example => Some("--example"),
            TargetKind::BuildScript => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            TargetKind::Lib => "lib",
            TargetKind::Bin => "bin",
            TargetKind::Test => "test",
            TargetKind::Bench => "bench",
            TargetKind::Example => "example",
            TargetKind::BuildScript => "build-script",
            TargetKind::ProcMacro => "proc-macro",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CargoTarget {
    pub name: String,
    pub kind: TargetKind,
    /// The crate root, project relative.
    pub src_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub test: bool,
    #[serde(default)]
    pub doctest: bool,
}

impl CargoTarget {
    /// `lib`, `bin:shop`, `test:checkout`: how configurations name a target.
    pub fn key(&self) -> String {
        match self.kind {
            TargetKind::Lib | TargetKind::ProcMacro => "lib".into(),
            k => format!("{}:{}", k.id(), self.name),
        }
    }

    /// The crate name in Rust paths (`shop-core` → `shop_core`).
    pub fn crate_name(&self) -> String {
        self.name.replace('-', "_")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub name: String,
    /// Features and dependencies it enables (`dep:serde`, `serde/derive`).
    pub enables: Vec<String>,
    /// Part of the `default` feature.
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub name: String,
    pub req: String,
    /// `normal`, `dev` or `build`.
    pub kind: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CargoPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub edition: String,
    /// `Cargo.toml`, project relative.
    pub manifest: String,
    /// The package folder, project relative (`""` for the root).
    pub dir: String,
    pub targets: Vec<CargoTarget>,
    pub features: Vec<Feature>,
    pub dependencies: Vec<Dependency>,
}

impl CargoPackage {
    pub fn target(&self, key: &str) -> Option<&CargoTarget> {
        self.targets.iter().find(|t| t.key() == key)
    }

    pub fn lib(&self) -> Option<&CargoTarget> {
        self.targets
            .iter()
            .find(|t| matches!(t.kind, TargetKind::Lib | TargetKind::ProcMacro))
    }
}

/// A node of a dependency tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyNode {
    pub name: String,
    pub version: String,
    /// `registry`, `git`, `path` or `` (a workspace member).
    pub source: String,
    /// `normal`, `dev` or `build`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DependencyNode>,
    /// Shown before in this tree, not expanded again (`cargo tree`'s `(*)`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeated: bool,
}

/// A Cargo workspace as the Cargo tool window shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CargoWorkspace {
    /// The workspace root, project relative (`""`: the project root).
    pub root: String,
    /// The target directory (absolute, or project relative when inside the project).
    pub target_directory: String,
    pub members: Vec<CargoPackage>,
    /// One tree per member, in member order.
    pub trees: Vec<DependencyNode>,
}

impl CargoWorkspace {
    pub fn package(&self, name: &str) -> Option<&CargoPackage> {
        self.members.iter().find(|p| p.name == name)
    }

    /// The member and target whose crate root is `path` (project relative).
    pub fn target_of_root(&self, path: &str) -> Option<(&CargoPackage, &CargoTarget)> {
        self.members.iter().find_map(|p| {
            p.targets
                .iter()
                .find(|t| t.src_path == path)
                .map(|t| (p, t))
        })
    }

    /// The member that contains `path` (the deepest package folder).
    pub fn package_of(&self, path: &str) -> Option<&CargoPackage> {
        self.members
            .iter()
            .filter(|p| p.dir.is_empty() || path.starts_with(&format!("{}/", p.dir)))
            .max_by_key(|p| p.dir.len())
    }
}

// ---- cargo metadata JSON ----------------------------------------------------------------

#[derive(Deserialize)]
struct RawMetadata {
    packages: Vec<RawPackage>,
    #[serde(default)]
    workspace_members: Vec<String>,
    #[serde(default)]
    resolve: Option<RawResolve>,
    workspace_root: String,
    #[serde(default)]
    target_directory: String,
}

#[derive(Deserialize)]
struct RawPackage {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    edition: String,
    manifest_path: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    targets: Vec<RawTarget>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Deserialize)]
struct RawTarget {
    name: String,
    kind: Vec<String>,
    src_path: String,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
    #[serde(default = "yes")]
    test: bool,
    #[serde(default = "yes")]
    doctest: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct RawDependency {
    name: String,
    #[serde(default)]
    req: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    rename: Option<String>,
}

#[derive(Deserialize)]
struct RawResolve {
    nodes: Vec<RawNode>,
}

#[derive(Deserialize)]
struct RawNode {
    id: String,
    #[serde(default)]
    deps: Vec<RawNodeDep>,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Deserialize)]
struct RawNodeDep {
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<RawDepKind>,
}

#[derive(Deserialize)]
struct RawDepKind {
    #[serde(default)]
    kind: Option<String>,
}

/// `path` relative to `root` with `/` separators; absolute paths outside stay absolute.
pub fn relative(path: &str, root: &str) -> String {
    let p = path.replace('\\', "/");
    let r = root.replace('\\', "/");
    let r = r.trim_end_matches('/');
    if p == r {
        return String::new();
    }
    match p.strip_prefix(&format!("{r}/")) {
        Some(rest) if !r.is_empty() => rest.to_string(),
        _ => p,
    }
}

fn dir_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default()
}

fn source_kind(source: Option<&str>) -> String {
    match source {
        None => "path".into(),
        Some(s) if s.starts_with("git+") => "git".into(),
        Some(s) if s.starts_with("path+") => "path".into(),
        Some(_) => "registry".into(),
    }
}

/// Parse `cargo metadata --format-version 1` output; paths become relative to
/// `project_root` (absolute, as cargo prints them).
pub fn parse_metadata(json: &str, project_root: &str) -> Result<CargoWorkspace, String> {
    let raw: RawMetadata =
        serde_json::from_str(json).map_err(|e| format!("unexpected cargo metadata: {e}"))?;
    let rel = |p: &str| relative(p, project_root);
    let by_id: HashMap<&str, &RawPackage> =
        raw.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let members: Vec<CargoPackage> = raw
        .workspace_members
        .iter()
        .filter_map(|id| by_id.get(id.as_str()))
        .map(|p| {
            let default: HashSet<&str> = p
                .features
                .get("default")
                .map(|d| d.iter().map(String::as_str).collect())
                .unwrap_or_default();
            let manifest = rel(&p.manifest_path);
            CargoPackage {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                edition: p.edition.clone(),
                dir: dir_of(&manifest),
                manifest,
                targets: p
                    .targets
                    .iter()
                    .map(|t| CargoTarget {
                        name: t.name.clone(),
                        kind: TargetKind::from_kinds(&t.kind),
                        src_path: rel(&t.src_path),
                        required_features: t.required_features.clone(),
                        test: t.test,
                        doctest: t.doctest,
                    })
                    .collect(),
                features: p
                    .features
                    .iter()
                    .filter(|(name, _)| name.as_str() != "default")
                    .map(|(name, enables)| Feature {
                        name: name.clone(),
                        enables: enables.clone(),
                        default: default.contains(name.as_str()),
                    })
                    .collect(),
                dependencies: p
                    .dependencies
                    .iter()
                    .map(|d| Dependency {
                        name: d.rename.clone().unwrap_or_else(|| d.name.clone()),
                        req: d.req.clone(),
                        kind: d.kind.clone().unwrap_or_else(|| "normal".into()),
                        optional: d.optional,
                    })
                    .collect(),
            }
        })
        .collect();
    let trees = match &raw.resolve {
        Some(resolve) => {
            let nodes: HashMap<&str, &RawNode> =
                resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
            members
                .iter()
                .map(|m| {
                    let mut seen = HashSet::new();
                    tree(&m.id, "normal", &by_id, &nodes, &mut seen, 0)
                })
                .collect()
        }
        None => members
            .iter()
            .map(|m| DependencyNode {
                name: m.name.clone(),
                version: m.version.clone(),
                source: String::new(),
                kind: "normal".into(),
                features: Vec::new(),
                children: m
                    .dependencies
                    .iter()
                    .map(|d| DependencyNode {
                        name: d.name.clone(),
                        version: d.req.clone(),
                        source: String::new(),
                        kind: d.kind.clone(),
                        features: Vec::new(),
                        children: Vec::new(),
                        repeated: false,
                    })
                    .collect(),
                repeated: false,
            })
            .collect(),
    };
    let target_directory = rel(&raw.target_directory);
    Ok(CargoWorkspace {
        root: rel(&raw.workspace_root),
        target_directory,
        members,
        trees,
    })
}

const MAX_DEPTH: usize = 12;

fn tree(
    id: &str,
    kind: &str,
    packages: &HashMap<&str, &RawPackage>,
    nodes: &HashMap<&str, &RawNode>,
    seen: &mut HashSet<String>,
    depth: usize,
) -> DependencyNode {
    let pkg = packages.get(id);
    let node = nodes.get(id);
    let repeated = depth > 0 && !seen.insert(id.to_string());
    let mut children = Vec::new();
    if !repeated && depth < MAX_DEPTH {
        if let Some(n) = node {
            let mut deps: Vec<(&str, String)> = n
                .deps
                .iter()
                .filter_map(|d| {
                    // Only the root's dev-dependencies, like `cargo tree`.
                    let kinds: Vec<String> = d
                        .dep_kinds
                        .iter()
                        .map(|k| k.kind.clone().unwrap_or_else(|| "normal".into()))
                        .collect();
                    let kind = ["normal", "build", "dev"].into_iter().find(|k| {
                        kinds.is_empty() && *k == "normal" || kinds.iter().any(|x| x == k)
                    })?;
                    (depth == 0 || kind != "dev").then(|| (d.pkg.as_str(), kind.to_string()))
                })
                .collect();
            let order = |k: &str| match k {
                "normal" => 0,
                "build" => 1,
                _ => 2,
            };
            deps.sort_by(|a, b| {
                order(&a.1).cmp(&order(&b.1)).then_with(|| {
                    let name =
                        |id: &str| packages.get(id).map(|p| p.name.clone()).unwrap_or_default();
                    name(a.0).cmp(&name(b.0))
                })
            });
            for (dep, kind) in deps {
                children.push(tree(dep, &kind, packages, nodes, seen, depth + 1));
            }
        }
    }
    DependencyNode {
        name: pkg
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.to_string()),
        version: pkg.map(|p| p.version.clone()).unwrap_or_default(),
        source: pkg
            .map(|p| source_kind(p.source.as_deref()))
            .unwrap_or_default(),
        kind: kind.to_string(),
        features: node.map(|n| n.features.clone()).unwrap_or_default(),
        children,
        repeated,
    }
}

// ---- Modules and files ------------------------------------------------------------------

/// The module path of `file` in the crate whose root is `root` (both project relative):
/// `src/lib.rs` → `""`, `src/cart.rs` and `src/cart/mod.rs` → `cart`, `src/cart/tests.rs`
/// → `cart::tests`. `None` when the file cannot belong to the crate: outside the root's
/// folder, another target's root (`src/main.rs`, `src/bin/*` for a library), or another
/// test / example crate next to an integration test.
pub fn module_path(root: &str, file: &str, kind: TargetKind) -> Option<String> {
    if root == file {
        return Some(String::new());
    }
    let root_dir = dir_of(root);
    let rest = if root_dir.is_empty() {
        file
    } else {
        file.strip_prefix(&format!("{root_dir}/"))?
    };
    let rest = rest.strip_suffix(".rs")?;
    let stem = root.rsplit('/').next()?.trim_end_matches(".rs");
    let rest = match kind {
        TargetKind::Lib | TargetKind::ProcMacro => {
            if rest == "main" || rest.starts_with("bin/") {
                return None;
            }
            rest
        }
        // `src/main.rs` (and `tests/<name>/main.rs`) own their folder, except other roots.
        _ if stem == "main" => {
            if kind == TargetKind::Bin && (rest.starts_with("bin/") || rest == "lib") {
                return None;
            }
            rest
        }
        // `src/bin/tool.rs` owns `src/bin/tool/…`, `tests/checkout.rs` owns `tests/checkout/…`.
        _ => rest.strip_prefix(&format!("{stem}/"))?,
    };
    let rest = rest.strip_suffix("/mod").unwrap_or(rest);
    if rest.is_empty() || rest == "mod" {
        return Some(String::new());
    }
    Some(rest.replace('/', "::"))
}

/// The crate a source file belongs to: the target and the file's module path. Libraries win
/// over binaries for shared folders.
pub fn crate_of<'a>(
    ws: &'a CargoWorkspace,
    file: &str,
) -> Option<(&'a CargoPackage, &'a CargoTarget, String)> {
    let pkg = ws.package_of(file)?;
    let rank = |k: TargetKind| match k {
        TargetKind::Lib | TargetKind::ProcMacro => 0,
        TargetKind::Bin => 1,
        _ => 2,
    };
    let mut targets: Vec<&CargoTarget> = pkg.targets.iter().collect();
    // Exact roots first, then by rank.
    targets.sort_by_key(|t| (t.src_path != file, rank(t.kind)));
    targets
        .into_iter()
        .filter(|t| t.kind != TargetKind::BuildScript)
        .find_map(|t| module_path(&t.src_path, file, t.kind).map(|m| (pkg, t, m)))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn shop() -> CargoWorkspace {
        parse_metadata(
            include_str!("../tests/fixtures/metadata-shop.json"),
            "/home/anna/Projects/shop",
        )
        .unwrap()
    }

    #[test]
    fn members_targets_and_features() {
        let ws = shop();
        assert_eq!(ws.root, "");
        assert_eq!(ws.target_directory, "target");
        let names: Vec<&str> = ws.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["shop-core", "shop-cli"]);
        let core = ws.package("shop-core").unwrap();
        assert_eq!(core.dir, "crates/shop-core");
        assert_eq!(core.manifest, "crates/shop-core/Cargo.toml");
        assert_eq!(core.edition, "2021");
        let targets: Vec<(String, &str)> = core
            .targets
            .iter()
            .map(|t| (t.key(), t.src_path.as_str()))
            .collect();
        assert!(targets.contains(&("lib".into(), "crates/shop-core/src/lib.rs")));
        assert!(targets.contains(&("test:checkout".into(), "crates/shop-core/tests/checkout.rs")));
        let features: Vec<(&str, bool)> = core
            .features
            .iter()
            .map(|f| (f.name.as_str(), f.default))
            .collect();
        assert_eq!(features, [("discounts", false), ("json", true)]);
        let cli = ws.package("shop-cli").unwrap();
        assert_eq!(cli.target("bin:shop").unwrap().crate_name(), "shop");
        assert!(cli.target("example:report").is_some());
        assert_eq!(cli.dependencies[0].name, "anyhow");
    }

    #[test]
    fn dependency_trees_mark_repeated_packages() {
        let ws = shop();
        let cli = &ws.trees[1];
        assert_eq!(
            (cli.name.as_str(), cli.source.as_str()),
            ("shop-cli", "path")
        );
        let names: Vec<&str> = cli.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["anyhow", "shop-core"]);
        let core = &cli.children[1];
        let serde = core.children.iter().find(|c| c.name == "serde").unwrap();
        assert_eq!(serde.source, "registry");
        assert!(serde.features.contains(&"derive".to_string()));
        fn count(n: &DependencyNode, name: &str) -> (usize, usize) {
            let own = (n.name == name) as usize;
            let rep = (n.name == name && n.repeated) as usize;
            n.children.iter().fold((own, rep), |(a, b), c| {
                let (x, y) = count(c, name);
                (a + x, b + y)
            })
        }
        let (all, repeated) = count(cli, "proc-macro2");
        assert_eq!(all - repeated, 1, "expanded once");
    }

    #[test]
    fn forge_itself_without_dependencies() {
        let ws = parse_metadata(
            include_str!("../tests/fixtures/metadata-forge-nodeps.json"),
            "/home/user/PHPForge",
        )
        .unwrap();
        assert!(ws.members.len() > 50, "{}", ws.members.len());
        let app = ws.package("forge-app").unwrap();
        assert_eq!(app.dir, "app/src-tauri");
        assert!(app
            .targets
            .iter()
            .any(|t| t.kind == TargetKind::BuildScript));
        let output = ws.package("forge-output").unwrap();
        assert_eq!(
            output.lib().unwrap().src_path,
            "crates/forge-output/src/lib.rs"
        );
        // Without `resolve` the trees list the direct dependencies.
        let i = ws
            .members
            .iter()
            .position(|m| m.name == "forge-output")
            .unwrap();
        assert!(ws.trees[i].children.iter().any(|c| c.name == "serde"));
        assert_eq!(
            crate_of(&ws, "crates/forge-output/src/formats/libtest.rs").map(|(p, t, m)| (
                p.name.as_str(),
                t.key(),
                m
            )),
            Some(("forge-output", "lib".into(), "formats::libtest".into()))
        );
    }

    #[test]
    fn module_paths_of_files() {
        use TargetKind::*;
        assert_eq!(
            module_path("src/lib.rs", "src/lib.rs", Lib).as_deref(),
            Some("")
        );
        assert_eq!(
            module_path("src/lib.rs", "src/cart.rs", Lib).as_deref(),
            Some("cart")
        );
        assert_eq!(
            module_path("src/lib.rs", "src/cart/mod.rs", Lib).as_deref(),
            Some("cart")
        );
        assert_eq!(
            module_path("src/lib.rs", "src/cart/tests.rs", Lib).as_deref(),
            Some("cart::tests")
        );
        assert_eq!(module_path("src/lib.rs", "src/main.rs", Lib), None);
        assert_eq!(module_path("src/lib.rs", "src/bin/tool.rs", Lib), None);
        assert_eq!(module_path("src/lib.rs", "tests/a.rs", Lib), None);
        assert_eq!(
            module_path("src/main.rs", "src/cli.rs", Bin).as_deref(),
            Some("cli")
        );
        assert_eq!(
            module_path("src/bin/tool.rs", "src/bin/tool/args.rs", Bin).as_deref(),
            Some("args")
        );
        assert_eq!(module_path("src/bin/tool.rs", "src/cli.rs", Bin), None);
        assert_eq!(
            module_path("tests/checkout.rs", "tests/common.rs", Test),
            None
        );
        assert_eq!(
            module_path("tests/checkout.rs", "tests/checkout/fixtures.rs", Test).as_deref(),
            Some("fixtures")
        );
        let ws = shop();
        let (p, t, m) = crate_of(&ws, "crates/shop-core/src/cart.rs").unwrap();
        assert_eq!(
            (p.name.as_str(), t.key(), m.as_str()),
            ("shop-core", "lib".into(), "cart")
        );
        let (_, t, m) = crate_of(&ws, "crates/shop-cli/src/main.rs").unwrap();
        assert_eq!((t.key(), m.as_str()), ("bin:shop".into(), ""));
        let (_, t, _) = crate_of(&ws, "crates/shop-core/tests/checkout.rs").unwrap();
        assert_eq!(t.key(), "test:checkout");
    }
}

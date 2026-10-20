//! Workspace-wide PHP index: parallel indexing, mtime-keyed persistent cache, fuzzy symbol
//! search, ancestor lookup and lazy resolution of vendor classes through Composer's PSR-4
//! metadata.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, UNIX_EPOCH};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::fuzzy::fuzzy_match;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind};
use crate::parser;
use crate::structure::{self, FileStructure};
use crate::{PhpError, Result};

/// How `vendor/` is treated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VendorMode {
    /// Never look into vendor.
    Skip,
    /// Don't index vendor up front; load single classes on demand via PSR-4 (default).
    #[default]
    Lazy,
    /// Index vendor together with the project.
    Eager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexConfig {
    /// Directory names skipped anywhere in the tree.
    pub exclude_dirs: Vec<String>,
    pub vendor: VendorMode,
    /// Files larger than this are skipped (bytes).
    pub max_file_size: u64,
    pub extensions: Vec<String>,
    /// Cache file; `None` uses a per-project file in the user's cache directory, so the
    /// index never ends up inside the project (like PHPStorm's system caches).
    pub cache_file: Option<PathBuf>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            exclude_dirs: [".git", ".forge", ".idea", "node_modules", ".svn", ".hg"]
                .into_iter()
                .map(String::from)
                .collect(),
            vendor: VendorMode::Lazy,
            max_file_size: 2 * 1024 * 1024,
            extensions: vec!["php".into(), "phtml".into(), "inc".into()],
            cache_file: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub files: usize,
    pub parsed: usize,
    pub cached: usize,
    pub classes: usize,
    pub functions: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    /// Classes, interfaces, traits and enums (Go to Class, Ctrl+N).
    #[default]
    Class,
    /// Every symbol: class-likes, members, functions, constants (Go to Symbol).
    Symbol,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolHit {
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    /// Owning class FQN for members, namespace for class-likes and functions.
    pub container: String,
    pub path: String,
    pub offset: usize,
    pub line: u32,
    pub column: u32,
    pub score: i32,
    /// Matched char positions in `name`.
    pub positions: Vec<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vendor: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    mtime_ms: u64,
    size: u64,
    index: FileIndex,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    files: HashMap<String, Entry>,
}

const CACHE_VERSION: u32 = 1;

pub struct ProjectIndex {
    root: PathBuf,
    config: IndexConfig,
    files: HashMap<String, Entry>,
    /// lowercase FQN -> relative path.
    class_map: HashMap<String, String>,
    psr4: Vec<(String, PathBuf)>,
    /// lowercase FQNs already looked up in vendor (hit or miss).
    vendor_tried: HashSet<String>,
}

fn to_rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl ProjectIndex {
    pub fn new(root: impl Into<PathBuf>, config: IndexConfig) -> Self {
        Self {
            root: root.into(),
            config,
            files: HashMap::new(),
            class_map: HashMap::new(),
            psr4: Vec::new(),
            vendor_tried: HashSet::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &IndexConfig {
        &self.config
    }

    pub fn cache_path(&self) -> PathBuf {
        self.config
            .cache_file
            .clone()
            .unwrap_or_else(|| default_cache_path(&self.root))
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn file(&self, rel: &str) -> Option<&FileIndex> {
        self.files.get(rel).map(|e| &e.index)
    }

    pub fn files(&self) -> impl Iterator<Item = &FileIndex> {
        self.files.values().map(|e| &e.index)
    }

    pub fn classes(&self) -> impl Iterator<Item = &ClassLike> {
        self.files().flat_map(|f| f.classes.iter())
    }

    fn is_vendor(rel: &str) -> bool {
        rel.starts_with("vendor/") || rel.contains("/vendor/")
    }

    /// Load a previously saved cache. Returns false if there is none or it is stale/corrupt.
    pub fn load_cache(&mut self) -> bool {
        let Ok(bytes) = std::fs::read(self.cache_path()) else {
            return false;
        };
        match serde_json::from_slice::<CacheFile>(&bytes) {
            Ok(c) if c.version == CACHE_VERSION => {
                self.files = c.files;
                self.rebuild_maps();
                true
            }
            _ => false,
        }
    }

    pub fn save_cache(&self) -> Result<()> {
        let path = self.cache_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| PhpError::Io {
                path: dir.display().to_string(),
                source: e,
            })?;
        }
        let cache = CacheFile {
            version: CACHE_VERSION,
            files: self.files.clone(),
        };
        let json = serde_json::to_vec(&cache).map_err(|e| PhpError::Cache(e.to_string()))?;
        std::fs::write(&path, json).map_err(|e| PhpError::Io {
            path: path.display().to_string(),
            source: e,
        })
    }

    fn rebuild_maps(&mut self) {
        self.class_map.clear();
        for (rel, e) in &self.files {
            for c in &e.index.classes {
                let key = c.fqn.to_ascii_lowercase();
                // Project classes win over vendor duplicates.
                let replace = match self.class_map.get(&key) {
                    Some(existing) => Self::is_vendor(existing) && !Self::is_vendor(rel),
                    None => true,
                };
                if replace {
                    self.class_map.insert(key, rel.clone());
                }
            }
        }
    }

    /// Collect indexable files (relative path, size, mtime).
    fn scan(&self) -> Vec<(String, u64, u64)> {
        let exclude: HashSet<&str> = self
            .config
            .exclude_dirs
            .iter()
            .map(String::as_str)
            .collect();
        let vendor = self.config.vendor;
        let root = self.root.clone();
        walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 || !e.file_type().is_dir() {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                if exclude.contains(name.as_ref()) {
                    return false;
                }
                !(name == "vendor" && e.depth() == 1 && vendor != VendorMode::Eager)
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| self.config.extensions.iter().any(|ext| ext == x))
            })
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                (meta.len() <= self.config.max_file_size)
                    .then(|| (to_rel(&root, e.path()), meta.len(), mtime_ms(&meta)))
            })
            .collect()
    }

    /// (Re)index the workspace. Unchanged files (same mtime and size) are reused from the
    /// in-memory/cached state. `progress(done, total)` is called from worker threads.
    pub fn index_workspace(&mut self, progress: impl Fn(usize, usize) + Sync) -> IndexStats {
        let started = Instant::now();
        let found = self.scan();
        let total = found.len();
        let mut keep: HashMap<String, Entry> = HashMap::with_capacity(total);
        let mut todo: Vec<(String, u64, u64)> = Vec::new();
        for (rel, size, mtime) in found {
            match self.files.remove(&rel) {
                Some(e) if e.mtime_ms == mtime && e.size == size => {
                    keep.insert(rel, e);
                }
                _ => todo.push((rel, size, mtime)),
            }
        }
        let cached = keep.len();
        let done = AtomicUsize::new(cached);
        progress(cached, total);
        let root = self.root.clone();
        let parsed: Vec<(String, Entry)> = todo
            .par_iter()
            .map_init(parser::new_parser, |p, (rel, size, mtime)| {
                let src = std::fs::read(root.join(rel))
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default();
                let index = parser::index_with(p, rel, &src);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(25) || n == total {
                    progress(n, total);
                }
                (
                    rel.clone(),
                    Entry {
                        mtime_ms: *mtime,
                        size: *size,
                        index,
                    },
                )
            })
            .collect();
        let parsed_count = parsed.len();
        // Lazily loaded vendor files stay around unless vendor was rescanned.
        if self.config.vendor != VendorMode::Eager {
            for (rel, e) in self.files.drain() {
                if Self::is_vendor(&rel) {
                    keep.insert(rel, e);
                }
            }
        }
        keep.extend(parsed);
        self.files = keep;
        self.rebuild_maps();
        self.psr4 = load_psr4(&self.root);
        self.vendor_tried.clear();
        progress(total, total);
        IndexStats {
            files: total,
            parsed: parsed_count,
            cached,
            classes: self.files.values().map(|e| e.index.classes.len()).sum(),
            functions: self.files.values().map(|e| e.index.functions.len()).sum(),
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }

    /// Re-index a single file from (possibly unsaved) content.
    pub fn update_file(&mut self, rel: &str, source: &str) -> &FileIndex {
        let index = parser::index_source(rel, source);
        let (mtime, size) = std::fs::metadata(self.root.join(rel))
            .map(|m| (mtime_ms(&m), m.len()))
            .unwrap_or((0, 0));
        // Remove old class entries for this file.
        self.class_map.retain(|_, p| p != rel);
        for c in &index.classes {
            self.class_map
                .entry(c.fqn.to_ascii_lowercase())
                .or_insert_with(|| rel.to_string());
        }
        self.files.insert(
            rel.to_string(),
            Entry {
                mtime_ms: mtime,
                size,
                index,
            },
        );
        &self.files[rel].index
    }

    pub fn remove_file(&mut self, rel: &str) {
        if self.files.remove(rel).is_some() {
            self.rebuild_maps();
        }
    }

    /// Class by FQN from what is already indexed.
    pub fn find_class(&self, fqn: &str) -> Option<&ClassLike> {
        let fqn = fqn.trim_start_matches('\\');
        let rel = self.class_map.get(&fqn.to_ascii_lowercase())?;
        self.files
            .get(rel)?
            .index
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
    }

    /// Class by FQN, loading it from vendor on demand (PSR-4) when not indexed yet.
    pub fn resolve_class(&mut self, fqn: &str) -> Option<ClassLike> {
        let fqn = fqn.trim_start_matches('\\');
        if let Some(c) = self.find_class(fqn) {
            return Some(c.clone());
        }
        if self.config.vendor == VendorMode::Skip {
            return None;
        }
        let key = fqn.to_ascii_lowercase();
        if !self.vendor_tried.insert(key) {
            return None;
        }
        if self.psr4.is_empty() {
            self.psr4 = load_psr4(&self.root);
        }
        let path = psr4_candidates(&self.psr4, fqn)
            .into_iter()
            .find(|p| p.is_file())?;
        let source = std::fs::read_to_string(&path).ok()?;
        let rel = to_rel(&self.root, &path);
        self.update_file(&rel, &source);
        self.find_class(fqn).cloned()
    }

    /// All ancestors (parents, interfaces, traits; transitively), nearest first.
    pub fn ancestors(&mut self, class: &ClassLike) -> Vec<ClassLike> {
        let mut out: Vec<ClassLike> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(class.fqn.to_ascii_lowercase());
        let mut queue: VecDeque<String> = VecDeque::new();
        let push_refs = |c: &ClassLike, q: &mut VecDeque<String>| {
            for n in c.traits.iter().chain(&c.extends).chain(&c.implements) {
                q.push_back(n.clone());
            }
        };
        push_refs(class, &mut queue);
        while let Some(fqn) = queue.pop_front() {
            if !seen.insert(fqn.to_ascii_lowercase()) || out.len() > 64 {
                continue;
            }
            if let Some(c) = self.resolve_class(&fqn) {
                push_refs(&c, &mut queue);
                out.push(c);
            }
        }
        out
    }

    /// Members visible on `class` that come from ancestors, as (owner FQN, member). Members
    /// overridden in `class` (or a nearer ancestor) are skipped.
    pub fn inherited_members(&mut self, class: &ClassLike) -> Vec<(String, Member)> {
        let mut seen: HashSet<(SymbolKind, String)> = class
            .members
            .iter()
            .map(|m| (m.kind, m.name.to_ascii_lowercase()))
            .collect();
        let mut out = Vec::new();
        for anc in self.ancestors(class) {
            for m in &anc.members {
                if seen.insert((m.kind, m.name.to_ascii_lowercase())) {
                    out.push((anc.fqn.clone(), m.clone()));
                }
            }
        }
        out
    }

    /// Path of the file declaring `fqn`, if indexed.
    pub fn class_path(&self, fqn: &str) -> Option<&str> {
        self.class_map
            .get(&fqn.trim_start_matches('\\').to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Structure of `file` for the Structure tool window, optionally with inherited members.
    pub fn file_structure(&mut self, file: &FileIndex, include_inherited: bool) -> FileStructure {
        structure::build(file, |c| {
            if !include_inherited {
                return Vec::new();
            }
            self.inherited_members(c)
                .into_iter()
                .map(|(owner, m)| {
                    let path = self.class_path(&owner).unwrap_or_default().to_string();
                    (owner, path, m)
                })
                .collect()
        })
    }

    /// Fuzzy search (Go to Class / Go to Symbol).
    pub fn search(&self, query: &str, kind: SearchKind, limit: usize) -> Vec<SymbolHit> {
        let query = query.trim();
        let (class_q, member_q) = match kind {
            SearchKind::Symbol => match query.split_once("::").or_else(|| query.split_once('.')) {
                Some((c, m)) => (Some(c.trim()), m.trim()),
                None => (None, query),
            },
            SearchKind::Class => (None, query),
        };
        let mut hits: Vec<SymbolHit> = Vec::new();
        for (rel, e) in &self.files {
            let vendor = Self::is_vendor(rel);
            let penalty = if vendor { 30 } else { 0 };
            for c in &e.index.classes {
                let deprecated = c.doc.as_ref().is_some_and(|d| d.is_deprecated());
                if class_q.is_none() {
                    let target: &str = if member_q.contains('\\') {
                        &c.fqn
                    } else {
                        &c.name
                    };
                    if let Some(m) = fuzzy_match(member_q, target) {
                        let positions = if member_q.contains('\\') {
                            let off = c.fqn.chars().count() - c.name.chars().count();
                            m.positions
                                .iter()
                                .filter(|p| **p >= off)
                                .map(|p| p - off)
                                .collect()
                        } else {
                            m.positions
                        };
                        hits.push(SymbolHit {
                            name: c.name.clone(),
                            fqn: c.fqn.clone(),
                            kind: c.kind,
                            container: c.namespace.clone(),
                            path: rel.clone(),
                            offset: c.name_range.start,
                            line: c.name_range.line,
                            column: c.name_range.column,
                            score: m.score - penalty,
                            positions,
                            vendor,
                            deprecated,
                        });
                    }
                }
                if kind != SearchKind::Symbol {
                    continue;
                }
                let class_score = match class_q {
                    Some(q) => match fuzzy_match(q, &c.name) {
                        Some(m) => m.score,
                        None => continue,
                    },
                    None => 0,
                };
                for mem in &c.members {
                    if let Some(m) = fuzzy_match(member_q, &mem.name) {
                        hits.push(SymbolHit {
                            name: mem.name.clone(),
                            fqn: format!("{}::{}", c.fqn, mem.name),
                            kind: mem.kind,
                            container: c.fqn.clone(),
                            path: rel.clone(),
                            offset: mem.name_range.start,
                            line: mem.name_range.line,
                            column: mem.name_range.column,
                            score: m.score + class_score / 2 - penalty - 2,
                            positions: m.positions,
                            vendor,
                            deprecated: mem.doc.as_ref().is_some_and(|d| d.is_deprecated()),
                        });
                    }
                }
            }
            if kind == SearchKind::Symbol && class_q.is_none() {
                for f in &e.index.functions {
                    if let Some(m) = fuzzy_match(member_q, &f.name) {
                        hits.push(SymbolHit {
                            name: f.name.clone(),
                            fqn: f.fqn.clone(),
                            kind: SymbolKind::Function,
                            container: f.namespace.clone(),
                            path: rel.clone(),
                            offset: f.name_range.start,
                            line: f.name_range.line,
                            column: f.name_range.column,
                            score: m.score - penalty,
                            positions: m.positions,
                            vendor,
                            deprecated: f.doc.as_ref().is_some_and(|d| d.is_deprecated()),
                        });
                    }
                }
                for k in &e.index.constants {
                    if let Some(m) = fuzzy_match(member_q, &k.name) {
                        hits.push(SymbolHit {
                            name: k.name.clone(),
                            fqn: k.fqn.clone(),
                            kind: SymbolKind::Constant,
                            container: k
                                .fqn
                                .rsplit_once('\\')
                                .map(|(n, _)| n.to_string())
                                .unwrap_or_default(),
                            path: rel.clone(),
                            offset: k.range.start,
                            line: k.range.line,
                            column: k.range.column,
                            score: m.score - penalty,
                            positions: m.positions,
                            vendor,
                            deprecated: false,
                        });
                    }
                }
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.name.len().cmp(&b.name.len()))
                .then_with(|| a.fqn.cmp(&b.fqn))
        });
        hits.truncate(limit);
        hits
    }
}

/// PSR-4 prefixes from the project's composer.json and vendor/composer/installed.json,
/// longest prefix first.
fn load_psr4(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut add = |autoload: &serde_json::Value, base: &Path| {
        for key in ["psr-4", "psr-0"] {
            if let Some(map) = autoload.get(key).and_then(|v| v.as_object()) {
                for (prefix, dirs) in map {
                    let dirs: Vec<&str> = match dirs {
                        serde_json::Value::String(s) => vec![s.as_str()],
                        serde_json::Value::Array(a) => {
                            a.iter().filter_map(|v| v.as_str()).collect()
                        }
                        _ => continue,
                    };
                    for d in dirs {
                        let mut dir = base.join(d);
                        if key == "psr-0" {
                            dir = dir.join(prefix.trim_end_matches('\\').replace('\\', "/"));
                        }
                        out.push((prefix.trim_start_matches('\\').to_string(), dir));
                    }
                }
            }
        }
    };
    if let Ok(s) = std::fs::read_to_string(root.join("composer.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(a) = v.get("autoload") {
                add(a, root);
            }
            if let Some(a) = v.get("autoload-dev") {
                add(a, root);
            }
        }
    }
    let composer_dir = root.join("vendor/composer");
    if let Ok(s) = std::fs::read_to_string(composer_dir.join("installed.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            let packages = v.get("packages").cloned().unwrap_or(v);
            for p in packages.as_array().into_iter().flatten() {
                let base = match p.get("install-path").and_then(|v| v.as_str()) {
                    Some(ip) => composer_dir.join(ip),
                    None => match p.get("name").and_then(|v| v.as_str()) {
                        Some(name) => root.join("vendor").join(name),
                        None => continue,
                    },
                };
                if let Some(a) = p.get("autoload") {
                    add(a, &base);
                }
            }
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.0.len()));
    out
}

fn psr4_candidates(psr4: &[(String, PathBuf)], fqn: &str) -> Vec<PathBuf> {
    psr4.iter()
        .filter(|(prefix, _)| {
            fqn.len() >= prefix.len() && fqn[..prefix.len()].eq_ignore_ascii_case(prefix)
        })
        .map(|(prefix, dir)| {
            let rest = &fqn[prefix.len()..];
            dir.join(format!(
                "{}.php",
                rest.trim_start_matches('\\').replace('\\', "/")
            ))
        })
        .collect()
}

/// Short display name for a FQN (re-exported for consumers).
pub fn display_name(fqn: &str) -> &str {
    short_name(fqn)
}

/// `<cache dir>/forge/php-index/<project name>-<path hash>.json`, falling back to the
/// project's `.forge/cache` when the platform has no cache directory.
fn default_cache_path(root: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let Some(cache) = dirs::cache_dir() else {
        return root.join(".forge/cache/php-index.json");
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    cache
        .join("forge")
        .join("php-index")
        .join(format!("{name}-{:016x}.json", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            r,
            "composer.json",
            r#"{"autoload": {"psr-4": {"App\\": "app/"}}}"#,
        );
        write(
            r,
            "app/Http/Controllers/OrderController.php",
            "<?php\nnamespace App\\Http\\Controllers;\nuse Illuminate\\Routing\\Controller as Base;\nclass OrderController extends Base {\n public function show() {}\n public function index() {}\n}\n",
        );
        write(
            r,
            "app/Models/Order.php",
            "<?php\nnamespace App\\Models;\n/** @deprecated */\nclass Order extends Model implements \\JsonSerializable { public $total; const PAID = 'paid'; public function jsonSerialize(): mixed {} }\nfunction order_helper() {}\n",
        );
        write(
            r,
            "app/Models/Model.php",
            "<?php\nnamespace App\\Models;\nabstract class Model { use HasAttributes; public function save(): bool {} abstract public function table(): string; }\n",
        );
        write(
            r,
            "app/Models/HasAttributes.php",
            "<?php\nnamespace App\\Models;\ntrait HasAttributes { protected array $attributes = []; public function getAttribute($k) {} }\n",
        );
        write(
            r,
            "node_modules/x/evil.php",
            "<?php class ShouldNotIndex {}",
        );
        write(
            r,
            "vendor/composer/installed.json",
            r#"{"packages": [{"name": "laravel/framework", "install-path": "../laravel/framework", "autoload": {"psr-4": {"Illuminate\\": "src/Illuminate/"}}}]}"#,
        );
        write(
            r,
            "vendor/laravel/framework/src/Illuminate/Routing/Controller.php",
            "<?php\nnamespace Illuminate\\Routing;\nabstract class Controller { public function middleware($m) {} public function callAction($method, $parameters) {} }\n",
        );
        dir
    }

    #[test]
    fn indexes_project_and_skips_excluded_and_vendor() {
        let dir = project();
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        let calls = AtomicUsize::new(0);
        let stats = idx.index_workspace(|_, _| {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(stats.files, 4);
        assert_eq!(stats.parsed, 4);
        assert!(calls.load(Ordering::Relaxed) >= 2);
        assert!(idx.find_class("App\\Models\\Order").is_some());
        assert!(idx.find_class("\\app\\models\\order").is_some());
        assert!(idx.find_class("ShouldNotIndex").is_none());
        assert!(idx.find_class("Illuminate\\Routing\\Controller").is_none());
    }

    #[test]
    fn lazily_resolves_vendor_classes_and_ancestors() {
        let dir = project();
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        idx.index_workspace(|_, _| {});
        let ctrl = idx
            .find_class("App\\Http\\Controllers\\OrderController")
            .unwrap()
            .clone();
        let anc = idx.ancestors(&ctrl);
        assert_eq!(anc.len(), 1);
        assert_eq!(anc[0].fqn, "Illuminate\\Routing\\Controller");
        // Now part of the index.
        assert!(idx.find_class("Illuminate\\Routing\\Controller").is_some());

        let order = idx.find_class("App\\Models\\Order").unwrap().clone();
        let names: Vec<String> = idx.ancestors(&order).into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["Model", "HasAttributes"]);
        let inherited: Vec<String> = idx
            .inherited_members(&order)
            .into_iter()
            .map(|(_, m)| m.name)
            .collect();
        assert_eq!(
            inherited,
            vec!["save", "table", "attributes", "getAttribute"]
        );
    }

    #[test]
    fn skip_mode_never_touches_vendor() {
        let dir = project();
        let cfg = IndexConfig {
            vendor: VendorMode::Skip,
            ..Default::default()
        };
        let mut idx = ProjectIndex::new(dir.path(), cfg);
        idx.index_workspace(|_, _| {});
        assert!(idx
            .resolve_class("Illuminate\\Routing\\Controller")
            .is_none());
    }

    #[test]
    fn eager_mode_indexes_vendor() {
        let dir = project();
        let cfg = IndexConfig {
            vendor: VendorMode::Eager,
            ..Default::default()
        };
        let mut idx = ProjectIndex::new(dir.path(), cfg);
        let stats = idx.index_workspace(|_, _| {});
        assert_eq!(stats.files, 5);
        assert!(idx.find_class("Illuminate\\Routing\\Controller").is_some());
    }

    #[test]
    fn cache_round_trip_reuses_unchanged_files() {
        let dir = project();
        let cache = tempfile::tempdir().unwrap();
        let config = || IndexConfig {
            cache_file: Some(cache.path().join("php-index.json")),
            ..IndexConfig::default()
        };
        let mut idx = ProjectIndex::new(dir.path(), config());
        idx.index_workspace(|_, _| {});
        idx.save_cache().unwrap();
        assert!(cache.path().join("php-index.json").is_file());

        let mut again = ProjectIndex::new(dir.path(), config());
        assert!(again.load_cache());
        assert!(again.find_class("App\\Models\\Order").is_some());
        let stats = again.index_workspace(|_, _| {});
        assert_eq!(stats.cached, 4);
        assert_eq!(stats.parsed, 0);

        // Changing a file's size invalidates its entry.
        write(
            dir.path(),
            "app/Models/Model.php",
            "<?php\nnamespace App\\Models;\nabstract class Model { public function save(): bool {} public function fresh() {} }\n",
        );
        let stats = again.index_workspace(|_, _| {});
        assert_eq!(stats.parsed, 1);
        assert!(again
            .find_class("App\\Models\\Model")
            .unwrap()
            .method("fresh")
            .is_some());
    }

    #[test]
    fn corrupt_cache_is_ignored() {
        let dir = project();
        write(dir.path(), ".forge/cache/php-index.json", "{nope");
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        assert!(!idx.load_cache());
    }

    #[test]
    fn searches_classes_and_symbols() {
        let dir = project();
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        idx.index_workspace(|_, _| {});

        let hits = idx.search("OC", SearchKind::Class, 10);
        assert_eq!(hits[0].name, "OrderController");
        assert_eq!(hits[0].positions, vec![0, 5]);
        assert_eq!(hits[0].container, "App\\Http\\Controllers");

        let hits = idx.search("order", SearchKind::Class, 10);
        assert_eq!(hits[0].name, "Order");
        assert!(hits[0].deprecated);

        let hits = idx.search("models\\mod", SearchKind::Class, 10);
        assert_eq!(hits[0].fqn, "App\\Models\\Model");

        let hits = idx.search("save", SearchKind::Symbol, 10);
        assert_eq!(hits[0].fqn, "App\\Models\\Model::save");
        assert_eq!(hits[0].kind, SymbolKind::Method);

        let hits = idx.search("OrdCon::sh", SearchKind::Symbol, 10);
        assert_eq!(hits[0].name, "show");

        let hits = idx.search("order_help", SearchKind::Symbol, 10);
        assert_eq!(hits[0].kind, SymbolKind::Function);

        assert!(idx
            .search("PAID", SearchKind::Symbol, 10)
            .iter()
            .any(|h| h.kind == SymbolKind::Constant));
        assert!(idx.search("save", SearchKind::Class, 10).is_empty());
    }

    #[test]
    fn update_file_replaces_symbols() {
        let dir = project();
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        idx.index_workspace(|_, _| {});
        idx.update_file(
            "app/Models/Order.php",
            "<?php\nnamespace App\\Models;\nclass Invoice {}\n",
        );
        assert!(idx.find_class("App\\Models\\Order").is_none());
        assert!(idx.find_class("App\\Models\\Invoice").is_some());
        idx.remove_file("app/Models/Order.php");
        assert!(idx.find_class("App\\Models\\Invoice").is_none());
    }
}

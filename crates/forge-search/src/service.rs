use std::collections::HashSet;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use forge_core::search::{FileMatch, FindRequest, FindResult};
use forge_core::{ForgeError, FsChanges, Result};
use forge_find::Finder;
use forge_search_core::{FileHit, Index, Outcome, Query, RefreshStats, Scope, SymbolHit};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use thread_priority::{set_current_thread_priority, ThreadPriority};

use rayon::prelude::*;

use crate::index::{self, EXCLUDED_DIRS};
use crate::lazy::{group_rows, FoundText, LazyFile, Rows};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(300);
const SAVE_INTERVAL: Duration = Duration::from_secs(60);

/// How the index is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOptions {
    /// Run the indexing threads below normal CPU priority, so the rest of the machine (and the IDE)
    /// keeps its share while a big project is indexed.
    pub low_priority: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self { low_priority: true }
    }
}

/// One project's search index. Share it behind an `Arc`.
pub struct SearchService {
    /// The threads that build the index; searches use the global pool.
    pool: rayon::ThreadPool,
    root: PathBuf,
    cache: Option<PathBuf>,
    index: RwLock<Index>,
    generation: AtomicU64,
    /// Serialises refreshes; searches never take it.
    gate: Mutex<()>,
    indexing: AtomicBool,
    /// True once the index holds a complete listing (cache loaded or first refresh done).
    ready: AtomicBool,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl SearchService {
    /// `cache` is where the index is persisted between runs; a missing or stale file is ignored.
    pub fn open(root: impl Into<PathBuf>, cache: Option<PathBuf>) -> Self {
        Self::open_with(
            root,
            cache,
            IndexOptions {
                low_priority: false,
            },
        )
    }

    /// [`SearchService::open`] with options for the indexing threads.
    pub fn open_with(
        root: impl Into<PathBuf>,
        cache: Option<PathBuf>,
        options: IndexOptions,
    ) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("forge-index-{i}"))
            .start_handler(move |_| {
                if options.low_priority {
                    let _ = set_current_thread_priority(ThreadPriority::Min);
                }
            })
            .build()
            .expect("cannot start the indexing threads");
        let index = cache
            .as_deref()
            .and_then(|p| fs::File::open(p).ok())
            .and_then(|f| Index::read_from(BufReader::new(f)))
            .unwrap_or_default();
        let ready = !index.is_empty();
        Self {
            pool,
            root: root.into(),
            cache,
            index: RwLock::new(index),
            generation: AtomicU64::new(0),
            gate: Mutex::new(()),
            indexing: AtomicBool::new(false),
            ready: AtomicBool::new(ready),
            watcher: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &rayon::ThreadPool {
        &self.pool
    }

    /// Where the index of the project at `root` is cached between runs.
    pub fn cache_path(root: &Path) -> Option<PathBuf> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        root.hash(&mut hasher);
        dirs::cache_dir().map(|d| {
            d.join("Forge")
                .join("search")
                .join(format!("{:016x}.bin", hasher.finish()))
        })
    }

    /// Whether the index covers the project, so it can answer instead of a directory walk.
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    pub fn file_count(&self) -> usize {
        self.index
            .read()
            .expect("search index lock poisoned")
            .file_count()
    }

    /// Apply a debounced file watcher batch (workspace-relative paths).
    pub fn apply_changes(&self, changes: &FsChanges) {
        let paths: Vec<PathBuf> = changes
            .changed
            .iter()
            .chain(&changes.removed)
            .map(|p| self.root.join(p))
            .collect();
        self.update_paths(&paths);
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Bring the whole index in line with the disk and persist it. Searches keep running meanwhile.
    pub fn refresh(&self) -> RefreshStats {
        let _gate = self.gate.lock().expect("search gate poisoned");
        self.indexing.store(true, Ordering::Relaxed);
        let stats = self.refresh_scope(&Scope::full());
        let missing = self.cache.as_deref().is_some_and(|p| !p.exists());
        if stats != RefreshStats::default() || missing {
            self.save();
        }
        self.ready.store(true, Ordering::Relaxed);
        self.indexing.store(false, Ordering::Relaxed);
        stats
    }

    fn refresh_scope(&self, scope: &Scope) -> RefreshStats {
        self.pool.install(|| self.refresh_scope_on_pool(scope))
    }

    fn refresh_scope_on_pool(&self, scope: &Scope) -> RefreshStats {
        let disk = index::scan(&self.root, scope);
        let plan = self
            .index
            .read()
            .expect("search index lock poisoned")
            .plan(disk, scope);
        if plan.rebuild {
            let mut fresh = Index::new();
            index::load_chunks(&self.root, &self.pool, plan.stale, |batch| {
                fresh.add_batch(batch)
            });
            *self.index.write().expect("search index lock poisoned") = fresh;
        } else {
            self.index
                .write()
                .expect("search index lock poisoned")
                .remove(&plan.gone);
            index::load_chunks(&self.root, &self.pool, plan.stale, |batch| {
                self.index
                    .write()
                    .expect("search index lock poisoned")
                    .add_batch(batch);
            });
        }
        plan.stats
    }

    /// Re-index just what changed under `paths` (absolute), as reported by a file watcher.
    pub fn update_paths(&self, paths: &[PathBuf]) {
        let mut scopes: HashSet<Scope> = HashSet::new();
        for path in paths {
            let Ok(rel) = path.strip_prefix(&self.root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let excluded = rel.split('/').any(|c| EXCLUDED_DIRS.contains(&c));
            if rel.is_empty() || excluded {
                continue;
            }
            if path.is_dir()
                || self
                    .index
                    .read()
                    .expect("search index lock poisoned")
                    .has_entries_under(&rel)
            {
                scopes.insert(Scope {
                    dir: rel.clone(),
                    recursive: true,
                });
            }
            let parent = rel.rsplit_once('/').map_or("", |(p, _)| p).to_string();
            scopes.insert(Scope {
                dir: parent,
                recursive: false,
            });
        }
        if scopes.is_empty() {
            return;
        }
        let _gate = self.gate.lock().expect("search gate poisoned");
        for scope in scopes {
            self.refresh_scope(&scope);
        }
    }

    /// Persist the index. A failed write is logged and leaves the previous cache untouched.
    pub fn save(&self) {
        let Some(path) = &self.cache else { return };
        let tmp = path.with_extension("tmp");
        let index = self.index.read().expect("search index lock poisoned");
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::File::create(&tmp))
            .map(|f| {
                let mut w = BufWriter::new(f);
                index.write_to(&mut w) && std::io::Write::flush(&mut w).is_ok()
            })
            .and_then(|ok| {
                if ok {
                    fs::rename(&tmp, path)
                } else {
                    Err(std::io::Error::other("serialize failed"))
                }
            });
        if let Err(e) = written {
            tracing::warn!("could not save search index to {}: {e}", path.display());
            let _ = fs::remove_file(&tmp);
        }
    }

    /// Keep the index current from file-system events. Holds only a weak reference to the service.
    pub fn watch(self: &Arc<Self>) -> notify::Result<()> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(&self.root, RecursiveMode::Recursive)?;
        *self.watcher.lock().expect("watcher lock poisoned") = Some(watcher);

        let service = Arc::downgrade(self);
        std::thread::spawn(move || watch_loop(rx, service));
        Ok(())
    }

    /// Find in Files. A newer call cancels a search still running on an older one.
    pub fn find_in_files(&self, query: &Query) -> Result<Outcome> {
        let compiled = query
            .compile()
            .map_err(|e| ForgeError::Other(e.to_string()))?;
        let mine = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let index = self.index.read().expect("search index lock poisoned");
        let mut outcome = forge_search_core::run(
            &index,
            &compiled,
            |path| std::fs::read(self.root.join(path)).ok(),
            || self.generation.load(Ordering::Relaxed) != mine,
        );
        outcome.indexing = self.indexing.load(Ordering::Relaxed);
        Ok(outcome)
    }

    /// Find in Files with `forge-find` semantics: the trigram index narrows the files, `forge-find`
    /// reads the lines, so results match a directory walk exactly.
    pub fn find_text(&self, req: &FindRequest) -> Result<FindResult> {
        let found = self.find_text_lazy(req)?;
        Ok(FindResult {
            files: found.page(0, usize::MAX),
            match_count: found.match_count,
            truncated: found.truncated,
            error: found.error.clone(),
        })
    }

    /// [`SearchService::find_text`] without building the rows: the result keeps match ranges and
    /// reads the text of the rows a page asks for.
    pub fn find_text_lazy(&self, req: &FindRequest) -> Result<FoundText> {
        let finder = match Finder::new(req) {
            Ok(f) => f,
            Err(e) => return Ok(FoundText::failed(self.root.clone(), e)),
        };
        let literals = Query {
            pattern: req.query.clone(),
            regex: req.regex,
            case_sensitive: req.case_sensitive,
            whole_words: req.whole_words,
            ..Query::default()
        }
        .literals();
        let mut paths: Vec<String> = self
            .index
            .read()
            .expect("search index lock poisoned")
            .candidate_paths(&literals)
            .into_iter()
            .filter(|p| finder.wants(p))
            .map(str::to_string)
            .collect();
        paths.sort_unstable();

        // A whole-buffer scan with the full regex engine finds the matches; lines it cannot describe
        // (a match spanning lines, text that is not UTF-8) and patterns it cannot compile go to
        // `forge-find`, so results stay identical.
        let prefilter = regex::bytes::RegexBuilder::new(&forge_find::pattern(req))
            .case_insensitive(!req.case_sensitive)
            .multi_line(true)
            .crlf(true)
            .build()
            .ok();

        let limit = finder.limit();
        let found = std::sync::atomic::AtomicUsize::new(0);
        let per_file: Vec<(String, Rows, usize, bool)> = paths
            .par_iter()
            .filter_map(|path| {
                if found.load(Ordering::Relaxed) >= limit {
                    return None;
                }
                let bytes = std::fs::read(self.root.join(path)).ok()?;
                if forge_find::is_binary(&bytes) {
                    return None;
                }
                let ranges = match &prefilter {
                    Some(re) => {
                        let ranges = line_ranges(re, &bytes);
                        if ranges.as_ref().is_some_and(Vec::is_empty) {
                            return None;
                        }
                        ranges
                    }
                    None => None,
                };
                let compact = ranges.filter(|_| {
                    bytes.len() <= u32::MAX as usize && std::str::from_utf8(&bytes).is_ok()
                });
                let (rows, count, cut) = match compact {
                    Some(ranges) => {
                        let (ranges, row_ends, count, cut) = group_rows(&bytes, &ranges, limit);
                        (Rows::Ranges { ranges, row_ends }, count, cut)
                    }
                    None => {
                        let (lines, count, cut) =
                            finder.search(&String::from_utf8_lossy(&bytes), limit);
                        (Rows::Lines(lines), count, cut)
                    }
                };
                if rows.len() == 0 {
                    return None;
                }
                found.fetch_add(count, Ordering::Relaxed);
                Some((path.clone(), rows, count, cut))
            })
            .collect();

        let mut result = FoundText {
            root: self.root.clone(),
            files: Vec::new(),
            match_count: 0,
            truncated: false,
            error: None,
        };
        for (path, rows, count, cut) in per_file {
            let remaining = limit - result.match_count;
            if count <= remaining {
                result.match_count += count;
                result.files.push(LazyFile { path, rows });
                if cut {
                    result.truncated = true;
                    break;
                }
                continue;
            }
            result.truncated = true;
            if let Some((rows, count)) = self.cut_to(&path, rows, remaining, &finder) {
                result.match_count += count;
                result.files.push(LazyFile { path, rows });
            }
            break;
        }
        Ok(result)
    }

    /// The first rows of a file that fit `budget` matches, as `Finder::search` would keep them.
    fn cut_to(
        &self,
        path: &str,
        rows: Rows,
        budget: usize,
        finder: &Finder,
    ) -> Option<(Rows, usize)> {
        match rows {
            Rows::Ranges {
                mut ranges,
                mut row_ends,
            } => {
                let (mut keep, mut taken, mut before) = (0, 0, 0);
                for &end in &row_ends {
                    let in_row = (end - before) as usize;
                    if taken + in_row > budget {
                        break;
                    }
                    taken += in_row;
                    before = end;
                    keep += 1;
                }
                row_ends.truncate(keep);
                ranges.truncate(before as usize);
                (keep > 0).then_some((Rows::Ranges { ranges, row_ends }, taken))
            }
            Rows::Lines(_) => {
                let text = self.read_text(path)?;
                let (lines, count, _) = finder.search(&text, budget);
                (!lines.is_empty()).then_some((Rows::Lines(lines), count))
            }
        }
    }

    fn read_text(&self, rel: &str) -> Option<String> {
        let bytes = std::fs::read(self.root.join(rel)).ok()?;
        (!forge_find::is_binary(&bytes)).then(|| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// "Go to File" over the indexed paths, ranked like `Workspace::find_files`.
    pub fn find_ranked(&self, query: &str, limit: usize, exact: bool) -> Vec<FileMatch> {
        let opts = forge_fuzzy::MatchOptions {
            strict: false,
            exact,
        };
        let index = self.index.read().expect("search index lock poisoned");
        let paths: Vec<&str> = index.live().map(|r| r.path.as_str()).collect();
        let mut hits: Vec<FileMatch> = paths
            .par_chunks(1 << 14)
            .flat_map(|chunk| {
                forge_fuzzy::rank_files_with(query, chunk.iter().copied(), limit, opts)
            })
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        hits.truncate(limit);
        hits
    }

    /// Fuzzy search over declared classes (`types_only`) or every declared symbol, leaving out
    /// files with one of `skip_extensions` (languages a language server already answers for).
    pub fn find_symbols(
        &self,
        query: &str,
        types_only: bool,
        limit: usize,
        skip_extensions: &[String],
        exact: bool,
    ) -> Vec<SymbolHit> {
        self.index
            .read()
            .expect("search index lock poisoned")
            .find_symbols(query, types_only, limit, skip_extensions, exact)
    }

    /// Fuzzy file-name search over every indexed path (Search Everywhere, Go to File).
    pub fn find_files(&self, query: &str, limit: usize, exact: bool) -> Vec<FileHit> {
        self.index
            .read()
            .expect("search index lock poisoned")
            .find_files(query, limit, exact)
    }
}

/// Non-empty matches of `re` in `bytes`, or `None` when one touches a line break: `forge-find`
/// matches line by line, so those files go through its own search instead.
fn line_ranges(re: &regex::bytes::Regex, bytes: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    for m in re.find_iter(bytes) {
        if m.is_empty() {
            continue;
        }
        if bytes[m.range()].iter().any(|&b| b == b'\n' || b == b'\r') {
            return None;
        }
        out.push((m.start(), m.end()));
    }
    Some(out)
}

fn watch_loop(rx: mpsc::Receiver<notify::Result<notify::Event>>, service: Weak<SearchService>) {
    let mut dirty: HashSet<PathBuf> = HashSet::new();
    let mut overflowed = false;
    let mut unsaved = false;
    let mut last_save = Instant::now();
    loop {
        match rx.recv_timeout(WATCH_DEBOUNCE) {
            Ok(Ok(event)) => {
                dirty.extend(event.paths);
                continue;
            }
            Ok(Err(_)) => {
                overflowed = true;
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let Some(service) = service.upgrade() else {
            return;
        };
        if overflowed {
            overflowed = false;
            dirty.clear();
            service.refresh();
        } else if !dirty.is_empty() {
            let paths: Vec<PathBuf> = dirty.drain().collect();
            service.update_paths(&paths);
            unsaved = true;
        }
        if unsaved && last_save.elapsed() >= SAVE_INTERVAL {
            service.save();
            unsaved = false;
            last_save = Instant::now();
        }
    }
}

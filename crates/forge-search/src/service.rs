use std::collections::HashSet;
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use forge_search_core::{FileHit, Index, Outcome, Query, RefreshStats, Scope, SymbolHit};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::index::{self, EXCLUDED_DIRS};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(300);
const SAVE_INTERVAL: Duration = Duration::from_secs(60);

/// One project's search index. Share it behind an `Arc`.
pub struct SearchService {
    root: PathBuf,
    cache: Option<PathBuf>,
    index: RwLock<Index>,
    generation: AtomicU64,
    /// Serialises refreshes; searches never take it.
    gate: Mutex<()>,
    indexing: AtomicBool,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl SearchService {
    /// `cache` is where the index is persisted between runs; a missing or stale file is ignored.
    pub fn open(root: impl Into<PathBuf>, cache: Option<PathBuf>) -> Self {
        let index = cache
            .as_deref()
            .and_then(|p| fs::File::open(p).ok())
            .and_then(|f| Index::read_from(BufReader::new(f)))
            .unwrap_or_default();
        Self {
            root: root.into(),
            cache,
            index: RwLock::new(index),
            generation: AtomicU64::new(0),
            gate: Mutex::new(()),
            indexing: AtomicBool::new(false),
            watcher: Mutex::new(None),
        }
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
        self.indexing.store(false, Ordering::Relaxed);
        stats
    }

    fn refresh_scope(&self, scope: &Scope) -> RefreshStats {
        let disk = index::scan(&self.root, scope);
        let plan = self
            .index
            .read()
            .expect("search index lock poisoned")
            .plan(disk, scope);
        if plan.rebuild {
            let mut fresh = Index::new();
            index::load_chunks(&self.root, plan.stale, |batch| fresh.add_batch(batch));
            *self.index.write().expect("search index lock poisoned") = fresh;
        } else {
            self.index
                .write()
                .expect("search index lock poisoned")
                .remove(&plan.gone);
            index::load_chunks(&self.root, plan.stale, |batch| {
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

    /// Fuzzy search over declared classes (`types_only`) or every declared symbol, leaving out
    /// files with one of `skip_extensions` (languages a language server already answers for).
    pub fn find_symbols(
        &self,
        query: &str,
        types_only: bool,
        limit: usize,
        skip_extensions: &[String],
    ) -> Vec<SymbolHit> {
        self.index
            .read()
            .expect("search index lock poisoned")
            .find_symbols(query, types_only, limit, skip_extensions)
    }

    /// Fuzzy file-name search over every indexed path (Search Everywhere, Go to File).
    pub fn find_files(&self, query: &str, limit: usize) -> Vec<FileHit> {
        self.index
            .read()
            .expect("search index lock poisoned")
            .find_files(query, limit)
    }
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

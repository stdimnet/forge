use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::time::UNIX_EPOCH;

use forge_search_core::{Analysis, Loaded, Scanned, Scope, MAX_CONTENT_BYTES};
use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;

const BINARY_SNIFF_BYTES: u64 = 8192;
const LOAD_CHUNK_FILES: usize = 2048;
const LOAD_CHUNK_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".idea",
    ".forge",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    ".next",
];

pub(crate) fn scan(root: &Path, scope: &Scope) -> Vec<Scanned> {
    if !root.join(&scope.dir).is_dir() {
        return Vec::new();
    }
    // Always walk from the root so ignore files of every ancestor apply; prune everything off the scope's path.
    let (walk_root, walk_scope) = (root.to_path_buf(), scope.clone());
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(move |e| {
            if e.depth() == 0 || !e.file_type().is_some_and(|t| t.is_dir()) {
                return true;
            }
            if e.file_name()
                .to_str()
                .is_some_and(|n| EXCLUDED_DIRS.contains(&n))
            {
                return false;
            }
            e.path()
                .strip_prefix(&walk_root)
                .is_ok_and(|rel| walk_scope.walks_into(&rel.to_string_lossy().replace('\\', "/")))
        })
        .build_parallel();

    let (tx, rx) = mpsc::channel();
    let root = root.to_path_buf();
    let scope = scope.clone();
    walker.run(|| {
        let (tx, root, scope) = (tx.clone(), root.clone(), scope.clone());
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let Ok(meta) = entry.metadata() else {
                return WalkState::Continue;
            };
            let Ok(rel) = entry.path().strip_prefix(&root) else {
                return WalkState::Continue;
            };
            let path = rel.to_string_lossy().replace('\\', "/");
            if !scope.contains(&path) {
                return WalkState::Continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos() as u64);
            let _ = tx.send(Scanned {
                path,
                mtime,
                size: meta.len(),
            });
            WalkState::Continue
        })
    });
    drop(tx);
    let mut out: Vec<Scanned> = rx.into_iter().collect();
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Read and analyze `stale` in bounded chunks, handing each to `sink` so memory stays flat.
/// Reading runs one chunk ahead of the sink, so the index is fed while the next chunk loads.
pub(crate) fn load_chunks(
    root: &Path,
    pool: &rayon::ThreadPool,
    stale: Vec<Scanned>,
    mut sink: impl FnMut(Loaded),
) {
    let (tx, rx) = mpsc::sync_channel::<Loaded>(1);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let mut rest = stale.into_iter().peekable();
            loop {
                let mut chunk: Vec<Scanned> = Vec::new();
                let mut bytes = 0;
                while let Some(next) = rest.peek() {
                    if !chunk.is_empty()
                        && (chunk.len() >= LOAD_CHUNK_FILES || bytes + next.size > LOAD_CHUNK_BYTES)
                    {
                        break;
                    }
                    bytes += next.size;
                    chunk.extend(rest.next());
                }
                if chunk.is_empty() {
                    break;
                }
                let loaded: Loaded = pool.install(|| {
                    chunk
                        .into_par_iter()
                        .map(|s| {
                            let analysis = read_file(root, &s)
                                .map_or_else(Analysis::name_only, |b| {
                                    Analysis::of_with(&s.path, &b, &crate::parse::symbols)
                                });
                            (s, analysis)
                        })
                        .collect()
                });
                if tx.send(loaded).is_err() {
                    break;
                }
            }
        });
        for batch in rx {
            sink(batch);
        }
    });
}

/// The file's bytes, or `None` when it is too big, unreadable or binary (decided from its head,
/// so large binaries are never read in full).
fn read_file(root: &Path, s: &Scanned) -> Option<Vec<u8>> {
    if s.size > MAX_CONTENT_BYTES {
        return None;
    }
    let mut file = fs::File::open(root.join(&s.path)).ok()?;
    let mut bytes = Vec::with_capacity(s.size as usize);
    (&mut file)
        .take(BINARY_SNIFF_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.contains(&0) {
        return None;
    }
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

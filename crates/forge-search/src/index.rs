use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use ignore::WalkBuilder;
use rayon::prelude::*;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::symbols::{self, RawSymbol, SymbolHit, SymbolTable};
use crate::trigram;

const FORMAT_VERSION: u32 = 4;
const SEGMENT_FILES: u32 = 16_384;
const MAX_SEGMENTS: usize = 48;
const MAX_CONTENT_BYTES: u64 = 4 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8192;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FileRec {
    pub path: String,
    mtime: u64,
    size: u64,
    /// False for binary or oversized files: listed by name, never searched by content.
    pub content: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

/// Part of the tree a refresh looks at: the whole project, or one directory.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Scope {
    /// Workspace-relative directory, empty for the project root.
    pub dir: String,
    pub recursive: bool,
}

impl Scope {
    pub fn full() -> Self {
        Self {
            dir: String::new(),
            recursive: true,
        }
    }

    pub fn is_full(&self) -> bool {
        self.dir.is_empty() && self.recursive
    }

    /// Whether the walk must descend into directory `rel`: it leads to the scope or lies inside it.
    fn walks_into(&self, rel: &str) -> bool {
        if self.dir.is_empty() {
            return self.recursive;
        }
        let under = |outer: &str, inner: &str| {
            inner
                .strip_prefix(outer)
                .is_some_and(|r| r.starts_with('/'))
        };
        rel == self.dir || under(rel, &self.dir) || (self.recursive && under(&self.dir, rel))
    }

    fn contains(&self, path: &str) -> bool {
        let rest = if self.dir.is_empty() {
            path
        } else {
            match path
                .strip_prefix(self.dir.as_str())
                .and_then(|r| r.strip_prefix('/'))
            {
                Some(rest) => rest,
                None => return false,
            }
        };
        self.recursive || !rest.contains('/')
    }
}

/// Sorted trigram keys over one contiguous buffer of serialized bitmaps: no per-key allocations.
#[derive(Debug, Serialize, Deserialize)]
struct Segment {
    keys: Vec<u32>,
    /// `keys.len() + 1` offsets into `data`.
    offsets: Vec<u32>,
    data: Vec<u8>,
}

impl Segment {
    fn freeze(map: HashMap<u32, RoaringBitmap>) -> Self {
        let mut entries: Vec<(u32, RoaringBitmap)> = map.into_iter().collect();
        entries.sort_unstable_by_key(|e| e.0);
        let mut seg = Segment {
            keys: Vec::with_capacity(entries.len()),
            offsets: vec![0],
            data: Vec::new(),
        };
        for (key, bitmap) in entries {
            bitmap
                .serialize_into(&mut seg.data)
                .expect("writing to a Vec cannot fail");
            seg.keys.push(key);
            seg.offsets.push(seg.data.len() as u32);
        }
        seg
    }

    fn get(&self, key: u32) -> Option<RoaringBitmap> {
        let i = self.keys.binary_search(&key).ok()?;
        RoaringBitmap::deserialize_from(
            &self.data[self.offsets[i] as usize..self.offsets[i + 1] as usize],
        )
        .ok()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Index {
    version: u32,
    files: Vec<Option<FileRec>>,
    /// Immutable postings, one segment per `SEGMENT_FILES` indexed files; ids ascend across segments.
    segments: Vec<Segment>,
    /// Postings of the files added since the last segment was frozen.
    head: HashMap<u32, RoaringBitmap>,
    head_first_id: u32,
    symbols: SymbolTable,
    #[serde(skip)]
    by_path: HashMap<String, u32>,
}

#[derive(Debug)]
pub(crate) struct Scanned {
    pub path: String,
    mtime: u64,
    size: u64,
}

pub(crate) type Loaded = Vec<(Scanned, Option<Vec<u32>>, Vec<RawSymbol>)>;

/// What a refresh has to do, computed without touching the index.
pub(crate) struct Plan {
    pub gone: Vec<u32>,
    pub stale: Vec<Scanned>,
    pub stats: RefreshStats,
    /// Too many dead slots: rebuild into a fresh index instead of patching this one.
    pub rebuild: bool,
}

impl Index {
    pub fn new() -> Self {
        Self {
            version: FORMAT_VERSION,
            files: Vec::new(),
            segments: Vec::new(),
            head: HashMap::new(),
            head_first_id: 0,
            symbols: SymbolTable::default(),
            by_path: HashMap::new(),
        }
    }

    pub fn read_from(reader: impl Read) -> Option<Self> {
        let mut index: Index = bincode::deserialize_from(reader).ok()?;
        if index.version != FORMAT_VERSION {
            return None;
        }
        index.by_path = index
            .files
            .iter()
            .enumerate()
            .filter_map(|(id, f)| f.as_ref().map(|f| (f.path.clone(), id as u32)))
            .collect();
        Some(index)
    }

    pub fn write_to(&self, writer: impl Write) -> bool {
        bincode::serialize_into(writer, self).is_ok()
    }

    pub fn rec(&self, id: u32) -> Option<&FileRec> {
        self.files.get(id as usize)?.as_ref()
    }

    pub fn live(&self) -> impl Iterator<Item = &FileRec> {
        self.files.iter().flatten()
    }

    pub fn find_symbols(
        &self,
        query: &str,
        types_only: bool,
        limit: usize,
        skip_extensions: &[String],
    ) -> Vec<SymbolHit> {
        self.symbols
            .search(&self.files, query, types_only, limit, skip_extensions)
    }

    pub fn has_entries_under(&self, dir: &str) -> bool {
        let prefix = format!("{dir}/");
        self.by_path.keys().any(|p| p.starts_with(&prefix))
    }

    pub fn plan(&self, disk: Vec<Scanned>, scope: &Scope) -> Plan {
        let dead = self.files.iter().filter(|f| f.is_none()).count();
        let bloated = dead > 64 && dead * 2 > self.files.len();
        if scope.is_full() && (bloated || self.segments.len() > MAX_SEGMENTS) {
            let added = disk.len();
            return Plan {
                gone: Vec::new(),
                stale: disk,
                stats: RefreshStats {
                    added,
                    ..Default::default()
                },
                rebuild: true,
            };
        }

        let on_disk: HashSet<&str> = disk.iter().map(|s| s.path.as_str()).collect();
        let gone: Vec<u32> = self
            .by_path
            .iter()
            .filter(|(p, _)| scope.contains(p) && !on_disk.contains(p.as_str()))
            .map(|(_, &id)| id)
            .collect();
        let mut stats = RefreshStats {
            removed: gone.len(),
            ..Default::default()
        };

        let mut stale = Vec::new();
        for s in disk {
            match self.by_path.get(&s.path).and_then(|&id| self.rec(id)) {
                Some(rec) if rec.mtime == s.mtime && rec.size == s.size => continue,
                Some(_) => stats.changed += 1,
                None => stats.added += 1,
            }
            stale.push(s);
        }
        Plan {
            gone,
            stale,
            stats,
            rebuild: false,
        }
    }

    pub fn remove(&mut self, ids: &[u32]) {
        for &id in ids {
            self.tombstone(id);
        }
    }

    pub fn add_batch(&mut self, batch: Loaded) {
        for (s, grams, syms) in batch {
            if let Some(&old) = self.by_path.get(&s.path) {
                self.tombstone(old);
            }
            let id = self.files.len() as u32;
            if let Some(grams) = &grams {
                for &g in grams {
                    self.head.entry(g).or_default().push(id);
                }
            }
            self.symbols.add_file(id, &syms);
            self.by_path.insert(s.path.clone(), id);
            self.files.push(Some(FileRec {
                path: s.path,
                mtime: s.mtime,
                size: s.size,
                content: grams.is_some(),
            }));
        }
        if self.files.len() as u32 - self.head_first_id >= SEGMENT_FILES {
            self.segments
                .push(Segment::freeze(std::mem::take(&mut self.head)));
            self.head_first_id = self.files.len() as u32;
        }
    }

    fn tombstone(&mut self, id: u32) {
        if let Some(rec) = self.files[id as usize].take() {
            self.by_path.remove(&rec.path);
        }
    }

    /// Live content files that may contain every literal set (any literal of each set).
    /// An empty `literal_sets` means no prefilter: every content file.
    pub fn candidates(&self, literal_sets: &[Vec<Vec<u8>>]) -> Vec<u32> {
        let mut acc: Option<RoaringBitmap> = None;
        for set in literal_sets {
            let union = set.iter().fold(RoaringBitmap::new(), |mut u, l| {
                u |= self.literal_ids(l);
                u
            });
            acc = Some(match acc {
                Some(prev) => prev & union,
                None => union,
            });
        }
        let keep = |&id: &u32| self.rec(id).is_some_and(|r| r.content);
        match acc {
            Some(ids) => ids.into_iter().filter(keep).collect(),
            None => (0..self.files.len() as u32).filter(keep).collect(),
        }
    }

    fn trigram_ids(&self, gram: u32) -> RoaringBitmap {
        let mut ids = self.head.get(&gram).cloned().unwrap_or_default();
        for seg in &self.segments {
            if let Some(more) = seg.get(gram) {
                ids |= more;
            }
        }
        ids
    }

    fn literal_ids(&self, literal: &[u8]) -> RoaringBitmap {
        let mut acc: Option<RoaringBitmap> = None;
        for g in trigram::of_text(literal) {
            let ids = self.trigram_ids(g);
            let next = match acc {
                Some(prev) => prev & ids,
                None => ids,
            };
            if next.is_empty() {
                return next;
            }
            acc = Some(next);
        }
        acc.unwrap_or_default()
    }
}

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
        .build();

    let mut out = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = rel.to_string_lossy().replace('\\', "/");
        if !scope.contains(&path) {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as u64);
        out.push(Scanned {
            path,
            mtime,
            size: meta.len(),
        });
    }
    out
}

/// Read and trigram `stale` in bounded chunks, handing each to `sink` so memory stays flat.
pub(crate) fn load_chunks(root: &Path, stale: Vec<Scanned>, mut sink: impl FnMut(Loaded)) {
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
        let loaded: Loaded = chunk
            .into_par_iter()
            .map(|s| match read_file(root, &s) {
                Some((grams, syms)) => (s, Some(grams), syms),
                None => (s, None, Vec::new()),
            })
            .collect();
        sink(loaded);
    }
}

fn read_file(root: &Path, s: &Scanned) -> Option<(Vec<u32>, Vec<RawSymbol>)> {
    if s.size > MAX_CONTENT_BYTES {
        return None;
    }
    let mut file = fs::File::open(root.join(&s.path)).ok()?;
    let mut bytes = Vec::with_capacity(s.size as usize);
    file.read_to_end(&mut bytes).ok()?;
    if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
        return None;
    }
    Some((trigram::of_text(&bytes), symbols::extract(&s.path, &bytes)))
}

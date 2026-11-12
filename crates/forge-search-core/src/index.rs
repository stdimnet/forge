use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{Read, Write};

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::symbols::{self, RawSymbol, SymbolHit, SymbolTable};
use crate::trigram;

const FORMAT_VERSION: u32 = 7;
/// Independent postings maps of the head, so one batch is inserted by several threads at once.
const SHARDS: usize = 16;
const SEGMENT_FILES: u32 = 16_384;
const MAX_SEGMENTS: usize = 48;
/// Larger files are listed by name but never searched by content. The same limit as PHPStorm's
/// `idea.max.content.load.filesize`.
pub const MAX_CONTENT_BYTES: u64 = 20 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRec {
    pub path: String,
    mtime: u64,
    size: u64,
    /// False for binary or oversized files: listed by name, never searched by content.
    pub content: bool,
}

/// A symbol extractor tried before the line patterns; `None` means it does not cover the file.
pub type SymbolParser = dyn Fn(&str, &[u8]) -> Option<Vec<crate::RawSymbol>>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

/// What an index holds: how many files, and how many of them are listed by name only.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub content: usize,
    pub binary: usize,
    pub oversized: usize,
}

/// Part of the tree a refresh looks at: the whole project, or one directory.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
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
    pub fn walks_into(&self, rel: &str) -> bool {
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

    pub fn contains(&self, path: &str) -> bool {
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
    fn freeze(shards: Vec<GramMap>) -> Self {
        let mut entries: Vec<(u32, RoaringBitmap)> = shards.into_iter().flatten().collect();
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
pub struct Index {
    version: u32,
    files: Vec<Option<FileRec>>,
    /// Immutable postings, one segment per `SEGMENT_FILES` indexed files; ids ascend across segments.
    segments: Vec<Segment>,
    /// Postings of the files added since the last segment was frozen.
    head: Vec<GramMap>,
    head_first_id: u32,
    symbols: SymbolTable,
    #[serde(skip)]
    by_path: HashMap<String, u32>,
}

#[derive(Debug)]
pub struct Scanned {
    pub path: String,
    pub mtime: u64,
    pub size: u64,
}

/// What a file's content contributes to the index.
pub struct Analysis {
    grams: Option<Grams>,
    symbols: Vec<RawSymbol>,
}

/// A file's sorted trigrams grouped by shard: `ends[i]` is where shard `i`'s run stops.
struct Grams {
    list: Vec<u32>,
    ends: [u32; SHARDS],
}

fn shard_of(gram: u32) -> usize {
    gram as usize % SHARDS
}

impl Grams {
    fn new(sorted: Vec<u32>) -> Self {
        let mut counts = [0u32; SHARDS];
        for &g in &sorted {
            counts[shard_of(g)] += 1;
        }
        let mut ends = [0u32; SHARDS];
        let mut next = [0usize; SHARDS];
        let mut at = 0;
        for i in 0..SHARDS {
            next[i] = at;
            at += counts[i] as usize;
            ends[i] = at as u32;
        }
        let mut list = vec![0u32; sorted.len()];
        for g in sorted {
            let shard = shard_of(g);
            list[next[shard]] = g;
            next[shard] += 1;
        }
        Self { list, ends }
    }

    fn shard(&self, i: usize) -> &[u32] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        &self.list[start..self.ends[i] as usize]
    }
}

fn new_head() -> Vec<GramMap> {
    (0..SHARDS).map(|_| GramMap::default()).collect()
}

impl Analysis {
    /// Name-only entry for files that cannot be read or searched.
    pub fn name_only() -> Self {
        Self {
            grams: None,
            symbols: Vec::new(),
        }
    }

    /// Trigrams and symbols of `bytes`; binary and oversized content is indexed by name only.
    pub fn of(path: &str, bytes: &[u8]) -> Self {
        Self::of_with(path, bytes, &|_, _| None)
    }

    /// [`Analysis::of`] with a better symbol extractor tried first; `None` from it falls back to
    /// the line patterns.
    pub fn of_with(path: &str, bytes: &[u8], parse: &SymbolParser) -> Self {
        if bytes.len() as u64 > MAX_CONTENT_BYTES
            || bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
        {
            return Self::name_only();
        }
        Self {
            grams: Some(Grams::new(trigram::of_text(bytes))),
            symbols: parse(path, bytes).unwrap_or_else(|| symbols::extract(path, bytes)),
        }
    }
}

pub type Loaded = Vec<(Scanned, Analysis)>;

/// What a refresh has to do, computed without touching the index.
pub struct Plan {
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
            head: new_head(),
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
        exact: bool,
    ) -> Vec<SymbolHit> {
        self.symbols.search(
            &self.files,
            query,
            types_only,
            limit,
            skip_extensions,
            exact,
        )
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
        let mut postings: Vec<(u32, Grams)> = Vec::new();
        for (
            s,
            Analysis {
                grams,
                symbols: syms,
            },
        ) in batch
        {
            if let Some(&old) = self.by_path.get(&s.path) {
                self.tombstone(old);
            }
            let id = self.files.len() as u32;
            let content = grams.is_some();
            if let Some(grams) = grams {
                postings.push((id, grams));
            }
            self.symbols.add_file(id, &syms);
            self.by_path.insert(s.path.clone(), id);
            self.files.push(Some(FileRec {
                path: s.path,
                mtime: s.mtime,
                size: s.size,
                content,
            }));
        }
        let insert = |(shard, map): (usize, &mut GramMap)| {
            for (id, grams) in &postings {
                for &g in grams.shard(shard) {
                    map.entry(g).or_default().push(*id);
                }
            }
        };
        par_iter_mut!(self.head).enumerate().for_each(insert);
        if self.files.len() as u32 - self.head_first_id >= SEGMENT_FILES {
            let head = std::mem::replace(&mut self.head, new_head());
            self.segments.push(Segment::freeze(head));
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

    /// Paths of the live content files that may contain every literal set.
    pub fn candidate_paths(&self, literal_sets: &[Vec<Vec<u8>>]) -> Vec<&str> {
        self.candidates(literal_sets)
            .into_iter()
            .filter_map(|id| self.rec(id))
            .map(|r| r.path.as_str())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn stats(&self) -> IndexStats {
        let mut stats = IndexStats::default();
        for rec in self.live() {
            stats.files += 1;
            if rec.content {
                stats.content += 1;
            } else if rec.size > MAX_CONTENT_BYTES {
                stats.oversized += 1;
            } else {
                stats.binary += 1;
            }
        }
        stats
    }

    pub fn file_count(&self) -> usize {
        self.files.len() - self.files.iter().filter(|f| f.is_none()).count()
    }

    fn trigram_ids(&self, gram: u32) -> RoaringBitmap {
        let mut ids = self.head[shard_of(gram)]
            .get(&gram)
            .cloned()
            .unwrap_or_default();
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

    /// Fuzzy file-name search over every indexed path.
    pub fn find_files(&self, query: &str, limit: usize, exact: bool) -> Vec<FileHit> {
        if query.trim().is_empty() || limit == 0 {
            return Vec::new();
        }
        let paths: Vec<&str> = self.live().map(|r| r.path.as_str()).collect();

        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let pattern = pattern(query, exact);
        let mut scored = pattern.match_list(paths, &mut matcher);
        scored.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(a.0.len().cmp(&b.0.len()))
                .then(a.0.cmp(b.0))
        });
        scored.truncate(limit);

        let mut buf = Vec::new();
        scored
            .into_iter()
            .map(|(path, score)| {
                let mut matched = Vec::new();
                pattern.indices(Utf32Str::new(path, &mut buf), &mut matcher, &mut matched);
                matched.sort_unstable();
                matched.dedup();
                FileHit {
                    path: path.to_string(),
                    score,
                    matched,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub path: String,
    pub score: u32,
    /// Character indices of `path` that matched the query, for highlighting.
    pub matched: Vec<u32>,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

/// Trigram keys are already well spread, so a multiply replaces SipHash on the hot indexing path.
#[derive(Default)]
pub struct GramHasher(u64);

impl Hasher for GramHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type GramMap = HashMap<u32, RoaringBitmap, BuildHasherDefault<GramHasher>>;

/// The fuzzy pattern for `query`, or one that matches each word as a substring.
pub(crate) fn pattern(query: &str, exact: bool) -> Pattern {
    if exact {
        Pattern::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Substring,
        )
    } else {
        Pattern::parse(query, CaseMatching::Smart, Normalization::Smart)
    }
}

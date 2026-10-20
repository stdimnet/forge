use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::symbols::{self, RawSymbol, SymbolHit, SymbolTable};
use crate::trigram;

const FORMAT_VERSION: u32 = 4;
const SEGMENT_FILES: u32 = 16_384;
const MAX_SEGMENTS: usize = 48;
/// Larger files are listed by name but never searched by content.
pub const MAX_CONTENT_BYTES: u64 = 4 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRec {
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
pub struct Index {
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
pub struct Scanned {
    pub path: String,
    pub mtime: u64,
    pub size: u64,
}

/// What a file's content contributes to the index.
pub struct Analysis {
    grams: Option<Vec<u32>>,
    symbols: Vec<RawSymbol>,
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
        if bytes.len() as u64 > MAX_CONTENT_BYTES
            || bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
        {
            return Self::name_only();
        }
        Self {
            grams: Some(trigram::of_text(bytes)),
            symbols: symbols::extract(path, bytes),
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

    /// Fuzzy file-name search over every indexed path.
    pub fn find_files(&self, query: &str, limit: usize) -> Vec<FileHit> {
        if query.trim().is_empty() || limit == 0 {
            return Vec::new();
        }
        let paths: Vec<&str> = self.live().map(|r| r.path.as_str()).collect();

        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
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

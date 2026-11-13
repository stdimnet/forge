//! Where change sets and contents live: in memory (WebAssembly, tests) or on disk
//! ([`DiskStorage`]: an append-only change log plus compressed, deduplicated contents).

use std::collections::{BTreeMap, HashMap};
use std::io;

use sha2::{Digest, Sha256};

use crate::model::ChangeSet;

/// The content hash: the first 128 bits of SHA-256, hex encoded.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Change log and content store of one project.
pub trait Storage {
    /// Every change set, ordered by id (a set appended again replaces the earlier copy).
    fn load(&mut self) -> io::Result<Vec<ChangeSet>>;
    /// Persist a new or updated change set.
    fn append(&mut self, set: &ChangeSet) -> io::Result<()>;
    /// Replace the whole log (after a cleanup).
    fn rewrite(&mut self, sets: &[ChangeSet]) -> io::Result<()>;
    /// Store a content under its hash (nothing happens when it is already stored).
    fn put_blob(&mut self, hash: &str, bytes: &[u8]) -> io::Result<()>;
    fn get_blob(&self, hash: &str) -> io::Result<Option<Vec<u8>>>;
    /// Stored size of every content (compressed size on disk).
    fn blob_sizes(&self) -> io::Result<HashMap<String, u64>>;
    fn delete_blob(&mut self, hash: &str) -> io::Result<()>;
}

/// Everything in memory.
#[derive(Debug, Default, Clone)]
pub struct MemStorage {
    sets: BTreeMap<u64, ChangeSet>,
    blobs: HashMap<String, Vec<u8>>,
}

impl Storage for MemStorage {
    fn load(&mut self) -> io::Result<Vec<ChangeSet>> {
        Ok(self.sets.values().cloned().collect())
    }
    fn append(&mut self, set: &ChangeSet) -> io::Result<()> {
        self.sets.insert(set.id, set.clone());
        Ok(())
    }
    fn rewrite(&mut self, sets: &[ChangeSet]) -> io::Result<()> {
        self.sets = sets.iter().map(|s| (s.id, s.clone())).collect();
        Ok(())
    }
    fn put_blob(&mut self, hash: &str, bytes: &[u8]) -> io::Result<()> {
        self.blobs
            .entry(hash.to_string())
            .or_insert_with(|| bytes.to_vec());
        Ok(())
    }
    fn get_blob(&self, hash: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.blobs.get(hash).cloned())
    }
    fn blob_sizes(&self) -> io::Result<HashMap<String, u64>> {
        Ok(self
            .blobs
            .iter()
            .map(|(h, b)| (h.clone(), b.len() as u64))
            .collect())
    }
    fn delete_blob(&mut self, hash: &str) -> io::Result<()> {
        self.blobs.remove(hash);
        Ok(())
    }
}

#[cfg(feature = "disk")]
pub use disk::DiskStorage;

#[cfg(feature = "disk")]
mod disk {
    use std::collections::{BTreeMap, HashMap};
    use std::fs;
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::path::{Path, PathBuf};

    use flate2::read::ZlibDecoder;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    use super::Storage;
    use crate::model::ChangeSet;

    const LOG: &str = "changes.jsonl";
    const CONTENT: &str = "content";

    /// `<dir>/changes.jsonl` (one change set per line; a set written again replaces the
    /// earlier line when loading) and `<dir>/content/<2 hex>/<hash>` (zlib compressed).
    #[derive(Debug, Clone)]
    pub struct DiskStorage {
        dir: PathBuf,
    }

    impl DiskStorage {
        pub fn new(dir: impl Into<PathBuf>) -> Self {
            DiskStorage { dir: dir.into() }
        }

        pub fn dir(&self) -> &Path {
            &self.dir
        }

        fn log(&self) -> PathBuf {
            self.dir.join(LOG)
        }

        fn blob_path(&self, hash: &str) -> PathBuf {
            let prefix = hash.get(..2).unwrap_or("00");
            self.dir.join(CONTENT).join(prefix).join(hash)
        }
    }

    fn line(set: &ChangeSet) -> io::Result<String> {
        let mut json = serde_json::to_string(set).map_err(io::Error::other)?;
        json.push('\n');
        Ok(json)
    }

    impl Storage for DiskStorage {
        fn load(&mut self) -> io::Result<Vec<ChangeSet>> {
            // Bytes, not a string: a line torn inside a multi-byte character must not make the
            // whole log unreadable.
            let text = match fs::read(self.log()) {
                Ok(t) => String::from_utf8_lossy(&t).into_owned(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let mut sets = BTreeMap::new();
            let mut lines = 0usize;
            for l in text.lines().filter(|l| !l.trim().is_empty()) {
                lines += 1;
                // A torn last line (crash while writing) is skipped.
                if let Ok(set) = serde_json::from_str::<ChangeSet>(l) {
                    sets.insert(set.id, set);
                }
            }
            let sets: Vec<ChangeSet> = sets.into_values().collect();
            // Updated sets (merged operations) leave older copies behind: compact.
            if lines > sets.len() + sets.len() / 2 + 16 {
                self.rewrite(&sets)?;
            }
            Ok(sets)
        }

        fn append(&mut self, set: &ChangeSet) -> io::Result<()> {
            fs::create_dir_all(&self.dir)?;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .read(true)
                .append(true)
                .open(self.log())?;
            let mut text = line(set)?;
            // A torn last line (crash while writing) has no line break: start a new line, or
            // this set would be glued onto the fragment and fail to parse on every load.
            if f.seek(SeekFrom::End(0))? > 0 {
                f.seek(SeekFrom::End(-1))?;
                let mut last = [0u8];
                f.read_exact(&mut last)?;
                if last[0] != b'\n' {
                    text.insert(0, '\n');
                }
            }
            f.write_all(text.as_bytes())
        }

        fn rewrite(&mut self, sets: &[ChangeSet]) -> io::Result<()> {
            fs::create_dir_all(&self.dir)?;
            let tmp = self.dir.join(format!("{LOG}.tmp"));
            let mut text = String::new();
            for s in sets {
                text.push_str(&line(s)?);
            }
            fs::write(&tmp, text)?;
            fs::rename(&tmp, self.log())
        }

        fn put_blob(&mut self, hash: &str, bytes: &[u8]) -> io::Result<()> {
            let path = self.blob_path(hash);
            if path.exists() {
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(bytes)?;
            let packed = enc.finish()?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, packed)?;
            fs::rename(&tmp, &path)
        }

        fn get_blob(&self, hash: &str) -> io::Result<Option<Vec<u8>>> {
            let packed = match fs::read(self.blob_path(hash)) {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let mut out = Vec::new();
            ZlibDecoder::new(&packed[..]).read_to_end(&mut out)?;
            Ok(Some(out))
        }

        fn blob_sizes(&self) -> io::Result<HashMap<String, u64>> {
            let mut sizes = HashMap::new();
            let root = self.dir.join(CONTENT);
            let Ok(dirs) = fs::read_dir(&root) else {
                return Ok(sizes);
            };
            for d in dirs.flatten() {
                let Ok(files) = fs::read_dir(d.path()) else {
                    continue;
                };
                for f in files.flatten() {
                    let name = f.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".tmp") {
                        continue;
                    }
                    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                    sizes.insert(name, len);
                }
            }
            Ok(sizes)
        }

        fn delete_blob(&mut self, hash: &str) -> io::Result<()> {
            match fs::remove_file(self.blob_path(hash)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::model::{ChangeKind, Entry, FileState};

        fn set(id: u64, name: &str) -> ChangeSet {
            ChangeSet {
                id,
                time: 1000 * id as i64,
                kind: ChangeKind::Save,
                name: name.into(),
                entries: vec![Entry {
                    path: "a.txt".into(),
                    before: FileState::Absent,
                    after: FileState::File("aa".into()),
                    from: None,
                }],
            }
        }

        #[test]
        fn log_keeps_the_last_copy_of_a_set_and_skips_torn_lines() {
            let dir = tempfile::tempdir().unwrap();
            let mut s = DiskStorage::new(dir.path().join("lh"));
            assert!(s.load().unwrap().is_empty());
            s.append(&set(1, "one")).unwrap();
            s.append(&set(2, "two")).unwrap();
            s.append(&set(1, "one again")).unwrap();
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("lh").join(LOG))
                .unwrap();
            f.write_all(b"{\"id\":3,\"ti").unwrap();
            let sets = s.load().unwrap();
            assert_eq!(sets.len(), 2);
            assert_eq!(sets[0].name, "one again");
            s.rewrite(&sets[1..]).unwrap();
            assert_eq!(s.load().unwrap().len(), 1);
        }

        #[test]
        fn a_set_appended_after_a_torn_line_survives() {
            let dir = tempfile::tempdir().unwrap();
            let mut s = DiskStorage::new(dir.path().join("lh"));
            s.append(&set(1, "one")).unwrap();
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("lh").join(LOG))
                .unwrap();
            // Torn inside a multi-byte character, like a crash in the middle of a write.
            f.write_all(&"{\"id\":2,\"name\":\"\u{e4}".as_bytes()[..17])
                .unwrap();
            drop(f);
            assert_eq!(s.load().unwrap().len(), 1);
            s.append(&set(3, "three")).unwrap();
            let sets = s.load().unwrap();
            assert_eq!(
                sets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
                ["one", "three"]
            );
            // Compaction keeps it too.
            s.rewrite(&sets).unwrap();
            assert_eq!(s.load().unwrap().len(), 2);
        }

        #[test]
        fn contents_are_compressed_and_deduplicated() {
            let dir = tempfile::tempdir().unwrap();
            let mut s = DiskStorage::new(dir.path());
            let text = "<?php\n".repeat(500);
            let h = crate::storage::content_hash(text.as_bytes());
            s.put_blob(&h, text.as_bytes()).unwrap();
            s.put_blob(&h, text.as_bytes()).unwrap();
            let sizes = s.blob_sizes().unwrap();
            assert_eq!(sizes.len(), 1);
            assert!(sizes[&h] < text.len() as u64 / 10);
            assert_eq!(s.get_blob(&h).unwrap().unwrap(), text.as_bytes());
            s.delete_blob(&h).unwrap();
            s.delete_blob(&h).unwrap();
            assert!(s.get_blob(&h).unwrap().is_none());
        }
    }
}

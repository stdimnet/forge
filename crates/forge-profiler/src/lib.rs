//! Profiler snapshots for the shared Profiler tool window (PHPStorm's Xdebug / SPX snapshot
//! views and the V8 CPU and memory profilers):
//!
//! - [`cachegrind`]: Xdebug profiler snapshots (`cachegrind.out.*`, gzip or plain).
//! - [`spx`]: php-spx full reports (metadata JSON plus the `.txt.gz` trace).
//! - [`v8`]: V8 CPU profiles (`.cpuprofile`) and sampling heap profiles (`.heapprofile`),
//!   with locations mapped through [`sourcemap`]s.
//! - [`heapsnapshot`]: V8 heap snapshots (`.heapsnapshot`) streamed into a summary by
//!   constructor with retained sizes (dominator tree) and the biggest objects.
//! - [`model`]: the [`Snapshot`] every format becomes: Execution Statistics
//!   ([`FunctionStats`]), callers / callees ([`CallEdge`]) and `goforge-pprof`'s
//!   [`goforge_pprof::ProfileView`] (call tree, flame graph, method list).
//! - [`demo`]: snapshots of the browser-mode demo projects.

pub mod cachegrind;
pub mod demo;
pub mod heapsnapshot;
pub mod model;
pub mod sourcemap;
pub mod spx;
pub mod v8;

use std::io::Read;

pub use heapsnapshot::{HeapClass, HeapObject, HeapSummary};
pub use model::{CallEdge, FunctionStats, InfoItem, Snapshot, SnapshotKind};
pub use sourcemap::SourceMap;
pub use v8::MapResolver;

/// The file formats Forge opens as profiler snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Cachegrind,
    /// The metadata of an SPX report (its trace is the companion file).
    SpxMetadata,
    /// The trace of an SPX report (its metadata is the companion file).
    SpxTrace,
    CpuProfile,
    HeapProfile,
    HeapSnapshot,
}

/// Plain text of a file that may be gzip compressed.
pub(crate) fn text(data: &[u8]) -> Result<String, String> {
    if data.starts_with(&[0x1f, 0x8b]) {
        let mut out = String::new();
        flate2::read::MultiGzDecoder::new(data)
            .read_to_string(&mut out)
            .map_err(|e| format!("cannot decompress: {e}"))?;
        Ok(out)
    } else {
        Ok(String::from_utf8_lossy(data).into_owned())
    }
}

/// A URL path with `%xx` escapes decoded.
pub(crate) fn url_path(p: &str) -> String {
    let b = p.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // file:///C:/x → C:/x
    match s.as_bytes() {
        [b'/', d, b':', ..] if d.is_ascii_alphabetic() => s[1..].to_string(),
        _ => s,
    }
}

/// The file name of a path.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Which format a file is, from its name and its first bytes (decompressed when gzip).
pub fn detect(name: &str, head: &[u8]) -> Option<Format> {
    let file = file_name(name).to_ascii_lowercase();
    if file.contains("cachegrind.out") || file.contains("callgrind.out") {
        return Some(Format::Cachegrind);
    }
    if file.ends_with(".cpuprofile") {
        return Some(Format::CpuProfile);
    }
    if file.ends_with(".heapprofile") {
        return Some(Format::HeapProfile);
    }
    if file.ends_with(".heapsnapshot") {
        return Some(Format::HeapSnapshot);
    }
    if file.starts_with("spx-full-") && file.ends_with(".json") {
        return Some(Format::SpxMetadata);
    }
    let start = if head.starts_with(&[0x1f, 0x8b]) {
        let mut buf = vec![0u8; 512];
        let mut d = flate2::read::GzDecoder::new(head);
        let mut got = 0;
        while got < buf.len() {
            match d.read(&mut buf[got..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => got += k,
            }
        }
        buf.truncate(got);
        buf
    } else {
        head[..head.len().min(512)].to_vec()
    };
    let s = String::from_utf8_lossy(&start);
    let t = s.trim_start();
    if t.starts_with("[events]") {
        return Some(Format::SpxTrace);
    }
    if [
        "version:",
        "creator:",
        "events:",
        "# callgrind",
        "cmd:",
        "positions:",
    ]
    .iter()
    .any(|p| t.starts_with(p))
    {
        return Some(Format::Cachegrind);
    }
    if t.starts_with('{') {
        let compact: String = t.chars().filter(|c| !c.is_whitespace()).take(200).collect();
        if compact.starts_with("{\"snapshot\"") {
            return Some(Format::HeapSnapshot);
        }
        if compact.contains("\"enabled_metrics\"") || compact.starts_with("{\"key\":\"spx-") {
            return Some(Format::SpxMetadata);
        }
        if compact.starts_with("{\"head\"") {
            return Some(Format::HeapProfile);
        }
        if compact.starts_with("{\"nodes\"") || compact.contains("\"callFrame\"") {
            return Some(Format::CpuProfile);
        }
    }
    None
}

/// How to open a snapshot.
#[derive(Default, Clone, Copy)]
pub struct OpenOptions<'a> {
    /// The metric to show (default: the format's first).
    pub metric: Option<usize>,
    /// The other file of an SPX report (the trace of a metadata file, or the reverse).
    pub companion: Option<&'a [u8]>,
    /// Source maps of V8 scripts.
    pub maps: Option<MapResolver<'a>>,
}

/// Open a snapshot file read into memory (`name`: its path or file name).
pub fn open(name: &str, data: &[u8], options: OpenOptions<'_>) -> Result<Snapshot, String> {
    let file = file_name(name);
    match detect(name, data) {
        Some(Format::Cachegrind) => cachegrind::parse(data, file, options.metric),
        Some(Format::SpxMetadata) => {
            let meta = spx::metadata(data)?;
            let trace = options.companion.ok_or_else(|| {
                format!("the SPX trace {} was not found", spx::trace_name(&meta.key))
            })?;
            spx::parse(Some(&meta), trace, &meta.key, options.metric)
        }
        Some(Format::SpxTrace) => {
            let meta = options.companion.and_then(|m| spx::metadata(m).ok());
            let key = file.trim_end_matches(".gz").trim_end_matches(".txt");
            spx::parse(meta.as_ref(), data, key, options.metric)
        }
        Some(Format::CpuProfile) => v8::cpu_profile(data, file, options.metric, options.maps),
        Some(Format::HeapProfile) => v8::heap_profile(data, file, options.metric, options.maps),
        Some(Format::HeapSnapshot) => heapsnapshot::parse(data, file),
        None => Err(format!(
            "{file} is not a profiler snapshot (Xdebug cachegrind, SPX report, .cpuprofile, .heapprofile or .heapsnapshot)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats_by_name_and_content() {
        assert_eq!(
            detect("/tmp/cachegrind.out.123.gz", b""),
            Some(Format::Cachegrind)
        );
        assert_eq!(
            detect("x", b"version: 1\ncreator: xdebug"),
            Some(Format::Cachegrind)
        );
        assert_eq!(detect("CPU.1.cpuprofile", b""), Some(Format::CpuProfile));
        assert_eq!(
            detect("x.json", b"{\"nodes\":[{\"id\":1"),
            Some(Format::CpuProfile)
        );
        assert_eq!(detect("x.json", b"{\"head\":{"), Some(Format::HeapProfile));
        assert_eq!(
            detect("x.json", b" {\n \"snapshot\": {"),
            Some(Format::HeapSnapshot)
        );
        assert_eq!(detect("spx-full-1.json", b""), Some(Format::SpxMetadata));
        assert_eq!(
            detect("a.json", b"{\"key\":\"spx-full-1\""),
            Some(Format::SpxMetadata)
        );
        assert_eq!(detect("k.txt", b"[events]\n"), Some(Format::SpxTrace));
        assert_eq!(detect("notes.txt", b"hello"), None);
        assert!(open("notes.txt", b"hello", OpenOptions::default()).is_err());
        assert_eq!(url_path("/a%20b/c.js"), "/a b/c.js");
        assert_eq!(url_path("/C:/x/y.js"), "C:/x/y.js");
        assert_eq!(url_path("/bad%zz"), "/bad%zz");
    }

    #[test]
    fn opens_every_fixture() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
        let read = |f: &str| std::fs::read(format!("{dir}{f}")).unwrap();
        let spx_meta = read("spx-full-20261028_101500-devbox-4242-1804289383.json");
        let spx_trace = read("spx-full-20261028_101500-devbox-4242-1804289383.txt.gz");
        type Case<'a> = (&'a str, Vec<u8>, Option<&'a [u8]>, SnapshotKind);
        let cases: Vec<Case> = vec![
            (
                "cachegrind.out.4242",
                read("cachegrind.out.4242"),
                None,
                SnapshotKind::Xdebug,
            ),
            (
                "spx-full-20261028_101500-devbox-4242-1804289383.json",
                spx_meta.clone(),
                Some(&spx_trace),
                SnapshotKind::Spx,
            ),
            (
                "spx-full-20261028_101500-devbox-4242-1804289383.txt.gz",
                spx_trace.clone(),
                Some(&spx_meta),
                SnapshotKind::Spx,
            ),
            (
                "app.cpuprofile",
                read("app.cpuprofile"),
                None,
                SnapshotKind::V8Cpu,
            ),
            (
                "app.heapprofile",
                read("app.heapprofile"),
                None,
                SnapshotKind::V8HeapSampling,
            ),
            (
                "small.heapsnapshot",
                read("small.heapsnapshot"),
                None,
                SnapshotKind::V8Heap,
            ),
        ];
        for (name, data, companion, kind) in cases {
            let s = open(
                name,
                &data,
                OpenOptions {
                    companion,
                    ..OpenOptions::default()
                },
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(s.kind, kind, "{name}");
            assert!(s.total > 0, "{name}");
            let json = serde_json::to_value(&s).unwrap();
            assert!(json["callsLabel"].is_string());
        }
        // The SPX trace's metadata gives it its title.
        let s = open(
            "spx-full-20261028_101500-devbox-4242-1804289383.txt.gz",
            &spx_trace,
            OpenOptions {
                companion: Some(&spx_meta),
                ..OpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(s.title, "php bin/report.php");
        let missing = open("spx-full-1.json", &spx_meta, OpenOptions::default());
        assert!(missing.unwrap_err().contains(".txt.gz"));
    }

    #[test]
    fn maps_files_of_a_snapshot() {
        let mut s = demo::snapshot(SnapshotKind::Xdebug, "/home/anna/shop", None);
        s.map_files(&|f| {
            f.strip_prefix("/home/anna/shop/")
                .map(|r| format!("/opt/project/{r}"))
        });
        assert!(s
            .functions
            .iter()
            .any(|f| f.file.starts_with("/opt/project/")));
        let v = s.view.unwrap();
        assert!(v.top.iter().any(|r| r.file.starts_with("/opt/project/")));
    }
}

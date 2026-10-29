//! Source maps (revision 3) for V8 profiles of bundled or compiled scripts: the map of a
//! script (`//# sourceMappingURL=` comment, inline `data:` URL or `<script>.map`), the
//! Base64 VLQ `mappings`, and the original position of a generated one.

use base64::Engine;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMap {
    #[serde(default)]
    sources: Vec<Option<String>>,
    #[serde(default)]
    source_root: Option<String>,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    mappings: String,
}

/// A segment: generated column → source, original line and column (0-based), name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    column: u32,
    source: u32,
    line: u32,
    original_column: u32,
    name: Option<u32>,
}

/// A decoded source map.
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Source paths, resolved against the map's folder and `sourceRoot`.
    pub sources: Vec<String>,
    pub names: Vec<String>,
    /// Segments per generated line, sorted by column.
    lines: Vec<Vec<Segment>>,
}

/// An original position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Original {
    pub source: String,
    /// 0-based line and column.
    pub line: u32,
    pub column: u32,
    pub name: Option<String>,
}

fn vlq_digit(c: u8) -> Option<i64> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as i64)
}

/// Decode a segment's VLQ values.
fn vlq(segment: &str) -> Option<Vec<i64>> {
    let mut out = Vec::new();
    let (mut value, mut shift) = (0i64, 0u32);
    for c in segment.bytes() {
        let d = vlq_digit(c)?;
        value += (d & 31) << shift;
        if d & 32 != 0 {
            shift += 5;
            if shift > 60 {
                return None;
            }
        } else {
            out.push(if value & 1 == 1 {
                -(value >> 1)
            } else {
                value >> 1
            });
            value = 0;
            shift = 0;
        }
    }
    Some(out)
}

/// `a/b/../c` → `a/c` (keeps a leading `/`).
pub fn normalize(path: &str) -> String {
    let abs = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !abs {
                    parts.push("..");
                }
            }
            _ => parts.push(p),
        }
    }
    let joined = parts.join("/");
    if abs {
        format!("/{joined}")
    } else {
        joined
    }
}

fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// A source's path: `file://` URLs and absolute paths as they are, bundler schemes
/// (`webpack://app/./src/x.ts`) and relative paths against the map's folder.
fn resolve_source(map_path: &str, root: &str, source: &str) -> String {
    if let Some(p) = source.strip_prefix("file://") {
        return normalize(&crate::url_path(p));
    }
    let source = match source.find("://") {
        // webpack://<package>/./src/x.ts → ./src/x.ts
        Some(i) => {
            let rest = &source[i + 3..];
            match rest.find('/') {
                Some(j) => &rest[j + 1..],
                None => rest,
            }
        }
        None => source,
    };
    let joined = if root.is_empty() {
        source.to_string()
    } else {
        format!("{}/{}", root.trim_end_matches('/'), source)
    };
    if joined.starts_with('/') {
        normalize(&joined)
    } else {
        normalize(&format!("{}/{}", dirname(map_path), joined))
    }
}

impl SourceMap {
    /// Parse a map read from `map_path` (sources are resolved against its folder).
    pub fn parse(json: &str, map_path: &str) -> Result<SourceMap, String> {
        let raw: RawMap = serde_json::from_str(json).map_err(|e| format!("bad source map: {e}"))?;
        let root = raw.source_root.unwrap_or_default();
        let sources = raw
            .sources
            .iter()
            .map(|s| resolve_source(map_path, &root, s.as_deref().unwrap_or("")))
            .collect();
        let mut lines = Vec::new();
        let (mut source, mut line, mut column, mut name) = (0i64, 0i64, 0i64, 0i64);
        for l in raw.mappings.split(';') {
            let mut segments = Vec::new();
            let mut gen_col = 0i64;
            for seg in l.split(',').filter(|s| !s.is_empty()) {
                let Some(v) = vlq(seg) else { continue };
                gen_col += v[0];
                if v.len() >= 4 {
                    source += v[1];
                    line += v[2];
                    column += v[3];
                    let n = if v.len() >= 5 {
                        name += v[4];
                        Some(name.max(0) as u32)
                    } else {
                        None
                    };
                    segments.push(Segment {
                        column: gen_col.max(0) as u32,
                        source: source.max(0) as u32,
                        line: line.max(0) as u32,
                        original_column: column.max(0) as u32,
                        name: n,
                    });
                }
            }
            segments.sort_by_key(|s| s.column);
            lines.push(segments);
        }
        Ok(SourceMap {
            sources,
            names: raw.names,
            lines,
        })
    }

    /// The original position of a generated one (0-based line and column).
    pub fn lookup(&self, line: u32, column: u32) -> Option<Original> {
        let segments = self.lines.get(line as usize)?;
        let seg = match segments.partition_point(|s| s.column <= column) {
            0 => segments.first()?,
            i => &segments[i - 1],
        };
        Some(Original {
            source: self.sources.get(seg.source as usize)?.clone(),
            line: seg.line,
            column: seg.original_column,
            name: seg.name.and_then(|n| self.names.get(n as usize)).cloned(),
        })
    }
}

/// The source map of a script: its `sourceMappingURL` (an inline `data:` URL or a file
/// next to it), else `<script>.map`. `read` reads a local file.
pub fn for_script(script: &str, read: &dyn Fn(&str) -> Option<String>) -> Option<SourceMap> {
    let text = read(script);
    if let Some(text) = &text {
        let url = text
            .lines()
            .rev()
            .take(5)
            .find_map(|l| {
                l.trim()
                    .strip_prefix("//# sourceMappingURL=")
                    .or_else(|| l.trim().strip_prefix("//@ sourceMappingURL="))
            })
            .map(str::trim);
        if let Some(url) = url {
            if let Some(data) = url.strip_prefix("data:") {
                let b64 = data.split_once(";base64,").map(|(_, b)| b)?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
                let json = String::from_utf8(bytes).ok()?;
                return SourceMap::parse(&json, script).ok();
            }
            let path = if url.starts_with('/') {
                url.to_string()
            } else if let Some(p) = url.strip_prefix("file://") {
                crate::url_path(p)
            } else {
                normalize(&format!("{}/{}", dirname(script), url))
            };
            if let Some(json) = read(&path) {
                return SourceMap::parse(&json, &path).ok();
            }
        }
    }
    let path = format!("{script}.map");
    read(&path).and_then(|json| SourceMap::parse(&json, &path).ok())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Encode VLQ values (for test maps).
    pub fn encode(values: &[i64]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for v in values {
            let mut n = if *v < 0 { ((-v) << 1) | 1 } else { v << 1 };
            loop {
                let mut d = n & 31;
                n >>= 5;
                if n > 0 {
                    d |= 32;
                }
                out.push(CHARS[d as usize] as char);
                if n == 0 {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn decodes_vlq_and_looks_up_positions() {
        assert_eq!(vlq("AAgBC").unwrap(), vec![0, 0, 16, 1]);
        assert_eq!(
            vlq(&encode(&[-17, 123_456, 0])).unwrap(),
            vec![-17, 123_456, 0]
        );
        // line 0: col 0 → a.ts 0:0, col 10 → a.ts 4:2 "render"; line 1: col 4 → b.ts 9:0
        let mappings = format!(
            "{},{};{}",
            encode(&[0, 0, 0, 0]),
            encode(&[10, 0, 4, 2, 0]),
            encode(&[4, 1, 5, -2])
        );
        let json = format!(
            r#"{{"version":3,"sources":["../src/a.ts","webpack://app/./src/b.ts"],"names":["render"],"mappings":"{mappings}"}}"#
        );
        let map = SourceMap::parse(&json, "/p/dist/app.js.map").unwrap();
        assert_eq!(map.sources, ["/p/src/a.ts", "/p/dist/src/b.ts"]);
        let o = map.lookup(0, 12).unwrap();
        assert_eq!((o.source.as_str(), o.line, o.column), ("/p/src/a.ts", 4, 2));
        assert_eq!(o.name.as_deref(), Some("render"));
        assert_eq!(map.lookup(0, 3).unwrap().line, 0);
        let b = map.lookup(1, 4).unwrap();
        assert_eq!(
            (b.source.as_str(), b.line, b.column),
            ("/p/dist/src/b.ts", 9, 0)
        );
        assert!(map.lookup(7, 0).is_none());
    }

    #[test]
    fn finds_the_map_of_a_script() {
        let map = r#"{"version":3,"sources":["a.ts"],"names":[],"mappings":"AAAA"}"#;
        let files = |p: &str| -> Option<String> {
            match p {
                "/p/dist/app.js" => Some("x();\n//# sourceMappingURL=maps/app.js.map\n".into()),
                "/p/dist/maps/app.js.map" => Some(map.into()),
                "/p/dist/other.js" => Some("y();\n".into()),
                "/p/dist/other.js.map" => Some(map.into()),
                "/p/dist/inline.js" => Some(format!(
                    "z();\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,{}\n",
                    base64::engine::general_purpose::STANDARD.encode(map)
                )),
                _ => None,
            }
        };
        assert_eq!(
            for_script("/p/dist/app.js", &files).unwrap().sources,
            ["/p/dist/maps/a.ts"]
        );
        assert_eq!(
            for_script("/p/dist/other.js", &files).unwrap().sources,
            ["/p/dist/a.ts"]
        );
        assert_eq!(
            for_script("/p/dist/inline.js", &files).unwrap().sources,
            ["/p/dist/a.ts"]
        );
        assert!(for_script("/p/dist/none.js", &files).is_none());
        assert_eq!(normalize("/a/b/../c/./d"), "/a/c/d");
    }
}

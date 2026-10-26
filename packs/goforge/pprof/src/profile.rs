//! `profile.proto` (github.com/google/pprof): sample types, samples with their call stacks
//! (location ids, leaf first), locations with their (inlined) lines, functions and the
//! string table. Files are usually gzip compressed.

use std::collections::HashMap;
use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::proto::{push_numbers, Reader, Value};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueType {
    /// `cpu`, `samples`, `alloc_space`, `inuse_objects`, `contentions`, `delay`, …
    pub kind: String,
    /// `nanoseconds`, `count`, `bytes`.
    pub unit: String,
}

/// A frame of a call stack: a function and the line it was at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub function: String,
    pub file: String,
    /// 1-based line in `file` (0: unknown).
    pub line: u32,
    /// The function's first line.
    pub start_line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sample {
    /// Indexes into [`Profile::frames`], leaf first.
    pub stack: Vec<usize>,
    pub values: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub sample_types: Vec<ValueType>,
    /// Index of the sample type shown first.
    pub default_sample_type: usize,
    pub samples: Vec<Sample>,
    pub frames: Vec<Frame>,
    pub period_type: Option<ValueType>,
    pub period: i64,
    pub time_nanos: i64,
    pub duration_nanos: i64,
}

#[derive(Default)]
struct RawLocation {
    /// (function id, line), innermost (inlined callee) first.
    lines: Vec<(u64, i64)>,
    address: u64,
}

#[derive(Default)]
struct RawFunction {
    name: i64,
    system_name: i64,
    filename: i64,
    start_line: i64,
}

fn value_type(b: &[u8]) -> Result<(i64, i64), String> {
    let mut r = Reader::new(b);
    let (mut t, mut u) = (0, 0);
    while let Some((f, v)) = r.next_field()? {
        match f {
            1 => t = v.as_i64(),
            2 => u = v.as_i64(),
            _ => {}
        }
    }
    Ok((t, u))
}

fn bytes<'a>(v: Value<'a>) -> Result<&'a [u8], String> {
    match v {
        Value::Bytes(b) => Ok(b),
        _ => Err("expected a message".into()),
    }
}

/// Whether the data is gzip compressed.
pub fn is_gzip(data: &[u8]) -> bool {
    data.starts_with(&[0x1f, 0x8b])
}

/// Decode a profile (gzip compressed or not).
pub fn decode(data: &[u8]) -> Result<Profile, String> {
    let raw;
    let data = if is_gzip(data) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(data)
            .read_to_end(&mut out)
            .map_err(|e| format!("not a pprof profile: broken gzip data ({e})"))?;
        raw = out;
        &raw[..]
    } else {
        data
    };
    decode_message(data).map_err(|e| format!("not a pprof profile: {e}"))
}

fn decode_message(data: &[u8]) -> Result<Profile, String> {
    let mut r = Reader::new(data);
    let mut strings: Vec<String> = Vec::new();
    let mut sample_types: Vec<(i64, i64)> = Vec::new();
    let mut samples: Vec<(Vec<u64>, Vec<i64>)> = Vec::new();
    let mut locations: HashMap<u64, RawLocation> = HashMap::new();
    let mut functions: HashMap<u64, RawFunction> = HashMap::new();
    let mut period_type = None;
    let mut p = Profile::default();
    let mut default_type = 0i64;
    while let Some((field, v)) = r.next_field()? {
        match field {
            1 => sample_types.push(value_type(bytes(v)?)?),
            2 => {
                let mut sr = Reader::new(bytes(v)?);
                let (mut locs, mut vals) = (Vec::new(), Vec::new());
                while let Some((f, v)) = sr.next_field()? {
                    match f {
                        1 => push_numbers(v, &mut locs)?,
                        2 => push_numbers(v, &mut vals)?,
                        _ => {}
                    }
                }
                samples.push((locs, vals.into_iter().map(|v| v as i64).collect()));
            }
            4 => {
                let mut lr = Reader::new(bytes(v)?);
                let mut id = 0;
                let mut loc = RawLocation::default();
                while let Some((f, v)) = lr.next_field()? {
                    match f {
                        1 => id = v.as_u64(),
                        3 => loc.address = v.as_u64(),
                        4 => {
                            let mut line = Reader::new(bytes(v)?);
                            let (mut func, mut n) = (0, 0);
                            while let Some((f, v)) = line.next_field()? {
                                match f {
                                    1 => func = v.as_u64(),
                                    2 => n = v.as_i64(),
                                    _ => {}
                                }
                            }
                            loc.lines.push((func, n));
                        }
                        _ => {}
                    }
                }
                locations.insert(id, loc);
            }
            5 => {
                let mut fr = Reader::new(bytes(v)?);
                let mut id = 0;
                let mut func = RawFunction::default();
                while let Some((f, v)) = fr.next_field()? {
                    match f {
                        1 => id = v.as_u64(),
                        2 => func.name = v.as_i64(),
                        3 => func.system_name = v.as_i64(),
                        4 => func.filename = v.as_i64(),
                        5 => func.start_line = v.as_i64(),
                        _ => {}
                    }
                }
                functions.insert(id, func);
            }
            6 => strings.push(String::from_utf8_lossy(bytes(v)?).into_owned()),
            9 => p.time_nanos = v.as_i64(),
            10 => p.duration_nanos = v.as_i64(),
            11 => period_type = Some(value_type(bytes(v)?)?),
            12 => p.period = v.as_i64(),
            14 => default_type = v.as_i64(),
            _ => {}
        }
    }
    if sample_types.is_empty() {
        return Err("no sample types".into());
    }
    let s = |i: i64| -> String {
        usize::try_from(i)
            .ok()
            .and_then(|i| strings.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let vt = |(t, u): (i64, i64)| ValueType {
        kind: s(t),
        unit: s(u),
    };
    p.sample_types = sample_types.iter().copied().map(vt).collect();
    p.period_type = period_type.map(vt);
    if default_type != 0 {
        let name = s(default_type);
        p.default_sample_type = p
            .sample_types
            .iter()
            .position(|t| t.kind == name)
            .unwrap_or(0);
    } else {
        // pprof shows the last sample type by default (`cpu` after `samples`).
        p.default_sample_type = p.sample_types.len() - 1;
    }
    // Frames, interned.
    let mut index: HashMap<Frame, usize> = HashMap::new();
    let mut intern = |frame: Frame, frames: &mut Vec<Frame>| -> usize {
        *index.entry(frame.clone()).or_insert_with(|| {
            frames.push(frame);
            frames.len() - 1
        })
    };
    let mut frames = Vec::new();
    let mut loc_frames: HashMap<u64, Vec<usize>> = HashMap::new();
    for (id, loc) in &locations {
        let mut out = Vec::new();
        if loc.lines.is_empty() {
            out.push(intern(
                Frame {
                    function: format!("0x{:x}", loc.address),
                    ..Frame::default()
                },
                &mut frames,
            ));
        }
        for (func, line) in &loc.lines {
            let f = functions.get(func);
            let name = f
                .map(|f| match s(f.name) {
                    n if n.is_empty() => s(f.system_name),
                    n => n,
                })
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("0x{:x}", loc.address));
            out.push(intern(
                Frame {
                    function: name,
                    file: f.map(|f| s(f.filename)).unwrap_or_default(),
                    line: u32::try_from(*line).unwrap_or(0),
                    start_line: f
                        .and_then(|f| u32::try_from(f.start_line).ok())
                        .unwrap_or(0),
                },
                &mut frames,
            ));
        }
        loc_frames.insert(*id, out);
    }
    p.samples = samples
        .into_iter()
        .map(|(locs, values)| Sample {
            stack: locs
                .iter()
                .flat_map(|l| loc_frames.get(l).cloned().unwrap_or_default())
                .collect(),
            values,
        })
        .collect();
    p.frames = frames;
    Ok(p)
}

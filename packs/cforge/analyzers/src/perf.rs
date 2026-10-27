//! Linux perf: the `perf record -g` command line of "Profile with perf", and `perf script`
//! output (one call stack per sample, leaf first, with `srcline` lines) turned into a
//! [`goforge_pprof::Profile`], so the shared Profiler shows the same call tree, method list
//! and flame graph as for pprof profiles.

use std::collections::HashMap;

use goforge_pprof::{Frame, Profile, ProfileView, Sample, ValueType, ViewOptions};

/// `perf record` options of a profiling run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordOptions {
    /// Sampling frequency in Hz (`-F`); 0: perf's default.
    pub frequency: u32,
    /// `--call-graph` mode: `fp` (frame pointers, perf's `-g`), `dwarf` or `lbr`.
    pub call_graph: String,
    /// Extra arguments for `perf record`.
    pub extra: Vec<String>,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            frequency: 999,
            call_graph: "fp".into(),
            extra: Vec::new(),
        }
    }
}

/// `perf record -g -o <data> [-F n] [--call-graph mode] -- <program> <args…>` (arguments for
/// `perf`).
pub fn record_args(
    data_file: &str,
    options: &RecordOptions,
    program: &str,
    args: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = vec!["record".into(), "-o".into(), data_file.into()];
    match options.call_graph.trim() {
        "" | "fp" => out.push("-g".into()),
        mode => {
            out.push("--call-graph".into());
            out.push(mode.into());
        }
    }
    if options.frequency > 0 {
        out.push("-F".into());
        out.push(options.frequency.to_string());
    }
    out.extend(options.extra.iter().cloned());
    out.push("--".into());
    out.push(program.into());
    out.extend(args.iter().cloned());
    out
}

/// `perf script -i <data> -F …` with source lines (arguments for `perf`).
pub fn script_args(data_file: &str) -> Vec<String> {
    vec![
        "script".into(),
        "-i".into(),
        data_file.into(),
        "-F".into(),
        "comm,pid,tid,time,period,event,ip,sym,dso,srcline".into(),
    ]
}

/// A sample being read: its weight and frames (function, file, line), leaf first.
type Pending = Option<(i64, Vec<(String, String, u32)>)>;

/// The header of a sample: `comm pid/tid time: period event:`.
struct Header {
    time: Option<f64>,
    period: Option<i64>,
    event: String,
}

fn header(line: &str) -> Option<Header> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let ev = tokens.iter().rposition(|t| t.ends_with(':'))?;
    let event = tokens[ev].trim_end_matches(':');
    let event = event.split(':').next().unwrap_or(event).to_string();
    let period = ev
        .checked_sub(1)
        .and_then(|i| tokens[i].parse::<i64>().ok());
    let time = tokens[..ev]
        .iter()
        .rev()
        .find(|t| t.ends_with(':'))
        .and_then(|t| t.trim_end_matches(':').parse::<f64>().ok());
    Some(Header {
        time,
        period,
        event,
    })
}

/// A frame line: `55d0c0a01a2e Polygon::area() const+0x2e (/path/shapes_app)`.
fn frame_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let (addr, rest) = t.split_once(' ')?;
    if addr.is_empty() || !addr.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let rest = rest.trim();
    let (sym, dso) = match rest.rfind(" (") {
        Some(i) if rest.ends_with(')') => (&rest[..i], &rest[i + 2..rest.len() - 1]),
        _ => (rest, ""),
    };
    let sym = match sym.rfind("+0x") {
        Some(i) if sym[i + 3..].bytes().all(|b| b.is_ascii_hexdigit()) => &sym[..i],
        _ => sym,
    };
    let dso_name = dso.rsplit('/').next().unwrap_or(dso);
    let function = match sym.trim() {
        "" | "[unknown]" if dso_name.starts_with('[') => dso_name.to_string(),
        "" | "[unknown]" => format!("[unknown] ({dso_name})"),
        s => s.to_string(),
    };
    Some((function, dso.to_string()))
}

/// A `srcline` line: `  /path/file.cpp:17`.
fn srcline(line: &str) -> Option<(String, u32)> {
    let t = line.trim();
    if t.contains(' ') {
        return None;
    }
    let (file, line) = t.rsplit_once(':')?;
    let line: u32 = line.parse().ok()?;
    (file != "??" && !file.is_empty() && line > 0).then(|| (file.to_string(), line))
}

/// Parse `perf script` output into a profile (sample types `samples` and the event's).
pub fn parse(text: &str) -> Result<Profile, String> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut index: HashMap<(String, String, u32), usize> = HashMap::new();
    let mut samples: Vec<Sample> = Vec::new();
    let mut event = String::new();
    let mut period = 0i64;
    let (mut first, mut last) = (None::<f64>, None::<f64>);
    let mut cur: Pending = None;
    let mut flush = |cur: &mut Pending, frames: &mut Vec<Frame>, samples: &mut Vec<Sample>| {
        let Some((weight, stack)) = cur.take() else {
            return;
        };
        if stack.is_empty() {
            return;
        }
        let ids = stack
            .into_iter()
            .map(|key| {
                *index.entry(key.clone()).or_insert_with(|| {
                    frames.push(Frame {
                        function: key.0.clone(),
                        file: key.1.clone(),
                        line: key.2,
                        start_line: key.2,
                    });
                    frames.len() - 1
                })
            })
            .collect();
        samples.push(Sample {
            stack: ids,
            values: vec![1, weight],
        });
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            flush(&mut cur, &mut frames, &mut samples);
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if !indented {
            flush(&mut cur, &mut frames, &mut samples);
            let Some(h) = header(line) else { continue };
            if event.is_empty() {
                event = h.event.clone();
            }
            if h.event != event {
                // One event per profile (the first one recorded).
                continue;
            }
            if let Some(t) = h.time {
                first.get_or_insert(t);
                last = Some(t);
            }
            let w = h.period.unwrap_or(1).max(1);
            if period == 0 {
                period = w;
            }
            cur = Some((w, Vec::new()));
            continue;
        }
        let Some((_, stack)) = cur.as_mut() else {
            continue;
        };
        if let Some((function, _dso)) = frame_line(line) {
            stack.push((function, String::new(), 0));
        } else if let Some((file, l)) = srcline(line) {
            if let Some(top) = stack.last_mut() {
                if top.1.is_empty() {
                    top.1 = file;
                    top.2 = l;
                }
            }
        }
    }
    flush(&mut cur, &mut frames, &mut samples);
    if samples.is_empty() {
        return Err("perf recorded no samples".into());
    }
    let (kind, unit) = match event.as_str() {
        "cpu-clock" | "task-clock" => ("cpu".to_string(), "nanoseconds".to_string()),
        e => (e.to_string(), "count".to_string()),
    };
    // Frames of one function on different lines are separate frames; the views merge them
    // by function and file. Keep the first line as the function's line.
    let mut first_line: HashMap<(String, String), u32> = HashMap::new();
    for f in frames.iter_mut() {
        let l = *first_line
            .entry((f.function.clone(), f.file.clone()))
            .or_insert(f.line);
        f.start_line = l;
    }
    Ok(Profile {
        sample_types: vec![
            ValueType {
                kind: "samples".into(),
                unit: "count".into(),
            },
            ValueType {
                kind: kind.clone(),
                unit: unit.clone(),
            },
        ],
        default_sample_type: 1,
        samples,
        frames,
        period_type: Some(ValueType { kind, unit }),
        period,
        time_nanos: 0,
        duration_nanos: match (first, last) {
            (Some(a), Some(b)) if b > a => ((b - a) * 1e9) as i64,
            _ => 0,
        },
    })
}

/// `perf script` output → the Profiler's views.
pub fn open(text: &str, sample_index: Option<usize>) -> Result<ProfileView, String> {
    Ok(goforge_pprof::view(
        &parse(text)?,
        sample_index,
        ViewOptions::default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = include_str!("../tests/fixtures/perf-script.txt");

    #[test]
    fn builds_the_call_tree_from_perf_script() {
        let p = parse(SCRIPT).unwrap();
        assert_eq!(p.samples.len(), 142);
        assert_eq!(p.sample_types[1].kind, "cpu");
        assert_eq!(p.period, 250_000);
        assert!(p.duration_nanos > 30_000_000);
        let v = open(SCRIPT, None).unwrap();
        assert_eq!(v.total, 142 * 250_000);
        // root → _start → __libc_start_main → __libc_start_call_main → main.
        let start = &v.tree.children[0];
        assert_eq!(start.name, "_start");
        assert_eq!(start.total, 142 * 250_000);
        let main = &start.children[0].children[0].children[0];
        assert_eq!(main.name, "main");
        assert_eq!(main.line, 31);
        let kernel = v
            .top
            .iter()
            .find(|r| r.name == "[kernel.kallsyms]")
            .unwrap();
        assert_eq!(kernel.flat, 2 * 250_000);
        let area = v
            .top
            .iter()
            .find(|r| r.name == "Polygon::area() const")
            .unwrap();
        assert_eq!(area.flat, 41 * 250_000);
        assert_eq!(area.cum, 64 * 250_000);
        assert_eq!(
            area.file,
            "/home/anna/Projects/shapes/src/geometry/polygon.cpp"
        );
        assert_eq!(area.line, 17);
        let hypot = v
            .top
            .iter()
            .find(|r| r.name.starts_with("__hypot_finite"))
            .unwrap();
        assert_eq!(hypot.file, "");
        assert_eq!(v.top[0].name, "Polygon::area() const");
        // The samples count type.
        let counts = open(SCRIPT, Some(0)).unwrap();
        assert_eq!(counts.total, 142);
    }

    #[test]
    fn other_events_and_headers() {
        let text = "app 1/1 10.0: 1200 cycles:u: \n\t    401000 work+0x10 (/tmp/app)\n\t    401100 main+0x20 (/tmp/app)\n\napp 1/1 10.5: 800 cycles:u: \n\t    401100 main+0x20 (/tmp/app)\n";
        let v = open(text, None).unwrap();
        assert_eq!(v.sample_types[1].kind, "cycles");
        assert_eq!(v.sample_types[1].unit, "count");
        assert_eq!(v.total, 2000);
        assert_eq!(v.tree.children[0].name, "main");
        assert_eq!(v.tree.children[0].self_value, 800);
        assert!(parse("").is_err());
        assert!(parse("just text\n").is_err());
    }

    #[test]
    fn command_lines() {
        let args = record_args(
            "/tmp/p.data",
            &RecordOptions::default(),
            "/b/app",
            &["-v".into()],
        );
        assert_eq!(
            args.join(" "),
            "record -o /tmp/p.data -g -F 999 -- /b/app -v"
        );
        let dwarf = RecordOptions {
            frequency: 0,
            call_graph: "dwarf".into(),
            extra: vec![],
        };
        assert_eq!(
            record_args("p", &dwarf, "a", &[]).join(" "),
            "record -o p --call-graph dwarf -- a"
        );
        assert_eq!(script_args("p")[..3], ["script", "-i", "p"]);
    }
}

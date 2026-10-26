//! Writing profiles: [`ProfileBuilder`] encodes samples with named call stacks as
//! `profile.proto` (optionally gzip compressed). Tests use it, and browser mode shows the
//! profile of the demo project ([`demo_profile`]).

use std::collections::HashMap;
use std::io::Write;

use crate::proto::write;

/// A function of a stack: name, file and line.
pub type Site<'a> = (&'a str, &'a str, u32);

#[derive(Default)]
pub struct ProfileBuilder {
    strings: Vec<String>,
    string_index: HashMap<String, u64>,
    sample_types: Vec<(u64, u64)>,
    period_type: Option<(u64, u64)>,
    period: i64,
    duration_nanos: i64,
    functions: Vec<(u64, u64, u32)>,
    function_index: HashMap<(String, String), u64>,
    locations: Vec<(u64, u32)>,
    location_index: HashMap<(u64, u32), u64>,
    samples: Vec<(Vec<u64>, Vec<i64>)>,
}

impl ProfileBuilder {
    pub fn new(sample_types: &[(&str, &str)]) -> Self {
        let mut b = ProfileBuilder::default();
        b.string("");
        b.sample_types = sample_types
            .iter()
            .map(|(t, u)| (b.string(t), b.string(u)))
            .collect();
        b
    }

    pub fn period(mut self, kind: &str, unit: &str, period: i64) -> Self {
        self.period_type = Some((self.string(kind), self.string(unit)));
        self.period = period;
        self
    }

    pub fn duration(mut self, nanos: i64) -> Self {
        self.duration_nanos = nanos;
        self
    }

    fn string(&mut self, s: &str) -> u64 {
        if let Some(i) = self.string_index.get(s) {
            return *i;
        }
        self.strings.push(s.to_string());
        let i = (self.strings.len() - 1) as u64;
        self.string_index.insert(s.to_string(), i);
        i
    }

    fn location(&mut self, (name, file, line): Site<'_>) -> u64 {
        let key = (name.to_string(), file.to_string());
        let func = match self.function_index.get(&key) {
            Some(f) => *f,
            None => {
                let n = self.string(name);
                let f = self.string(file);
                self.functions.push((n, f, line));
                let id = self.functions.len() as u64;
                self.function_index.insert(key, id);
                id
            }
        };
        *self.location_index.entry((func, line)).or_insert_with(|| {
            self.locations.push((func, line));
            self.locations.len() as u64
        })
    }

    /// Add a sample: its stack root first, and one value per sample type.
    pub fn sample(&mut self, stack_root_first: &[Site<'_>], values: &[i64]) -> &mut Self {
        let mut ids: Vec<u64> = stack_root_first.iter().map(|s| self.location(*s)).collect();
        ids.reverse();
        self.samples.push((ids, values.to_vec()));
        self
    }

    /// The `profile.proto` message.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let value_type = |t: u64, u: u64| {
            let mut m = Vec::new();
            write::uint(&mut m, 1, t);
            write::uint(&mut m, 2, u);
            m
        };
        for (t, u) in &self.sample_types {
            write::bytes(&mut out, 1, &value_type(*t, *u));
        }
        for (locs, values) in &self.samples {
            let mut m = Vec::new();
            write::packed(&mut m, 1, locs);
            write::packed(&mut m, 2, &values.iter().map(|v| *v as u64).collect::<Vec<_>>());
            write::bytes(&mut out, 2, &m);
        }
        for (i, (func, line)) in self.locations.iter().enumerate() {
            let mut m = Vec::new();
            write::uint(&mut m, 1, i as u64 + 1);
            write::uint(&mut m, 3, 0x40_0000 + (i as u64) * 0x10);
            let mut l = Vec::new();
            write::uint(&mut l, 1, *func);
            write::uint(&mut l, 2, u64::from(*line));
            write::bytes(&mut m, 4, &l);
            write::bytes(&mut out, 4, &m);
        }
        for (i, (name, file, line)) in self.functions.iter().enumerate() {
            let mut m = Vec::new();
            write::uint(&mut m, 1, i as u64 + 1);
            write::uint(&mut m, 2, *name);
            write::uint(&mut m, 3, *name);
            write::uint(&mut m, 4, *file);
            write::uint(&mut m, 5, u64::from(*line));
            write::bytes(&mut out, 5, &m);
        }
        for s in &self.strings {
            write::bytes(&mut out, 6, s.as_bytes());
        }
        write::uint(&mut out, 10, self.duration_nanos as u64);
        if let Some((t, u)) = self.period_type {
            write::bytes(&mut out, 11, &value_type(t, u));
        }
        write::uint(&mut out, 12, self.period as u64);
        out
    }

    /// The message, gzip compressed like `go test -cpuprofile` writes it.
    pub fn encode_gzip(&self) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&self.encode()).expect("in-memory write");
        gz.finish().expect("in-memory write")
    }
}

/// A CPU profile of the browser-mode demo project (`ledger`, `BenchmarkReport`): 10 ms
/// samples over two seconds.
pub fn demo_profile() -> Vec<u8> {
    const M: &str = "internal/money/money.go";
    const R: &str = "internal/report/report.go";
    const RT: &str = "/usr/local/go/src/runtime/proc.go";
    const FMT: &str = "/usr/local/go/src/fmt/print.go";
    const STR: &str = "/usr/local/go/src/strconv/itoa.go";
    const MAL: &str = "/usr/local/go/src/runtime/malloc.go";
    const TST: &str = "/usr/local/go/src/testing/benchmark.go";
    let root: [Site<'_>; 3] = [
        ("runtime.goexit", "/usr/local/go/src/runtime/asm_amd64.s", 1700),
        ("testing.(*B).launch", TST, 300),
        ("testing.(*B).runN", TST, 190),
    ];
    let bench = ("github.com/acme/ledger/internal/report.BenchmarkReport", "internal/report/report_test.go", 41);
    let render = ("github.com/acme/ledger/internal/report.Render", R, 28);
    let line = ("github.com/acme/ledger/internal/report.formatLine", R, 55);
    let string = ("github.com/acme/ledger/internal/money.Amount.String", M, 33);
    let total = ("github.com/acme/ledger/internal/report.Total", R, 17);
    let round = ("github.com/acme/ledger/internal/money.Round", M, 28);
    let parse = ("github.com/acme/ledger/internal/money.Parse", M, 14);
    let sprintf = ("fmt.Sprintf", FMT, 237);
    let doprintf = ("fmt.(*pp).doPrintf", FMT, 1021);
    let itoa = ("strconv.FormatInt", STR, 26);
    let alloc = ("runtime.mallocgc", MAL, 1009);
    let gc = ("runtime.gcBgMarkWorker", "/usr/local/go/src/runtime/mgc.go", 1363);
    let sched = ("runtime.schedule", RT, 3980);
    let mut b = ProfileBuilder::new(&[("samples", "count"), ("cpu", "nanoseconds")])
        .period("cpu", "nanoseconds", 10_000_000)
        .duration(2_040_000_000);
    let ms = 10_000_000;
    let mut add = |stack: &[Site<'_>], n: i64| {
        let mut full: Vec<Site<'_>> = root.to_vec();
        full.push(bench);
        full.extend_from_slice(stack);
        b.sample(&full, &[n, n * ms]);
    };
    add(&[render, line, string, sprintf, doprintf], 46);
    add(&[render, line, string, sprintf, alloc], 18);
    add(&[render, line, string, itoa], 21);
    add(&[render, line, sprintf, doprintf], 25);
    add(&[render, line], 12);
    add(&[render, total, round], 14);
    add(&[render, total, parse, itoa], 9);
    add(&[render, total], 6);
    add(&[render, alloc], 11);
    add(&[render], 4);
    b.sample(&[gc, alloc], &[13, 13 * ms]);
    b.sample(&[sched], &[7, 7 * ms]);
    b.encode_gzip()
}

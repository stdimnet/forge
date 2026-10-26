//! GoForge's profiler data: pprof profiles (`go test -cpuprofile`, `-memprofile`,
//! `-blockprofile`, `-mutexprofile`, `net/http/pprof`, `runtime/pprof`) decoded without a
//! protobuf library, and the views of the Profiler tool window.
//!
//! - [`proto`]: the protobuf wire format (reading, and writing for [`builder`]).
//! - [`profile`]: `profile.proto` → [`Profile`] (gzip or plain).
//! - [`view`]: top functions, call tree and flame graph ([`ProfileView`]).
//! - [`builder`]: [`ProfileBuilder`] for tests and the browser-mode demo profile.

pub mod builder;
pub mod profile;
pub mod proto;
pub mod view;

pub use builder::{demo_profile, ProfileBuilder};
pub use profile::{decode, Frame, Profile, Sample, ValueType};
pub use view::{view, CallNode, ProfileView, TopRow, ViewOptions};

/// Decode a profile and compute its views in one go.
pub fn open(data: &[u8], sample_index: Option<usize>) -> Result<ProfileView, String> {
    Ok(view(&decode(data)?, sample_index, ViewOptions::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPU: &[u8] = include_bytes!("../tests/fixtures/cpu.pprof");
    const MEM: &[u8] = include_bytes!("../tests/fixtures/mem.pprof");

    #[test]
    fn reads_a_cpu_profile_written_by_go_test() {
        let p = decode(CPU).unwrap();
        let types: Vec<_> = p.sample_types.iter().map(|t| t.kind.as_str()).collect();
        assert_eq!(types, ["samples", "cpu"]);
        assert_eq!(p.sample_types[1].unit, "nanoseconds");
        assert_eq!(p.default_sample_type, 1);
        assert_eq!(p.period, 10_000_000);
        assert!(p.duration_nanos > 0);
        let v = view(&p, None, ViewOptions::default());
        assert_eq!(v.total, p.samples.iter().map(|s| s.values[1]).sum::<i64>());
        assert!(v.total > 0);
        assert_eq!(v.tree.total, v.total);
        // The benchmark's own code is in the profile, with its source file.
        let string = v
            .top
            .iter()
            .find(|r| r.name.ends_with("money.Amount.String"))
            .expect("Amount.String in the top list");
        assert!(string.file.ends_with("internal/money/money.go"));
        assert!(string.cum >= string.flat && string.cum > 0);
        // Flat values add up to the total; cumulative values never exceed it.
        assert_eq!(v.top.iter().map(|r| r.flat).sum::<i64>(), v.total);
        assert!(v.top.iter().all(|r| r.cum <= v.total));
    }

    #[test]
    fn reads_a_memory_profile() {
        let p = decode(MEM).unwrap();
        let types: Vec<_> = p.sample_types.iter().map(|t| t.kind.as_str()).collect();
        assert_eq!(
            types,
            [
                "alloc_objects",
                "alloc_space",
                "inuse_objects",
                "inuse_space"
            ]
        );
        assert_eq!(p.sample_types[1].unit, "bytes");
        let v = view(&p, Some(1), ViewOptions::default());
        assert_eq!(v.sample_index, 1);
        assert!(v.total > 0);
    }

    #[test]
    fn builds_views_from_a_known_profile() {
        let data = demo_profile();
        let v = open(&data, None).unwrap();
        assert_eq!(v.sample_types[v.sample_index].kind, "cpu");
        assert_eq!(v.total, 186 * 10_000_000);
        assert_eq!(v.duration_nanos, 2_040_000_000);
        let top = &v.top[0];
        assert_eq!(top.name, "fmt.(*pp).doPrintf");
        assert_eq!(top.flat, 71 * 10_000_000);
        let render = v
            .top
            .iter()
            .find(|r| r.name.ends_with("report.Render"))
            .unwrap();
        assert_eq!(render.cum, 166 * 10_000_000);
        assert_eq!(render.flat, 4 * 10_000_000);
        assert_eq!(
            (render.file.as_str(), render.line),
            ("internal/report/report.go", 28)
        );
        // root → goexit (166 + gc's and the scheduler's own stacks are separate roots).
        let names: Vec<_> = v.tree.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "runtime.goexit",
                "runtime.gcBgMarkWorker",
                "runtime.schedule"
            ]
        );
        let goexit = &v.tree.children[0];
        assert_eq!(goexit.total, 166 * 10_000_000);
        // Not gzip compressed works as well; garbage does not.
        let plain = {
            use std::io::Read;
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&data[..])
                .read_to_end(&mut out)
                .unwrap();
            out
        };
        assert_eq!(open(&plain, None).unwrap().total, v.total);
        assert!(decode(b"\x1f\x8bnot gzip").is_err());
        assert!(decode(b"hello world").is_err());
        let json = serde_json::to_value(&v).unwrap();
        assert!(json["tree"]["self"].is_number());
    }

    #[test]
    fn prunes_tiny_nodes() {
        let mut b = ProfileBuilder::new(&[("cpu", "nanoseconds")]);
        b.sample(&[("main", "m.go", 1), ("big", "m.go", 5)], &[10_000]);
        b.sample(&[("main", "m.go", 1), ("tiny", "m.go", 9)], &[1]);
        let p = decode(&b.encode()).unwrap();
        let v = view(
            &p,
            None,
            ViewOptions {
                min_fraction: 0.01,
                ..ViewOptions::default()
            },
        );
        assert_eq!(v.pruned, 1);
        assert_eq!(v.tree.children[0].children.len(), 1);
        assert_eq!(v.top.len(), 3, "the top list keeps every function");
    }
}

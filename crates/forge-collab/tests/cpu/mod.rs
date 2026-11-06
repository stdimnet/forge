//! CPU time of this test process and of its named threads, for the performance tests. Read
//! from Linux `/proc`: the process from `stat` (`utime` + `stime`, in clock ticks of 10 ms,
//! threads that ended included), live threads from `schedstat` (nanoseconds); `None`
//! elsewhere.

#![allow(dead_code)]

use std::time::Duration;

/// `CLK_TCK` is 100 on every Linux platform Forge runs on.
const TICK: Duration = Duration::from_millis(10);

/// User plus system time from a `stat` line of `/proc`.
fn stat_ticks(stat: &str) -> Option<u64> {
    // The command name is in parentheses and may contain spaces: count fields after it.
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// CPU time the whole process used so far (all threads, also those that ended).
pub fn process() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    Some(TICK * stat_ticks(&stat)? as u32)
}

/// CPU time used so far by the live threads whose name starts with `prefix` (all of them
/// for `""`).
pub fn threads(prefix: &str) -> Option<Duration> {
    let mut nanos = 0;
    for task in std::fs::read_dir("/proc/self/task").ok()? {
        let Ok(task) = task else { continue };
        let name = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
        if !name.trim_end().starts_with(prefix) {
            continue;
        }
        let Ok(sched) = std::fs::read_to_string(task.path().join("schedstat")) else {
            continue;
        };
        nanos += sched
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0);
    }
    Some(Duration::from_nanos(nanos))
}

pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `cpu` over `wall` as a percentage of one core.
pub fn percent(cpu: Duration, wall: Duration) -> f64 {
    100.0 * cpu.as_secs_f64() / wall.as_secs_f64().max(1e-9)
}

#[test]
fn stat_lines_are_parsed() {
    let line = "4242 (tokio-rt (x)) S 1 2 3 4 5 6 7 8 9 10 123 45 0 0";
    assert_eq!(stat_ticks(line), Some(168));
    if cfg!(target_os = "linux") {
        assert!(process().is_some());
    }
}

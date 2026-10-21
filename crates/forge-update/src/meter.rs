//! Download speed over a sliding window, for the "9.8 MB/s" next to the progress bar.

use std::collections::VecDeque;

/// Bytes per second over the last [`SpeedMeter::WINDOW_MS`] milliseconds.
#[derive(Debug, Clone, Default)]
pub struct SpeedMeter {
    /// `(milliseconds since start, bytes downloaded so far)`.
    samples: VecDeque<(u64, u64)>,
}

impl SpeedMeter {
    pub const WINDOW_MS: u64 = 2_000;

    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `downloaded` bytes have arrived by `at_ms`, and return the current speed
    /// in bytes per second (0 until two samples are apart in time).
    pub fn record(&mut self, at_ms: u64, downloaded: u64) -> u64 {
        self.samples.push_back((at_ms, downloaded));
        while self.samples.len() > 2
            && self
                .samples
                .get(1)
                .is_some_and(|(t, _)| at_ms.saturating_sub(*t) >= Self::WINDOW_MS)
        {
            self.samples.pop_front();
        }
        let (t0, b0) = self.samples.front().copied().unwrap_or_default();
        let elapsed = at_ms.saturating_sub(t0);
        if elapsed == 0 {
            return 0;
        }
        downloaded.saturating_sub(b0) * 1000 / elapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_over_a_sliding_window() {
        let mut m = SpeedMeter::new();
        assert_eq!(m.record(0, 0), 0);
        assert_eq!(m.record(500, 5_000_000), 10_000_000);
        assert_eq!(m.record(1_000, 10_000_000), 10_000_000);
        // The download slows down; old samples leave the window.
        m.record(2_000, 12_000_000);
        m.record(3_000, 13_000_000);
        let slow = m.record(4_000, 14_000_000);
        assert!(slow <= 1_500_000, "{slow}");
        // Same timestamp twice does not divide by zero.
        let mut m = SpeedMeter::new();
        m.record(10, 1);
        assert_eq!(m.record(10, 2), 0);
    }
}

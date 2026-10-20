//! Simulated updates for development: `FORGE_SIMULATE_UPDATE=0.2.0 pnpm tauri dev` and
//! `?simulateUpdate=0.2.0` in browser mode show the update offer with a fake download.
//! Nothing is downloaded or installed.

use crate::feed::UpdateOffer;
use crate::{parse_version, Result};

/// The size the simulated download pretends to have (94 MB, like a Linux AppImage).
pub const SIMULATED_SIZE: u64 = 94 * 1024 * 1024;
/// Delay between two simulated chunks.
pub const CHUNK_INTERVAL_MS: u64 = 170;

/// The offer for a simulated `version`.
pub fn offer(version: &str, current: &str) -> Result<UpdateOffer> {
    let version = parse_version(version)?;
    Ok(UpdateOffer::new(
        &version.to_string(),
        current,
        Some("Simulated update for development.".into()),
        None,
        Some(SIMULATED_SIZE),
        true,
    ))
}

/// Chunk sizes of a simulated download: 6–13 MB at a time (varying, but the same every run)
/// until `total` is reached.
#[derive(Debug, Clone)]
pub struct SimulatedDownload {
    total: u64,
    sent: u64,
    step: u64,
}

impl SimulatedDownload {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            sent: 0,
            step: 0,
        }
    }

    pub fn total(&self) -> u64 {
        self.total
    }
}

impl Iterator for SimulatedDownload {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        if self.sent >= self.total {
            return None;
        }
        const MB: u64 = 1024 * 1024;
        let size = (6 + (self.step * 5) % 8) * MB;
        self.step += 1;
        let chunk = size.min(self.total - self.sent);
        self.sent += chunk;
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_offers_say_so() {
        let o = offer("v0.2.0", "0.1.0").unwrap();
        assert_eq!(o.version, "0.2.0");
        assert!(o.simulated);
        assert_eq!(o.size, Some(SIMULATED_SIZE));
        assert!(o.summary.contains("simulated"));
        assert!(offer("soon", "0.1.0").is_err());
    }

    #[test]
    fn simulated_download_adds_up() {
        let chunks: Vec<u64> = SimulatedDownload::new(SIMULATED_SIZE).collect();
        assert_eq!(chunks.iter().sum::<u64>(), SIMULATED_SIZE);
        assert!(chunks.len() > 5 && chunks.len() < 20);
        assert!(SimulatedDownload::new(0).next().is_none());
    }
}

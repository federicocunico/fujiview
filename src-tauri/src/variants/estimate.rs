//! Disk-space and duration estimates for a variant job.

use std::collections::VecDeque;
use std::path::Path;

/// Initial guess for one full-resolution 40 MP JPEG (Fine).
pub const DEFAULT_VARIANT_BYTES: u64 = 25 * 1024 * 1024;
/// Free space that must remain after the job.
pub const SAFETY_MARGIN_BYTES: u64 = 5 * 1024 * 1024 * 1024;
/// Re-check free space every N conversions.
pub const DISK_CHECK_EVERY: usize = 20;
/// Initial guess before any conversion has been timed.
pub const DEFAULT_SECONDS_PER_CONVERSION: f64 = 4.0;
const WINDOW: usize = 20;

pub fn required_bytes(pending: usize, average_variant_bytes: u64) -> u64 {
    pending as u64 * average_variant_bytes
}

pub fn has_room(free: u64, required: u64) -> bool {
    free >= required.saturating_add(SAFETY_MARGIN_BYTES)
}

/// Free bytes on the volume holding `path` (or its nearest existing ancestor).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    platform_free_bytes(existing)
}

#[cfg(windows)]
fn platform_free_bytes(path: &Path) -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut available = 0u64;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut available), None, None) }.ok()?;
    Some(available)
}

#[cfg(not(windows))]
fn platform_free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// Moving averages of conversion duration and output size.
#[derive(Debug, Clone, Default)]
pub struct Meter {
    seconds: VecDeque<f64>,
    total_bytes: u64,
    files: u64,
}

impl Meter {
    pub fn record(&mut self, seconds: f64, bytes: u64) {
        if self.seconds.len() == WINDOW {
            self.seconds.pop_front();
        }
        self.seconds.push_back(seconds);
        self.total_bytes += bytes;
        self.files += 1;
    }

    pub fn seconds_per_conversion(&self) -> f64 {
        if self.seconds.is_empty() {
            DEFAULT_SECONDS_PER_CONVERSION
        } else {
            self.seconds.iter().sum::<f64>() / self.seconds.len() as f64
        }
    }

    pub fn average_bytes(&self) -> u64 {
        if self.files == 0 {
            DEFAULT_VARIANT_BYTES
        } else {
            self.total_bytes / self.files
        }
    }

    pub fn eta_seconds(&self, remaining: usize) -> u64 {
        (self.seconds_per_conversion() * remaining as f64).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_requires_safety_margin() {
        let need = required_bytes(1120, DEFAULT_VARIANT_BYTES);
        assert_eq!(need, 1120 * 25 * 1024 * 1024);
        assert!(!has_room(need, need));
        assert!(has_room(need + SAFETY_MARGIN_BYTES, need));
        assert!(!has_room(0, 1));
    }

    #[test]
    fn meter_uses_defaults_then_real_values() {
        let mut meter = Meter::default();
        assert_eq!(meter.average_bytes(), DEFAULT_VARIANT_BYTES);
        assert_eq!(meter.eta_seconds(10), 40);
        meter.record(2.0, 10);
        meter.record(4.0, 30);
        assert_eq!(meter.seconds_per_conversion(), 3.0);
        assert_eq!(meter.average_bytes(), 20);
        assert_eq!(meter.eta_seconds(100), 300);
    }

    #[test]
    fn meter_window_is_bounded() {
        let mut meter = Meter::default();
        for _ in 0..50 {
            meter.record(10.0, 1);
        }
        for _ in 0..WINDOW {
            meter.record(1.0, 1);
        }
        assert_eq!(meter.seconds_per_conversion(), 1.0);
    }

    #[test]
    fn free_bytes_on_existing_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("a").join("b");
        if cfg!(windows) {
            assert!(free_bytes(&missing).unwrap() > 0);
        }
    }
}

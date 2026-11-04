//! Host and process resource sampling.
//!
//! Lives in the core (not in the UI) so any front-end — current GPUI shell or a
//! future JS/TS/Python client — can show the same numbers through
//! [`EngineEvent::SystemUsage`]. Linux is the supported sampler today; other
//! platforms report zeros and the UI shows "n/d" instead of inventing data.

use std::time::Instant;

/// One resource sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SystemUsage {
    /// Host CPU usage in percent (`0..=100`). `None` when unsupported.
    pub cpu_percent: Option<f32>,
    /// Total RAM in MiB.
    pub ram_total_mb: Option<u64>,
    /// Used RAM in MiB (`total - available`, not cache).
    pub ram_used_mb: Option<u64>,
    /// Resident memory of this process in MiB.
    pub app_mb: Option<u64>,
}

/// Stateful sampler: CPU needs two `/proc/stat` reads to compute a delta.
#[derive(Debug, Default)]
pub struct SystemSampler {
    previous: Option<(u64, u64)>,
    last_at: Option<Instant>,
}

impl SystemSampler {
    /// Creates an empty sampler. The first sample has no CPU percentage yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a sample, computing CPU usage against the previous call.
    pub fn sample(&mut self) -> SystemUsage {
        #[cfg(target_os = "linux")]
        {
            let cpu = std::fs::read_to_string("/proc/stat")
                .ok()
                .and_then(|text| parse_proc_stat(&text));
            let mem = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|text| parse_meminfo(&text));
            let rss = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|text| parse_status_rss_kb(&text));

            let cpu_percent = match (self.previous, cpu) {
                (Some((idle0, total0)), Some((idle1, total1))) => {
                    let total = total1.saturating_sub(total0);
                    let idle = idle1.saturating_sub(idle0);
                    if total == 0 {
                        Some(0.0)
                    } else {
                        let busy = total.saturating_sub(idle);
                        Some((busy as f32 / total as f32) * 100.0)
                    }
                }
                _ => None,
            };
            if let Some(values) = cpu {
                self.previous = Some(values);
            }
            self.last_at = Some(Instant::now());

            let (ram_total_mb, ram_used_mb, app_mb) = match (mem, rss) {
                (Some((total_kb, available_kb)), rss_kb) => (
                    Some(total_kb / 1024),
                    Some(total_kb.saturating_sub(available_kb) / 1024),
                    rss_kb.map(|kb| kb / 1024),
                ),
                (None, rss_kb) => (None, None, rss_kb.map(|kb| kb / 1024)),
            };

            SystemUsage {
                cpu_percent,
                ram_total_mb,
                ram_used_mb,
                app_mb,
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.last_at = Some(Instant::now());
            SystemUsage::default()
        }
    }

    /// Seconds since the previous sample, if any.
    pub fn elapsed_secs(&self) -> Option<f32> {
        self.last_at.map(|instant| instant.elapsed().as_secs_f32())
    }
}

/// Parses the aggregate `cpu` line of `/proc/stat`.
///
/// Returns `(idle_all, total)`, where `idle_all` includes `iowait`.
pub fn parse_proc_stat(input: &str) -> Option<(u64, u64)> {
    let line = input.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }

    let idle = values[3];
    let iowait = values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().sum();
    Some((idle + iowait, total))
}

/// Parses `MemTotal` and `MemAvailable` (in kB) from `/proc/meminfo`.
pub fn parse_meminfo(input: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;

    for line in input.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next()?;
        let value: u64 = parts.next()?.parse().ok()?;
        match key {
            "MemTotal:" => total = Some(value),
            "MemAvailable:" => available = Some(value),
            _ => {}
        }
    }

    Some((total?, available?))
}

/// Parses `VmRSS` (in kB) from `/proc/self/status`.
pub fn parse_status_rss_kb(input: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next()? != "VmRSS:" {
            return None;
        }
        parts.next()?.parse().ok()
    })
}

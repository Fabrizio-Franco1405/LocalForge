//! Host, process and GPU resource sampling.
//!
//! Uses `sysinfo` so the same code reports CPU/RAM on Linux, Windows and macOS.
//! VRAM is best-effort: AMD sysfs on Linux and `nvidia-smi` on any OS that has
//! it. Unsupported combinations return `None` and the UI shows "n/d" instead of
//! inventing a number.

use std::process::Command;

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessesToUpdate, RefreshKind, System};

/// One resource sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SystemUsage {
    /// Host CPU usage in percent (`0..=100`). `None` before the first delta.
    pub cpu_percent: Option<f32>,
    /// Total RAM in MiB.
    pub ram_total_mb: Option<u64>,
    /// Used RAM in MiB.
    pub ram_used_mb: Option<u64>,
    /// Resident memory of this process in MiB.
    pub app_mb: Option<u64>,
    /// GPU memory in use, in MiB (AMD sysfs or nvidia-smi).
    pub vram_used_mb: Option<u64>,
    /// Total GPU memory, in MiB.
    pub vram_total_mb: Option<u64>,
    /// Resident memory of `llama-server`, when it is running.
    pub llm_mb: Option<u64>,
}

/// Stateful sampler. CPU values need a previous sample to compute a delta.
#[derive(Debug)]
pub struct SystemSampler {
    system: System,
    pid: Option<Pid>,
    started: bool,
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSampler {
    /// Creates a sampler and primes CPU counters.
    pub fn new() -> Self {
        // Only CPU + memory: `new_all()` would enumerate every process on
        // startup, which is needlessly expensive.
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_cpu_usage();
        system.refresh_memory();

        Self {
            system,
            pid: sysinfo::get_current_pid().ok(),
            started: false,
        }
    }

    /// Takes a sample.
    pub fn sample(&mut self) -> SystemUsage {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();

        let cpu_percent = if self.started {
            let usage = self.system.global_cpu_usage();
            (usage.is_finite()).then_some(usage.clamp(0.0, 100.0))
        } else {
            self.started = true;
            None
        };

        let app_mb = self.pid.and_then(|pid| {
            self.system
                .refresh_processes(ProcessesToUpdate::Some(&[pid]));
            self.system
                .process(pid)
                .map(|process| process.memory() / (1024 * 1024))
        });

        let ram_total_mb = Some(self.system.total_memory() / (1024 * 1024));
        let ram_used_mb = Some(self.system.used_memory() / (1024 * 1024));
        let (vram_used_mb, vram_total_mb) = vram_usage();

        SystemUsage {
            cpu_percent,
            ram_total_mb,
            ram_used_mb,
            app_mb,
            vram_used_mb,
            vram_total_mb,
            llm_mb: None,
        }
    }

    /// Resident memory (MiB) of another process, refreshed on demand.
    pub fn process_mb(&mut self, pid: u32) -> Option<u64> {
        let pid = Pid::from_u32(pid);
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[pid]));
        self.system
            .process(pid)
            .map(|process| process.memory() / (1024 * 1024))
    }
}

/// Best-effort VRAM usage for the active GPU.
pub fn vram_usage() -> (Option<u64>, Option<u64>) {
    if let Some(values) = vram_from_sysfs() {
        return values;
    }

    vram_from_nvidia_smi()
}

#[cfg(target_os = "linux")]
fn vram_from_sysfs() -> Option<(Option<u64>, Option<u64>)> {
    let mut best: Option<(u64, u64)> = None;

    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("card"))
        {
            continue;
        }

        let device = path.join("device");
        let used = read_number(device.join("mem_info_vram_used"));
        let total = read_number(device.join("mem_info_vram_total"));
        if let (Some(used), Some(total)) = (used, total)
            && total > 0
            && best.is_none_or(|(_, best_total)| total > best_total)
        {
            best = Some((used, total));
        }
    }

    best.map(|(used, total)| (Some(used / (1024 * 1024)), Some(total / (1024 * 1024))))
}

#[cfg(not(target_os = "linux"))]
fn vram_from_sysfs() -> Option<(Option<u64>, Option<u64>)> {
    None
}

fn vram_from_nvidia_smi() -> (Option<u64>, Option<u64>) {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }

    parse_nvidia_smi(&String::from_utf8_lossy(&output.stdout)).unwrap_or((None, None))
}

/// Parses `nvidia-smi --query-gpu=memory.used,memory.total` output.
///
/// Takes the first GPU and accepts extra whitespace. Returns MiB.
pub fn parse_nvidia_smi(input: &str) -> Option<(Option<u64>, Option<u64>)> {
    let line = input.lines().next()?.trim();
    let (used, total) = line.split_once(',')?;
    let used = used.trim().parse::<u64>().ok()?;
    let total = total.trim().parse::<u64>().ok()?;

    Some((Some(used), Some(total)))
}

#[cfg(target_os = "linux")]
fn read_number(path: std::path::PathBuf) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

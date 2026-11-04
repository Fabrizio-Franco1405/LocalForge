//! Hardware and environment detection for the Ajustes → Sistema page.
//!
//! Everything is read-only and cross-platform: `sysinfo` for OS/CPU/RAM and
//! optional CLI probes (`vulkaninfo`, `llama-server --list-devices`) with a
//! short timeout. No GPU driver is linked and no unsafe code is used.

use std::path::Path;
use std::time::Duration;

use shared::{DiagnosticItem, VoxConfig};
use sysinfo::System;

/// Collects a hardware report.
pub async fn collect(config: &VoxConfig) -> Vec<DiagnosticItem> {
    let mut items = base_items();

    let gpus = detect_gpus(config).await;
    items.push(DiagnosticItem {
        name: "GPU (Vulkan)".to_owned(),
        value: if gpus.is_empty() {
            "no detectada · instala el runtime Vulkan".to_owned()
        } else {
            gpus.join(" · ")
        },
        ok: !gpus.is_empty(),
    });

    items.push(DiagnosticItem {
        name: "Backend de inferencia".to_owned(),
        value: "Vulkan (sin CUDA / ROCm)".to_owned(),
        ok: true,
    });

    items.push(tool_item("llama-server", &config.llm.binary));
    items.push(tool_item("whisper-cli", &config.stt.binary));
    items.push(tool_item("Trellis.cpp", &config.scene3d.trellis_binary));
    items.push(tool_item("vulkaninfo", Path::new("vulkaninfo")));

    items
}

fn base_items() -> Vec<DiagnosticItem> {
    let system = System::new_all();
    let os = match (System::name(), System::os_version()) {
        (Some(name), Some(version)) => format!("{name} {version}"),
        (Some(name), None) => name,
        _ => std::env::consts::OS.to_owned(),
    };
    let arch = System::cpu_arch().unwrap_or_else(|| std::env::consts::ARCH.to_owned());
    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .filter(|brand| !brand.is_empty())
        .unwrap_or_else(|| "CPU desconocida".to_owned());
    let threads = system.cpus().len();
    let cores = system.physical_core_count().unwrap_or(threads);
    let ram_gb = system.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    vec![
        DiagnosticItem {
            name: "Sistema operativo".to_owned(),
            value: format!(
                "{os} · kernel {}",
                System::kernel_version().unwrap_or_else(|| "n/d".to_owned())
            ),
            ok: true,
        },
        DiagnosticItem {
            name: "Arquitectura".to_owned(),
            value: arch,
            ok: true,
        },
        DiagnosticItem {
            name: "CPU".to_owned(),
            value: format!("{cpu} · {cores} núcleos / {threads} hilos"),
            ok: true,
        },
        DiagnosticItem {
            name: "Memoria RAM".to_owned(),
            value: format!("{ram_gb:.1} GiB"),
            ok: true,
        },
    ]
}

async fn detect_gpus(config: &VoxConfig) -> Vec<String> {
    if let Some(output) = run(&config.llm.binary, &["--list-devices"]).await {
        let devices = parse_llama_devices(&output);
        if !devices.is_empty() {
            return devices;
        }
    }

    if let Some(output) = run(Path::new("vulkaninfo"), &["--summary"]).await {
        return parse_vulkaninfo(&output);
    }

    Vec::new()
}

async fn run(binary: &Path, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(4),
        tokio::process::Command::new(binary).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn tool_item(name: &str, binary: &Path) -> DiagnosticItem {
    if binary.as_os_str().is_empty() {
        return DiagnosticItem {
            name: name.to_owned(),
            value: "sin configurar".to_owned(),
            ok: false,
        };
    }

    match shared::tools::resolve_binary(binary) {
        Some(path) => DiagnosticItem {
            name: name.to_owned(),
            value: path.display().to_string(),
            ok: true,
        },
        None => DiagnosticItem {
            name: name.to_owned(),
            value: format!("no encontrado: {}", binary.display()),
            ok: false,
        },
    }
}

/// Extracts GPU names from `vulkaninfo --summary`.
pub fn parse_vulkaninfo(summary: &str) -> Vec<String> {
    let mut devices = Vec::new();

    for line in summary.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("deviceName")
            && let Some((_, value)) = rest.split_once('=')
        {
            let value = value.trim();
            if !value.is_empty() && !devices.iter().any(|device| device == value) {
                devices.push(value.to_owned());
            }
        }
    }

    devices
}

/// Extracts device lines from `llama-server --list-devices`.
pub fn parse_llama_devices(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("Vulkan") || line.starts_with("CUDA"))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

//! Tests for the core resource sampler, VRAM parsing and hardware detection.

use controller::hardware::{parse_llama_devices, parse_vulkaninfo};
use controller::system::{SystemSampler, parse_nvidia_smi};

#[test]
fn parses_nvidia_smi_first_gpu() {
    assert_eq!(
        parse_nvidia_smi("2048, 8192\n"),
        Some((Some(2048), Some(8192)))
    );
}

#[test]
fn nvidia_smi_accepts_extra_spaces() {
    assert_eq!(
        parse_nvidia_smi("  512 , 16384 \n"),
        Some((Some(512), Some(16384)))
    );
}

#[test]
fn nvidia_smi_rejects_garbage() {
    assert_eq!(parse_nvidia_smi("not a number"), None);
}

#[test]
fn parses_vulkaninfo_devices() {
    let summary = "\
GPU0:
\tdeviceName        = AMD Radeon RX 5700 XT (RADV NAVI10)
\tdriverName        = radv
GPU1:
\tdeviceName        = AMD Radeon Graphics (RADV RENOIR)
";
    let devices = parse_vulkaninfo(summary);

    assert_eq!(devices.len(), 2);
    assert!(devices[0].contains("RX 5700 XT"));
}

#[test]
fn parses_llama_server_devices() {
    let text = "\
Available devices:
  Vulkan0: AMD Radeon RX 5700 XT (8176 MiB, 7800 MiB free)
  Vulkan1: AMD Radeon Graphics (18069 MiB, 18000 MiB free)
";
    let devices = parse_llama_devices(text);

    assert_eq!(devices.len(), 2);
    assert!(devices[0].starts_with("Vulkan0:"));
}

#[test]
fn sampler_reports_memory() {
    let mut sampler = SystemSampler::new();
    let sample = sampler.sample();

    assert!(sample.ram_total_mb.unwrap_or(0) > 0);
    assert!(sample.ram_used_mb.unwrap_or(0) > 0);
    assert!(sample.app_mb.unwrap_or(0) > 0);
}

#[test]
fn model_scan_finds_gguf_recursively() {
    let base = std::env::temp_dir().join(format!("localforge-scan-{}", std::process::id()));
    let nested = base.join("sub");
    std::fs::create_dir_all(&nested).expect("temp dir");

    std::fs::write(base.join("alpha.gguf"), b"x").expect("alpha");
    std::fs::write(nested.join("beta.GGUF"), b"x").expect("beta");
    std::fs::write(base.join("notes.txt"), b"x").expect("notes");

    let entries = controller::models::scan(&base);
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();

    assert!(names.contains(&"alpha"), "names: {names:?}");
    assert!(names.contains(&"beta"), "names: {names:?}");
    assert!(!names.iter().any(|name| name.contains("notes")));

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn compact_gpu_keeps_name_and_vram() {
    let line = "Vulkan0: AMD Radeon RX 5700 XT (RADV NAVI10) (8176 MiB, 494 MiB free)";

    assert_eq!(
        controller::hardware::compact_gpu(line),
        "AMD Radeon RX 5700 XT · 8176 MiB"
    );
}

#[test]
fn split_gguf_only_lists_first_shard() {
    let base = std::env::temp_dir().join(format!("localforge-split-{}", std::process::id()));
    std::fs::create_dir_all(&base).expect("dir");
    std::fs::write(base.join("model-00001-of-00002.gguf"), b"x").expect("one");
    std::fs::write(base.join("model-00002-of-00002.gguf"), b"x").expect("two");

    let entries = controller::models::scan(&base);
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();

    assert_eq!(names, vec!["model-00001-of-00002"]);

    std::fs::remove_dir_all(&base).ok();
}

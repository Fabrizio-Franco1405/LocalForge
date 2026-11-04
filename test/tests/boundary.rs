//! Architectural boundary: the core must stay UI-agnostic.
//!
//! If a core crate ever depends on GPUI, swapping the UI (JS/TS, Python, web)
//! stops being a protocol change and becomes a rewrite. This test fails early.

const CORE_CRATES: [&str; 7] = [
    "shared",
    "llm_engine",
    "controller",
    "stt",
    "tts",
    "audio_engine",
    "scene3d",
];

#[test]
fn core_crates_do_not_depend_on_gpui() {
    for krate in CORE_CRATES {
        let manifest = std::fs::read_to_string(format!("../crates/{krate}/Cargo.toml"))
            .unwrap_or_else(|error| panic!("no se pudo leer {krate}: {error}"));

        assert!(
            !manifest.contains("gpui"),
            "{krate}/Cargo.toml depende de GPUI; el core debe ser agnóstico a la UI"
        );
    }
}

#[test]
fn core_sources_do_not_import_gpui() {
    for krate in CORE_CRATES {
        let dir = format!("../crates/{krate}/src");
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|error| panic!("{dir}: {error}"));

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

                for line in source.lines() {
                    let line = line.trim_start();
                    if (line.starts_with("use ") || line.starts_with("extern crate "))
                        && line.contains("gpui")
                    {
                        panic!("{} importa GPUI: {line}", path.display());
                    }
                }
            }
        }
    }
}

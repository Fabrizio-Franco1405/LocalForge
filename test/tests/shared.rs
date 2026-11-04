//! Tests for `shared`: configuration, events, text cleanup and path tools.
//!
//! These are pure: no window, no GPU, no microphone and no child process.

use std::path::{Path, PathBuf};

use shared::text::{for_speech, truncate_chars};
use shared::tools::{command_program, expand_path, file_status, resolve_binary};
use shared::{
    AudioConfig, EngineCommand, EngineEvent, LlmConfig, LlmMode, LlmState, TtsConfig, VadConfig,
    VoxConfig, engine_channel,
};

mod config {
    use super::*;

    #[test]
    fn defaults_are_valid_and_loopback() {
        let cfg = VoxConfig::default();

        assert_eq!(cfg.llm.host, "127.0.0.1");
        assert_eq!(cfg.llm.port, 9931);
        assert_eq!(cfg.llm.mode, LlmMode::Managed);
        assert_eq!(cfg.audio.sample_rate, 16_000);
        assert_eq!(cfg.audio.vad.min_silence_ms, 650);
        assert_eq!(cfg.llm.base_url(), "http://127.0.0.1:9931");
        assert!(!cfg.stt.enabled);
    }

    #[test]
    fn wildcard_host_maps_to_loopback_for_clients() {
        let cfg = LlmConfig {
            host: "0.0.0.0".to_owned(),
            ..LlmConfig::default()
        };

        assert_eq!(cfg.base_url(), "http://127.0.0.1:9931");
    }

    #[test]
    fn partial_toml_is_accepted() {
        let parsed: VoxConfig = toml::from_str(
            r#"
            [llm]
            port = 9940
            "#,
        )
        .expect("partial TOML should parse");

        assert_eq!(parsed.llm.port, 9940);
        assert_eq!(parsed.stt.language, "es");
        assert!(parsed.llm.model.is_none());
    }

    #[test]
    fn round_trip_preserves_values() {
        let cfg = VoxConfig {
            llm: LlmConfig {
                model: Some(PathBuf::from("/tmp/model.gguf")),
                ..LlmConfig::default()
            },
            tts: TtsConfig {
                voice: "em_alex".to_owned(),
                ..TtsConfig::default()
            },
            audio: AudioConfig {
                vad: VadConfig {
                    min_silence_ms: 800,
                    ..VadConfig::default()
                },
                ..AudioConfig::default()
            },
            ..VoxConfig::default()
        };

        let mut cfg = cfg;
        cfg.llm
            .model_aliases
            .insert("/tmp/model.gguf".to_owned(), "Qwen local".to_owned());

        let rendered = toml::to_string_pretty(&cfg).expect("serialize");
        let parsed: VoxConfig = toml::from_str(&rendered).expect("deserialize");

        assert_eq!(cfg, parsed);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let cfg = VoxConfig::load(Path::new("/definitely/not/here.toml"))
            .expect("missing file uses defaults");

        assert_eq!(cfg, VoxConfig::default());
    }
}

mod events {
    use super::*;

    #[test]
    fn handle_round_trips_commands_and_events() {
        let (handle, commands, events) = engine_channel();

        assert!(handle.send(EngineCommand::StartLlm));
        assert!(matches!(
            commands.recv().expect("command"),
            EngineCommand::StartLlm
        ));

        events
            .send(EngineEvent::Llm {
                state: LlmState::Ready,
                detail: "ok".to_owned(),
            })
            .expect("event");

        let drained = handle.drain();
        assert_eq!(drained.len(), 1);
        assert!(matches!(
            drained[0],
            EngineEvent::Llm {
                state: LlmState::Ready,
                ..
            }
        ));
    }

    #[test]
    fn state_labels_are_stable() {
        assert_eq!(LlmState::Ready.label(), "Listo");
        assert_eq!(LlmState::Error.label(), "Error");
    }
}

mod speech_text {
    use super::*;

    #[test]
    fn removes_code_fences_but_keeps_prose() {
        let input = "Aquí tienes:\n```rust\nfn main() {}\n```\nEspero que ayude.";
        assert_eq!(for_speech(input), "Aquí tienes: Espero que ayude.");
    }

    #[test]
    fn keeps_inline_code_content_without_backticks() {
        assert_eq!(
            for_speech("Usa `llama-server --port 9931` para empezar."),
            "Usa llama-server --port 9931 para empezar."
        );
    }

    #[test]
    fn converts_links_to_their_text() {
        assert_eq!(
            for_speech("Mira [la documentación](https://example.com) ahora."),
            "Mira la documentación ahora."
        );
    }

    #[test]
    fn strips_list_markers_and_headers() {
        let input = "## Pasos\n1. Abre la app\n- Pulsa iniciar\n* Listo";
        assert_eq!(for_speech(input), "Pasos Abre la app Pulsa iniciar Listo");
    }

    #[test]
    fn removes_emojis_and_zwj_sequences() {
        assert_eq!(for_speech("Hola 👋🏽 mundo 🚀!"), "Hola mundo !");
        assert_eq!(for_speech("Familia 👨‍👩‍👧 lista"), "Familia lista");
    }

    #[test]
    fn preserves_spanish_accents_and_punctuation() {
        assert_eq!(
            for_speech("¿Qué tal? ¡Muy bien! Añadí código, ¿eh?"),
            "¿Qué tal? ¡Muy bien! Añadí código, ¿eh?"
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(for_speech("  uno \n\n dos\t tres  "), "uno dos tres");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(for_speech("   \n\n "), "");
    }

    #[test]
    fn truncate_keeps_utf8_boundaries() {
        assert_eq!(truncate_chars("hola", 10), "hola");
        assert_eq!(truncate_chars("hola mundo", 4), "hola…");
        assert_eq!(truncate_chars("acción", 4), "acci…");
    }
}

mod tools {
    use super::*;

    #[test]
    fn empty_path_expands_to_none() {
        assert_eq!(expand_path("   "), None);
    }

    #[test]
    fn absolute_path_is_preserved() {
        assert_eq!(
            expand_path("/tmp/model.gguf"),
            Some(PathBuf::from("/tmp/model.gguf"))
        );
    }

    #[test]
    fn tilde_is_expanded() {
        let expanded = expand_path("~/model.gguf").expect("expanded");

        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("model.gguf"));
        assert!(!expanded.to_string_lossy().contains('~'));
    }

    #[test]
    fn missing_absolute_binary_is_reported() {
        assert!(resolve_binary(Path::new("/definitely/not/a/binary")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn shell_is_found_in_path() {
        assert!(resolve_binary(Path::new("sh")).is_some());
    }

    #[test]
    fn file_status_distinguishes_missing() {
        let (ok, value) = file_status(Some(Path::new("/definitely/not/here")));

        assert!(!ok);
        assert!(value.contains("no existe"));
    }

    #[test]
    fn command_program_reads_first_token() {
        assert_eq!(
            command_program("piper --model voice.onnx --output_file {out}"),
            Some("piper")
        );
        assert_eq!(command_program("   "), None);
    }
}

#[test]
fn scene3d_is_disabled_by_default() {
    let cfg = VoxConfig::default();

    assert!(!cfg.scene3d.enabled);
    assert_eq!(cfg.scene3d.backend, "disabled");
}

//! Tests for `llm_engine`: argument building, SSE parsing and server probing.
//!
//! No binary and no model are needed: the only network test targets port 1.

use std::path::PathBuf;
use std::time::Duration;

use llm_engine::{LlamaManager, LlmError, LlmHealth, parse_delta, take_sse_payloads};
use shared::{LlmConfig, LlmMode};

fn configured() -> LlmConfig {
    LlmConfig {
        model: Some(PathBuf::from("/models/qwen.gguf")),
        port: 9931,
        gpu_layers: 99,
        ctx_size: 4096,
        threads: 8,
        ..LlmConfig::default()
    }
}

mod args {
    use super::*;

    #[test]
    fn build_args_requires_a_model() {
        let config = LlmConfig {
            model: None,
            ..configured()
        };

        assert!(matches!(
            LlamaManager::build_args(&config),
            Err(LlmError::ModelMissing)
        ));
    }

    #[test]
    fn build_args_contains_vulkan_and_server_settings() {
        let args = LlamaManager::build_args(&configured()).expect("valid args");
        let joined = args.join(" ");

        assert!(joined.contains("--model /models/qwen.gguf"));
        assert!(joined.contains("--port 9931"));
        assert!(joined.contains("--gpu-layers 99"));
        assert!(joined.contains("--ctx-size 4096"));
        assert!(joined.contains("--threads 8"));
        assert!(!joined.contains("--jinja"));
        assert!(!joined.contains("--no-webui"));
    }

    #[test]
    fn extra_args_are_appended_last() {
        let config = LlmConfig {
            extra_args: vec!["--flash-attn".to_owned(), "on".to_owned()],
            ..configured()
        };
        let args = LlamaManager::build_args(&config).expect("valid args");

        assert_eq!(&args[args.len() - 2..], &["--flash-attn", "on"]);
    }
}

mod sse {
    use super::*;

    #[test]
    fn payloads_split_and_keep_partial_tail() {
        let mut buffer = "data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: {\"partial\"".to_owned();

        let payloads = take_sse_payloads(&mut buffer);

        assert_eq!(payloads, vec!["{\"a\":1}", "{\"b\":2}"]);
        assert_eq!(buffer, "data: {\"partial\"");
    }

    #[test]
    fn payloads_ignore_comments_and_empty_data() {
        let mut buffer = ": keep-alive\n\ndata:\n\ndata: [DONE]\n\n".to_owned();

        assert_eq!(take_sse_payloads(&mut buffer), vec!["[DONE]"]);
    }

    #[test]
    fn delta_extracts_content_and_ignores_role_chunks() {
        let role_only = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_delta(role_only), Ok(None));

        let content = r#"{"choices":[{"delta":{"content":"Hola"}}]}"#;
        assert_eq!(parse_delta(content), Ok(Some("Hola".to_owned())));

        let empty = r#"{"choices":[{"delta":{"content":""}}]}"#;
        assert_eq!(parse_delta(empty), Ok(None));
    }

    #[test]
    fn delta_rejects_invalid_json() {
        assert!(parse_delta("{not json}").is_err());
    }
}

mod server {
    use super::*;

    #[test]
    fn health_maps_unknown_server_to_down() {
        // El puerto 1 no debería estar escuchando durante los tests.
        let config = LlmConfig {
            port: 1,
            ..configured()
        };
        let manager = LlamaManager::new(config, Duration::from_secs(1)).expect("manager");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        assert_eq!(runtime.block_on(manager.health()), LlmHealth::Down);
    }

    #[test]
    fn external_mode_builds_urls_with_or_without_v1() {
        let mut config = LlmConfig {
            endpoint: "http://127.0.0.1:11434".to_owned(),
            mode: LlmMode::External,
            ..configured()
        };

        assert_eq!(
            config.chat_completions_url(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert_eq!(config.models_url(), "http://127.0.0.1:11434/v1/models");

        config.endpoint = "http://127.0.0.1:1234/v1/".to_owned();

        assert_eq!(
            config.chat_completions_url(),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
        assert_eq!(config.models_url(), "http://127.0.0.1:1234/v1/models");
    }

    #[test]
    fn external_mode_needs_no_model_to_start() {
        let config = LlmConfig {
            mode: LlmMode::External,
            model: None,
            endpoint: "http://127.0.0.1:11434".to_owned(),
            ..configured()
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let mut manager = LlamaManager::new(config, Duration::from_secs(1)).expect("manager");

        runtime.block_on(async {
            manager.start().await.expect("external start is a no-op");
            manager
                .wait_ready_for(Duration::from_millis(1))
                .await
                .expect("external mode is not waited on");
        });
    }
}

#[test]
fn external_mode_without_endpoint_is_not_ready() {
    let config = LlmConfig {
        mode: LlmMode::External,
        endpoint: String::new(),
        ..configured()
    };
    let manager = LlamaManager::new(config, Duration::from_secs(1)).expect("manager");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let probe = runtime.block_on(manager.probe());

    assert!(!probe.ready);
    assert!(
        probe.detail.contains("endpoint"),
        "detail was: {}",
        probe.detail
    );
}

#[test]
fn split_shard_points_to_first_file() {
    let dir = std::env::temp_dir().join(format!("localforge-shard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let first = dir.join("model-00001-of-00002.gguf");
    let second = dir.join("model-00002-of-00002.gguf");
    std::fs::write(&first, b"x").expect("first");
    std::fs::write(&second, b"x").expect("second");

    assert_eq!(llm_engine::first_split_shard(&second), first);
    assert_eq!(llm_engine::first_split_shard(&first), first);

    std::fs::remove_dir_all(&dir).ok();
}

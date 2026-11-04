//! End-to-end controller tests against a local fake OpenAI server.
//!
//! They exercise the real controller thread, the real HTTP client and the real
//! SSE parser, with no GPU, model or external binary.

use std::time::{Duration, Instant};

use controller::spawn;
use localforge_tests::mock_server::MockServer;
use shared::{EngineCommand, EngineEvent, EngineHandle, LlmConfig, LlmMode, LlmState, VoxConfig};

fn wait_until_ready(handle: &EngineHandle, seconds: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        if let Some(EngineEvent::Llm { state, detail }) =
            handle.recv_timeout(Duration::from_millis(100))
            && state == LlmState::Ready
        {
            return Some(detail);
        }
    }

    None
}

fn external_config(endpoint: String) -> VoxConfig {
    VoxConfig {
        llm: LlmConfig {
            mode: LlmMode::External,
            endpoint,
            auto_start: false,
            ..LlmConfig::default()
        },
        ..VoxConfig::default()
    }
}

#[test]
fn detects_external_server_without_a_configured_model() {
    let server = MockServer::start();
    let config = VoxConfig {
        llm: LlmConfig {
            mode: LlmMode::Managed,
            model: None,
            host: "127.0.0.1".to_owned(),
            port: server.port(),
            auto_start: false,
            ..LlmConfig::default()
        },
        ..VoxConfig::default()
    };

    let handle = spawn(config);
    assert!(
        handle.send(EngineCommand::ProbeLlm),
        "controller should accept commands"
    );

    let detail = wait_until_ready(&handle, 5).expect("external server must be detected");
    assert!(detail.contains("Servidor"), "detail was: {detail}");

    let _ = handle.send(EngineCommand::Shutdown);
}

#[test]
fn chats_against_external_endpoint_and_uses_reported_model() {
    let server = MockServer::start();
    // A bare host:port must be normalized to http://host:port.
    let handle = spawn(external_config(format!("127.0.0.1:{}", server.port())));

    let detail = wait_until_ready(&handle, 5).expect("external server must be detected");
    assert!(detail.contains("openai") || detail.contains("OpenAI") || detail.contains("listo"));

    let generation = 1_u64;
    assert!(handle.send(EngineCommand::SendMessage {
        generation,
        prompt: "Hola".to_owned(),
    }));

    let mut reply = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut finished = false;

    while Instant::now() < deadline && !finished {
        if let Some(event) = handle.recv_timeout(Duration::from_millis(100)) {
            match event {
                EngineEvent::Token { delta, .. } => reply.push_str(&delta),
                EngineEvent::ReplyDone { .. } => finished = true,
                EngineEvent::Error { message, .. } => panic!("controller error: {message}"),
                _ => {}
            }
        }
    }

    assert!(finished, "reply should finish, got: {reply:?}");
    assert_eq!(reply, "Hola mundo");
    assert!(
        server.body().contains("\"model\":\"mock-qwen\""),
        "chat request should use the reported model id, body was: {}",
        server.body()
    );

    let _ = handle.send(EngineCommand::Shutdown);
}

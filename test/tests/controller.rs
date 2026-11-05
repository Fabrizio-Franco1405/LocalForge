//! End-to-end controller tests against a local fake OpenAI server.
//!
//! They exercise the real controller thread, the real HTTP client and the real
//! SSE parser, with no GPU, model or external binary.

use std::path::PathBuf;
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

/// Collects every event received during `duration`.
fn collect_for(handle: &EngineHandle, duration: Duration) -> Vec<EngineEvent> {
    let deadline = Instant::now() + duration;
    let mut events = Vec::new();

    while Instant::now() < deadline {
        if let Some(event) = handle.recv_timeout(Duration::from_millis(50)) {
            events.push(event);
        }
    }

    events
}

/// A managed configuration whose binary/model do not exist, so any attempt to
/// spawn it fails immediately and visibly.
fn stopped_managed_config() -> VoxConfig {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind free port");
    let port = listener.local_addr().expect("free port addr").port();
    drop(listener);

    VoxConfig {
        llm: LlmConfig {
            mode: LlmMode::Managed,
            binary: PathBuf::from("/nonexistent/localforge-test/llama-server"),
            model: Some(PathBuf::from("/nonexistent/localforge-test/model.gguf")),
            host: "127.0.0.1".to_owned(),
            port,
            auto_start: false,
            ..LlmConfig::default()
        },
        ..VoxConfig::default()
    }
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

#[test]
fn reloading_settings_keeps_a_stopped_engine_stopped() {
    let config = stopped_managed_config();
    let handle = spawn(config.clone());

    // Let the initial probe finish, then stop explicitly and clear its events.
    std::thread::sleep(Duration::from_millis(400));
    let _ = handle.drain();
    assert!(handle.send(EngineCommand::StopLlm));
    std::thread::sleep(Duration::from_millis(200));
    let _ = handle.drain();

    assert!(
        handle.send(EngineCommand::ReloadConfig(Box::new(config))),
        "controller should accept ReloadConfig"
    );

    let events = collect_for(&handle, Duration::from_millis(1_200));
    assert!(
        !events.iter().any(|event| matches!(
            event,
            EngineEvent::Llm {
                state: LlmState::Starting | LlmState::Error,
                ..
            }
        )),
        "applying settings must not spawn a stopped engine: {events:?}"
    );

    let _ = handle.send(EngineCommand::Shutdown);
}

#[test]
fn sending_a_message_keeps_a_stopped_engine_stopped() {
    let handle = spawn(stopped_managed_config());

    std::thread::sleep(Duration::from_millis(400));
    let _ = handle.drain();
    assert!(handle.send(EngineCommand::StopLlm));
    std::thread::sleep(Duration::from_millis(200));
    let _ = handle.drain();

    assert!(handle.send(EngineCommand::SendMessage {
        generation: 1,
        prompt: "Hola".to_owned(),
    }));

    let events = collect_for(&handle, Duration::from_millis(1_200));
    assert!(
        events.iter().any(|event| matches!(
            event,
            EngineEvent::Error {
                scope: "llm",
                ..
            }
        )),
        "the UI should be told the engine is off: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            EngineEvent::Llm {
                state: LlmState::Starting | LlmState::Ready,
                ..
            }
        )),
        "sending a message must not start a stopped engine: {events:?}"
    );

    let _ = handle.send(EngineCommand::Shutdown);
}

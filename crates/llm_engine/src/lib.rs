//! `llama-server` process management and OpenAI-compatible chat streaming.
//!
//! The manager owns one child process, keeps its logs in a ring buffer and
//! exposes a small async API to the controller. It never depends on GPUI.
//!
//! ```no_run
//! use llm_engine::{LlamaManager, ChatMessage};
//! use shared::LlmConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let cfg = LlmConfig::default();
//! let mut engine = LlamaManager::new(cfg, std::time::Duration::from_secs(90))?;
//! engine.start().await?;
//! engine.wait_ready().await?;
//!
//! let rx = engine
//!     .chat_stream(&[ChatMessage::user("Hola")], 0.7, 128)
//!     .await?;
//! drop(rx);
//! engine.stop().await;
//! # Ok(())
//! # }
//! ```

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use shared::LlmConfig;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

const LOG_CAPACITY: usize = 200;

/// Conversation role understood by llama.cpp's chat endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt.
    System,
    /// End user.
    User,
    /// Model answer.
    Assistant,
}

/// One entry of the chat history sent to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Who produced the content.
    pub role: Role,
    /// Message text.
    pub content: String,
}

impl ChatMessage {
    /// Builds a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// Builds a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Builds an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// Streaming events produced by [`LlamaManager::chat_stream`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatDelta {
    /// A piece of the reply.
    Delta(String),
    /// The server sent `[DONE]` or closed the stream normally.
    Done,
    /// The stream failed after it started; the reply is truncated.
    Error(String),
}

/// Result of a health probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmHealth {
    /// The process is not listening.
    Down,
    /// The process is alive but the model is not loaded yet (HTTP 503).
    Loading,
    /// HTTP 200 with `{"status":"ok"}`.
    Ready,
}

/// Detailed status of an OpenAI-compatible server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerProbe {
    /// An endpoint answered successfully.
    pub ready: bool,
    /// The host answered, but the model is still loading or the route failed.
    pub reachable: bool,
    /// URL that answered (or the last one tried).
    pub url: String,
    /// Human readable status.
    pub detail: String,
    /// Model ids reported by `/v1/models`.
    pub models: Vec<String>,
}

impl ServerProbe {
    /// First model id reported by the server, if any.
    pub fn first_model(&self) -> Option<String> {
        self.models.first().cloned()
    }
}

/// Errors returned by [`LlamaManager`].
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// No model path is configured.
    #[error("no hay un modelo GGUF configurado (Ajustes → Modelo)")]
    ModelMissing,
    /// The model path is not valid UTF-8.
    #[error("la ruta del modelo no es UTF-8 válida: {0:?}")]
    InvalidPath(std::path::PathBuf),
    /// The child could not be spawned.
    #[error("no se pudo ejecutar {binary}: {source}")]
    Spawn {
        /// Binary as configured.
        binary: std::path::PathBuf,
        /// OS error.
        source: std::io::Error,
    },
    /// No managed process is running.
    #[error("llama-server no está en ejecución")]
    NotRunning,
    /// The process exited during startup.
    #[error("llama-server terminó durante el arranque (código {code:?}): {logs}")]
    Exited {
        /// Exit code, if any.
        code: Option<i32>,
        /// Last log lines.
        logs: String,
    },
    /// The model did not become ready in time.
    #[error("tiempo de espera agotado cargando el modelo: {logs}")]
    StartTimeout {
        /// Last log lines.
        logs: String,
    },
    /// The HTTP request failed.
    #[error("error HTTP: {0}")]
    Http(#[from] reqwest::Error),
    /// The server answered with a non-success status.
    #[error("llama-server respondió {status}: {body}")]
    HttpStatus {
        /// Status code.
        status: u16,
        /// Truncated response body.
        body: String,
    },
    /// An I/O error.
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
}

/// Owns one `llama-server` child process.
#[derive(Debug)]
pub struct LlamaManager {
    config: LlmConfig,
    client: reqwest::Client,
    child: Option<Child>,
    logs: Arc<Mutex<VecDeque<String>>>,
    startup_timeout: Duration,
    started_at: Option<Instant>,
}

impl LlamaManager {
    /// Creates a manager. The process is not spawned until [`start`] is called.
    ///
    /// [`start`]: Self::start
    pub fn new(config: LlmConfig, startup_timeout: Duration) -> Result<Self, LlmError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()?;

        Ok(Self {
            config,
            client,
            child: None,
            logs: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAPACITY))),
            startup_timeout,
            started_at: None,
        })
    }

    /// Current configuration.
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// Replaces the configuration. The caller is responsible for restarting a
    /// running engine.
    pub fn set_config(&mut self, config: LlmConfig) {
        self.config = config;
    }

    /// Builds the argv passed to `llama-server`. Pure function, unit-tested.
    pub fn build_args(config: &LlmConfig) -> Result<Vec<String>, LlmError> {
        let model = config.model.as_ref().ok_or(LlmError::ModelMissing)?;
        let model = model
            .to_str()
            .ok_or_else(|| LlmError::InvalidPath(model.clone()))?;

        let mut args = vec![
            "--model".to_owned(),
            model.to_owned(),
            "--host".to_owned(),
            config.host.clone(),
            "--port".to_owned(),
            config.port.to_string(),
            "--ctx-size".to_owned(),
            config.ctx_size.to_string(),
            "--gpu-layers".to_owned(),
            config.gpu_layers.to_string(),
            "--threads".to_owned(),
            config.threads.to_string(),
            "--jinja".to_owned(),
            "--no-webui".to_owned(),
        ];

        args.extend(config.extra_args.iter().cloned());
        Ok(args)
    }

    /// Spawns the process if it is not already managed.
    ///
    /// In [`LlmMode::External`] mode there is nothing to spawn: the user
    /// owns the server.
    pub async fn start(&mut self) -> Result<(), LlmError> {
        if !self.config.is_managed() {
            return Ok(());
        }
        if self.child.is_some() {
            return Ok(());
        }

        let args = Self::build_args(&self.config)?;
        let mut command = Command::new(&self.config.binary);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command.spawn().map_err(|source| LlmError::Spawn {
            binary: self.config.binary.clone(),
            source,
        })?;

        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, Arc::clone(&self.logs), "out");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, Arc::clone(&self.logs), "err");
        }

        tracing::info!(?args, "llama-server iniciado");
        self.child = Some(child);
        self.started_at = Some(Instant::now());
        Ok(())
    }

    /// Polls `/health` until the model is ready or the startup timeout expires.
    pub async fn wait_ready(&mut self) -> Result<(), LlmError> {
        self.wait_ready_for(self.startup_timeout).await
    }

    /// Polls `/health` with an explicit timeout.
    pub async fn wait_ready_for(&mut self, timeout: Duration) -> Result<(), LlmError> {
        if !self.config.is_managed() {
            return Ok(());
        }

        let deadline = Instant::now() + timeout;

        loop {
            match self.health().await {
                LlmHealth::Ready => return Ok(()),
                LlmHealth::Loading | LlmHealth::Down => {}
            }

            match self.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        return Err(LlmError::Exited {
                            code: status.code(),
                            logs: self.last_logs_joined(20),
                        });
                    }
                    Ok(None) => {}
                    Err(error) => return Err(LlmError::Io(error)),
                },
                None => return Err(LlmError::NotRunning),
            }

            if Instant::now() >= deadline {
                return Err(LlmError::StartTimeout {
                    logs: self.last_logs_joined(20),
                });
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Probes the health endpoint.
    ///
    /// Managed mode uses llama.cpp's `/health`; external mode uses the
    /// OpenAI-compatible `/v1/models` because third-party servers may not
    /// implement `/health`.
    pub async fn health(&self) -> LlmHealth {
        let probe = self.probe().await;
        if probe.ready {
            LlmHealth::Ready
        } else if probe.reachable {
            LlmHealth::Loading
        } else {
            LlmHealth::Down
        }
    }

    /// Full status probe used by the controller and the tools page.
    ///
    /// It asks both `/health` (llama.cpp) and `/v1/models` (any
    /// OpenAI-compatible server), so a server started outside VoxLocal is
    /// detected even when the app has no model configured.
    pub async fn probe(&self) -> ServerProbe {
        let base = self.config.http_base();
        let origin = strip_v1(&base);
        let mut probe = ServerProbe {
            url: base.clone(),
            ..ServerProbe::default()
        };

        let mut health_urls = vec![format!("{}/health", origin.trim_end_matches('/'))];
        if origin != base {
            health_urls.insert(0, format!("{}/health", base.trim_end_matches('/')));
        }

        for url in health_urls {
            match self.get_json(&url).await {
                Ok((status, body)) if status.is_success() => {
                    probe.ready = true;
                    probe.reachable = true;
                    probe.detail = format!("Servidor listo · {origin}");
                    probe.models = parse_models(&body);
                    break;
                }
                Ok((status, _)) if status.as_u16() == 503 => {
                    probe.reachable = true;
                    probe.detail =
                        format!("El servidor responde pero el modelo sigue cargando ({origin})");
                }
                Ok((status, _)) => {
                    probe.reachable = true;
                    if probe.detail.is_empty() {
                        probe.detail = format!("El servidor responde HTTP {status} en {origin}");
                    }
                }
                Err(_) => {}
            }
        }

        // `/v1/models` also gives us the model id needed by strict
        // OpenAI-compatible servers.
        let models_url = self.config.models_url();
        if let Ok((status, body)) = self.get_json(&models_url).await {
            if status.is_success() {
                probe.reachable = true;
                let models = parse_models(&body);
                if probe.models.is_empty() {
                    probe.models = models;
                }
                if !probe.ready {
                    probe.ready = true;
                    probe.detail = if probe.models.is_empty() {
                        format!("Servidor OpenAI listo · {base}")
                    } else {
                        format!(
                            "Servidor OpenAI listo · {} modelo(s) · {base}",
                            probe.models.len()
                        )
                    };
                }
            } else if status.as_u16() == 503 {
                probe.reachable = true;
                if probe.detail.is_empty() {
                    probe.detail = "El modelo sigue cargando".to_owned();
                }
            } else if probe.detail.is_empty() {
                probe.reachable = true;
                probe.detail = format!("El servidor responde HTTP {status} en {base}");
            }
        }

        if probe.detail.is_empty() {
            probe.detail = format!("Sin respuesta en {base} (¿está iniciado llama-server?)");
        }

        probe
    }

    async fn get_json(&self, url: &str) -> Result<(reqwest::StatusCode, String), reqwest::Error> {
        let mut request = self.client.get(url).timeout(Duration::from_secs(2));
        if !self.config.api_key.is_empty() {
            request = request.bearer_auth(&self.config.api_key);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Ok((status, body))
    }

    /// Checks whether a managed child exited. Returns its exit code and the
    /// last log lines, and clears the child so it can be restarted.
    pub fn poll_exit(&mut self) -> Option<(Option<i32>, String)> {
        let status = self.child.as_mut()?.try_wait().ok().flatten()?;

        self.child = None;
        self.started_at = None;
        Some((status.code(), self.last_logs_joined(20)))
    }

    /// Returns the model id to send in chat requests.
    ///
    /// `llama-server` ignores it, but vLLM, LM Studio and gateways validate
    /// it. The configured `model_id` wins; otherwise the caller may inject the
    /// first id reported by `/v1/models`.
    pub fn effective_model_id(&self) -> String {
        let configured = self.config.model_id.trim();
        if !configured.is_empty() {
            configured.to_owned()
        } else {
            "local".to_owned()
        }
    }

    /// Sends a chat request and returns a channel of deltas.
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<mpsc::Receiver<ChatDelta>, LlmError> {
        let body = serde_json::json!({
            "model": self.effective_model_id(),
            "messages": messages,
            "stream": true,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "cache_prompt": true,
        });

        let response = {
            let mut request = self
                .client
                .post(self.config.chat_completions_url())
                .json(&body);

            if !self.config.api_key.is_empty() {
                request = request.bearer_auth(&self.config.api_key);
            }

            request.send().await?
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let body = body.chars().take(500).collect();
            return Err(LlmError::HttpStatus { status, body });
        }

        let mut stream = response.bytes_stream();
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = tx.send(ChatDelta::Error(error.to_string())).await;
                        return;
                    }
                };

                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                buffer.push_str(&text);

                for payload in take_sse_payloads(&mut buffer) {
                    if payload == "[DONE]" {
                        let _ = tx.send(ChatDelta::Done).await;
                        return;
                    }

                    match parse_delta(&payload) {
                        Ok(Some(delta)) => {
                            if tx.send(ChatDelta::Delta(delta)).await.is_err() {
                                return;
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let _ = tx.send(ChatDelta::Error(error)).await;
                            return;
                        }
                    }
                }
            }

            // A final event without the trailing blank line.
            let tail = buffer.trim();
            if !tail.is_empty() {
                for payload in take_sse_payloads(&mut format!("{tail}\n\n")) {
                    if payload == "[DONE]" {
                        break;
                    }
                    if let Ok(Some(delta)) = parse_delta(&payload) {
                        let _ = tx.send(ChatDelta::Delta(delta)).await;
                    }
                }
            }

            let _ = tx.send(ChatDelta::Done).await;
        });

        Ok(rx)
    }

    /// Stops the child and waits briefly for it to exit.
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
            tracing::info!("llama-server detenido");
        }
        self.started_at = None;
    }

    /// Returns the last `n` log lines, newest last.
    pub fn last_logs(&self, n: usize) -> Vec<String> {
        let logs = self.logs.lock().unwrap_or_else(|error| error.into_inner());
        let skip = logs.len().saturating_sub(n);
        logs.iter().skip(skip).cloned().collect()
    }

    fn last_logs_joined(&self, n: usize) -> String {
        self.last_logs(n).join(" | ")
    }

    /// Whether a child process is currently managed.
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// Time elapsed since the last start, if any.
    pub fn uptime(&self) -> Option<Duration> {
        self.started_at.map(|instant| instant.elapsed())
    }
}

impl Drop for LlamaManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}

/// Removes a trailing `/v1` so `/health` can be tried at the origin.
fn strip_v1(base: &str) -> String {
    base.trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base.trim_end_matches('/'))
        .to_owned()
}

/// Extracts model ids from an OpenAI `/v1/models` response.
fn parse_models(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };

    let list = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("models").and_then(serde_json::Value::as_array));

    list.map(|entries| {
        entries
            .iter()
            .filter_map(|entry| {
                entry
                    .get("id")
                    .or_else(|| entry.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .collect()
    })
    .unwrap_or_default()
}

fn spawn_log_reader<R>(reader: R, logs: Arc<Mutex<VecDeque<String>>>, stream: &'static str)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "llama", stream, "{line}");
            let mut logs = logs.lock().unwrap_or_else(|error| error.into_inner());
            if logs.len() == LOG_CAPACITY {
                logs.pop_front();
            }
            let line: String = line.chars().take(2048).collect();
            logs.push_back(format!("[{stream}] {line}"));
        }
    });
}

/// Extracts complete `data:` payloads from an SSE buffer.
///
/// The buffer keeps any incomplete trailing event. Pure function, unit-tested.
pub fn take_sse_payloads(buffer: &mut String) -> Vec<String> {
    let mut payloads = Vec::new();

    while let Some(position) = buffer.find("\n\n") {
        let block: String = buffer.drain(..position + 2).collect();
        for line in block.lines() {
            let line = line.trim_start();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if !data.is_empty() {
                    payloads.push(data.to_owned());
                }
            }
        }
    }

    payloads
}

/// Parses one SSE `data:` payload and extracts `choices[0].delta.content`.
///
/// Returns `Ok(None)` for role-only chunks. Pure function, unit-tested.
pub fn parse_delta(payload: &str) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct Chunk {
        #[serde(default)]
        choices: Vec<Choice>,
    }

    #[derive(Deserialize)]
    struct Choice {
        #[serde(default)]
        delta: Delta,
    }

    #[derive(Deserialize, Default)]
    struct Delta {
        #[serde(default)]
        content: Option<String>,
    }

    let chunk: Chunk =
        serde_json::from_str(payload).map_err(|error| format!("SSE inválido: {error}"))?;

    Ok(chunk
        .choices
        .into_iter()
        .find_map(|choice| choice.delta.content)
        .filter(|content| !content.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::LlmMode;
    use std::path::PathBuf;

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

    #[test]
    fn build_args_requires_a_model() {
        let mut config = configured();
        config.model = None;
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
        assert!(joined.contains("--jinja"));
        assert!(joined.contains("--no-webui"));
    }

    #[test]
    fn extra_args_are_appended_last() {
        let mut config = configured();
        config.extra_args = vec!["--flash-attn".to_owned(), "on".to_owned()];
        let args = LlamaManager::build_args(&config).expect("valid args");
        assert_eq!(&args[args.len() - 2..], &["--flash-attn", "on"]);
    }

    #[test]
    fn sse_payloads_split_and_keep_partial_tail() {
        let mut buffer = "data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: {\"partial\"".to_owned();
        let payloads = take_sse_payloads(&mut buffer);

        assert_eq!(payloads, vec!["{\"a\":1}", "{\"b\":2}"]);
        assert_eq!(buffer, "data: {\"partial\"");
    }

    #[test]
    fn sse_payloads_ignore_comments_and_empty_data() {
        let mut buffer = ": keep-alive\n\ndata:\n\ndata: [DONE]\n\n".to_owned();
        assert_eq!(take_sse_payloads(&mut buffer), vec!["[DONE]"]);
    }

    #[test]
    fn parse_delta_extracts_content_and_ignores_role_chunks() {
        let payload = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_delta(payload), Ok(None));

        let payload = r#"{"choices":[{"delta":{"content":"Hola"}}]}"#;
        assert_eq!(parse_delta(payload), Ok(Some("Hola".to_owned())));

        let payload = r#"{"choices":[{"delta":{"content":""}}]}"#;
        assert_eq!(parse_delta(payload), Ok(None));
    }

    #[test]
    fn parse_delta_rejects_invalid_json() {
        assert!(parse_delta("{not json}").is_err());
    }

    #[test]
    fn health_maps_unknown_server_to_down() {
        // Port 1 is not expected to be listening in tests.
        let mut config = configured();
        config.port = 1;
        let manager = LlamaManager::new(config, Duration::from_secs(1)).expect("manager");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        assert_eq!(runtime.block_on(manager.health()), LlmHealth::Down);
    }

    #[test]
    fn external_mode_builds_urls_with_or_without_v1() {
        let mut config = configured();
        config.mode = LlmMode::External;

        config.endpoint = "http://127.0.0.1:11434".to_owned();
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
        let mut config = configured();
        config.mode = LlmMode::External;
        config.model = None;
        config.endpoint = "http://127.0.0.1:11434".to_owned();

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

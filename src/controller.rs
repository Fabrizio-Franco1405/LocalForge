//! Engine controller thread.
//!
//! Owns every external process and HTTP client. The UI only sees
//! [`EngineCommand`] / [`EngineEvent`] values. The controller runs on a
//! dedicated OS thread with its own tokio runtime, so GPUI's executor never
//! blocks on process startup or model loading.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use llm_engine::{ChatDelta, ChatMessage, LlamaManager};
use shared::{
    engine_channel, DiagnosticItem, EngineCommand, EngineEvent, EngineHandle, LlmState, LlmMode,
    TtsBackend, VoxConfig,
};
use tokio::sync::mpsc;

/// Completion notices produced by background stream readers.
enum Internal {
    ReplyFinished { generation: u64, text: String },
}

struct Controller {
    config: VoxConfig,
    commands: Arc<Receiver<EngineCommand>>,
    events: Sender<EngineEvent>,
    internal_tx: mpsc::UnboundedSender<Internal>,
    internal_rx: mpsc::UnboundedReceiver<Internal>,
    llm: Option<LlamaManager>,
    history: Vec<ChatMessage>,
    streaming: Option<u64>,
    active: Arc<AtomicU64>,
}

/// Starts the controller and returns the handle used by the UI.
pub fn spawn(config: VoxConfig) -> EngineHandle {
    let (handle, commands, events) = engine_channel();

    std::thread::Builder::new()
        .name("voxlocal-controller".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build();

            match runtime {
                Ok(runtime) => {
                    runtime.block_on(Controller::new(config, commands, events).run());
                }
                Err(error) => tracing::error!(%error, "no se pudo crear el runtime del controlador"),
            }
        })
        .ok();

    handle
}

impl Controller {
    fn new(
        config: VoxConfig,
        commands: Receiver<EngineCommand>,
        events: Sender<EngineEvent>,
    ) -> Self {
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();

        Self {
            config,
            commands: Arc::new(commands),
            events,
            internal_tx,
            internal_rx,
            llm: None,
            history: Vec::new(),
            streaming: None,
            active: Arc::new(AtomicU64::new(0)),
        }
    }

    async fn run(mut self) {
        self.emit(EngineEvent::Llm {
            state: LlmState::Stopped,
            detail: "Sin iniciar".to_owned(),
        });

        if self.config.llm.auto_start {
            self.start_llm().await;
        }

        loop {
            tokio::select! {
                command = next_command(Arc::clone(&self.commands)) => {
                    match command {
                        Some(command) => {
                            if !self.handle(command).await {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                Some(internal) = self.internal_rx.recv() => {
                    self.handle_internal(internal);
                }
            }
        }

        if let Some(mut llm) = self.llm.take() {
            llm.stop().await;
        }
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    fn emit_error(&self, scope: &'static str, message: impl Into<String>) {
        self.emit(EngineEvent::Error {
            scope,
            message: message.into(),
        });
    }

    async fn start_llm(&mut self) {
        if let Some(mut old) = self.llm.take() {
            old.stop().await;
        }

        if !self.config.llm.is_managed() {
            let endpoint = self.config.llm.chat_completions_url();
            if self.config.llm.endpoint.trim().is_empty() {
                self.emit(EngineEvent::Llm {
                    state: LlmState::Error,
                    detail: "Falta el endpoint externo (Ajustes → Motor LLM)".to_owned(),
                });
            } else {
                self.emit(EngineEvent::Llm {
                    state: LlmState::Ready,
                    detail: endpoint,
                });
            }
            return;
        }

        if self.config.llm.model.is_none() {
            self.emit(EngineEvent::Llm {
                state: LlmState::Stopped,
                detail: "Conecta un modelo GGUF en Ajustes → Motor LLM".to_owned(),
            });
            return;
        }

        let timeout = Duration::from_secs(self.config.llm.startup_timeout_secs.max(5));
        let mut manager = match LlamaManager::new(self.config.llm.clone(), timeout) {
            Ok(manager) => manager,
            Err(error) => {
                self.emit_error("llm", error.to_string());
                return;
            }
        };

        let detail = self
            .config
            .llm
            .model
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "sin modelo configurado".to_owned());

        self.emit(EngineEvent::Llm {
            state: LlmState::Starting,
            detail,
        });

        if let Err(error) = manager.start().await {
            self.emit_error("llm", error.to_string());
            self.llm = Some(manager);
            return;
        }

        match manager.wait_ready().await {
            Ok(()) => {
                let detail = format!(
                    "{} · {} capas GPU",
                    self.config
                        .llm
                        .model
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "modelo".to_owned()),
                    self.config.llm.gpu_layers
                );
                self.emit(EngineEvent::Llm {
                    state: LlmState::Ready,
                    detail,
                });
            }
            Err(error) => {
                self.emit_error("llm", error.to_string());
                self.emit(EngineEvent::Llm {
                    state: LlmState::Error,
                    detail: "El motor no llegó a estar listo".to_owned(),
                });
            }
        }

        self.llm = Some(manager);
    }

    async fn stop_llm(&mut self) {
        if let Some(mut llm) = self.llm.take() {
            llm.stop().await;
        }
        self.active.store(0, Ordering::Relaxed);
        self.streaming = None;
        self.emit(EngineEvent::Llm {
            state: LlmState::Stopped,
            detail: "Detenido por el usuario".to_owned(),
        });
    }

    async fn handle(&mut self, command: EngineCommand) -> bool {
        match command {
            EngineCommand::StartLlm => self.start_llm().await,
            EngineCommand::StopLlm => self.stop_llm().await,
            EngineCommand::RestartLlm => {
                self.stop_llm().await;
                self.start_llm().await;
            }
            EngineCommand::ReloadConfig(config) => {
                if let Some(mut llm) = self.llm.take() {
                    llm.stop().await;
                }
                self.history.clear();
                self.streaming = None;
                self.active.store(0, Ordering::Relaxed);
                self.config = *config;

                if self.config.llm.auto_start {
                    self.start_llm().await;
                } else {
                    self.emit(EngineEvent::Llm {
                        state: LlmState::Stopped,
                        detail: "Configuración aplicada".to_owned(),
                    });
                }
                self.probe();
            }
            EngineCommand::SendMessage { generation, prompt } => {
                self.send_message(generation, prompt).await;
            }
            EngineCommand::Cancel { generation } => {
                if self.streaming == Some(generation) {
                    self.active.store(0, Ordering::Relaxed);
                    self.streaming = None;
                }
            }
            EngineCommand::Transcribe { .. } => {
                self.emit_error(
                    "stt",
                    "La captura de micrófono todavía no está conectada; escribe el mensaje por ahora.",
                );
            }
            EngineCommand::Speak { .. } => {
                if self.config.tts.backend == TtsBackend::Disabled {
                    self.emit_error(
                        "tts",
                        "No hay motor TTS conectado (Ajustes → Síntesis).",
                    );
                } else {
                    self.emit_error(
                        "tts",
                        "El motor TTS está configurado, pero la reproducción aún no está conectada.",
                    );
                }
            }
            EngineCommand::SetListening(_) => {
                self.emit_error(
                    "audio",
                    "La captura de audio aún no está conectada en esta compilación.",
                );
            }
            EngineCommand::ProbeTools => self.probe(),
            EngineCommand::Shutdown => return false,
        }

        true
    }

    async fn send_message(&mut self, generation: u64, prompt: String) {
        if self.config.llm.is_managed() && self.llm.is_none() {
            self.start_llm().await;
        }

        if self.llm.is_none() {
            self.emit_error(
                "llm",
                "El motor no está disponible. Revisa Ajustes → Motor LLM y pulsa Aplicar.",
            );
            return;
        }

        self.history.push(ChatMessage::user(prompt));

        let mut messages = Vec::with_capacity(self.history.len() + 1);
        if !self.config.llm.system_prompt.trim().is_empty() {
            messages.push(ChatMessage::system(self.config.llm.system_prompt.clone()));
        }
        messages.extend(self.history.iter().cloned());

        self.active.store(generation, Ordering::Relaxed);

        let stream = {
            let Some(llm) = self.llm.as_ref() else {
                return;
            };
            llm.chat_stream(
                &messages,
                self.config.llm.temperature,
                self.config.llm.max_tokens,
            )
            .await
        };

        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                self.history.pop();
                self.emit_error("llm", error.to_string());
                return;
            }
        };

        self.streaming = Some(generation);
        let events = self.events.clone();
        let internal = self.internal_tx.clone();
        let active = Arc::clone(&self.active);

        tokio::spawn(async move {
            let mut text = String::new();

            while let Some(delta) = stream.recv().await {
                match delta {
                    ChatDelta::Delta(chunk) => {
                        if active.load(Ordering::Relaxed) != generation {
                            continue;
                        }
                        text.push_str(&chunk);
                        let _ = events.send(EngineEvent::Token {
                            generation,
                            delta: chunk,
                        });
                    }
                    ChatDelta::Done => break,
                    ChatDelta::Error(message) => {
                        if active.load(Ordering::Relaxed) == generation {
                            let _ = events.send(EngineEvent::Error {
                                scope: "llm",
                                message,
                            });
                        }
                        break;
                    }
                }
            }

            let _ = internal.send(Internal::ReplyFinished { generation, text });
        });
    }

    fn handle_internal(&mut self, internal: Internal) {
        match internal {
            Internal::ReplyFinished { generation, text } => {
                if self.streaming != Some(generation) {
                    return;
                }

                if !text.trim().is_empty() {
                    self.history.push(ChatMessage::assistant(text.clone()));
                }
                self.streaming = None;
                self.active.store(0, Ordering::Relaxed);

                self.emit(EngineEvent::ReplyDone { generation, text });
            }
        }
    }

    fn probe(&self) {
        let cfg = &self.config;
        let mut items = Vec::new();

        match cfg.llm.mode {
            LlmMode::Managed => {
                items.push(binary_item("llama-server", &cfg.llm.binary));
                let (ok, value) = shared::tools::file_status(cfg.llm.model.as_deref());
                items.push(DiagnosticItem {
                    name: "Modelo GGUF".to_owned(),
                    value,
                    ok,
                });
                items.push(DiagnosticItem {
                    name: "Endpoint".to_owned(),
                    value: cfg.llm.chat_completions_url(),
                    ok: true,
                });
            }
            LlmMode::External => {
                let endpoint = cfg.llm.endpoint.trim();
                items.push(DiagnosticItem {
                    name: "Endpoint externo".to_owned(),
                    value: if endpoint.is_empty() {
                        "sin configurar".to_owned()
                    } else {
                        cfg.llm.chat_completions_url()
                    },
                    ok: endpoint.starts_with("http://") || endpoint.starts_with("https://"),
                });
            }
        }

        if cfg.stt.enabled {
            items.push(binary_item("whisper-cli", &cfg.stt.binary));
            let (ok, value) = shared::tools::file_status(cfg.stt.model.as_deref());
            items.push(DiagnosticItem {
                name: "Modelo Whisper".to_owned(),
                value,
                ok,
            });
        } else {
            items.push(DiagnosticItem {
                name: "whisper-cli".to_owned(),
                value: "dictado desactivado (opcional)".to_owned(),
                ok: true,
            });
        }

        match cfg.tts.backend {
            TtsBackend::Disabled => items.push(DiagnosticItem {
                name: "TTS".to_owned(),
                value: "sin conectar (opcional)".to_owned(),
                ok: true,
            }),
            TtsBackend::Command => {
                let program = shared::tools::command_program(&cfg.tts.command);
                let (ok, value) = match program {
                    Some(program) => {
                        let resolved =
                            shared::tools::resolve_binary(std::path::Path::new(program));
                        match resolved {
                            Some(path) => (true, format!("{} · {}", path.display(), cfg.tts.command)),
                            None => (false, format!("no encontrado: {program}")),
                        }
                    }
                    None => (false, "comando vacío".to_owned()),
                };
                items.push(DiagnosticItem {
                    name: "TTS (comando)".to_owned(),
                    value,
                    ok,
                });
            }
            TtsBackend::KokoroCli => {
                items.push(binary_item("Python (Kokoro)", &cfg.tts.python));
                let (ok, value) = shared::tools::file_status(Some(&cfg.tts.worker_script));
                items.push(DiagnosticItem {
                    name: "Script Kokoro".to_owned(),
                    value,
                    ok,
                });
                let (ok, value) = shared::tools::file_status(Some(&cfg.tts.model_dir));
                items.push(DiagnosticItem {
                    name: "Modelos Kokoro".to_owned(),
                    value,
                    ok,
                });
            }
        }

        let data_dir = cfg.data_dir();
        items.push(DiagnosticItem {
            name: "Carpeta de datos".to_owned(),
            value: if data_dir.exists() {
                data_dir.display().to_string()
            } else {
                format!("aún no existe: {}", data_dir.display())
            },
            ok: data_dir.exists(),
        });

        self.emit(EngineEvent::Diagnostics { items });
    }
}

fn binary_item(name: &str, binary: &std::path::Path) -> DiagnosticItem {
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

async fn next_command(receiver: Arc<Receiver<EngineCommand>>) -> Option<EngineCommand> {
    loop {
        match receiver.try_recv() {
            Ok(command) => return Some(command),
            Err(TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(TryRecvError::Disconnected) => return None,
        }
    }
}

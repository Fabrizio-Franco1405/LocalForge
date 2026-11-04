//! Engine controller thread.
//!
//! Owns every external process and HTTP client. The UI only sees
//! [`EngineCommand`] / [`EngineEvent`] values. The controller runs on a
//! dedicated OS thread with its own tokio runtime, so GPUI's executor never
//! blocks on process startup or model loading.
//!
//! The LLM state is never assumed: every change comes from an HTTP probe, so
//! a `llama-server` started outside VoxLocal is detected and used directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use llm_engine::{ChatDelta, ChatMessage, LlamaManager, ServerProbe};
use shared::{
    DiagnosticItem, EngineCommand, EngineEvent, EngineHandle, LlmMode, LlmState, TtsBackend,
    VoxConfig, engine_channel,
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
    /// Last successful probe and its human-readable detail.
    probe: ServerProbe,
    state: LlmState,
    detail: String,
    /// Model id reported by `/v1/models`, used by strict OpenAI servers.
    resolved_model: String,
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
                Err(error) => {
                    tracing::error!(%error, "no se pudo crear el runtime del controlador")
                }
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
            probe: ServerProbe::default(),
            state: LlmState::Stopped,
            detail: "Sin iniciar".to_owned(),
            resolved_model: String::new(),
            history: Vec::new(),
            streaming: None,
            active: Arc::new(AtomicU64::new(0)),
        }
    }

    async fn run(mut self) {
        self.set_state(LlmState::Stopped, "Buscando servidor…".to_owned());

        // Always probe first: a server started by the user must be detected
        // even when no GGUF model is configured in VoxLocal.
        self.refresh(self.config.llm.auto_start).await;

        let mut ticker = tokio::time::interval(Duration::from_secs(3));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
                _ = ticker.tick() => {
                    self.tick().await;
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

    /// Emits a lifecycle event only when something actually changed.
    fn set_state(&mut self, state: LlmState, detail: String) {
        if self.state == state && self.detail == detail {
            return;
        }
        self.state = state;
        self.detail = detail.clone();
        self.emit(EngineEvent::Llm { state, detail });
    }

    fn ensure_manager(&mut self) -> bool {
        if self.llm.is_some() {
            return true;
        }

        let timeout = Duration::from_secs(self.config.llm.startup_timeout_secs.max(5));
        match LlamaManager::new(self.config.llm.clone(), timeout) {
            Ok(manager) => {
                self.llm = Some(manager);
                true
            }
            Err(error) => {
                self.emit_error("llm", error.to_string());
                self.set_state(LlmState::Error, error.to_string());
                false
            }
        }
    }

    /// Applies the current config to the manager, keeping the resolved model id.
    fn sync_manager_config(&mut self) {
        if let Some(manager) = self.llm.as_mut() {
            let mut config = self.config.llm.clone();
            if config.model_id.trim().is_empty() {
                config.model_id = self.resolved_model.clone();
            }
            manager.set_config(config);
        }
    }

    fn adopt_models(&mut self, probe: &ServerProbe) {
        if !self.resolved_model.is_empty() {
            return;
        }

        let configured = self.config.llm.model_id.trim();
        if !configured.is_empty() {
            self.resolved_model = configured.to_owned();
            return;
        }

        if let Some(model) = probe.first_model() {
            self.resolved_model = model;
            self.sync_manager_config();
        }
    }

    /// Re-probes the configured endpoint and updates the lifecycle state.
    ///
    /// `force_start` controls whether a managed `llama-server` is spawned when
    /// nothing is listening. Booting honours `auto_start`; Apply and Restart
    /// force it so the buttons never look inert.
    async fn refresh(&mut self, force_start: bool) {
        if !self.ensure_manager() {
            return;
        }
        self.sync_manager_config();

        let probe = match self.llm.as_ref() {
            Some(manager) => manager.probe().await,
            None => return,
        };
        self.adopt_models(&probe);
        self.probe = probe.clone();

        if probe.ready {
            self.set_state(LlmState::Ready, probe.detail);
            return;
        }

        if !self.config.llm.is_managed() {
            self.external_state(&probe);
            return;
        }

        if !force_start && !self.llm.as_ref().is_some_and(LlamaManager::is_running) {
            let hint = if self.config.llm.model.is_some() {
                format!("{} · pulsa Aplicar o Reiniciar para arrancar", probe.detail)
            } else {
                format!(
                    "{} · configura un .gguf o inicia llama-server con --port {}",
                    probe.detail, self.config.llm.port
                )
            };
            self.set_state(LlmState::Stopped, hint);
            return;
        }

        if self.config.llm.model.is_none() {
            self.set_state(
                LlmState::Stopped,
                format!(
                    "Conecta un modelo GGUF o inicia llama-server en el puerto {}",
                    self.config.llm.port
                ),
            );
            return;
        }

        let model_name = self
            .config
            .llm
            .model
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "modelo".to_owned());

        if !self.llm.as_ref().is_some_and(LlamaManager::is_running) {
            self.set_state(
                LlmState::Starting,
                format!("Lanzando llama-server · {model_name}"),
            );
            let start = match self.llm.as_mut() {
                Some(manager) => manager.start().await,
                None => return,
            };
            if let Err(error) = start {
                // The port may be taken by a server that just came up; probe
                // once more before declaring failure.
                if let Some(manager) = self.llm.as_ref() {
                    let probe = manager.probe().await;
                    if probe.ready {
                        self.probe = probe.clone();
                        self.set_state(LlmState::Ready, probe.detail);
                        return;
                    }
                }
                self.emit_error("llm", error.to_string());
                self.set_state(LlmState::Error, error.to_string());
                return;
            }
        }

        self.set_state(
            LlmState::Starting,
            format!(
                "Cargando {model_name} · {} capas GPU",
                self.config.llm.gpu_layers
            ),
        );

        let ready = match self.llm.as_mut() {
            Some(manager) => manager.wait_ready().await,
            None => return,
        };

        match ready {
            Ok(()) => {
                let detail = format!(
                    "{model_name} · {} capas GPU · {}",
                    self.config.llm.gpu_layers,
                    self.config.llm.base_url()
                );
                self.set_state(LlmState::Ready, detail);
            }
            Err(error) => {
                self.emit_error("llm", error.to_string());
                self.set_state(
                    LlmState::Error,
                    "El motor no llegó a estar listo".to_owned(),
                );
            }
        }
    }

    fn external_state(&mut self, probe: &ServerProbe) {
        if self.config.llm.endpoint.trim().is_empty() {
            self.set_state(
                LlmState::Error,
                "Falta el endpoint externo (Ajustes → Motor LLM)".to_owned(),
            );
        } else if probe.reachable {
            self.set_state(LlmState::Starting, probe.detail.clone());
        } else {
            self.set_state(
                LlmState::Error,
                format!(
                    "No se pudo conectar a {} · inicia el servidor o revisa el puerto",
                    self.config.llm.chat_completions_url()
                ),
            );
        }
    }

    /// Probing pass while idle: detects external servers, child exits and
    /// servers that went away.
    /// Probing pass while idle: detects external servers, child exits and
    /// servers that went away.
    async fn tick(&mut self) {
        if let Some(manager) = self.llm.as_mut()
            && let Some((code, logs)) = manager.poll_exit()
        {
            let message = format!(
                "llama-server terminó (código {code:?}). {}",
                truncate(&logs, 400)
            );
            self.emit_error("llm", message.clone());
            self.set_state(LlmState::Error, message);
        }

        if !self.ensure_manager() {
            return;
        }
        self.sync_manager_config();

        let probe = match self.llm.as_ref() {
            Some(manager) => manager.probe().await,
            None => return,
        };
        self.adopt_models(&probe);
        self.probe = probe.clone();

        // 1. Si está listo, actualizamos botón Y herramientas
        if probe.ready {
            self.set_state(LlmState::Ready, probe.detail);
            self.emit_diagnostics().await; // <--- AÑADIDO AQUÍ
            return;
        }

        // 2. Si está arrancando, actualizamos botón Y herramientas
        if self.llm.as_ref().is_some_and(LlamaManager::is_running) {
            self.set_state(LlmState::Starting, probe.detail);
            self.emit_diagnostics().await; // <--- AÑADIDO AQUÍ
            return;
        }

        // 3. Si es alcanzable, actualizamos botón Y herramientas
        if probe.reachable {
            self.set_state(LlmState::Starting, probe.detail);
            self.emit_diagnostics().await; // <--- AÑADIDO AQUÍ
            return;
        }

        // 4. Si está apagado, actualizamos botón Y herramientas
        if self.state != LlmState::Error {
            let hint = if self.config.llm.is_managed() {
                format!(
                    "{} · inicia llama-server o pulsa Reiniciar motor",
                    probe.detail
                )
            } else {
                probe.detail.clone()
            };
            self.set_state(LlmState::Stopped, hint);
        }
        self.emit_diagnostics().await; // <--- Y AQUÍ AL FINAL POR SI ACASO
    }

    async fn probe_now(&mut self) -> ServerProbe {
        if !self.ensure_manager() {
            return self.probe.clone();
        }
        self.sync_manager_config();

        let probe = match self.llm.as_ref() {
            Some(manager) => manager.probe().await,
            None => return self.probe.clone(),
        };
        self.adopt_models(&probe);
        self.probe = probe.clone();

        if probe.ready {
            self.set_state(LlmState::Ready, probe.detail.clone());
        } else if probe.reachable {
            self.set_state(LlmState::Starting, probe.detail.clone());
        } else if self.state != LlmState::Error {
            self.set_state(LlmState::Stopped, probe.detail.clone());
        }

        probe
    }

    async fn stop_child(&mut self) {
        if let Some(mut manager) = self.llm.take() {
            manager.stop().await;
        }
        self.active.store(0, Ordering::Relaxed);
        self.streaming = None;
    }

    async fn handle(&mut self, command: EngineCommand) -> bool {
        match command {
            EngineCommand::StartLlm => self.refresh(true).await,
            EngineCommand::StopLlm => {
                self.stop_child().await;
                self.set_state(LlmState::Stopped, "Detenido por el usuario".to_owned());
            }
            EngineCommand::RestartLlm => {
                self.stop_child().await;
                self.resolved_model.clear();
                self.refresh(true).await;
            }
            EngineCommand::ReloadConfig(config) => {
                self.stop_child().await;
                self.history.clear();
                self.active.store(0, Ordering::Relaxed);
                self.config = *config;
                self.resolved_model.clear();
                self.llm = None;
                self.refresh(true).await;
            }
            EngineCommand::ProbeLlm => {
                self.probe_now().await;
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
                    self.emit_error("tts", "No hay motor TTS conectado (Ajustes → Síntesis).");
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
            EngineCommand::ProbeTools => self.emit_diagnostics().await,
            EngineCommand::Shutdown => return false,
        }

        true
    }

    async fn send_message(&mut self, generation: u64, prompt: String) {
        // If the status is stale (server just started), probe before failing.
        if self.state != LlmState::Ready {
            self.refresh(self.config.llm.is_managed()).await;
        }

        if self.state != LlmState::Ready {
            self.emit_error(
                "llm",
                format!(
                    "El motor no está listo: {}. Revisa Ajustes → Motor LLM.",
                    self.detail
                ),
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

        // --- ¡AQUÍ ESTÁ LA SOLUCIÓN! Guardamos el ctx_size antes del spawn ---
        let total_context = self.config.llm.ctx_size;

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
                    // --- USAMOS LA VARIABLE LOCAL ---
                    ChatDelta::Usage(used) => {
                        if active.load(Ordering::Relaxed) == generation {
                            let _ = events.send(EngineEvent::ContextUsage {
                                used,
                                total: total_context,
                            });
                        }
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
                if !text.trim().is_empty() {
                    self.history.push(ChatMessage::assistant(text.clone()));
                }

                // --- LÓGICA DE VENTANA DESLIZANTE ---
                // Mantenemos el System Prompt (índice 0) y solo 10 mensajes recientes.
                // Si el historial crece demasiado, borramos los más antiguos.
                while self.history.len() > 11 {
                    self.history.remove(1); // Elimina el mensaje de usuario más antiguo
                    self.history.remove(1); // Elimina la respuesta del asistente más antigua
                }

                self.streaming = None;
                self.active.store(0, Ordering::Relaxed);
                self.emit(EngineEvent::ReplyDone { generation, text });
            }
        }
    }

    async fn emit_diagnostics(&mut self) {
        let probe = self.probe_now().await;
        let cfg = &self.config;
        let mut items = Vec::new();

        items.push(DiagnosticItem {
            name: "Servidor LLM (en vivo)".to_owned(),
            value: probe.detail.clone(),
            ok: probe.ready,
        });

        items.push(DiagnosticItem {
            name: "Endpoint de chat".to_owned(),
            value: cfg.llm.chat_completions_url(),
            ok: true,
        });

        if let Some(model) = probe.first_model() {
            items.push(DiagnosticItem {
                name: "Modelo reportado".to_owned(),
                value: model,
                ok: true,
            });
        }

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
                    name: "Escucha gestionada".to_owned(),
                    value: format!(
                        "{}:{} · {} capas GPU",
                        cfg.llm.host, cfg.llm.port, cfg.llm.gpu_layers
                    ),
                    ok: true,
                });
            }
            LlmMode::External => {
                let endpoint = cfg.llm.normalized_endpoint();
                items.push(DiagnosticItem {
                    name: "Endpoint externo".to_owned(),
                    value: if endpoint.is_empty() {
                        "sin configurar".to_owned()
                    } else {
                        endpoint.clone()
                    },
                    ok: !endpoint.is_empty(),
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
                        match shared::tools::resolve_binary(std::path::Path::new(program)) {
                            Some(path) => {
                                (true, format!("{} · {}", path.display(), cfg.tts.command))
                            }
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

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(max).collect();
    truncated.push('…');
    truncated
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
    use std::time::Instant;

    /// Minimal OpenAI-compatible server used by the controller tests.
    struct MockServer {
        port: u16,
        stop: Arc<AtomicBool>,
        last_body: Arc<Mutex<String>>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl MockServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
            let port = listener.local_addr().expect("addr").port();
            listener
                .set_nonblocking(true)
                .expect("non blocking listener");

            let stop = Arc::new(AtomicBool::new(false));
            let last_body = Arc::new(Mutex::new(String::new()));
            let stop_thread = Arc::clone(&stop);
            let body_thread = Arc::clone(&last_body);

            let join = std::thread::spawn(move || {
                while !stop_thread.load(AtomicOrdering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => handle_mock(stream, &body_thread),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });

            Self {
                port,
                stop,
                last_body,
                join: Some(join),
            }
        }

        fn body(&self) -> String {
            self.last_body.lock().expect("body lock").clone()
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.stop.store(true, AtomicOrdering::Relaxed);
            // Wake the accept loop so the thread can exit.
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn handle_mock(mut stream: TcpStream, last_body: &Arc<Mutex<String>>) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");

        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            match stream.read(&mut chunk) {
                Ok(0) => return,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                Err(_) => return,
            }

            if let Some(position) = find_bytes(&buffer, b"\r\n\r\n") {
                break position + 4;
            }
        };

        let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
        let path = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_owned();
        let content_length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        while buffer.len() < header_end + content_length {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                Err(_) => break,
            }
        }

        let body = String::from_utf8_lossy(&buffer[header_end..]).to_string();
        if path.contains("chat/completions") {
            *last_body.lock().expect("body lock") = body;
        }

        let response = if path.ends_with("/v1/models") || path == "/models" {
            let body = r#"{"object":"list","data":[{"id":"mock-qwen","object":"model"}]}"#;
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        } else if path.contains("chat/completions") {
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hola\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\" mundo\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        } else {
            let body = r#"{"error":"not found"}"#;
            format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };

        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

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
        let mut config = VoxConfig::default();
        config.llm.mode = LlmMode::External;
        config.llm.endpoint = endpoint;
        config.llm.auto_start = false;
        config
    }

    #[test]
    fn detects_external_server_without_a_configured_model() {
        let server = MockServer::start();
        let mut config = VoxConfig::default();
        config.llm.mode = LlmMode::Managed;
        config.llm.model = None;
        config.llm.host = "127.0.0.1".to_owned();
        config.llm.port = server.port;
        config.llm.auto_start = false;

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
        // Bare host:port must be normalized to http://host:port.
        let handle = spawn(external_config(format!("127.0.0.1:{}", server.port)));

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
    fn truncate_keeps_utf8_boundaries() {
        assert_eq!(truncate("hola", 10), "hola");
        assert_eq!(truncate("hola mundo", 4), "hola…");
    }
}

//! Commands and events exchanged between the GPUI front-end and the engine
//! controller. Neither side shares engine internals: the UI sends commands and
//! consumes events, nothing else.

use std::sync::Arc;

use crate::config::VoxConfig;

/// Lifecycle of the local LLM server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmState {
    /// No process managed by the app.
    Stopped,
    /// Process spawned, model still loading.
    Starting,
    /// `/health` returned 200.
    Ready,
    /// Process exited or never started.
    Error,
}

impl LlmState {
    /// Short human label used by the status bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stopped => "Detenido",
            Self::Starting => "Cargando modelo…",
            Self::Ready => "Listo",
            Self::Error => "Error",
        }
    }
}

/// Operations the UI can request from the controller.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    /// Start `llama-server` if it is not running.
    StartLlm,
    /// Stop `llama-server` and its in-flight requests.
    StopLlm,
    /// Stop and start again with the current configuration.
    RestartLlm,
    /// Replace the configuration; restarts the engine when needed.
    ReloadConfig(Box<VoxConfig>),
    /// Send a user prompt and stream the assistant reply.
    SendMessage {
        /// Monotonic id used to discard stale events.
        generation: u64,
        /// User text.
        prompt: String,
    },
    /// Abort the in-flight generation identified by `generation`.
    Cancel {
        /// Generation to cancel.
        generation: u64,
    },
    /// Transcribe 16 kHz mono audio.
    Transcribe {
        /// Monotonic id used to discard stale events.
        generation: u64,
        /// PCM samples in `-1.0..=1.0`.
        samples: Arc<[f32]>,
    },
    /// Synthesize and play a reply.
    Speak {
        /// Monotonic id used to discard stale events.
        generation: u64,
        /// Text to speak. It is cleaned before synthesis.
        text: String,
    },
    /// Enable or disable microphone capture.
    SetListening(bool),
    /// Ask the controller to validate the connected tools and paths.
    ProbeTools,
    /// Re-probe the LLM endpoint and refresh its live status.
    ProbeLlm,
    /// Stop all processes and end the controller loop.
    Shutdown,
}

/// One row of the tools/diagnostics page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticItem {
    /// Tool or check name.
    pub name: String,
    /// Resolved path, endpoint or human explanation.
    pub value: String,
    /// Whether the check passed.
    pub ok: bool,
}

/// Notifications the controller sends to the UI.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// LLM lifecycle changed.
    Llm {
        /// New state.
        state: LlmState,
        /// Human readable detail (model, error, device).
        detail: String,
    },
    /// A streamed token from the assistant.
    Token {
        /// Generation this token belongs to.
        generation: u64,
        /// Text delta (may contain several characters).
        delta: String,
    },
    /// The assistant reply finished (or was cancelled).
    ReplyDone {
        /// Generation that finished.
        generation: u64,
        /// Full text produced so far.
        text: String,
    },
    /// A transcription finished.
    Transcript {
        /// Generation that produced it.
        generation: u64,
        /// Recognized text.
        text: String,
    },
    /// Microphone input level (`0.0..=1.0`) for a waveform.
    SpeechLevel(f32),
    /// Text-to-speech started or stopped.
    Speaking(bool),
    /// Result of [`EngineCommand::ProbeTools`].
    Diagnostics {
        /// Checks, in display order.
        items: Vec<DiagnosticItem>,
    },
    ContextUsage {
        /// Tokens usados en el historial actual.
        used: u32,
        /// Límite máximo configurado (-c).
        total: u32,
    },
    /// A recoverable error. The operation that produced it is finished.
    Error {
        /// Component that failed (`llm`, `stt`, `tts`, `audio`).
        scope: &'static str,
        /// Human readable message.
        message: String,
    },
}

/// Clonable facade used by the UI.
///
/// The command sender and event receiver live on opposite ends of the
/// controller thread, so no engine type ever reaches the UI.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    commands: crossbeam_channel::Sender<EngineCommand>,
    events: crossbeam_channel::Receiver<EngineEvent>,
}

impl EngineHandle {
    /// Sends a command, returning `false` if the controller is gone.
    pub fn send(&self, command: EngineCommand) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Non-blocking event read.
    pub fn try_recv(&self) -> Option<EngineEvent> {
        self.events.try_recv().ok()
    }

    /// Drains all pending events. The UI pump calls this once per tick.
    pub fn drain(&self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.try_recv() {
            events.push(event);
        }
        events
    }

    /// Blocks until an event arrives or the timeout expires. For tests and
    /// tools, never for the UI thread.
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<EngineEvent> {
        self.events.recv_timeout(timeout).ok()
    }
}

/// Creates the two ends of the UI ⇄ controller channel.
pub fn engine_channel() -> (
    EngineHandle,
    crossbeam_channel::Receiver<EngineCommand>,
    crossbeam_channel::Sender<EngineEvent>,
) {
    let (command_tx, command_rx) = crossbeam_channel::unbounded();
    let (event_tx, event_rx) = crossbeam_channel::unbounded();

    (
        EngineHandle {
            commands: command_tx,
            events: event_rx,
        },
        command_rx,
        event_tx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_round_trips_commands_and_events() {
        let (handle, commands, events) = engine_channel();

        assert!(handle.send(EngineCommand::StartLlm));
        assert!(matches!(commands.recv().unwrap(), EngineCommand::StartLlm));

        events
            .send(EngineEvent::Llm {
                state: LlmState::Ready,
                detail: "ok".to_owned(),
            })
            .unwrap();

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

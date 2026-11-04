//! Configuration model for VoxLocal.
//!
//! Every section has sensible defaults and is optional in the TOML file, so a
//! partial configuration keeps working across releases. Environment variables
//! use the `VOXLOCAL_` prefix and `__` as the nesting separator:
//!
//! ```text
//! VOXLOCAL_LLM__PORT=9940
//! VOXLOCAL_AUDIO__VAD__MIN_SILENCE_MS=800
//! ```

use std::path::{Path, PathBuf};

use figment::providers::Format as _;
use serde::{Deserialize, Serialize};

/// Root configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxConfig {
    /// Text generation engine (llama.cpp).
    pub llm: LlmConfig,
    /// Speech-to-text engine (whisper.cpp).
    pub stt: SttConfig,
    /// Text-to-speech engine (Kokoro ONNX).
    pub tts: TtsConfig,
    /// Capture, playback and voice activity detection.
    pub audio: AudioConfig,
    /// Desktop user interface.
    pub ui: UiConfig,
    /// Filesystem locations.
    pub paths: PathsConfig,
}

impl VoxConfig {
    /// Loads the configuration from `path`.
    ///
    /// A missing file is not an error: it yields the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        figment::Figment::new()
            .merge(figment::providers::Toml::file(path))
            .merge(figment::providers::Env::prefixed("VOXLOCAL_").split("__"))
            .extract()
            .map_err(|error| ConfigError::Invalid(Box::new(error)))
    }

    /// Saves the configuration as pretty TOML, creating parent directories.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let rendered = toml::to_string_pretty(self)
            .map_err(|error| ConfigError::Serialize(error.to_string()))?;

        std::fs::write(path, rendered).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Default per-user configuration path.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("voxlocal")
            .join("config.toml")
    }

    /// Directory for logs and future session files.
    pub fn data_dir(&self) -> PathBuf {
        self.paths.data_dir.clone().unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("voxlocal")
        })
    }

    /// Directory that contains `models/` and `bin/`.
    pub fn assets_dir(&self) -> PathBuf {
        self.paths
            .assets_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("assets"))
    }
}

/// How the chat endpoint is provided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmMode {
    /// VoxLocal spawns and supervises `llama-server`.
    #[default]
    Managed,
    /// The user provides an OpenAI-compatible server (Ollama, LM Studio,
    /// llama-server already running, a remote gateway...).
    External,
}

/// llama.cpp / OpenAI-compatible server configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Managed process or user-provided endpoint.
    pub mode: LlmMode,
    /// `llama-server` binary. Resolved through `PATH` when it has no separators.
    pub binary: PathBuf,
    /// Absolute path to a `.gguf` model (managed mode).
    pub model: Option<PathBuf>,
    /// Base URL for external mode, for example `http://127.0.0.1:11434/v1`.
    ///
    /// `127.0.0.1:9931` is accepted too; the scheme is added automatically.
    pub endpoint: String,
    /// Optional model id sent to external servers. When empty, the first id
    /// reported by `/v1/models` is used.
    pub model_id: String,
    /// Optional bearer token for external gateways. Stored in plain text.
    pub api_key: String,
    /// Bind address for the local server.
    pub host: String,
    /// Bind port. Recent llama.cpp builds default to 9931.
    pub port: u16,
    /// Layers offloaded to Vulkan (`-ngl`). 99 means "all".
    pub gpu_layers: i32,
    /// Context size (`-c`).
    pub ctx_size: u32,
    /// CPU threads used by the engine (`-t`).
    pub threads: u16,
    /// Sampling temperature.
    pub temperature: f32,
    /// Maximum tokens per reply.
    pub max_tokens: u32,
    /// System prompt prepended to every conversation.
    pub system_prompt: String,
    /// Extra raw arguments for advanced users.
    pub extra_args: Vec<String>,
    /// Start the engine when the app launches.
    pub auto_start: bool,
    /// How long to wait for the model to load.
    pub startup_timeout_secs: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            mode: LlmMode::Managed,
            binary: PathBuf::from("llama-server"),
            model: None,
            endpoint: String::new(),
            model_id: String::new(),
            api_key: String::new(),
            host: "127.0.0.1".to_owned(),
            port: 9931,
            gpu_layers: 99,
            ctx_size: 4096,
            threads: 8,
            temperature: 0.7,
            max_tokens: 512,
            system_prompt:
                "Eres un asistente de voz local. Responde de forma breve, clara y natural, sin Markdown."
                    .to_owned(),
            extra_args: Vec::new(),
            auto_start: true,
            startup_timeout_secs: 90,
        }
    }
}

impl LlmConfig {
    /// Base URL used by the HTTP client.
    ///
    /// Wildcard bind addresses are not reachable as clients; map them to
    /// loopback.
    pub fn base_url(&self) -> String {
        let host = match self.host.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            other => other,
        };
        format!("http://{host}:{}", self.port)
    }

    /// HTTP base for the selected mode.
    ///
    /// External endpoints may be written as `127.0.0.1:9931`; the missing
    /// scheme is added so the user can paste exactly what the server prints.
    pub fn http_base(&self) -> String {
        if self.mode == LlmMode::External && !self.endpoint.trim().is_empty() {
            let raw = self.endpoint.trim().trim_end_matches('/');
            if raw.starts_with("http://") || raw.starts_with("https://") {
                raw.to_owned()
            } else {
                format!("http://{raw}")
            }
        } else {
            self.base_url()
        }
    }

    /// Endpoint exactly as the user wrote it, normalized for display.
    pub fn normalized_endpoint(&self) -> String {
        if self.endpoint.trim().is_empty() {
            String::new()
        } else {
            self.http_base()
        }
    }

    /// Full chat-completions URL, accepting bases with or without `/v1`.
    pub fn chat_completions_url(&self) -> String {
        self.api_url("chat/completions")
    }

    /// Full models URL, used as an external-mode health probe.
    pub fn models_url(&self) -> String {
        self.api_url("models")
    }

    fn api_url(&self, suffix: &str) -> String {
        let base = self.http_base();
        if base.ends_with("/v1") {
            format!("{base}/{suffix}")
        } else {
            format!("{base}/v1/{suffix}")
        }
    }

    /// Whether VoxLocal owns the process.
    pub fn is_managed(&self) -> bool {
        self.mode == LlmMode::Managed
    }
}

/// whisper.cpp configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SttConfig {
    /// Whether dictation is enabled.
    pub enabled: bool,
    /// `whisper-cli` binary.
    pub binary: PathBuf,
    /// Path to a `ggml-*.bin` model.
    pub model: Option<PathBuf>,
    /// Language hint (`es`, `en`, ...).
    pub language: String,
    /// Threads used by whisper.cpp.
    pub threads: u16,
    /// Translate to English instead of transcribing.
    pub translate: bool,
    /// Extra raw arguments.
    pub extra_args: Vec<String>,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            binary: PathBuf::from("whisper-cli"),
            model: None,
            language: "es".to_owned(),
            threads: 8,
            translate: false,
            extra_args: Vec::new(),
        }
    }
}

/// Text-to-speech backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsBackend {
    /// The user has not connected a TTS tool yet.
    #[default]
    Disabled,
    /// Bundled helper script using `kokoro-onnx` (ONNX Runtime CPU). The user
    /// provides the interpreter, script and model files.
    KokoroCli,
    /// Any command that writes a WAV file; text is sent on stdin.
    Command,
}

/// Kokoro / external TTS configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    /// Which backend to use.
    pub backend: TtsBackend,
    /// Python interpreter inside the isolated virtual environment.
    pub python: PathBuf,
    /// Path to `tools/kokoro_worker.py`.
    pub worker_script: PathBuf,
    /// Directory containing the ONNX model and `voices-v1.0.bin`.
    pub model_dir: PathBuf,
    /// Kokoro voice id (`ef_dora`, `em_alex`, `af_heart`, ...).
    pub voice: String,
    /// Speaking speed (`0.5..=2.0`).
    pub speed: f32,
    /// Command template for [`TtsBackend::Command`].
    ///
    /// Supported placeholders: `{out}`, `{voice}`, `{speed}`. The text to
    /// synthesize is written to stdin.
    pub command: String,
    /// Extra raw arguments appended to the worker command.
    pub extra_args: Vec<String>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            backend: TtsBackend::Disabled,
            python: PathBuf::new(),
            worker_script: PathBuf::new(),
            model_dir: PathBuf::new(),
            voice: String::new(),
            speed: 1.0,
            command: String::new(),
            extra_args: Vec::new(),
        }
    }
}

/// Audio capture, playback and VAD.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Input device name; empty means the system default.
    pub input_device: String,
    /// Output device name; empty means the system default.
    pub output_device: String,
    /// Sample rate of the analysis pipeline (must stay 16 kHz for STT).
    pub sample_rate: u32,
    /// Playback gain (`0.0..=1.0`).
    pub playback_volume: f32,
    /// Voice activity detection parameters.
    pub vad: VadConfig,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            output_device: String::new(),
            sample_rate: 16_000,
            playback_volume: 1.0,
            vad: VadConfig::default(),
        }
    }
}

/// WebRTC VAD + endpointing parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VadConfig {
    /// WebRTC mode: `low`, `aggressive` or `quality`.
    pub mode: String,
    /// Voiced audio required to open an utterance.
    pub min_speech_ms: u32,
    /// Silence required to close an utterance.
    pub min_silence_ms: u32,
    /// Audio kept before the first voiced frame.
    pub padding_ms: u32,
    /// Hard limit for a single utterance.
    pub max_utterance_ms: u32,
    /// Enable full-duplex barge-in (needs headphones or AEC).
    pub full_duplex: bool,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            mode: "quality".to_owned(),
            min_speech_ms: 250,
            min_silence_ms: 650,
            padding_ms: 300,
            max_utterance_ms: 30_000,
            full_duplex: false,
        }
    }
}

/// User interface preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// `dark`, `light` or `system`.
    pub theme: String,
    /// Accent preset: `violet`, `blue`, `emerald`, `amber`, `rose` or `custom`.
    pub accent: String,
    /// Hex color (`#rrggbb`) used when `accent = "custom"`.
    pub accent_custom: String,
    /// Corner radius in pixels (`0..=20`).
    pub radius: f32,
    /// Base font size in pixels (`12..=20`).
    pub font_size: f32,
    /// Show the debug/status panel.
    pub show_debug: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            accent: "violet".to_owned(),
            accent_custom: "#7c5cff".to_owned(),
            radius: 8.0,
            font_size: 14.0,
            show_debug: false,
        }
    }
}

impl UiConfig {
    pub fn accent_hex(&self) -> String {
        match self.accent.as_str() {
            "violet" => "#7c5cff".to_owned(),
            "blue" => "#3b82f6".to_owned(),
            "emerald" => "#10b981".to_owned(),
            "amber" => "#f59e0b".to_owned(),
            "rose" => "#f43f5e".to_owned(),
            "custom" => self.accent_custom.clone(), // Si es custom, usa el campo de texto
            _ => "#7c5cff".to_owned(),              // Por defecto, violeta
        }
    }
}

/// Optional filesystem overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PathsConfig {
    /// Directory with `models/` and `bin/`.
    pub assets_dir: Option<PathBuf>,
    /// Directory for logs and sessions.
    pub data_dir: Option<PathBuf>,
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// I/O failure while reading or writing.
    #[error("no se pudo acceder a {path}: {source}")]
    Io {
        /// Offending path.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The TOML or environment values are not valid for the schema.
    #[error("configuración inválida: {0}")]
    Invalid(Box<figment::Error>),
    /// Serialization to TOML failed.
    #[error("no se pudo serializar la configuración: {0}")]
    Serialize(String),
}

//! Shared types for VoxLocal.
//!
//! This crate is a leaf: it must not depend on any other workspace crate or on
//! GPUI. It holds the configuration schema, the UI ⇄ engine contract and the
//! text utilities used by every backend.

pub mod config;
pub mod event;
pub mod text;
pub mod tools;

pub use config::{
    AudioConfig, ConfigError, LlmConfig, LlmMode, PathsConfig, SttConfig, TtsBackend, TtsConfig,
    UiConfig, VadConfig, VoxConfig,
};
pub use event::{
    engine_channel, DiagnosticItem, EngineCommand, EngineEvent, EngineHandle, LlmState,
};
pub use text::for_speech;

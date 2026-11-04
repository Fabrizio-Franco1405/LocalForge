//! LocalForge desktop UI.
//!
//! The crate only depends on `shared` (the UI ⇄ engine contract) and GPUI Kit.
//! It never imports a process manager, an HTTP client or an audio backend.

pub mod app;
pub mod chat;
pub mod config;
pub mod settings;
pub mod theme;

pub use app::{Page, VoxApp};
pub use config::{GlobalConfig, init};

//! Global configuration state shared by every view.
//!
//! The settings component writes through [`update`], which persists the TOML
//! file immediately. Engine processes are only restarted when the user presses
//! *Aplicar*, so typing a path does not spawn anything.
//!
//! [`GlobalConfig`] is `Clone`, and [`update`] re-installs it with
//! [`App::set_global`]. That is what notifies GPUI global observers, so views
//! (settings pages, status bar, theme) rebuild as soon as a value changes.
//! Mutating it in place with `global_mut` would **not** notify anyone.

use std::path::{Path, PathBuf};

use gpui_kit::*;
use shared::{EngineHandle, ModelEntry, VoxConfig};

/// Global holding the live configuration and the last applied snapshot.
#[derive(Clone)]
pub struct GlobalConfig {
    /// Values currently shown and edited in the settings page.
    pub config: VoxConfig,
    /// Values the controller is using. Compared with [`Self::config`] to show
    /// the "cambios sin aplicar" badge.
    pub applied: VoxConfig,
    /// Where the TOML file lives.
    pub path: PathBuf,
    /// Last persistence error, if any.
    pub save_error: Option<String>,
}

impl Global for GlobalConfig {}

impl GlobalConfig {
    /// Builds the global state.
    pub fn new(config: VoxConfig, path: PathBuf) -> Self {
        let applied = config.clone();
        Self {
            config,
            applied,
            path,
            save_error: None,
        }
    }

    /// Whether the edited values differ from the running ones.
    pub fn dirty(&self) -> bool {
        self.config != self.applied
    }

    /// Marks the current values as applied to the controller.
    pub fn mark_applied(&mut self) {
        self.applied = self.config.clone();
    }
}

/// Installs the globals. Call once, before opening the window.
pub fn init(cx: &mut App, config: VoxConfig, path: PathBuf) {
    cx.set_global(GlobalConfig::new(config, path));
    cx.set_global(ModelLibrary::default());
    cx.set_global(AudioDevices::default());
}

/// Audio devices reported by the controller.
#[derive(Clone, Default)]
pub struct AudioDevices {
    /// Microphone names.
    pub inputs: Vec<String>,
    /// Speaker names.
    pub outputs: Vec<String>,
}

impl Global for AudioDevices {}

/// Latest device list.
pub fn audio_devices(cx: &App) -> AudioDevices {
    cx.try_global::<AudioDevices>().cloned().unwrap_or_default()
}

/// Replaces the device list.
pub fn set_audio_devices(cx: &mut App, inputs: Vec<String>, outputs: Vec<String>) {
    cx.set_global(AudioDevices { inputs, outputs });
}

/// Models discovered by the controller.
#[derive(Clone, Default)]
pub struct ModelLibrary {
    /// `.gguf` files found in the configured folder.
    pub entries: Vec<ModelEntry>,
}

impl Global for ModelLibrary {}

/// Handle used by settings controls that need to ask the controller for work
/// (for example, rescanning the model folder).
#[derive(Clone)]
pub struct EngineBridge(pub EngineHandle);

impl Global for EngineBridge {}

/// Installs the engine handle so settings buttons can talk to the controller.
pub fn set_engine(cx: &mut App, engine: EngineHandle) {
    cx.set_global(EngineBridge(engine));
}

/// Cloned engine handle, if installed.
pub fn engine(cx: &App) -> Option<EngineHandle> {
    cx.try_global::<EngineBridge>()
        .map(|bridge| bridge.0.clone())
}

/// Current model library.
pub fn models(cx: &App) -> Vec<ModelEntry> {
    cx.try_global::<ModelLibrary>()
        .map(|library| library.entries.clone())
        .unwrap_or_default()
}

/// Friendly label for the selected model (alias, file stem or placeholder).
pub fn model_label(cx: &App) -> String {
    let cfg = current(cx);
    let Some(path) = cfg.llm.model.as_ref() else {
        return "Sin modelo".to_owned();
    };
    cfg.llm
        .model_aliases
        .get(&path.display().to_string())
        .filter(|alias| !alias.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("modelo")
                .to_owned()
        })
}

/// Replaces the model library and refreshes observers.
pub fn set_models(cx: &mut App, entries: Vec<ModelEntry>) {
    cx.set_global(ModelLibrary { entries });
}

/// Returns a copy of the live configuration.
pub fn current(cx: &App) -> VoxConfig {
    cx.global::<GlobalConfig>().config.clone()
}

/// Path of the TOML file.
pub fn path(cx: &App) -> PathBuf {
    cx.global::<GlobalConfig>().path.clone()
}

/// Whether there are unapplied changes.
pub fn is_dirty(cx: &App) -> bool {
    cx.try_global::<GlobalConfig>()
        .is_some_and(GlobalConfig::dirty)
}

/// Edits the configuration, writes it to disk, and notifies observers.
///
/// The running engines still only change when the user presses *Aplicar*; this
/// function only updates the edited draft and its persisted TOML.
pub fn update(cx: &mut App, f: impl FnOnce(&mut VoxConfig)) {
    let mut global = cx.global::<GlobalConfig>().clone();
    f(&mut global.config);

    let result = global.config.save(&global.path);
    global.save_error = result.err().map(|error| error.to_string());

    let ui = global.config.ui.clone();

    // `set_global` emits `NotifyGlobalObservers`, which is what makes the
    // settings pages rebuild (managed ↔ external fields) and the theme update.
    cx.set_global(global);

    // Re-apply appearance so accent / mode / radius / font size land at once.
    crate::theme::apply(cx, &ui, None);
}

/// Marks the current draft as applied without changing any value.
pub fn mark_applied(cx: &mut App) {
    let mut global = cx.global::<GlobalConfig>().clone();
    global.mark_applied();
    cx.set_global(global);
}

/// Writes the current configuration to `path`.
pub fn persist(cx: &mut App, path: &Path) {
    let result = cx.global::<GlobalConfig>().config.save(path);
    let global = cx.global_mut::<GlobalConfig>();
    global.save_error = result.err().map(|error| error.to_string());
}

/// Replaces both the live and the applied snapshot (used on load).
pub fn set(cx: &mut App, config: VoxConfig) {
    let config_path = path(cx);
    cx.set_global(GlobalConfig::new(config, config_path));
}

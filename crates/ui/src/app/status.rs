//! Bottom status bar.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{LlmMode, TtsBackend};

use crate::app::VoxApp;
use crate::config;

impl VoxApp {
    pub(super) fn status_bar(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let cfg = config::current(cx);
        let mode = match cfg.llm.mode {
            LlmMode::Managed => "gestionado",
            LlmMode::External => "externo",
        };
        let stt = if cfg.stt.enabled {
            "activo"
        } else {
            "desactivado"
        };
        let tts = match cfg.tts.backend {
            TtsBackend::Disabled => "desactivado",
            TtsBackend::KokoroCli => "Kokoro CLI",
            TtsBackend::Command => "comando",
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px_4()
            .py_1()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("LLM: {mode}"))
                    .child(format!("STT: {stt}"))
                    .child(format!("TTS: {tts}"))
                    .when(cfg.ui.show_debug, |this| {
                        this.child(format!("generación: {}", self.generation))
                            .child(format!("diagnósticos: {}", self.diagnostics.len()))
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("LocalForge v{}", env!("CARGO_PKG_VERSION"))),
            )
    }
}

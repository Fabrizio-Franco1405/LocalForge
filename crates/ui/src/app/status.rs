//! Bottom status bar.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
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
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Icon::new(IconName::Cpu)
                                    .small()
                                    .text_color(theme.muted_foreground),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                match self.cpu {
                                    Some(value) => format!("{value:.0}%"),
                                    None => "—".to_owned(),
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Icon::new(IconName::MemoryStick)
                                    .small()
                                    .text_color(theme.muted_foreground),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                match (self.ram_used_mb, self.ram_total_mb) {
                                    (Some(used), Some(total)) => {
                                        format!(
                                            "{:.1}/{:.1} GB",
                                            used as f32 / 1024.0,
                                            total as f32 / 1024.0
                                        )
                                    }
                                    _ => "—".to_owned(),
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "app {} MB",
                                self.app_mb
                                    .map_or_else(|| "—".to_owned(), |v| v.to_string())
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("LocalForge v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
    }
}

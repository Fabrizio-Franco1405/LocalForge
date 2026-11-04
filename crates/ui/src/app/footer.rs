//! Global footer: engine status and resource usage, visible in every workspace.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::*;
use shared::{LlmState, TtsBackend};

use crate::app::VoxApp;
use crate::config;

impl VoxApp {
    /// Bottom bar shared by all workspaces.
    pub(super) fn footer(&self, cx: &App) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let cfg = config::current(cx);

        let (llm_color, llm_label) = match self.llm {
            LlmState::Stopped => (theme.muted_foreground, "LLM detenido"),
            LlmState::Starting => (theme.warning, "LLM cargando"),
            LlmState::Ready => (theme.success, "LLM listo"),
            LlmState::Error => (theme.danger, "LLM error"),
        };

        let (mic_label, mic_color) = if self.listening {
            ("Mic activo", theme.success)
        } else if cfg.stt.enabled {
            ("Mic listo", theme.muted_foreground)
        } else {
            ("Mic off", theme.muted_foreground)
        };

        let (tts_label, tts_color) = if cfg.tts.backend == TtsBackend::Disabled {
            ("TTS off", theme.muted_foreground)
        } else {
            ("TTS listo", theme.success)
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(llm_color))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.foreground)
                                    .child(llm_label),
                            ),
                    )
                    .child(metric(
                        IconName::Cpu,
                        match self.cpu {
                            Some(value) => format!("{value:.0}%"),
                            None => "—".to_owned(),
                        },
                        theme.muted_foreground,
                        cx,
                    ))
                    .child(metric(
                        IconName::MemoryStick,
                        match (self.ram_used_mb, self.ram_total_mb) {
                            (Some(used), Some(total)) => format!(
                                "{:.1}/{:.1} GB",
                                used as f32 / 1024.0,
                                total as f32 / 1024.0
                            ),
                            _ => "—".to_owned(),
                        },
                        theme.muted_foreground,
                        cx,
                    ))
                    .child(metric(
                        IconName::CircuitBoard,
                        match (self.vram_used_mb, self.vram_total_mb) {
                            (Some(used), Some(total)) => format!(
                                "{:.1}/{:.1} GB",
                                used as f32 / 1024.0,
                                total as f32 / 1024.0
                            ),
                            _ => "—".to_owned(),
                        },
                        theme.muted_foreground,
                        cx,
                    ))
                    .child(metric(IconName::Mic, mic_label.to_owned(), mic_color, cx))
                    .child(metric(
                        IconName::Volume2,
                        tts_label.to_owned(),
                        tts_color,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "app {} MB",
                                self.app_mb
                                    .map_or_else(|| "—".to_owned(), |value| value.to_string())
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

fn metric(icon: IconName, text: String, color: Hsla, cx: &App) -> impl IntoElement {
    let theme = cx.theme().clone();

    div()
        .flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).small().text_color(color))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text),
        )
}

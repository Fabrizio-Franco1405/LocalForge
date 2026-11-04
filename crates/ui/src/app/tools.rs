//! Tools and diagnostics page.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{DiagnosticItem, LlmMode};

use crate::app::VoxApp;
use crate::config;

impl VoxApp {
    pub(super) fn tools_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let cfg = config::current(cx);

        div()
            .id("tools-scroll")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child("Herramientas conectadas"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Solo se comprueban rutas y binarios. No se descarga nada."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("probe")
                                    .small()
                                    .primary()
                                    .label(if self.probing {
                                        "Comprobando…"
                                    } else {
                                        "Comprobar de nuevo"
                                    })
                                    .disabled(self.probing)
                                    .on_click(cx.listener(|this, _, _, cx| this.probe(cx))),
                            )
                            .child(
                                Button::new("reload")
                                    .small()
                                    .ghost()
                                    .label("Aplicar y reiniciar")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.group_box)
                    .children(self.diagnostics.iter().map(|item| {
                        diagnostic_row(item, cx)
                    }))
                    .when(self.diagnostics.is_empty(), |this| {
                        this.child(
                            div()
                                .p_4()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(if self.probing {
                                    "Comprobando binarios y modelos…"
                                } else {
                                    "Pulsa «Comprobar de nuevo» para inspeccionar la configuración."
                                }),
                        )
                    }),
            )
            .child(info_card(
                "Archivo de configuración",
                &config::path(cx).display().to_string(),
                cx,
            ))
            .child(info_card(
                "Backend",
                "Vulkan es el único backend soportado. Si tu llama-server se compiló solo con CPU, \
                 lo verás en su salida y en el tiempo de carga, pero LocalForge no cambia drivers ni instala ROCm/CUDA.",
                cx,
            ))
            .child(info_card(
                "Modo actual",
                &match cfg.llm.mode {
                    LlmMode::Managed => format!(
                        "Gestionado: {} · modelo {}",
                        cfg.llm.binary.display(),
                        cfg.llm
                            .model
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "sin configurar".to_owned())
                    ),
                    LlmMode::External => format!("Externo: {}", cfg.llm.chat_completions_url()),
                },
                cx,
            ))
    }
}

fn diagnostic_row(item: &DiagnosticItem, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let color = if item.ok { theme.success } else { theme.danger };
    let label = if item.ok { "OK" } else { "Falta" };

    div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(theme.border)
        .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(color))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(item.name.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(item.value.clone()),
                ),
        )
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(color)
                .child(label),
        )
}
fn info_card(title: &'static str, body: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .p_3()
        .child(
            div()
                .text_sm()
                .font_semibold()
                .text_color(theme.foreground)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(body.to_owned()),
        )
}

//! Ajustes → Sistema: hardware detected by the core and tool diagnostics.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{DiagnosticItem, EngineCommand};

use crate::app::VoxApp;

impl VoxApp {
    pub(super) fn system_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let hardware = self.hardware.clone();
        let diagnostics = self.diagnostics.clone();
        let probing = self.probing;

        div()
            .id("system-scroll")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p_5()
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
                                    .child("Hardware detectado"),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                "Solo lectura: LocalForge no instala drivers ni descarga nada.",
                            )),
                    )
                    .child(
                        Button::new("refresh-hardware")
                            .small()
                            .primary()
                            .icon(IconName::RefreshCw)
                            .label("Actualizar")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_hardware(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .when(hardware.is_empty(), |this| {
                        this.child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.group_box)
                                .p_4()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Pulsa «Actualizar» para inspeccionar el equipo."),
                        )
                    })
                    .children(hardware.iter().map(|item| hardware_card(item, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .pt_2()
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
                                    .child("Herramientas"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Rutas configuradas y binarios encontrados."),
                            ),
                    )
                    .child(
                        Button::new("refresh-tools")
                            .small()
                            .ghost()
                            .icon(IconName::Wrench)
                            .label(if probing {
                                "Comprobando…"
                            } else {
                                "Comprobar"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.probe(cx))),
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
                    .when(diagnostics.is_empty(), |this| {
                        this.child(
                            div()
                                .p_4()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Pulsa «Comprobar» para validar binarios y modelos."),
                        )
                    })
                    .children(diagnostics.iter().map(|item| hardware_card(item, cx))),
            )
    }

    /// Loads hardware the first time the System tab is opened.
    pub(super) fn ensure_hardware(&mut self, cx: &mut Context<Self>) {
        if !self.hardware_loaded {
            self.refresh_hardware(cx);
        }
    }

    fn refresh_hardware(&mut self, cx: &mut Context<Self>) {
        self.hardware_loaded = true;
        self.engine.send(EngineCommand::ProbeHardware);
        self.engine.send(EngineCommand::ProbeTools);
        cx.notify();
    }
}

fn hardware_card(item: &DiagnosticItem, cx: &App) -> impl IntoElement {
    let theme = cx.theme().clone();
    let color = if item.ok { theme.success } else { theme.danger };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .w(px(258.))
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .p_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(theme.muted_foreground)
                        .child(item.name.clone()),
                )
                .child(
                    div()
                        .px_2()
                        .rounded_full()
                        .bg(color.opacity(0.14))
                        .text_xs()
                        .text_color(color)
                        .child(if item.ok { "OK" } else { "Revisar" }),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(item.value.clone()),
        )
}

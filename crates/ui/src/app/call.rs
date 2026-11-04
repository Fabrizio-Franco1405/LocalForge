//! Voice call workspace.
//!
//! The call is never started implicitly: entering the workspace only shows the
//! start screen. Starting a call enables the microphone (and the local engine
//! when it is not running) and ends cleanly with "Colgar".

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{EngineCommand, LlmMode, LlmState};

use crate::app::{SettingsTab, VoxApp, WorkspaceMode};
use crate::config;

impl VoxApp {
    pub(super) fn call_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();

        if !self.call_active {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_5()
                .size_full()
                .bg(theme.background)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(150.))
                        .h(px(150.))
                        .rounded_full()
                        .bg(theme.muted)
                        .child(
                            div()
                                .w(px(54.))
                                .h(px(54.))
                                .rounded_full()
                                .bg(theme.muted_foreground.opacity(0.35)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child("Llamada inactiva"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("La captura de micrófono y el motor no se inician hasta pulsar el botón."),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("start-call")
                                .primary()
                                .icon(IconName::Phone)
                                .label("Iniciar llamada")
                                .on_click(cx.listener(|this, _, _, cx| this.start_call(cx))),
                        )
                        .child(
                            Button::new("call-audio")
                                .ghost()
                                .icon(IconName::SlidersHorizontal)
                                .label("Configurar audio")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.workspace = WorkspaceMode::Settings;
                                    this.settings_tab = SettingsTab::Audio;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element();
        }

        let (color, label) = if self.streaming.is_some() {
            (theme.warning, "Pensando")
        } else if self.speaking {
            (theme.success, "Hablando")
        } else if self.listening {
            (theme.primary, "Escuchando")
        } else {
            (theme.muted_foreground, "Micrófono silenciado")
        };

        let energy = if self.listening {
            self.level.clamp(0.0, 1.0)
        } else if self.speaking {
            0.45
        } else {
            0.0
        };
        let breathe = self.pulse.sin() * 0.5 + 0.5;
        let diameter = 170.0 + energy * 80.0 + breathe * 10.0;
        let inner = diameter * 0.62;

        let caption = if !self.call_caption.is_empty() {
            self.call_caption.clone()
        } else {
            self.log
                .last_text()
                .unwrap_or("La llamada está activa. Hable cuando lo desee.")
                .to_owned()
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_between()
            .size_full()
            .py_10()
            .bg(theme.background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(diameter))
                            .h(px(diameter))
                            .rounded_full()
                            .bg(color.opacity(0.10))
                            .border_1()
                            .border_color(color.opacity(0.30))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(inner))
                                    .h(px(inner))
                                    .rounded_full()
                                    .bg(color.opacity(0.20))
                                    .border_1()
                                    .border_color(color.opacity(0.55))
                                    .child(
                                        div()
                                            .w(px(inner * 0.42))
                                            .h(px(inner * 0.42))
                                            .rounded_full()
                                            .bg(color),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(color.opacity(0.14))
                            .text_xs()
                            .font_semibold()
                            .text_color(color)
                            .child(label),
                    )
                    .child(
                        div()
                            .max_w(px(540.))
                            .px_6()
                            .text_center()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child(caption)
                            .when_some(self.notice.clone(), |this, notice| {
                                this.child(
                                    div()
                                        .pt_2()
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child(notice),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Button::new("call-mute")
                            .ghost()
                            .icon(if self.listening {
                                IconName::Mic
                            } else {
                                IconName::MicOff
                            })
                            .label(if self.listening {
                                "Silenciar"
                            } else {
                                "Activar micrófono"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mic(cx))),
                    )
                    .child(
                        Button::new("call-hangup")
                            .danger()
                            .icon(IconName::PhoneOff)
                            .label("Colgar")
                            .on_click(cx.listener(|this, _, _, cx| this.end_call(cx))),
                    )
                    .child(
                        Button::new("call-settings")
                            .ghost()
                            .icon(IconName::SlidersHorizontal)
                            .label("Audio")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace = WorkspaceMode::Settings;
                                this.settings_tab = SettingsTab::Audio;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn start_call(&mut self, cx: &mut Context<Self>) {
        self.call_active = true;
        self.listening = true;
        self.level = 0.0;
        self.call_caption = "Escuchando…".to_owned();
        self.engine.send(EngineCommand::SetListening(true));

        let managed = config::current(cx).llm.mode == LlmMode::Managed;
        if managed && !matches!(self.llm, LlmState::Ready | LlmState::Starting) {
            self.engine.send(EngineCommand::StartLlm);
            self.call_started_engine = true;
        }
        cx.notify();
    }

    fn end_call(&mut self, cx: &mut Context<Self>) {
        self.call_active = false;
        self.listening = false;
        self.level = 0.0;
        self.call_caption.clear();
        self.engine.send(EngineCommand::SetListening(false));
        if self.call_started_engine {
            self.engine.send(EngineCommand::StopLlm);
            self.call_started_engine = false;
        }
        cx.notify();
    }

    fn toggle_mic(&mut self, cx: &mut Context<Self>) {
        self.listening = !self.listening;
        if !self.listening {
            self.level = 0.0;
        }
        self.call_caption = if self.listening {
            "Escuchando…".to_owned()
        } else {
            "Micrófono silenciado.".to_owned()
        };
        self.engine
            .send(EngineCommand::SetListening(self.listening));
        cx.notify();
    }
}

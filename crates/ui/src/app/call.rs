//! Voice call workspace: an orb that reacts to `SpeechLevel`, the live caption
//! and the call controls. The audio engine is still a stub, so the orb also
//! reflects the LLM state (thinking/speaking) and stays idle otherwise.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::{SettingsTab, VoxApp, WorkspaceMode};
use shared::EngineCommand;

impl VoxApp {
    pub(super) fn call_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let (color, label) = if self.streaming.is_some() {
            (theme.warning, "Pensando")
        } else if self.speaking {
            (theme.success, "Hablando")
        } else if self.listening {
            (theme.primary, "Escuchando")
        } else {
            (theme.muted_foreground, "En espera")
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
                .unwrap_or("Pulsa «Hablar» y di algo…")
                .to_owned()
        };

        let muted = !self.listening;

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
                            .max_w(px(520.))
                            .px_6()
                            .text_center()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child(caption)
                            .when_some(self.notice.clone(), |this, notice| {
                                this.child(div().text_xs().text_color(theme.danger).child(notice))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Button::new("call-mic")
                            .ghost()
                            .icon(if muted {
                                IconName::MicOff
                            } else {
                                IconName::Mic
                            })
                            .label(if muted { "Hablar" } else { "Silenciar" })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_mic(cx))),
                    )
                    .child(
                        Button::new("call-hangup")
                            .danger()
                            .icon(IconName::PhoneOff)
                            .label("Colgar")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace = WorkspaceMode::Chat;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("call-audio")
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
    }

    fn toggle_mic(&mut self, cx: &mut Context<Self>) {
        self.listening = !self.listening;
        if !self.listening {
            self.level = 0.0;
        }
        self.call_caption = if self.listening {
            "Escuchando…".to_owned()
        } else {
            String::new()
        };
        self.engine
            .send(EngineCommand::SetListening(self.listening));
        cx.notify();
    }
}

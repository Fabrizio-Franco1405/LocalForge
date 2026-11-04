//! Main window: navigation shell, event pump and shared actions.
//!
//! The view never talks to an engine directly. It sends [`EngineCommand`]s and
//! drains [`EngineEvent`]s every 50 ms, which keeps rendering free of blocking
//! work and lets the controller live on its own thread.
//!
//! Layout: a fixed sidebar with the four sections (LLM, audio, 3D, general), a
//! header with the page title and actions, the page body and a status bar.
//! Rendering for each page lives in its own module (`chat`, `tools`); the
//! settings pages live in [`crate::settings`].

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{DiagnosticItem, EngineCommand, EngineEvent, EngineHandle, LlmMode, LlmState};

use crate::chat::ChatLog;
use crate::config;
use crate::settings;

mod chat;
mod sidebar;
mod status;
mod tools;

/// Top-level section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Conversation with the connected model.
    Chat,
    /// LLM motor: local llama.cpp or an external endpoint.
    Llm,
    /// Audio motor: STT, TTS, capture and VAD.
    Audio,
    /// 3D motor (preview).
    Scene3d,
    /// Appearance, paths and environment.
    General,
    /// Binary/model diagnostics (not a sidebar section).
    Tools,
}

impl Page {
    /// Sidebar label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Conversación",
            Self::Llm => "Motor LLM",
            Self::Audio => "Motor Audio",
            Self::Scene3d => "Motor 3D",
            Self::General => "Ajustes",
            Self::Tools => "Diagnóstico",
        }
    }

    /// One-line description shown under the header title.
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Chat => "Habla o escribe con el motor conectado.",
            Self::Llm => "llama.cpp local o un servidor externo compatible con OpenAI.",
            Self::Audio => "Whisper, VAD, TTS y captura de audio: todo opcional.",
            Self::Scene3d => "Generación local de escenas y mallas · próximamente.",
            Self::General => "Apariencia, rutas y diagnóstico del entorno.",
            Self::Tools => "Qué binarios y modelos ve LocalForge.",
        }
    }

    /// Stable id used by sidebar items and settings panels.
    pub fn id(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Llm => "llm",
            Self::Audio => "audio",
            Self::Scene3d => "scene3d",
            Self::General => "general",
            Self::Tools => "tools",
        }
    }
}

/// Application view.
pub struct VoxApp {
    page: Page,
    engine: EngineHandle,
    log: ChatLog,
    draft: Entity<InputState>,
    _draft_sub: Subscription,
    _config_sub: Subscription,
    _pump: Task<()>,
    llm: LlmState,
    llm_detail: String,
    generation: u64,
    streaming: Option<u64>,
    listening: bool,
    level: f32,
    cpu: Option<f32>,
    ram_used_mb: Option<u64>,
    ram_total_mb: Option<u64>,
    app_mb: Option<u64>,
    diagnostics: Vec<DiagnosticItem>,
    probing: bool,
    notice: Option<String>,
    scroll: ScrollHandle,
    context_used: u32,
    context_total: u32,
}

impl VoxApp {
    /// Builds the view and starts the event pump.
    pub fn new(window: &mut Window, cx: &mut Context<Self>, engine: EngineHandle) -> Self {
        let draft = cx
            .new(|cx| InputState::new(window, cx).placeholder("Escribe un mensaje y pulsa Enter…"));

        let _draft_sub = cx.subscribe_in(&draft, window, |this, state, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                let text = state.read(cx).value().to_string();
                this.submit(&text, window, cx);
            }
        });

        let _config_sub = cx.observe_global::<config::GlobalConfig>(|_, cx| {
            let ui = config::current(cx).ui;
            crate::theme::apply(cx, &ui, None);
            cx.refresh_windows();
            cx.notify();
        });

        let _pump = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if this.update(cx, |app, cx| app.pump(cx)).is_err() {
                    break;
                }
            }
        });

        let mut app = Self {
            page: Page::Chat,
            engine,
            log: ChatLog::new(),
            draft,
            _draft_sub,
            _config_sub,
            _pump,
            llm: LlmState::Stopped,
            llm_detail: "Sin iniciar".to_owned(),
            generation: 0,
            streaming: None,
            listening: false,
            level: 0.0,
            cpu: None,
            ram_used_mb: None,
            ram_total_mb: None,
            app_mb: None,
            diagnostics: Vec::new(),
            probing: false,
            notice: None,
            scroll: ScrollHandle::new(),
            context_used: 0,
            context_total: 0,
        };

        app.probe(cx);
        app
    }

    /// Drains controller events. Called by the pump every 50 ms.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let events = self.engine.drain();
        if events.is_empty() {
            return;
        }

        let mut changed = false;
        for event in events {
            changed |= self.handle_event(event);
        }

        if changed {
            cx.notify();
        }
    }

    fn handle_event(&mut self, event: EngineEvent) -> bool {
        match event {
            EngineEvent::Llm { state, detail } => {
                self.llm = state;
                self.llm_detail = detail;
                true
            }
            EngineEvent::Token { generation, delta } => {
                if self.streaming == Some(generation) && self.generation == generation {
                    let appended = self.log.append_delta(&delta);
                    if appended {
                        self.scroll.scroll_to_bottom();
                    }
                    appended
                } else {
                    false
                }
            }
            EngineEvent::ReplyDone { generation, text } => {
                if self.streaming == Some(generation) {
                    self.log.finish_reply(&text);
                    self.streaming = None;
                    self.scroll.scroll_to_bottom();
                    true
                } else {
                    false
                }
            }
            EngineEvent::Transcript {
                generation: _,
                text,
            } => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return false;
                }
                self.send_prompt(text);
                true
            }
            EngineEvent::SpeechLevel(level) => {
                if self.listening {
                    self.level = level;
                    true
                } else {
                    false
                }
            }
            EngineEvent::Speaking(speaking) => {
                if speaking {
                    self.llm_detail = "Hablando".to_owned();
                }
                false
            }
            EngineEvent::Diagnostics { items } => {
                self.diagnostics = items;
                self.probing = false;
                true
            }
            EngineEvent::ContextUsage { used, total } => {
                self.context_used = used;
                self.context_total = total;
                true
            }
            EngineEvent::SystemUsage {
                cpu_percent,
                ram_total_mb,
                ram_used_mb,
                app_mb,
            } => {
                self.cpu = cpu_percent;
                self.ram_total_mb = ram_total_mb;
                self.ram_used_mb = ram_used_mb;
                self.app_mb = app_mb;
                true
            }
            EngineEvent::Error { scope, message } => {
                if self.streaming.take().is_some() {
                    self.log.finish_reply("");
                }
                self.notice = Some(format!("{scope}: {message}"));
                true
            }
        }
    }

    fn send_prompt(&mut self, text: String) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        self.log.push_user(text.clone());
        self.log.begin_reply();
        self.streaming = Some(generation);
        self.notice = None;

        self.engine.send(EngineCommand::SendMessage {
            generation,
            prompt: text,
        });
        self.scroll.scroll_to_bottom();
    }

    fn submit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.streaming.is_some() {
            return;
        }

        self.send_prompt(text.to_owned());
        self.draft
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(generation) = self.streaming.take() {
            self.engine.send(EngineCommand::Cancel { generation });
            self.log.finish_reply("");
            cx.notify();
        }
    }

    fn clear_chat(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.log.clear();
        self.notice = None;
        cx.notify();
    }

    fn probe(&mut self, cx: &mut Context<Self>) {
        self.probing = true;
        self.engine.send(EngineCommand::ProbeTools);
        cx.notify();
    }

    /// Applies the draft and probes it, so pasting an external host and
    /// pressing "Probar conexión" works without a separate Apply step.
    fn probe_connection(&mut self, cx: &mut Context<Self>) {
        let cfg = config::current(cx);
        self.engine.send(EngineCommand::ReloadConfig(Box::new(cfg)));
        cx.global_mut::<config::GlobalConfig>().mark_applied();
        self.engine.send(EngineCommand::ProbeLlm);
        self.engine.send(EngineCommand::ProbeTools);
        self.probing = true;
        cx.notify();
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = config::current(cx);
        self.engine.send(EngineCommand::ReloadConfig(Box::new(cfg)));
        cx.global_mut::<config::GlobalConfig>().mark_applied();

        // 1. Extraemos la configuración de la UI a su propia variable
        let ui_config = config::current(cx).ui;

        // 2. Ahora pasamos la referencia a esa variable, no a cx
        crate::theme::apply(cx, &ui_config, Some(window));
        cx.refresh_windows();

        if let Some(error) = cx.global::<config::GlobalConfig>().save_error.clone() {
            window.push_notification(Notification::error(error), cx);
        } else {
            window.push_notification(
                Notification::success("Configuración aplicada al controlador"),
                cx,
            );
        }

        self.probing = true;
        self.engine.send(EngineCommand::ProbeTools);
        cx.notify();
    }

    fn start_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.engine.send(EngineCommand::StartLlm);
        window.push_notification(Notification::info("Arrancando el motor local…"), cx);
        cx.notify();
    }

    fn stop_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.engine.send(EngineCommand::StopLlm);
        window.push_notification(Notification::info("Motor local detenido"), cx);
        cx.notify();
    }

    fn restart_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.engine.send(EngineCommand::RestartLlm);
        window.push_notification(Notification::info("Reiniciando el motor…"), cx);
        cx.notify();
    }

    fn toggle_listening(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !config::current(cx).stt.enabled {
            window.push_notification(
                Notification::warning("Activa el dictado en Ajustes → Reconocimiento"),
                cx,
            );
            return;
        }

        self.listening = !self.listening;
        if !self.listening {
            self.level = 0.0;
        }
        self.engine
            .send(EngineCommand::SetListening(self.listening));
        cx.notify();
    }

    /// Header shown above every page: title, subtitle and shared actions.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dirty = config::is_dirty(cx);

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(self.page.label()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(self.page.subtitle()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(matches!(self.page, Page::Chat | Page::Llm), |this| {
                        let mode = config::current(cx).llm.mode;
                        this.child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_full()
                                .bg(theme.muted)
                                .text_xs()
                                .font_semibold()
                                .text_color(theme.muted_foreground)
                                .child(match mode {
                                    LlmMode::Managed => "LOCAL",
                                    LlmMode::External => "EXTERNO",
                                }),
                        )
                    })
                    .when(self.page == Page::General, |this| {
                        this.child(
                            Button::new("open-tools")
                                .small()
                                .ghost()
                                .icon(IconName::Activity)
                                .label("Diagnóstico")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.page = Page::Tools;
                                    this.probe(cx);
                                })),
                        )
                    })
                    .when(self.page == Page::Chat, |this| {
                        this.child(
                            Button::new("new-chat")
                                .small()
                                .ghost()
                                .icon(IconName::MessageSquare)
                                .label("Nueva")
                                .on_click(cx.listener(|this, _, _, cx| this.clear_chat(cx))),
                        )
                    })
                    .when(
                        matches!(self.page, Page::Chat | Page::Llm)
                            && config::current(cx).llm.mode == LlmMode::Managed,
                        |this| {
                            if matches!(self.llm, LlmState::Ready | LlmState::Starting) {
                                this.child(
                                    Button::new("stop-llm")
                                        .small()
                                        .ghost()
                                        .icon(IconName::Square)
                                        .label("Apagar")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.stop_engine(window, cx)
                                        })),
                                )
                            } else {
                                this.child(
                                    Button::new("start-llm")
                                        .small()
                                        .primary()
                                        .icon(IconName::Play)
                                        .label("Encender")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.start_engine(window, cx)
                                        })),
                                )
                            }
                        },
                    )
                    .when(self.page == Page::Llm, |this| {
                        this.child(
                            Button::new("restart-llm")
                                .small()
                                .ghost()
                                .icon(IconName::RotateCw)
                                .label("Reiniciar")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.restart_engine(window, cx)
                                })),
                        )
                    })
                    .when(self.page != Page::Chat, |this| {
                        this.child(
                            Button::new("back-chat")
                                .small()
                                .ghost()
                                .icon(IconName::MessageSquare)
                                .label("Chat")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.page = Page::Chat;
                                    cx.notify();
                                })),
                        )
                    })
                    .when(matches!(self.page, Page::Chat | Page::Llm), |this| {
                        this.child(
                            Button::new("header-probe")
                                .small()
                                .ghost()
                                .icon(IconName::RefreshCw)
                                .label("Probar")
                                .on_click(cx.listener(|this, _, _, cx| this.probe_connection(cx))),
                        )
                    })
                    .when(dirty, |this| {
                        this.child(
                            Button::new("header-apply")
                                .small()
                                .primary()
                                .icon(IconName::Check)
                                .label("Aplicar")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.apply(window, cx)),
                                ),
                        )
                    }),
            )
    }
}

impl Render for VoxApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(self.header(cx))
                    .child(div().flex_1().min_h(px(0.)).child(match self.page {
                        Page::Chat => self.chat_page(cx).into_any_element(),
                        Page::Tools => self.tools_page(cx).into_any_element(),
                        _ => settings::view(cx, self.page).into_any_element(),
                    }))
                    .child(self.status_bar(cx)),
            )
    }
}

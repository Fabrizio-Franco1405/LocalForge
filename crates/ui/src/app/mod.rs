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

use std::path::PathBuf;

use crate::chat::ChatLog;
use crate::config;

mod call;
mod chat;
mod footer;
mod settings_page;
mod sidebar;
mod studio;
mod system;

/// Top-level workspace selected in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    /// Text conversation.
    Chat,
    /// Immersive voice call.
    Call,
    /// 3D studio (Trellis).
    Studio3d,
    /// Engine configuration.
    Settings,
}

impl WorkspaceMode {
    /// Sidebar label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Call => "Llamada",
            Self::Studio3d => "Estudio 3D",
            Self::Settings => "Ajustes",
        }
    }

    /// One-line description shown under the header title.
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Chat => "Conversación de texto con el motor conectado.",
            Self::Call => "Modo voz inmersivo: habla y escucha sin teclado.",
            Self::Studio3d => "Genera mallas locales con tu Trellis.cpp.",
            Self::Settings => "Motores, audio, 3D y sistema.",
        }
    }

    /// Stable id used by sidebar items.
    pub fn id(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Call => "call",
            Self::Studio3d => "studio3d",
            Self::Settings => "settings",
        }
    }
}

/// Tabs inside the Ajustes workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// LLM engine.
    Llm,
    /// Audio engine (STT, TTS, VAD).
    Audio,
    /// 3D engine.
    Scene3d,
    /// Appearance, paths and diagnostics.
    General,
    /// Hardware and detected tools.
    System,
    /// MCP connections (placeholder).
    Mcp,
}

impl SettingsTab {
    /// Tab label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Llm => "Motor LLM",
            Self::Audio => "Motor Audio",
            Self::Scene3d => "Motor 3D",
            Self::General => "General",
            Self::System => "Sistema",
            Self::Mcp => "MCP",
        }
    }

    /// Stable id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Llm => "llm",
            Self::Audio => "audio",
            Self::Scene3d => "scene3d",
            Self::General => "general",
            Self::System => "system",
            Self::Mcp => "mcp",
        }
    }

    /// All tabs, in display order.
    pub const ALL: [Self; 6] = [
        Self::Llm,
        Self::Audio,
        Self::Scene3d,
        Self::General,
        Self::System,
        Self::Mcp,
    ];
}

/// Application view.
pub struct VoxApp {
    workspace: WorkspaceMode,
    settings_tab: SettingsTab,
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
    vram_used_mb: Option<u64>,
    vram_total_mb: Option<u64>,
    speaking: bool,
    pulse: f32,
    call_caption: String,
    hardware: Vec<DiagnosticItem>,
    hardware_loaded: bool,
    studio_image: Option<PathBuf>,
    attachments: Vec<PathBuf>,
    model_menu: bool,
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
            workspace: WorkspaceMode::Chat,
            settings_tab: SettingsTab::Llm,
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
            vram_used_mb: None,
            vram_total_mb: None,
            speaking: false,
            pulse: 0.0,
            call_caption: String::new(),
            hardware: Vec::new(),
            hardware_loaded: false,
            studio_image: None,
            attachments: Vec::new(),
            model_menu: false,
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
        let mut changed = false;

        // Cheap animation driver: only the call workspace needs a repaint at
        // 20 Hz, so other views stay idle when nothing happens.
        if self.workspace == WorkspaceMode::Call {
            self.pulse = (self.pulse + 0.16) % std::f32::consts::TAU;
            changed = true;
        }

        let events = self.engine.drain();
        for event in events {
            changed |= self.handle_event(event, cx);
        }

        if changed {
            cx.notify();
        }
    }

    fn handle_event(&mut self, event: EngineEvent, cx: &mut Context<Self>) -> bool {
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
                self.call_caption = text.clone();
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
                self.speaking = speaking;
                if speaking {
                    self.llm_detail = "Hablando".to_owned();
                }
                true
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
                vram_used_mb,
                vram_total_mb,
            } => {
                self.cpu = cpu_percent;
                self.ram_total_mb = ram_total_mb;
                self.ram_used_mb = ram_used_mb;
                self.app_mb = app_mb;
                self.vram_used_mb = vram_used_mb;
                self.vram_total_mb = vram_total_mb;
                true
            }
            EngineEvent::Models { entries } => {
                config::set_models(cx, entries);
                true
            }
            EngineEvent::Hardware { items } => {
                self.hardware = items;
                self.hardware_loaded = true;
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
        let text = self.with_attachments(text);
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

    /// Header shown above every workspace: title, subtitle and actions.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let dirty = config::is_dirty(cx);
        let managed = config::current(cx).llm.mode == LlmMode::Managed;
        let running = matches!(self.llm, LlmState::Ready | LlmState::Starting);

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
                            .child(self.workspace.label()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(self.workspace.subtitle()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(
                        matches!(self.workspace, WorkspaceMode::Chat | WorkspaceMode::Call),
                        |this| {
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
                        },
                    )
                    .when(self.workspace == WorkspaceMode::Chat, |this| {
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
                        matches!(self.workspace, WorkspaceMode::Chat | WorkspaceMode::Call)
                            && managed,
                        |this| {
                            if running {
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
                    .when(self.workspace == WorkspaceMode::Chat, |this| {
                        this.child(
                            Button::new("header-probe")
                                .small()
                                .ghost()
                                .icon(IconName::RefreshCw)
                                .label("Probar")
                                .on_click(cx.listener(|this, _, _, cx| this.probe_connection(cx))),
                        )
                    })
                    .when(
                        matches!(self.workspace, WorkspaceMode::Chat | WorkspaceMode::Call)
                            && managed,
                        |this| {
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
                        },
                    )
                    .when(self.workspace == WorkspaceMode::Studio3d, |this| {
                        this.child(
                            Button::new("studio-settings")
                                .small()
                                .ghost()
                                .icon(IconName::Settings)
                                .label("Configurar Trellis")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.workspace = WorkspaceMode::Settings;
                                    this.settings_tab = SettingsTab::Scene3d;
                                    cx.notify();
                                })),
                        )
                    })
                    .when(self.workspace == WorkspaceMode::Call, |this| {
                        this.child(
                            Button::new("call-settings")
                                .small()
                                .ghost()
                                .icon(IconName::SlidersHorizontal)
                                .label("Audio")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.workspace = WorkspaceMode::Settings;
                                    this.settings_tab = SettingsTab::Audio;
                                    cx.notify();
                                })),
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
        let theme = cx.theme().clone();
        let content = match self.workspace {
            WorkspaceMode::Chat => self.chat_page(cx).into_any_element(),
            WorkspaceMode::Call => self.call_page(cx).into_any_element(),
            WorkspaceMode::Studio3d => self.studio_page(cx).into_any_element(),
            WorkspaceMode::Settings => self.settings_page(cx).into_any_element(),
        };

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
                    .child(div().flex_1().min_h(px(0.)).child(content))
                    .child(self.footer(cx)),
            )
    }
}

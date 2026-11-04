//! Main window: navigation, chat, tools and the status bar.
//!
//! The view never talks to an engine directly. It sends [`EngineCommand`]s and
//! drains [`EngineEvent`]s every 50 ms, which keeps rendering free of blocking
//! work and lets the controller live on its own thread.

use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _, WindowExt as _,
};
use gpui_kit::prelude::{FluentBuilder as _, StatefulInteractiveElement as _};
use gpui_kit::*;
use shared::{
    DiagnosticItem, EngineCommand, EngineEvent, EngineHandle, LlmMode, LlmState, TtsBackend,
};

use crate::chat::{ChatLog, render_bubble};
use crate::config;
use crate::settings;

/// Top-level section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Conversation with the connected model.
    Chat,
    /// Configuration of every tool.
    Settings,
    /// What is connected and what is missing.
    Tools,
}

impl Page {
    /// Sidebar label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Conversación",
            Self::Settings => "Ajustes",
            Self::Tools => "Herramientas",
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
    diagnostics: Vec<DiagnosticItem>,
    probing: bool,
    notice: Option<String>,
    scroll: ScrollHandle,
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
            diagnostics: Vec::new(),
            probing: false,
            notice: None,
            scroll: ScrollHandle::new(),
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

    fn probe_connection(&mut self, cx: &mut Context<Self>) {
        self.engine.send(EngineCommand::ProbeLlm);
        self.engine.send(EngineCommand::ProbeTools);
        cx.notify();
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = config::current(cx);
        self.engine.send(EngineCommand::ReloadConfig(Box::new(cfg)));
        cx.global_mut::<config::GlobalConfig>().mark_applied();

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

    fn nav(&self, id: &'static str, page: Page, cx: &mut Context<Self>) -> Button {
        let button = Button::new(id).small().label(page.label());
        let button = if self.page == page {
            button.primary()
        } else {
            button.ghost()
        };
        button.on_click(cx.listener(move |this, _, _, cx| {
            this.page = page;
            cx.notify();
        }))
    }

    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dirty = config::is_dirty(cx);

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("VoxLocal"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("hub de voz local · trae tus propias herramientas"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.nav("nav-chat", Page::Chat, cx))
                    .child(self.nav("nav-settings", Page::Settings, cx))
                    .child(self.nav("nav-tools", Page::Tools, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("probe-llm")
                            .small()
                            .ghost()
                            .label("Comprobar")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.probe_connection(cx);
                            })),
                    )
                    .child(self.status_pill(cx))
                    .child(
                        Button::new("apply")
                            .small()
                            .primary()
                            .label(if dirty { "Aplicar cambios" } else { "Aplicado" })
                            .disabled(!dirty)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply(window, cx);
                            })),
                    ),
            )
    }

    fn status_pill(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let color = match self.llm {
            LlmState::Stopped => theme.muted_foreground,
            LlmState::Starting => theme.warning,
            LlmState::Ready => theme.success,
            LlmState::Error => theme.danger,
        };
        let background = match self.llm {
            LlmState::Error => theme.danger.opacity(0.15),
            _ => theme.muted,
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(background)
            .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(color))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.foreground)
                    .child(self.llm.label()),
            )
    }

    fn chat_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let cfg = config::current(cx);
        let subtitle = match cfg.llm.mode {
            LlmMode::Managed => format!("llama-server gestionado · {}", cfg.llm.base_url()),
            LlmMode::External => format!("servidor externo · {}", cfg.llm.chat_completions_url()),
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
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
                            .child("Conversación"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(subtitle),
                    )
                    .when(!self.llm_detail.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(self.llm_detail.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("probe-header")
                            .small()
                            .ghost()
                            .label("Comprobar conexión")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.probe_connection(cx);
                            })),
                    )
                    .child(
                        Button::new("restart")
                            .small()
                            .ghost()
                            .label("Reiniciar motor")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.restart_engine(window, cx);
                            })),
                    )
                    .child(
                        Button::new("clear")
                            .small()
                            .ghost()
                            .label("Nueva conversación")
                            .on_click(cx.listener(|this, _, _, cx| this.clear_chat(cx))),
                    ),
            )
    }

    fn empty_state(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let cfg = config::current(cx);

        let (llm_color, llm_value) = match self.llm {
            LlmState::Ready => (theme.success, "conectado"),
            LlmState::Starting => (theme.warning, "cargando"),
            LlmState::Error => (theme.danger, "error"),
            LlmState::Stopped => (theme.muted_foreground, "detenido"),
        };

        let stt_value = if cfg.stt.enabled {
            "activado"
        } else {
            "opcional"
        };
        let stt_color = if cfg.stt.enabled {
            theme.success
        } else {
            theme.muted_foreground
        };

        let tts_value = match cfg.tts.backend {
            TtsBackend::Disabled => "opcional",
            _ => "conectado",
        };
        let tts_color = if cfg.tts.backend == TtsBackend::Disabled {
            theme.muted_foreground
        } else {
            theme.success
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_5()
            .px_6()
            .py_10()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .max_w(px(820.))
                    .w_full()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.primary.opacity(0.28))
                    .bg(theme.group_box)
                    .p_8()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_full()
                                    .bg(theme.primary.opacity(0.14))
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.primary)
                                    .child("100% LOCAL · BYO TOOLS"),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_full()
                                    .bg(theme.muted)
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Vulkan · sin CUDA / ROCm"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child("Tu voz, tus modelos, tu máquina."),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        "VoxLocal no descarga ni instala nada: conecta tu llama.cpp, \
                                         tu whisper.cpp y tu TTS, y conversa sin salir del equipo.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(status_chip("LLM", llm_value, llm_color, cx))
                            .child(status_chip("STT", stt_value, stt_color, cx))
                            .child(status_chip("TTS", tts_value, tts_color, cx))
                            .child(status_chip("Backend", "Vulkan", theme.success, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("hero-settings")
                                    .primary()
                                    .label("Configurar motor")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.page = Page::Settings;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("hero-probe")
                                    .ghost()
                                    .label("Comprobar conexión")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.probe_connection(cx);
                                    })),
                            )
                            .child(
                                Button::new("hero-tools")
                                    .ghost()
                                    .label("Ver herramientas")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.page = Page::Tools;
                                        this.probe(cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .max_w(px(820.))
                    .w_full()
                    .child(feature_card(
                        "1",
                        "Conecta",
                        "Apunta VoxLocal a tu llama-server o a cualquier endpoint compatible con OpenAI. También detecta un servidor que ya esté corriendo.",
                        cx,
                    ))
                    .child(feature_card(
                        "2",
                        "Comprueba",
                        "Aplicar y Reiniciar vuelven a leer la configuración y a probar el puerto. Herramientas muestra qué rutas ve y qué falta.",
                        cx,
                    ))
                    .child(feature_card(
                        "3",
                        "Conversa",
                        "El chat funciona sin STT ni TTS. Cuando conectes Whisper y tu motor de voz, el mismo flujo se vuelve manos libres.",
                        cx,
                    )),
            )
    }

    fn message_list(&self, cx: &App) -> impl IntoElement {
        div().flex().flex_col().gap_3().p_4().children(
            self.log
                .bubbles()
                .iter()
                .map(|bubble| render_bubble(bubble, cx)),
        )
    }

    fn composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let stt_enabled = config::current(cx).stt.enabled;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.draft)))
                    .when(self.streaming.is_none(), |this| {
                        this.child(Button::new("send").primary().label("Enviar").on_click(
                            cx.listener(|this, _, window, cx| {
                                let text = this.draft.read(cx).value().to_string();
                                this.submit(&text, window, cx);
                            }),
                        ))
                    })
                    .when(self.streaming.is_some(), |this| {
                        this.child(
                            Button::new("stop")
                                .danger()
                                .label("Detener")
                                .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                        )
                    })
                    .when(stt_enabled, |this| {
                        this.child(
                            Button::new("mic")
                                .ghost()
                                .label(if self.listening {
                                    "Detener micrófono"
                                } else {
                                    "Dictar"
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_listening(window, cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Enter envía · Shift+Enter no está disponible en este campo"),
                    )
                    .when(self.listening, |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Nivel"),
                                )
                                .child(
                                    div()
                                        .w(px(120. * self.level.clamp(0.0, 1.0)))
                                        .h(px(4.))
                                        .rounded_full()
                                        .bg(theme.success),
                                ),
                        )
                    })
                    .when_some(self.notice.clone(), |this, notice| {
                        this.child(div().text_xs().text_color(theme.danger).child(notice))
                    }),
            )
    }

    fn chat_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.chat_header(cx))
            .child(
                div()
                    .id("chat-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .when(self.log.is_empty(), |this| this.child(self.empty_state(cx)))
                    .when(!self.log.is_empty(), |this| {
                        this.child(self.message_list(cx))
                    }),
            )
            .child(self.composer(cx))
    }

    fn tools_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                 lo verás en su salida y en el tiempo de carga, pero VoxLocal no cambia drivers ni instala ROCm/CUDA.",
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

    fn status_bar(&self, cx: &App) -> impl IntoElement {
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
                    .child(format!("VoxLocal v{}", env!("CARGO_PKG_VERSION"))),
            )
    }
}

impl Render for VoxApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.top_bar(cx))
            .child(div().flex_1().min_h(px(0.)).child(match self.page {
                Page::Chat => self.chat_page(cx).into_any_element(),
                Page::Settings => settings::view(cx).into_any_element(),
                Page::Tools => self.tools_page(cx).into_any_element(),
            }))
            .child(self.status_bar(cx))
    }
}

fn status_chip(
    label: &'static str,
    value: &'static str,
    color: Hsla,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_full()
        .bg(color.opacity(0.12))
        .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(color))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("{label}:")),
        )
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.foreground)
                .child(value),
        )
}

fn feature_card(
    number: &'static str,
    title: &'static str,
    body: &'static str,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .flex_1()
        .min_w(px(210.))
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .p_4()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(22.))
                        .h(px(22.))
                        .rounded_full()
                        .bg(theme.primary)
                        .text_color(theme.primary_foreground)
                        .text_xs()
                        .font_semibold()
                        .child(number),
                )
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(title),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(body),
        )
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

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn page_labels_are_stable() {
        assert_eq!(Page::Chat.label(), "Conversación");
        assert_eq!(Page::Settings.label(), "Ajustes");
        assert_eq!(Page::Tools.label(), "Herramientas");
    }
}

//! Conversation page: banner, messages, composer and context meter.

use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use shared::{EngineCommand, LlmState, TtsBackend};

use crate::app::{SettingsTab, VoxApp, WorkspaceMode};
use crate::chat::render_bubble;
use crate::config;

impl VoxApp {
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
                                    .child("ENTORNO LOCAL · COMPONENTES PROPIOS"),
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
                                    .child("Lenguaje, voz y 3D en el equipo local."),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        "LocalForge no descarga ni instala componentes: utiliza los binarios y modelos \
                                         instalados por el usuario. Permite conectar motores de lenguaje, voz y 3D sin servicios externos.",
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
                            .child(status_chip("3D", "próximamente", theme.muted_foreground, cx))
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
                                        this.workspace = WorkspaceMode::Settings;
                                        this.settings_tab = SettingsTab::Llm;
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
                                        this.workspace = WorkspaceMode::Settings;
                                        this.settings_tab = SettingsTab::System;
                                        this.ensure_hardware(cx);
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
                        "Conexión",
                        "El modo Local carga automáticamente el motor instalado; el modo Externo se conecta a un servidor ya iniciado. Cada modo muestra únicamente sus campos.",
                        cx,
                    ))
                    .child(feature_card(
                        "2",
                        "Verificación",
                        "Las acciones Aplicar y Reiniciar releen la configuración y verifican el puerto. El panel Sistema muestra las rutas detectadas y los componentes que faltan.",
                        cx,
                    ))
                    .child(feature_card(
                        "3",
                        "Conversación",
                        "El chat funciona sin STT ni TTS. Al conectar Whisper y un motor de voz, el flujo incorpora la conversación hablada. El motor 3D se integrará en su propia sección.",
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
                        // AQUÍ AGRUPAMOS EL TEXTO Y EL CONTADOR DE CONTEXTO
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Enter envía · una línea por mensaje"),
                            )
                            // --- BARRA DE CONTEXTO ---
                            .when(self.context_total > 0, |this| {
                                this.child(Self::render_context_bar(
                                    self.context_used,
                                    self.context_total,
                                    cx,
                                ))
                            }),
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

    pub(super) fn chat_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
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
            .child(self.chat_toolbar(cx))
            .child(self.composer(cx))
    }
    /// Dibuja el indicador de contexto.
    pub fn render_context_bar(used: u32, total: u32, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let is_high = used as f32 / total as f32 > 0.8; // Si pasa del 80%, se pone rojo
        let text_color = if is_high {
            theme.danger
        } else {
            theme.muted_foreground
        };

        div().flex().items_center().gap_2().child(
            div()
                .text_xs()
                .text_color(text_color)
                .child(format!("Memoria: {} / {} tokens", used, total)),
        )
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

impl VoxApp {
    /// Model picker, attachments and MCP entry point above the messages.
    fn chat_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let models = config::models(cx);
        let attachments = self.attachments.clone();
        let open = self.model_menu;

        let mut rows = Vec::with_capacity(models.len());
        for (index, entry) in models.iter().enumerate() {
            let path = entry.path.clone();
            let alias = entry.name.clone();
            let selected = config::current(cx)
                .llm
                .model
                .as_ref()
                .is_some_and(|current| current.display().to_string() == entry.path);
            let label = config::current(cx)
                .llm
                .model_aliases
                .get(&entry.path)
                .filter(|alias| !alias.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| entry.name.clone());

            rows.push(
                div()
                    .id(format!("model-row-{index}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .cursor_pointer()
                    .when(selected, |this| this.bg(theme.primary.opacity(0.12)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_model(path.clone(), alias.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().text_color(theme.foreground).child(label))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{} · {} MB", entry.name, entry.size_mb)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(entry.path.clone()),
                    ),
            );
        }

        let mut chips = Vec::with_capacity(attachments.len());
        for (index, path) in attachments.iter().enumerate() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("archivo")
                .to_owned();
            chips.push(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.muted)
                    .child(div().text_xs().text_color(theme.foreground).child(name))
                    .child(
                        Button::new(format!("remove-attachment-{index}"))
                            .small()
                            .ghost()
                            .icon(IconName::X)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if index < this.attachments.len() {
                                    this.attachments.remove(index);
                                }
                                cx.notify();
                            })),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .child(
                        Button::new("model-picker")
                            .small()
                            .ghost()
                            .icon(IconName::Bot)
                            .label(config::model_label(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model_menu = !this.model_menu;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("attach-files")
                            .small()
                            .ghost()
                            .icon(IconName::Paperclip)
                            .label("Adjuntar")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                                    this.attachments.extend(paths);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("mcp-servers")
                            .small()
                            .ghost()
                            .icon(IconName::Plug)
                            .label("MCP")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace = WorkspaceMode::Settings;
                                this.settings_tab = SettingsTab::Mcp;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("manage-models")
                            .small()
                            .ghost()
                            .icon(IconName::Settings)
                            .label("Modelos")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace = WorkspaceMode::Settings;
                                this.settings_tab = SettingsTab::Llm;
                                cx.notify();
                            })),
                    ),
            )
            .when(open, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .border_t_1()
                        .border_color(theme.border)
                        .when(models.is_empty(), |this| {
                            this.child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Sin modelos: seleccione una carpeta en Ajustes → Motor LLM."),
                            )
                        })
                        .children(rows),
                )
            })
            .when(!attachments.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_2()
                        .children(chips),
                )
            })
    }

    fn select_model(&mut self, path: String, alias: String, cx: &mut Context<Self>) {
        let model = PathBuf::from(&path);
        config::update(cx, |cfg| {
            cfg.llm.model_aliases.entry(path.clone()).or_insert(alias);
            cfg.llm.model = Some(model);
            cfg.llm.model_id.clear();
        });

        let cfg = config::current(cx);
        self.engine.send(EngineCommand::ReloadConfig(Box::new(cfg)));
        cx.global_mut::<config::GlobalConfig>().mark_applied();
        self.model_menu = false;
        self.notice = Some("Modelo cambiado: reiniciando el motor…".to_owned());
        cx.notify();
    }

    /// Appends attachment contents (text, bounded) to the outgoing prompt.
    pub(super) fn with_attachments(&mut self, text: String) -> String {
        if self.attachments.is_empty() {
            return text;
        }

        let mut prompt = text;
        for path in std::mem::take(&mut self.attachments) {
            prompt.push_str("\n\n[Adjunto: ");
            prompt.push_str(&path.display().to_string());
            prompt.push_str("]\n");
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    prompt.push_str(&shared::text::truncate_chars(&content, 8000));
                }
                Err(_) => prompt.push_str("(archivo binario; solo se adjunta la ruta)"),
            }
        }
        prompt
    }
}

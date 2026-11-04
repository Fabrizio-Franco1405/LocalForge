//! 3D studio workspace: Trellis configuration on the left, viewport on the right.
//!
//! The Trellis backend is not implemented yet, so generation/export are
//! explicitly disabled. The layout, file pickers and config wiring are ready.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::*;

use crate::app::{SettingsTab, VoxApp, WorkspaceMode};
use crate::config;
use crate::dialog::{Pick, pick_paths_for};

impl VoxApp {
    pub(super) fn studio_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let cfg = config::current(cx);
        let trellis = cfg.scene3d.trellis_binary.display().to_string();
        let models = cfg
            .scene3d
            .model_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "sin configurar".to_owned());
        let image = self
            .studio_image
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Sin imagen".to_owned());
        let resolution = cfg.scene3d.resolution;

        div()
            .flex()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w(px(330.))
                    .flex_shrink_0()
                    .h_full()
                    .p_4()
                    .border_r_1()
                    .border_color(theme.border)
                    .bg(theme.group_box)
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("Trellis.cpp"),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Generación de mallas 3D en local. El backend se integra como un motor adicional.",
                    ))
                    .child(path_row("Binario", &trellis, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("studio-pick-binary")
                                    .small()
                                    .ghost()
                                    .icon(IconName::FolderOpen)
                                    .label("Elegir binario")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        pick_paths_for(cx, Pick::File.options(), |_, cx, paths| {
                                            if let Some(path) = paths.into_iter().next() {
                                                config::update(cx, |cfg| {
                                                    cfg.scene3d.trellis_binary = path;
                                                });
                                            }
                                        });
                                    })),
                            )
                            .child(
                                Button::new("studio-pick-models")
                                    .small()
                                    .ghost()
                                    .icon(IconName::HardDrive)
                                    .label("Modelos")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        pick_paths_for(cx, Pick::Folder.options(), |_, cx, paths| {
                                            if let Some(path) = paths.into_iter().next() {
                                                config::update(cx, |cfg| {
                                                    cfg.scene3d.model_dir = Some(path);
                                                });
                                            }
                                        });
                                    })),
                            ),
                    )
                    .child(path_row("Modelos", &models, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(theme.muted_foreground)
                                    .child("Resolución"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(resolution_button("512", 512, resolution, cx))
                                    .child(resolution_button("1024", 1024, resolution, cx)),
                            ),
                    )
                    .child(path_row("Imagen", &image, cx))
                    .child(
                        Button::new("studio-pick-image")
                            .small()
                            .ghost()
                            .icon(IconName::Image)
                            .label("Elegir imagen")
                            .on_click(cx.listener(|_, _, _, cx| {
                                pick_paths_for(cx, Pick::File.options(), |this, cx, paths| {
                                    if let Some(path) = paths.into_iter().next() {
                                        this.studio_image = Some(path);
                                        cx.notify();
                                    }
                                });
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("studio-generate")
                            .primary()
                            .icon(IconName::Sparkles)
                            .label("Generar 3D (próximamente)")
                            .disabled(true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notice =
                                    Some("El backend Trellis aún no está implementado.".to_owned());
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("La configuración ya se guarda; falta el proceso Trellis."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .p_4()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child("Visor 3D"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("studio-export")
                                            .small()
                                            .ghost()
                                            .icon(IconName::Download)
                                            .label("Exportar GLB")
                                            .disabled(true),
                                    )
                                    .child(
                                        Button::new("studio-blender")
                                            .small()
                                            .ghost()
                                            .icon(IconName::Send)
                                            .label("Enviar a Blender")
                                            .disabled(true),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .flex_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.muted)
                            .child(Icon::new(IconName::Cuboid).text_color(theme.muted_foreground))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Sin malla generada"),
                            )
                            .child(
                                div().text_xs().text_color(theme.muted_foreground).child(
                                    "El visor se habilitará cuando el motor 3D esté conectado.",
                                ),
                            )
                            .child(
                                Button::new("studio-settings")
                                    .small()
                                    .ghost()
                                    .icon(IconName::Settings)
                                    .label("Configurar motor 3D")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.workspace = WorkspaceMode::Settings;
                                        this.settings_tab = SettingsTab::Scene3d;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}

fn path_row(label: &'static str, value: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme().clone();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.foreground)
                .child(truncate(value, 46)),
        )
}

fn resolution_button(
    label: &'static str,
    value: u32,
    current: u32,
    cx: &mut Context<VoxApp>,
) -> impl IntoElement {
    let active = value == current;
    let button = Button::new(format!("res-{value}")).small().label(label);

    let button = if active {
        button.primary()
    } else {
        button.ghost()
    };

    button.on_click(cx.listener(move |this, _, _, cx| {
        config::update(cx, |cfg| cfg.scene3d.resolution = value);
        let _ = this;
        cx.notify();
    }))
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let mut text: String = value.chars().take(max).collect();
    text.push('…');
    text
}

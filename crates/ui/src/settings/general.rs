//! General section: appearance, paths and diagnostics entry point.

use gpui_kit::component::setting::{SettingGroup, SettingItem, SettingPage};

use super::fields::*;

/// Builds the general section.
pub fn page() -> SettingPage {
    SettingPage::new("General")
        .description("Apariencia, rutas y diagnóstico del entorno. Nada se descarga ni se instala.")
        .default_open(true)
        .group(
            SettingGroup::new()
                .title("Apariencia")
                .item(SettingItem::new(
                    "Tema",
                    dropdown_field(
                        &THEMES,
                        |cfg| cfg.ui.theme.clone(),
                        |cfg, value| cfg.ui.theme = value,
                    ),
                ))
                .item(SettingItem::new(
                    "Color de acento",
                    dropdown_field(
                        &ACCENTS,
                        |cfg| cfg.ui.accent.clone(),
                        |cfg, value| cfg.ui.accent = value,
                    ),
                ))
                .item(
                    SettingItem::new(
                        "Acento personalizado",
                        text_field(
                            |cfg| cfg.ui.accent_custom.clone(),
                            |cfg, value| cfg.ui.accent_custom = value.trim().to_owned(),
                        ),
                    )
                    .description("Hex #rrggbb, usado cuando el acento es «Personalizado»."),
                )
                .item(SettingItem::new(
                    "Radio de esquinas",
                    number_field(
                        0.0,
                        20.0,
                        1.0,
                        |cfg| f64::from(cfg.ui.radius),
                        |cfg, value| cfg.ui.radius = value as f32,
                    ),
                ))
                .item(SettingItem::new(
                    "Tamaño de fuente",
                    number_field(
                        11.0,
                        20.0,
                        0.5,
                        |cfg| f64::from(cfg.ui.font_size),
                        |cfg, value| cfg.ui.font_size = value as f32,
                    ),
                ))
                .item(SettingItem::new(
                    "Mostrar panel de depuración",
                    bool_field(
                        |cfg| cfg.ui.show_debug,
                        |cfg, value| cfg.ui.show_debug = value,
                    ),
                )),
        )
        .group(
            SettingGroup::new()
                .title("Rutas")
                .item(
                    SettingItem::new(
                        "Carpeta de datos",
                        text_field(
                            |cfg| {
                                cfg.paths
                                    .data_dir
                                    .as_ref()
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            },
                            |cfg, value| {
                                cfg.paths.data_dir = shared::tools::expand_path(&value);
                            },
                        ),
                    )
                    .description("Vacío = carpeta estándar del sistema."),
                )
                .item(path_picker(
                    "Carpeta de datos",
                    Pick::Folder,
                    |cfg| cfg.paths.data_dir.clone(),
                    |cfg, value| cfg.paths.data_dir = value,
                ))
                .item(SettingItem::new(
                    "Carpeta de recursos",
                    text_field(
                        |cfg| {
                            cfg.paths
                                .assets_dir
                                .as_ref()
                                .map(|p| p.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        },
                        |cfg, value| {
                            cfg.paths.assets_dir = shared::tools::expand_path(&value);
                        },
                    ),
                ))
                .item(path_picker(
                    "Carpeta de recursos",
                    Pick::Folder,
                    |cfg| cfg.paths.assets_dir.clone(),
                    |cfg, value| cfg.paths.assets_dir = value,
                )),
        )
}

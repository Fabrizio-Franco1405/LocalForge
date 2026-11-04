//! 3D motor (preview).
//!
//! The section exists so the harness has a place for local scene/mesh
//! generation, but no backend is implemented yet. Fields are shown disabled on
//! purpose: it is better to say "coming soon" than to fake a working engine.

use gpui_kit::component::setting::{SettingGroup, SettingItem, SettingPage};

use super::fields::*;

/// Builds the 3D section.
pub fn page() -> SettingPage {
    SettingPage::new("Motor 3D")
        .description(
            "Generación local de escenas y mallas. Aún no hay backend: esta sección \
             reserva el contrato para cuando exista el crate scene3d.",
        )
        .default_open(true)
        .group(
            SettingGroup::new()
                .title("Próximamente")
                .description(
                    "Backends previstos: ComfyUI local, Blender CLI o un pipeline propio. \
                     Se integrarán como un motor más, con proceso y health-check propios.",
                )
                .item(
                    SettingItem::new(
                        "Activar motor 3D",
                        bool_field(
                            |cfg| cfg.scene3d.enabled,
                            |cfg, value| cfg.scene3d.enabled = value,
                        ),
                    )
                    .description("Sin efecto hasta que exista el motor.")
                    .disabled(true),
                )
                .item(
                    SettingItem::new(
                        "Backend",
                        text_field(
                            |cfg| cfg.scene3d.backend.clone(),
                            |cfg, value| cfg.scene3d.backend = value,
                        ),
                    )
                    .disabled(true),
                )
                .item(
                    SettingItem::new(
                        "Endpoint",
                        text_field(
                            |cfg| cfg.scene3d.endpoint.clone(),
                            |cfg, value| cfg.scene3d.endpoint = value,
                        ),
                    )
                    .description("ComfyUI u otro servicio local.")
                    .disabled(true),
                )
                .item(
                    SettingItem::new(
                        "Carpeta de modelos",
                        path_field(
                            |cfg| cfg.scene3d.model_dir.clone(),
                            |cfg, value| cfg.scene3d.model_dir = value,
                        ),
                    )
                    .disabled(true),
                ),
        )
}

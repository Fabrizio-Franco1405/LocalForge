//! LLM motor: local `llama-server` or an external OpenAI-compatible endpoint.
//!
//! The two modes do not share fields. Local shows the binary, the GGUF model
//! and the process settings; external shows the endpoint, the model id and the
//! API key. Switching the dropdown rebuilds the page immediately because the
//! global config notifies its observers on every edit.

use gpui_kit::Axis;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::*;
use shared::{EngineCommand, LlmMode, ModelEntry, VoxConfig};

use super::fields::*;

/// Builds the LLM section for the selected mode.
pub fn page(mode: LlmMode, cfg: &VoxConfig, models: &[ModelEntry]) -> SettingPage {
    let page = SettingPage::new("Motor LLM")
        .description(
            "Configure el binario, el modelo y los parámetros del motor local, o un endpoint \
             compatible con OpenAI. LocalForge no descarga modelos ni binarios.",
        )
        .default_open(true)
        .group(connection_group(mode));

    let page = match mode {
        LlmMode::Managed => page
            .group(library_group(cfg, models))
            .group(managed_group()),
        LlmMode::External => page,
    };

    page.group(generation_group())
}

fn connection_group(mode: LlmMode) -> SettingGroup {
    let group = SettingGroup::new().title("Conexión").item(
        SettingItem::new(
            "Modo",
            dropdown_field(
                &LLM_MODES,
                |cfg| llm_mode_name(cfg.llm.mode).to_owned(),
                |cfg, value| cfg.llm.mode = llm_mode_from_name(&value),
            ),
        )
        .description(
            "Local: LocalForge inicia y supervisa el proceso local. Externo: conexión a un \
             servidor ya iniciado (Ollama, LM Studio, llama.cpp, pasarela…).",
        ),
    );

    match mode {
        LlmMode::Managed => group
            .item(
                SettingItem::new(
                    "Binario llama-server",
                    text_field(
                        |cfg| cfg.llm.binary.to_string_lossy().into_owned(),
                        |cfg, value| cfg.llm.binary = value.trim().into(),
                    ),
                )
                .description("Nombre en PATH o ruta absoluta a la compilación Vulkan.")
                .layout(Axis::Vertical),
            )
            .item(path_picker(
                "llama-server",
                Pick::File,
                |cfg| Some(cfg.llm.binary.clone()),
                |cfg, value| {
                    if let Some(path) = value {
                        cfg.llm.binary = path;
                    }
                },
            ))
            .item(
                SettingItem::new(
                    "Ruta manual (.gguf)",
                    path_field(
                        |cfg| cfg.llm.model.clone(),
                        |cfg, value| cfg.llm.model = value,
                    ),
                )
                .description("Ruta absoluta al .gguf que quieras cargar.")
                .layout(Axis::Vertical),
            )
            .item(path_picker(
                "Modelo GGUF",
                Pick::File,
                |cfg| cfg.llm.model.clone(),
                |cfg, value| cfg.llm.model = value,
            )),
        LlmMode::External => group
            .item(
                SettingItem::new(
                    "Endpoint",
                    text_field(
                        |cfg| cfg.llm.endpoint.clone(),
                        |cfg, value| cfg.llm.endpoint = value.trim().to_owned(),
                    ),
                )
                .description(
                    "URL completa (http://127.0.0.1:11434/v1) o solo host:puerto. \
                     «Probar conexión» aplica y sondea /v1/models.",
                )
                .layout(Axis::Vertical),
            )
            .item(
                SettingItem::new(
                    "ID del modelo",
                    text_field(
                        |cfg| cfg.llm.model_id.clone(),
                        |cfg, value| cfg.llm.model_id = value.trim().to_owned(),
                    ),
                )
                .description("Opcional. Vacío = primer id que reporte /v1/models."),
            )
            .item(
                SettingItem::new(
                    "API key",
                    text_field(
                        |cfg| cfg.llm.api_key.clone(),
                        |cfg, value| cfg.llm.api_key = value.trim().to_owned(),
                    ),
                )
                .description("Opcional; se guarda en texto plano en config.toml."),
            ),
    }
}

fn managed_group() -> SettingGroup {
    SettingGroup::new()
        .title("Proceso gestionado")
        .description("Parámetros con los que LocalForge lanza llama-server.")
        .item(
            SettingItem::new(
                "Host de escucha",
                text_field(
                    |cfg| cfg.llm.host.clone(),
                    |cfg, value| cfg.llm.host = value.trim().to_owned(),
                ),
            )
            .description("IP de escucha, por ejemplo 127.0.0.1. Nunca 0.0.0.0 sin querer."),
        )
        .item(SettingItem::new(
            "Puerto",
            number_field(
                1024.0,
                65_535.0,
                1.0,
                |cfg| f64::from(cfg.llm.port),
                |cfg, value| cfg.llm.port = value as u16,
            ),
        ))
        .item(
            SettingItem::new(
                "Capas en GPU (-ngl)",
                number_field(
                    -1.0,
                    999.0,
                    1.0,
                    |cfg| f64::from(cfg.llm.gpu_layers),
                    |cfg, value| cfg.llm.gpu_layers = value as i32,
                ),
            )
            .description("99 = todas. En 8 GB suele convenir 24–32 según el modelo."),
        )
        .item(SettingItem::new(
            "Contexto (-c)",
            number_field(
                512.0,
                262_144.0,
                512.0,
                |cfg| f64::from(cfg.llm.ctx_size),
                |cfg, value| cfg.llm.ctx_size = value as u32,
            ),
        ))
        .item(SettingItem::new(
            "Hilos de CPU",
            number_field(
                1.0,
                64.0,
                1.0,
                |cfg| f64::from(cfg.llm.threads),
                |cfg, value| cfg.llm.threads = value as u16,
            ),
        ))
        .item(
            SettingItem::new(
                "Arrancar con la aplicación",
                bool_field(
                    |cfg| cfg.llm.auto_start,
                    |cfg, value| cfg.llm.auto_start = value,
                ),
            )
            .description(
                "Solo se aplica al abrir LocalForge. Desactivado, el motor empieza apagado \
                 y no se reinicia al aplicar ajustes ni al enviar mensajes.",
            ),
        )
        .item(SettingItem::new(
            "Timeout de arranque (s)",
            number_field(
                5.0,
                600.0,
                5.0,
                |cfg| cfg.llm.startup_timeout_secs as f64,
                |cfg, value| cfg.llm.startup_timeout_secs = value as u64,
            ),
        ))
        .item(
            SettingItem::new(
                "Argumentos extra",
                text_field(
                    |cfg| cfg.llm.extra_args.join(" "),
                    |cfg, value| {
                        cfg.llm.extra_args = value.split_whitespace().map(str::to_owned).collect();
                    },
                ),
            )
            .description("Se añaden al final, después de los flags gestionados."),
        )
}

fn generation_group() -> SettingGroup {
    SettingGroup::new()
        .title("Generación")
        .description("Se aplica en cada petición de chat, en ambos modos.")
        .item(SettingItem::new(
            "Temperatura",
            number_field(
                0.0,
                2.0,
                0.05,
                |cfg| (f64::from(cfg.llm.temperature) * 100.0).round() / 100.0,
                |cfg, value| cfg.llm.temperature = value as f32,
            ),
        ))
        .item(SettingItem::new(
            "Máximo de tokens",
            number_field(
                16.0,
                32_768.0,
                16.0,
                |cfg| f64::from(cfg.llm.max_tokens),
                |cfg, value| cfg.llm.max_tokens = value as u32,
            ),
        ))
        .item(
            SettingItem::new(
                "Prompt de sistema",
                text_field(
                    |cfg| cfg.llm.system_prompt.clone(),
                    |cfg, value| cfg.llm.system_prompt = value,
                ),
            )
            .description("Se envía al inicio de cada conversación."),
        )
}

/// Model folder, scanned dropdown and per-model alias.
fn library_group(cfg: &VoxConfig, models: &[ModelEntry]) -> SettingGroup {
    let group = SettingGroup::new()
        .title("Biblioteca de modelos")
        .description("Seleccione una carpeta con archivos .gguf y asigne un alias a cada modelo. La ruta manual permanece disponible como alternativa.")
        .item(
            SettingItem::new(
                "Carpeta de modelos",
                path_field(
                    |cfg| cfg.llm.models_dir.clone(),
                    |cfg, value| cfg.llm.models_dir = value,
                ),
            )
            .description("LocalForge busca .gguf en esta carpeta y sus subcarpetas.")
            .layout(Axis::Vertical),
        )
        .item(path_picker(
            "Carpeta de modelos",
            Pick::Folder,
            |cfg| cfg.llm.models_dir.clone(),
            |cfg, value| cfg.llm.models_dir = value,
        ))
        .item(scan_button());

    if models.is_empty() {
        return group.item(
            SettingItem::new(
                "Modelos encontrados",
                SettingField::render(|_, _, _| {
                    gpui_kit::div()
                        .text_xs()
                        .child("Sin modelos: seleccione la carpeta y pulse «Escanear».")
                }),
            )
            .description("La lista se rellena tras escanear la carpeta."),
        );
    }

    let mut options = Vec::with_capacity(models.len());
    for entry in models {
        let label = model_label(cfg, entry);
        let display = if label == entry.name {
            format!("{label} · {} MB", entry.size_mb)
        } else {
            format!("{label} · {} · {} MB", entry.name, entry.size_mb)
        };
        options.push((
            SharedString::from(entry.path.clone()),
            SharedString::from(display),
        ));
    }

    let mut group = group.item(
        SettingItem::new(
            "Modelo instalado",
            SettingField::dropdown(
                options,
                {
                    let models = models.to_vec();
                    move |cx: &App| {
                        let cfg = crate::config::current(cx);
                        cfg.llm
                            .model
                            .as_ref()
                            .map(|path| SharedString::from(path.display().to_string()))
                            .unwrap_or_else(|| {
                                models
                                    .first()
                                    .map(|entry| SharedString::from(entry.path.clone()))
                                    .unwrap_or_default()
                            })
                    }
                },
                |value: SharedString, cx: &mut App| {
                    let path = std::path::PathBuf::from(value.to_string());
                    crate::config::update(cx, |cfg| {
                        let stem = path
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or("modelo")
                            .to_owned();
                        cfg.llm
                            .model_aliases
                            .entry(path.display().to_string())
                            .or_insert(stem);
                        cfg.llm.model = Some(path);
                        cfg.llm.model_id.clear();
                    });
                },
            ),
        )
        .description("Al cambiar y pulsar Aplicar se detiene el motor actual y se carga el modelo seleccionado."),
    );

    group = group.title("Biblioteca de modelos");

    for entry in models {
        let path = entry.path.clone();
        let name = entry.name.clone();
        group = group.item(
            SettingItem::new(name.clone(), model_alias_field(path.clone(), name))
                .description(format!("Nombre visible · ruta original: {path}"))
                .layout(Axis::Vertical),
        );
    }

    group
}

fn model_alias_field(path: String, fallback: String) -> SettingField<SharedString> {
    let get_path = path.clone();
    SettingField::input(
        move |cx: &App| {
            let cfg = crate::config::current(cx);
            cfg.llm
                .model_aliases
                .get(&get_path)
                .filter(|alias| !alias.trim().is_empty())
                .cloned()
                .map(SharedString::from)
                .unwrap_or_else(|| SharedString::from(fallback.clone()))
        },
        move |value: SharedString, cx: &mut App| {
            let value = value.trim().to_owned();
            let path = path.clone();
            crate::config::update(cx, |cfg| {
                cfg.llm.model_aliases.insert(path.clone(), value);
            });
        },
    )
}

fn scan_button() -> SettingItem {
    SettingItem::render(|_options, _window, cx| {
        let theme = cx.theme();
        gpui_kit::div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("scan-models")
                    .small()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .label("Escanear")
                    .on_click(|_, _, cx| {
                        if let Some(engine) = crate::config::engine(cx) {
                            engine.send(EngineCommand::ScanModels);
                        }
                    }),
            )
            .child(
                gpui_kit::div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("El escaneo también se ejecuta al aplicar cambios."),
            )
    })
}

fn model_label(cfg: &VoxConfig, entry: &ModelEntry) -> String {
    cfg.llm
        .model_aliases
        .get(&entry.path)
        .filter(|alias| !alias.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| entry.name.clone())
}

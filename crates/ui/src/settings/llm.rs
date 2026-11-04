//! LLM motor: local `llama-server` or an external OpenAI-compatible endpoint.
//!
//! The two modes do not share fields. Local shows the binary, the GGUF model
//! and the process settings; external shows the endpoint, the model id and the
//! API key. Switching the dropdown rebuilds the page immediately because the
//! global config notifies its observers on every edit.

use gpui_kit::component::setting::{SettingGroup, SettingItem, SettingPage};
use shared::LlmMode;

use super::fields::*;

/// Builds the LLM section for the selected mode.
pub fn page(mode: LlmMode) -> SettingPage {
    let page = SettingPage::new("Motor LLM")
        .description(
            "Conecta tu llama.cpp local o cualquier servidor compatible con OpenAI. \
             LocalForge no descarga modelos ni binarios.",
        )
        .default_open(true)
        .group(connection_group(mode));

    let page = match mode {
        LlmMode::Managed => page.group(managed_group()),
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
            "Local: LocalForge arranca tu llama-server. Externo: solo se conecta a uno ya \
             encendido (Ollama, LM Studio, llama.cpp, gateway…).",
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
                .description("Nombre en PATH o ruta absoluta a tu compilación Vulkan."),
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
                    "Modelo GGUF",
                    path_field(
                        |cfg| cfg.llm.model.clone(),
                        |cfg, value| cfg.llm.model = value,
                    ),
                )
                .description("Ruta absoluta al .gguf que quieras cargar."),
            )
            .item(path_picker(
                "Modelo GGUF",
                Pick::Typed("Modelo GGUF", &["gguf"]),
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
                ),
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
        .item(SettingItem::new(
            "Autoarranque",
            bool_field(
                |cfg| cfg.llm.auto_start,
                |cfg, value| cfg.llm.auto_start = value,
            ),
        ))
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
                |cfg| f64::from(cfg.llm.temperature),
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

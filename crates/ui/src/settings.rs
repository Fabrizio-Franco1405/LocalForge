//! Settings pages bound to [`GlobalConfig`].
//!
//! VoxLocal is a hub: every path, binary and command here belongs to the user.
//! Nothing in this file downloads, installs or probes the network; the tools
//! page reports what is missing and leaves the decision to the user.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::*;
use shared::{LlmMode, TtsBackend, VoxConfig};

use crate::config;

const LLM_MODES: [(&str, &str); 2] = [
    ("Gestionado por VoxLocal", "managed"),
    ("Servidor externo", "external"),
];
const TTS_BACKENDS: [(&str, &str); 3] = [
    ("Desactivado", "disabled"),
    ("Kokoro CLI (BYO)", "kokoro_cli"),
    ("Comando personalizado", "command"),
];
const VAD_MODES: [(&str, &str); 3] = [
    ("Calidad", "quality"),
    ("Agresivo", "aggressive"),
    ("Bajo", "low"),
];
const LANGUAGES: [(&str, &str); 6] = [
    ("Español", "es"),
    ("English", "en"),
    ("Português", "pt"),
    ("Français", "fr"),
    ("Deutsch", "de"),
    ("Italiano", "it"),
];
const THEMES: [(&str, &str); 3] = [
    ("Oscuro", "dark"),
    ("Claro", "light"),
    ("Sistema", "system"),
];
const ACCENTS: [(&str, &str); 6] = [
    ("Violeta", "violet"),
    ("Azul", "blue"),
    ("Esmeralda", "emerald"),
    ("Ámbar", "amber"),
    ("Rosa", "rose"),
    ("Personalizado", "custom"),
];

fn text_field<G, S>(get: G, set: S) -> SettingField<SharedString>
where
    G: Fn(&VoxConfig) -> String + 'static,
    S: Fn(&mut VoxConfig, String) + 'static,
{
    SettingField::input(
        move |cx: &App| SharedString::from(get(&config::current(cx))),
        move |value: SharedString, cx: &mut App| {
            let value = value.to_string();
            config::update(cx, |cfg| set(cfg, value));
        },
    )
}

fn path_field<G, S>(get: G, set: S) -> SettingField<SharedString>
where
    G: Fn(&VoxConfig) -> Option<std::path::PathBuf> + 'static,
    S: Fn(&mut VoxConfig, Option<std::path::PathBuf>) + 'static,
{
    SettingField::input(
        move |cx: &App| {
            get(&config::current(cx))
                .map(|path| SharedString::from(path.to_string_lossy().into_owned()))
                .unwrap_or_default()
        },
        move |value: SharedString, cx: &mut App| {
            let value = value.trim().to_owned();
            let parsed = shared::tools::expand_path(&value);
            config::update(cx, |cfg| set(cfg, parsed));
        },
    )
}

/// What the native dialog should pick.
#[derive(Clone, Copy)]
enum Pick {
    /// Any existing file.
    File,
    /// A file filtered by extension.
    Typed(&'static str, &'static [&'static str]),
    /// A directory.
    Folder,
}

/// Native explorer button shown under a path input.
fn path_picker<G, S>(label: &'static str, pick: Pick, get: G, set: S) -> SettingItem
where
    G: Fn(&VoxConfig) -> Option<std::path::PathBuf> + 'static,
    S: Fn(&mut VoxConfig, Option<std::path::PathBuf>) + 'static,
{
    let get = std::rc::Rc::new(get);
    let set = std::rc::Rc::new(set);

    SettingItem::render(move |_options, _window, cx| {
        let current = get(&config::current(cx));
        let shown = current
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Sin seleccionar".to_owned());

        let get_pick = std::rc::Rc::clone(&get);
        let set_pick = std::rc::Rc::clone(&set);
        let picker = move |_event: &gpui_kit::ClickEvent, _window: &mut Window, cx: &mut App| {
            let mut dialog = rfd::FileDialog::new();

            if let Some(path) = get_pick(&config::current(cx)) {
                let directory = if path.is_dir() {
                    Some(path.clone())
                } else {
                    path.parent().map(std::path::Path::to_path_buf)
                };
                if let Some(directory) = directory {
                    dialog = dialog.set_directory(directory);
                }
            }

            let picked = match pick {
                Pick::Folder => dialog.pick_folder(),
                Pick::File => dialog.pick_file(),
                Pick::Typed(name, extensions) => dialog.add_filter(name, extensions).pick_file(),
            };

            if let Some(path) = picked {
                config::update(cx, |cfg| set_pick(cfg, Some(path)));
            }
        };

        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                Button::new(format!("pick-{label}"))
                    .small()
                    .label(match pick {
                        Pick::Folder => "Elegir carpeta…",
                        Pick::File | Pick::Typed(..) => "Elegir archivo…",
                    })
                    .on_click(picker),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(shown),
            )
    })
}

fn bool_field<G, S>(get: G, set: S) -> SettingField<bool>
where
    G: Fn(&VoxConfig) -> bool + 'static,
    S: Fn(&mut VoxConfig, bool) + 'static,
{
    SettingField::switch(
        move |cx: &App| get(&config::current(cx)),
        move |value: bool, cx: &mut App| config::update(cx, |cfg| set(cfg, value)),
    )
}

fn number_field<G, S>(min: f64, max: f64, step: f64, get: G, set: S) -> SettingField<f64>
where
    G: Fn(&VoxConfig) -> f64 + 'static,
    S: Fn(&mut VoxConfig, f64) + 'static,
{
    SettingField::number_input(
        NumberFieldOptions { min, max, step },
        move |cx: &App| get(&config::current(cx)),
        move |value: f64, cx: &mut App| config::update(cx, |cfg| set(cfg, value)),
    )
}

fn dropdown_field<G, S>(
    options: &[(&'static str, &'static str)],
    get: G,
    set: S,
) -> SettingField<SharedString>
where
    G: Fn(&VoxConfig) -> String + 'static,
    S: Fn(&mut VoxConfig, String) + 'static,
{
    let options = options
        .iter()
        .map(|(label, value)| (SharedString::from(*label), SharedString::from(*value)))
        .collect();

    SettingField::dropdown(
        options,
        move |cx: &App| SharedString::from(get(&config::current(cx))),
        move |value: SharedString, cx: &mut App| {
            let value = value.to_string();
            config::update(cx, |cfg| set(cfg, value));
            cx.refresh_windows(); // <--- AÑADE ESTA LÍNEA AQUÍ
        },
    )
}

fn llm_mode_name(mode: LlmMode) -> &'static str {
    match mode {
        LlmMode::Managed => "managed",
        LlmMode::External => "external",
    }
}

fn llm_mode_from_name(name: &str) -> LlmMode {
    match name {
        "external" => LlmMode::External,
        _ => LlmMode::Managed,
    }
}

fn tts_backend_name(backend: TtsBackend) -> &'static str {
    match backend {
        TtsBackend::Disabled => "disabled",
        TtsBackend::KokoroCli => "kokoro_cli",
        TtsBackend::Command => "command",
    }
}

fn tts_backend_from_name(name: &str) -> TtsBackend {
    match name {
        "kokoro_cli" => TtsBackend::KokoroCli,
        "command" => TtsBackend::Command,
        _ => TtsBackend::Disabled,
    }
}

/// Builds the full settings panel.
pub fn view(_cx: &App) -> impl IntoElement {
    Settings::new("voxlocal-settings").pages(vec![
        SettingPage::new("Motor LLM")
            .description(
                "Conecta tu propio llama-server o cualquier servidor compatible con OpenAI. \
                 VoxLocal no descarga modelos ni binarios.",
            )
            .default_open(true)
            .group(
                SettingGroup::new()
                    .title("Conexión")
                    .item(SettingItem::new(
                        "Modo",
                        dropdown_field(
                            &LLM_MODES,
                            |cfg| llm_mode_name(cfg.llm.mode).to_owned(),
                            |cfg, value| cfg.llm.mode = llm_mode_from_name(&value),
                        ),
                    ))
                    .item(
                        SettingItem::new(
                            "Endpoint OpenAI",
                            text_field(
                                |cfg| cfg.llm.endpoint.clone(),
                                |cfg, value| cfg.llm.endpoint = value,
                            ),
                        )
                        .description("Solo para modo externo. Ej: http://127.0.0.1:11434/v1"),
                    )
                    .item(
                        SettingItem::new(
                            "ID del modelo",
                            text_field(
                                |cfg| cfg.llm.model_id.clone(),
                                |cfg, value| cfg.llm.model_id = value.trim().to_owned(),
                            ),
                        )
                        .description(
                            "Opcional. Si se deja vacío, se usa el primer id que reporte /v1/models.",
                        ),
                    )
                    .item(
                        SettingItem::new(
                            "API key",
                            text_field(
                                |cfg| cfg.llm.api_key.clone(),
                                |cfg, value| cfg.llm.api_key = value,
                            ),
                        )
                        .description("Opcional; se guarda en texto plano en config.toml."),
                    ),
            )
            .group(
                SettingGroup::new()
                    .title("Proceso gestionado")
                    .description("Parámetros usados cuando VoxLocal lanza llama-server.")
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
                        .description("Ruta absoluta al .gguf que quieras usar."),
                    )
                    .item(path_picker(
                        "Modelo GGUF",
                        Pick::Typed("Modelo GGUF", &["gguf"]),
                        |cfg| cfg.llm.model.clone(),
                        |cfg, value| cfg.llm.model = value,
                    ))
                    .item(
                        SettingItem::new(
                            "Host de escucha",
                            text_field(
                                |cfg| cfg.llm.host.clone(),
                                |cfg, value| {
                                    // Si el valor contiene una barra (/), ignorarlo y poner 127.0.0.1
                                    if value.contains('/') {
                                        cfg.llm.host = "127.0.0.1".to_owned();
                                    } else {
                                        cfg.llm.host = value.trim().to_owned();
                                    }
                                },
                            ),
                        )
                        .description("Solo IP (ej: 127.0.0.1). No pegar rutas de archivos."),
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
                    .item(SettingItem::new(
                        "Argumentos extra",
                        text_field(
                            |cfg| cfg.llm.extra_args.join(" "),
                            |cfg, value| {
                                cfg.llm.extra_args =
                                    value.split_whitespace().map(str::to_owned).collect();
                            },
                        ),
                    )),
            )
            .group(
                SettingGroup::new()
                    .title("Generación")
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
                    ),
            ),
        SettingPage::new("Reconocimiento")
            .description("Whisper.cpp es opcional. Sin él puedes escribir en el chat.")
            .group(
                SettingGroup::new()
                    .title("Whisper")
                    .item(SettingItem::new(
                        "Activar dictado",
                        bool_field(
                            |cfg| cfg.stt.enabled,
                            |cfg, value| cfg.stt.enabled = value,
                        ),
                    ))
                    .item(
                        SettingItem::new(
                            "Binario whisper-cli",
                            text_field(
                                |cfg| cfg.stt.binary.to_string_lossy().into_owned(),
                                |cfg, value| cfg.stt.binary = value.trim().into(),
                            ),
                        )
                        .description("Nombre en PATH o ruta absoluta."),
                    )
                    .item(path_picker(
                        "whisper-cli",
                        Pick::File,
                        |cfg| Some(cfg.stt.binary.clone()),
                        |cfg, value| {
                            if let Some(path) = value {
                                cfg.stt.binary = path;
                            }
                        },
                    ))
                    .item(
                        SettingItem::new(
                            "Modelo ggml",
                            path_field(
                                |cfg| cfg.stt.model.clone(),
                                |cfg, value| cfg.stt.model = value,
                            ),
                        )
                        .description("Ej: ~/.cache/whisper.cpp/ggml-base.bin"),
                    )
                    .item(path_picker(
                        "Modelo ggml",
                        Pick::Typed("Modelo Whisper", &["bin"]),
                        |cfg| cfg.stt.model.clone(),
                        |cfg, value| cfg.stt.model = value,
                    ))
                    .item(SettingItem::new(
                        "Idioma",
                        dropdown_field(
                            &LANGUAGES,
                            |cfg| cfg.stt.language.clone(),
                            |cfg, value| cfg.stt.language = value,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Traducir al inglés",
                        bool_field(
                            |cfg| cfg.stt.translate,
                            |cfg, value| cfg.stt.translate = value,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Hilos de CPU",
                        number_field(
                            1.0,
                            64.0,
                            1.0,
                            |cfg| f64::from(cfg.stt.threads),
                            |cfg, value| cfg.stt.threads = value as u16,
                        ),
                    )),
            ),
        SettingPage::new("Síntesis")
            .description(
                "Conecta tu propio motor TTS. El texto se envía por stdin y el comando \
                 debe escribir un WAV en la ruta {out}.",
            )
            .group(
                SettingGroup::new()
                    .title("Backend")
                    .item(SettingItem::new(
                        "Motor",
                        dropdown_field(
                            &TTS_BACKENDS,
                            |cfg| tts_backend_name(cfg.tts.backend).to_owned(),
                            |cfg, value| cfg.tts.backend = tts_backend_from_name(&value),
                        ),
                    ))
                    .item(
                        SettingItem::new(
                            "Comando",
                            text_field(
                                |cfg| cfg.tts.command.clone(),
                                |cfg, value| cfg.tts.command = value,
                            ),
                        )
                        .description(
                            "Placeholders: {out}, {voice}, {speed}. Ej: mi-tts --voice {voice} -o {out}",
                        ),
                    )
                    .item(SettingItem::new(
                        "Voz",
                        text_field(
                            |cfg| cfg.tts.voice.clone(),
                            |cfg, value| cfg.tts.voice = value,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Velocidad",
                        number_field(
                            0.5,
                            2.0,
                            0.05,
                            |cfg| f64::from(cfg.tts.speed),
                            |cfg, value| cfg.tts.speed = value as f32,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Argumentos extra",
                        text_field(
                            |cfg| cfg.tts.extra_args.join(" "),
                            |cfg, value| {
                                cfg.tts.extra_args =
                                    value.split_whitespace().map(str::to_owned).collect();
                            },
                        ),
                    )),
            )
            .group(
                SettingGroup::new()
                    .title("Preset Kokoro CLI")
                    .description("Solo si ya tienes kokoro-onnx instalado en un entorno propio.")
                    .item(SettingItem::new(
                        "Intérprete Python",
                        text_field(
                            |cfg| cfg.tts.python.to_string_lossy().into_owned(),
                            |cfg, value| cfg.tts.python = value.trim().into(),
                        ),
                    ))
                    .item(path_picker(
                        "Python",
                        Pick::File,
                        |cfg| Some(cfg.tts.python.clone()),
                        |cfg, value| {
                            if let Some(path) = value {
                                cfg.tts.python = path;
                            }
                        },
                    ))
                    .item(SettingItem::new(
                        "Script worker",
                        text_field(
                            |cfg| cfg.tts.worker_script.to_string_lossy().into_owned(),
                            |cfg, value| cfg.tts.worker_script = value.trim().into(),
                        ),
                    ))
                    .item(path_picker(
                        "Script Kokoro",
                        Pick::Typed("Script Python", &["py"]),
                        |cfg| Some(cfg.tts.worker_script.clone()),
                        |cfg, value| {
                            if let Some(path) = value {
                                cfg.tts.worker_script = path;
                            }
                        },
                    ))
                    .item(SettingItem::new(
                        "Carpeta del modelo",
                        text_field(
                            |cfg| cfg.tts.model_dir.to_string_lossy().into_owned(),
                            |cfg, value| cfg.tts.model_dir = value.trim().into(),
                        ),
                    ))
                    .item(path_picker(
                        "Modelos Kokoro",
                        Pick::Folder,
                        |cfg| Some(cfg.tts.model_dir.clone()),
                        |cfg, value| {
                            if let Some(path) = value {
                                cfg.tts.model_dir = path;
                            }
                        },
                    )),
            ),
        SettingPage::new("Audio y VAD")
            .group(
                SettingGroup::new()
                    .title("Dispositivos")
                    .item(
                        SettingItem::new(
                            "Micrófono",
                            text_field(
                                |cfg| cfg.audio.input_device.clone(),
                                |cfg, value| cfg.audio.input_device = value,
                            ),
                        )
                        .description("Vacío = dispositivo por defecto del sistema."),
                    )
                    .item(
                        SettingItem::new(
                            "Salida",
                            text_field(
                                |cfg| cfg.audio.output_device.clone(),
                                |cfg, value| cfg.audio.output_device = value,
                            ),
                        )
                        .description("Vacío = dispositivo por defecto del sistema."),
                    )
                    .item(SettingItem::new(
                        "Frecuencia (Hz)",
                        number_field(
                            8_000.0,
                            48_000.0,
                            1_000.0,
                            |cfg| f64::from(cfg.audio.sample_rate),
                            |cfg, value| cfg.audio.sample_rate = value as u32,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Volumen de reproducción",
                        number_field(
                            0.0,
                            1.0,
                            0.05,
                            |cfg| f64::from(cfg.audio.playback_volume),
                            |cfg, value| cfg.audio.playback_volume = value as f32,
                        ),
                    )),
            )
            .group(
                SettingGroup::new()
                    .title("Detección de voz")
                    .item(SettingItem::new(
                        "Modo WebRTC",
                        dropdown_field(
                            &VAD_MODES,
                            |cfg| cfg.audio.vad.mode.clone(),
                            |cfg, value| cfg.audio.vad.mode = value,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Habla mínima (ms)",
                        number_field(
                            50.0,
                            2_000.0,
                            10.0,
                            |cfg| f64::from(cfg.audio.vad.min_speech_ms),
                            |cfg, value| cfg.audio.vad.min_speech_ms = value as u32,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Silencio para cerrar (ms)",
                        number_field(
                            100.0,
                            3_000.0,
                            10.0,
                            |cfg| f64::from(cfg.audio.vad.min_silence_ms),
                            |cfg, value| cfg.audio.vad.min_silence_ms = value as u32,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Relleno previo (ms)",
                        number_field(
                            0.0,
                            1_000.0,
                            10.0,
                            |cfg| f64::from(cfg.audio.vad.padding_ms),
                            |cfg, value| cfg.audio.vad.padding_ms = value as u32,
                        ),
                    ))
                    .item(SettingItem::new(
                        "Duración máxima (ms)",
                        number_field(
                            1_000.0,
                            120_000.0,
                            500.0,
                            |cfg| f64::from(cfg.audio.vad.max_utterance_ms),
                            |cfg, value| cfg.audio.vad.max_utterance_ms = value as u32,
                        ),
                    ))
                    .item(
                        SettingItem::new(
                            "Full duplex (barge-in)",
                            bool_field(
                                |cfg| cfg.audio.vad.full_duplex,
                                |cfg, value| cfg.audio.vad.full_duplex = value,
                            ),
                        )
                        .description("Requiere auriculares o cancelación de eco."),
                    ),
            ),
        SettingPage::new("Interfaz")
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
            ),
    ])
}

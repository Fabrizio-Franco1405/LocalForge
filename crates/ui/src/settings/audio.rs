//! Audio motor: speech-to-text, text-to-speech, capture and VAD.
//!
//! Everything the voice pipeline needs lives in this single page, so the
//! section is isolated from the LLM and 3D motors.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage};
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::*;

use super::fields::*;

/// Builds the audio section.
pub fn page(devices: &crate::config::AudioDevices) -> SettingPage {
    SettingPage::new("Motor Audio")
        .description(
            "Whisper.cpp para reconocimiento de voz, Kokoro o un comando externo para síntesis \
             y cpal para captura. Todos los motores de audio son opcionales.",
        )
        .default_open(true)
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
                    .description("Solo si kokoro-onnx ya está instalado en un entorno propio.")
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
            )
        .group(
                SettingGroup::new()
                    .title("Dispositivos")
                    .item(
                        SettingItem::new(
                            "Micrófono",
                            device_dropdown(
                                &devices.inputs,
                                |cfg| cfg.audio.input_device.clone(),
                                |cfg, value| cfg.audio.input_device = value,
                            ),
                        )
                        .description("Dispositivos detectados en el sistema. Vacío = predeterminado."),
                    )
                    .item(
                        SettingItem::new(
                            "Salida",
                            device_dropdown(
                                &devices.outputs,
                                |cfg| cfg.audio.output_device.clone(),
                                |cfg, value| cfg.audio.output_device = value,
                            ),
                        )
                        .description("Dispositivos detectados en el sistema. Vacío = predeterminado."),
                    )
                    .item(refresh_devices_item())
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
            )
}

fn device_dropdown<G, S>(devices: &[String], get: G, set: S) -> SettingField<SharedString>
where
    G: Fn(&shared::VoxConfig) -> String + 'static,
    S: Fn(&mut shared::VoxConfig, String) + 'static,
{
    let mut options = vec![(
        SharedString::from(""),
        SharedString::from("(predeterminado del sistema)"),
    )];
    for device in devices {
        options.push((
            SharedString::from(device.clone()),
            SharedString::from(device.clone()),
        ));
    }

    SettingField::dropdown(
        options,
        move |cx: &App| SharedString::from(get(&crate::config::current(cx))),
        move |value: SharedString, cx: &mut App| {
            let value = value.to_string();
            crate::config::update(cx, |cfg| set(cfg, value));
        },
    )
}

fn refresh_devices_item() -> SettingItem {
    SettingItem::render(|_options, _window, cx| {
        let theme = cx.theme();
        gpui_kit::div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("refresh-audio-devices")
                    .small()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .label("Actualizar dispositivos")
                    .on_click(|_, _, cx| {
                        if let Some(engine) = crate::config::engine(cx) {
                            engine.send(shared::EngineCommand::ScanAudioDevices);
                        }
                    }),
            )
            .child(
                gpui_kit::div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        "La lista se actualiza al iniciar la aplicación y al pulsar este botón.",
                    ),
            )
    })
}

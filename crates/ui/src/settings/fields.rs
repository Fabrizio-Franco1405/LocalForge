//! Settings pages bound to [`GlobalConfig`].
//!
//! LocalForge is a hub: every path, binary and command here belongs to the user.
//! Nothing in this file downloads, installs or probes the network; the tools
//! page reports what is missing and leaves the decision to the user.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::setting::{NumberFieldOptions, SettingField, SettingItem};
use gpui_kit::*;
use shared::{LlmMode, TtsBackend, VoxConfig};

use crate::config;

pub(super) const LLM_MODES: [(&str, &str); 2] = [
    ("Gestionado por LocalForge", "managed"),
    ("Servidor externo", "external"),
];
pub(super) const TTS_BACKENDS: [(&str, &str); 3] = [
    ("Desactivado", "disabled"),
    ("Kokoro CLI (BYO)", "kokoro_cli"),
    ("Comando personalizado", "command"),
];
pub(super) const VAD_MODES: [(&str, &str); 3] = [
    ("Calidad", "quality"),
    ("Agresivo", "aggressive"),
    ("Bajo", "low"),
];
pub(super) const LANGUAGES: [(&str, &str); 6] = [
    ("Español", "es"),
    ("English", "en"),
    ("Português", "pt"),
    ("Français", "fr"),
    ("Deutsch", "de"),
    ("Italiano", "it"),
];
pub(super) const THEMES: [(&str, &str); 3] = [
    ("Oscuro", "dark"),
    ("Claro", "light"),
    ("Sistema", "system"),
];
pub(super) const ACCENTS: [(&str, &str); 6] = [
    ("Violeta", "violet"),
    ("Azul", "blue"),
    ("Esmeralda", "emerald"),
    ("Ámbar", "amber"),
    ("Rosa", "rose"),
    ("Personalizado", "custom"),
];

pub(super) fn text_field<G, S>(get: G, set: S) -> SettingField<SharedString>
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

pub(super) fn path_field<G, S>(get: G, set: S) -> SettingField<SharedString>
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
pub(super) enum Pick {
    /// Any existing file.
    File,
    /// A file filtered by extension.
    Typed(&'static str, &'static [&'static str]),
    /// A directory.
    Folder,
}

/// Native explorer button shown under a path input.
pub(super) fn path_picker<G, S>(label: &'static str, pick: Pick, get: G, set: S) -> SettingItem
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

pub(super) fn bool_field<G, S>(get: G, set: S) -> SettingField<bool>
where
    G: Fn(&VoxConfig) -> bool + 'static,
    S: Fn(&mut VoxConfig, bool) + 'static,
{
    SettingField::switch(
        move |cx: &App| get(&config::current(cx)),
        move |value: bool, cx: &mut App| config::update(cx, |cfg| set(cfg, value)),
    )
}

pub(super) fn number_field<G, S>(min: f64, max: f64, step: f64, get: G, set: S) -> SettingField<f64>
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

pub(super) fn dropdown_field<G, S>(
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
            cx.refresh_windows();
        },
    )
}

pub(super) fn llm_mode_name(mode: LlmMode) -> &'static str {
    match mode {
        LlmMode::Managed => "managed",
        LlmMode::External => "external",
    }
}

pub(super) fn llm_mode_from_name(name: &str) -> LlmMode {
    match name {
        "external" => LlmMode::External,
        _ => LlmMode::Managed,
    }
}

pub(super) fn tts_backend_name(backend: TtsBackend) -> &'static str {
    match backend {
        TtsBackend::Disabled => "disabled",
        TtsBackend::KokoroCli => "kokoro_cli",
        TtsBackend::Command => "command",
    }
}

pub(super) fn tts_backend_from_name(name: &str) -> TtsBackend {
    match name {
        "kokoro_cli" => TtsBackend::KokoroCli,
        "command" => TtsBackend::Command,
        _ => TtsBackend::Disabled,
    }
}

# Settings and input recipes for LocalForge

## Global config

```rust
use gpui_kit::Global;
use shared::VoxConfig;

pub struct GlobalConfig(pub VoxConfig);

impl Global for GlobalConfig {}

pub fn set_config(cx: &mut App, config: VoxConfig) {
    cx.set_global(GlobalConfig(config));
}

pub fn update_config(cx: &mut App, f: impl FnOnce(&mut VoxConfig)) {
    cx.update_global::<GlobalConfig, _>(|g, _| f(&mut g.0));
    if let Err(error) = cx.global::<GlobalConfig>().0.save(&VoxConfig::default_path()) {
        tracing::warn!(%error, "no se pudo guardar la configuración");
    }
}
```

`update_global` **notifies observers** automatically? It marks the global as
changed; views must call `cx.notify()` or observe the global. In a settings
setter, the `Settings` component re-renders itself on interaction; for other
views call `cx.notify()`.

## Complete settings page

```rust
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};

fn settings_view(cx: &mut App) -> impl IntoElement {
    let cfg = |cx: &App| cx.global::<GlobalConfig>().0.clone();

    Settings::new("localforge-settings")
        .pages(vec![
            SettingPage::new("Modelo")
                .default_open(true)
                .group(
                    SettingGroup::new()
                        .title("llama.cpp + Vulkan")
                        .item(
                            SettingItem::new(
                                "Modelo GGUF",
                                SettingField::input(
                                    |cx| cfg(cx).llm.model.unwrap_or_default().to_string_lossy().into(),
                                    |value: SharedString, cx| {
                                        update_config(cx, |c| {
                                            c.llm.model = if value.is_empty() { None } else { Some(value.into()) };
                                        });
                                    },
                                ),
                            )
                            .description("Ruta absoluta al archivo .gguf"),
                        )
                        .item(
                            SettingItem::new(
                                "Puerto",
                                SettingField::number_input(
                                    NumberFieldOptions { min: 1024.0, max: 65535.0, ..Default::default() },
                                    |cx| cfg(cx).llm.port as f64,
                                    |value: f64, cx| {
                                        update_config(cx, |c| c.llm.port = value as u16);
                                    },
                                ),
                            ),
                        )
                        .item(
                            SettingItem::new(
                                "Capas en GPU (-ngl)",
                                SettingField::number_input(
                                    NumberFieldOptions { min: 0.0, max: 999.0, ..Default::default() },
                                    |cx| cfg(cx).llm.gpu_layers as f64,
                                    |value: f64, cx| {
                                        update_config(cx, |c| c.llm.gpu_layers = value as i32);
                                    },
                                ),
                            )
                            .description("99 = todas. 24–28 para 8B en 8 GB."),
                        )
                        .item(
                            SettingItem::new(
                                "Arrancar con la aplicación",
                                SettingField::switch(
                                    |cx| cfg(cx).llm.auto_start,
                                    |value: bool, cx| update_config(cx, |c| c.llm.auto_start = value),
                                ),
                            ),
                        ),
                ),
            SettingPage::new("Voz")
                .group(
                    SettingGroup::new()
                        .title("Whisper")
                        .item(SettingItem::new(
                            "Modelo",
                            SettingField::input(
                                |cx| cfg(cx).stt.model.unwrap_or_default().to_string_lossy().into(),
                                |value: SharedString, cx| {
                                    update_config(cx, |c| {
                                        c.stt.model = if value.is_empty() { None } else { Some(value.into()) };
                                    });
                                },
                            ),
                        ))
                        .item(SettingItem::new(
                            "Idioma",
                            SettingField::dropdown(
                                vec![
                                    ("Español".into(), "es".into()),
                                    ("English".into(), "en".into()),
                                    ("Français".into(), "fr".into()),
                                ],
                                |cx| cfg(cx).stt.language.clone().into(),
                                |value: SharedString, cx| {
                                    update_config(cx, |c| c.stt.language = value.to_string());
                                },
                            ),
                        )),
                ),
            SettingPage::new("Síntesis")
                .group(
                    SettingGroup::new()
                        .title("Kokoro")
                        .item(SettingItem::new(
                            "Voz",
                            SettingField::dropdown(
                                vec![
                                    ("Español — Dora".into(), "ef_dora".into()),
                                    ("Español — Alex".into(), "em_alex".into()),
                                    ("Inglés — Heart".into(), "af_heart".into()),
                                ],
                                |cx| cfg(cx).tts.voice.clone().into(),
                                |value: SharedString, cx| {
                                    update_config(cx, |c| c.tts.voice = value.to_string());
                                },
                            ),
                        ))
                        .item(SettingItem::new(
                            "Velocidad",
                            SettingField::number_input(
                                NumberFieldOptions { min: 0.5, max: 2.0, ..Default::default() },
                                |cx| cfg(cx).tts.speed as f64,
                                |value: f64, cx| update_config(cx, |c| c.tts.speed = value as f32),
                            ),
                        )),
                ),
        ])
}
```

## Rendering the settings page

`Settings` is a `RenderOnce` element. Store per-page selection inside it via
the stable id `"localforge-settings"`:

```rust
impl Render for VoxApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .child(match self.page {
                Page::Chat => self.chat(cx).into_any_element(),
                Page::Llm => settings::view(cx, Page::Llm).into_any_element(),
                Page::Audio => settings::view(cx, Page::Audio).into_any_element(),
            })
    }
}
```

If `Settings` needs an `Entity` in a future version, create it in `new`, not in
`render`.

## File picker for model paths

GPUI Kit does not ship a native file dialog. Options:

1. Text field + `~` expansion + validation on blur (v0.1).
2. `rfd` crate (native dialogs; adds GTK on Linux) behind a button:
   `SettingField::render(|options, _, cx| Button::new("pick").label("Elegir…").on_click(...))`.
3. Scan `assets/models/llm` on startup and offer a dropdown.

Prefer (3) for LLMs and (1) as fallback; it avoids a heavy dialog dependency and
is testable.

## Input with submit

```rust
use gpui_kit::component::input::{Input, InputEvent, InputState};

let input = cx.new(|cx| InputState::new(window, cx).placeholder("Escribe un mensaje…"));

let sub = cx.subscribe_in(&input, window, |this, state, event, window, cx| {
    if matches!(event, InputEvent::PressEnter { .. }) {
        let text = state.read(cx).value();
        if !text.trim().is_empty() {
            this.send_message(text, cx);
            state.update(cx, |s, cx| s.set_value("", window, cx));
        }
    }
});
```

Store `sub` in the view. Enter submits; Shift+Enter can insert a newline only if
you use a `Textarea` instead.

## Error notifications

```rust
use gpui_kit::component::notification::Notification;

window.push_notification(Notification::error(message), cx);
window.push_notification(Notification::success("Configuración guardada"), cx);
```

Use notifications for transient errors; keep a persistent status line for
engine state (`Stopped/Starting/Ready/Error`).

## Accessibility

- Give icon-only buttons `.accessibility_label("Detener respuesta")`.
- Do not encode state only by color; pair it with text ("Listo", "Error").
- Ensure focus order follows the visual layout; don't intercept Tab.

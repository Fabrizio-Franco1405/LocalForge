---
name: gpui-kit-desktop
description: >
  Use when building or changing VoxLocal's desktop UI with GPUI Kit 0.9:
  creating Views/Entities, wiring render and cx.listener, using the Settings
  component for config pages, InputState, tasks, Globals, themes, notifications,
  or writing UI integration tests. Includes GPUI 0.9 API pitfalls.
license: MIT
compatibility: GPUI Kit 0.9, Rust 1.92+, GPUI (Zed)
metadata:
  author: voxlocal
  version: "0.1.0"
---

# GPUI Kit desktop

## When to use this skill

Any change under `crates/ui` or `src/main.rs` that touches rendering, state,
window creation, settings forms, input fields, async UI pumps, or UI tests.

## Bootstrapping (verified with gpui-kit 0.9.0)

```rust
use gpui_kit::*;

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);                       // required before any component
        open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| MyView::new(cx))
        })
        .expect("failed to open window");
    });
}
```

- `use gpui_kit::*;` re-exports GPUI; do not add `gpui` directly.
- `init(cx)` initializes components, themes, icons, and i18n.
- `application().with_assets(gpui_kit::assets::Assets)` provides icons/fonts.
- Menus, keybindings, and actions are registered after `init`.

## State ownership

| Lifetime | Mechanism | Use for |
| --- | --- | --- |
| App | `Global` | config, theme, services shared by all windows |
| Feature/model | `Entity<T>` | chat history, settings draft, engine status |
| Window/View | struct fields | page selection, input handle, task handle |
| Transient | local in `render` | labels, colors, layout values |

Rules:

- Create `Entity` with `cx.new`; read with `.read(cx)`; mutate with
  `.update(cx, |value, cx| { ...; cx.notify(); })`.
- Keep `Subscription`s and `Task`s in fields of the owning view; dropping them
  unsubscribes/cancels.
- `Global` is keyed by type. Implement `Global` for a newtype:
  `pub struct GlobalConfig(pub VoxConfig); impl Global for GlobalConfig {}`
- Use `cx.observe(&entity, |this, _, cx| cx.notify())` to re-render on changes.
- Never create an `Entity` inside `render`.

## Render pattern

```rust
impl Render for VoxApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(header())
            .child(chat_list(&self.messages, cx))
            .child(input_row(&self.input))
    }
}
```

- `cx` is `Context<Self>`; it derefs to `App`, so `cx.global::<T>()`,
  `cx.theme()`, and `cx.spawn(...)` are available.
- Callbacks from buttons use `cx.listener(|this, event, window, cx| ...)` when
  they need `self`; use `|_, _, cx| ...` for stateless actions.
- `cx.notify()` schedules a repaint; call it after meaningful state changes,
  not on every render.
- `RenderOnce` components are values built during render; they do not retain
  state. Use `Entity` for retained state.

## Tasks and async work

```rust
struct VoxApp {
    _pump: Task<()>,
}

impl VoxApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let _pump = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if this.update(cx, |app, cx| app.pump(cx)).is_err() {
                    break;
                }
            }
        });
        Self { _pump }
    }
}
```

- `cx.spawn` runs on the foreground executor and can re-enter GPUI via
  `this.update`.
- `cx.background_spawn` runs `Send` CPU work; it cannot touch entities there.
- Dropping a `Task` cancels it. Replace the field to restart.
- Do not block the foreground executor with `std::thread::sleep`, blocking I/O,
  or `Runtime::block_on`.
- For cross-thread work use channels (VoxLocal's `EngineHandle`) and poll with a
  timer; this avoids mixing tokio's reactor with GPUI's executor.

## Settings component (config UI)

Import:

```rust
use gpui_kit::component::setting::{
    Settings, SettingPage, SettingGroup, SettingItem, SettingField,
};
```

Structure:

```
Settings
  SettingPage("Modelo")
    SettingGroup("llama.cpp")
      SettingItem("Ruta del modelo", SettingField::input(get, set))
```

Field constructors:

| Field | Signature |
| --- | --- |
| switch | `SettingField::switch(\|cx: &App\| bool, \|bool, &mut App\|)` |
| checkbox | same as switch, checkbox UI |
| input | `SettingField::input(\|cx\| SharedString, \|SharedString, &mut App\|)` |
| dropdown | `SettingField::dropdown(vec![(label, value)], get, set)` |
| number_input | `SettingField::number_input(NumberFieldOptions { min, max, ..Default::default() }, get, set)` |
| render | `SettingField::render(\|options, window, cx\| element)` |
| element | `SettingField::element(custom_setting_field_element)` |

Notes:

- Getter closures run during render; keep them cheap and side-effect free.
- Setter closures receive `&mut App`; use `cx.update_global::<GlobalConfig, _>(...)`.
- Persist to disk in the setter (or debounce); do not do I/O in the getter.
- `.description(...)`, `.keywords([...])`, `.disabled(true)`, `.layout(Axis::Vertical)`.
- `Settings::new("id").pages(vec![...])`; `SettingPage::new("Name").default_open(true)`.
- `Settings` is value-like; it stores its own search/page state. For a stable
  page selection across renders, give it a stable id.

## Input

```rust
let input = cx.new(|cx| InputState::new(window, cx).placeholder("Escribe…"));

cx.subscribe_in(&input, window, |this, state, event, window, cx| {
    if matches!(event, InputEvent::PressEnter { .. }) {
        let text = state.read(cx).value();
        this.send(text, cx);
        state.update(cx, |s, cx| s.set_value("", window, cx));
    }
});
```

- `InputState::value()` returns the text; `set_value` does not emit Change.
- Subscribe to `InputEvent::Change` only if you need live validation.
- Keep the `Subscription` in the view; otherwise the handler is dropped.
- `Input::new(&state)` renders; suffix/prefix accept any element.

## Components used by VoxLocal

- `Button` / `ButtonVariants`: `.primary()`, `.ghost()`, `.outline()`,
  `.label()`, `.icon(IconName::...)`, `.small()`.
- `Settings` family for config.
- `Message`/`Bubble` for chat rows (or plain divs with theme colors).
- `Notification`/`WindowExt` for errors.
- `Icon`/`IconName` for icons (no `Mic` glyph in 0.9; use a label or
  `IconName::AudioLines` if available in the current release).
- `Tooltip`, `Spinner`, `Progress` for status.
- `Root` view is optional for overlays; plain `div` is enough for v1.

## Speech component (optional path)

GPUI Kit 0.9 has a `speech` feature with `SpeechState`, `SpeechButton`,
`SpeechWaveform`, and the traits `SpeechRecognizer` / `AudioInput`.

- Enable with `gpui-kit = { version = "0.9.0", features = ["speech"] }`
  (adds cpal + ALSA on Linux).
- Linux has no system recognizer: you **must** provide a custom
  `SpeechRecognizer` (e.g., whisper.cpp).
- The state expects 16 kHz mono i16 in `push_audio`; `finish` commits the
  transcript.
- VoxLocal v0.1 uses its own `audio_engine` for VAD/barge-in, so the speech
  feature is not enabled by default. It is a good shortcut for a push-to-talk
  dictation field later.

## Theming

- Use semantic tokens: `cx.theme().background`, `.foreground`, `.muted`,
  `.muted_foreground`, `.border`, `.primary`, `.secondary`, `.danger`.
- Do not hardcode hex colors in views; themes (light/dark) must both work.
- Theme switching: set the global theme and notify; see GPUI Kit Theme docs.
- Text sizing via `.text_sm()`, `.text_lg()`, or the theme's font size.

## Common 0.9 pitfalls

| Pitfall | Fix |
| --- | --- |
| Missing `init(cx)` | components panic or render unstyled |
| `Entity` created in `render` | state resets every frame |
| `Subscription` not stored | callbacks stop firing |
| `Task` not stored | async work is cancelled immediately |
| Blocking in render/callback | UI freezes; use a channel + pump |
| Setting a global without notify | views observing the global do not repaint |
| Using old `StreamTrait::play` (cpal) | unrelated but same era: use `.start()` |
| Direct `gpui` dependency | version mismatch; use `gpui_kit::*` |

## UI test pattern

```rust
#[cfg(test)]
mod tests {
    // GPUI Kit exposes a test window extension; enable `test-support`.
    // Assert on accessible labels, not colors or pixel positions.
}
```

- Test behavior: clicking "Enviar" with empty input does nothing; a token
  event appends text; a settings change updates the global.
- Keep business logic out of `render` so it can be tested without a window.

## Verification

```bash
cargo run -p voxlocal            # the actual app
cargo clippy -p ui -- -D warnings
```

- Resize the window: no overlapping or clipped controls.
- Switch theme: contrast remains readable.
- Close the window: no panic, controller receives shutdown.
- Disconnect the model: error is visible, UI remains interactive.

## References

- [`references/settings-and-input.md`](references/settings-and-input.md) —
  complete settings/input recipes for VoxLocal's config.
- [`references/speech-and-tasks.md`](references/speech-and-tasks.md) — speech
  recognizer seam and task/async patterns.
- `https://gpui-kit.com/llms-full.txt` — authoritative API text (4.6 MB).

# Speech seam and async tasks in GPUI Kit 0.9

## SpeechState at a glance

Enable the feature:

```toml
gpui-kit = { version = "0.9.0", features = ["speech"] }
```

Key types (module `gpui_kit::component::speech`):

- `SpeechState`: session owner. Construct with `cx.new(SpeechState::new)`.
- `SpeechButton`, `SpeechWaveform`, `SpeechIndicator`: UI.
- `SpeechEvent`: `Started`, `Partial(String)`, `Final(String)`, `Cancelled`,
  `Error(SpeechError)`.
- `SpeechStatus`: `Idle`, `Connecting`, `Recording`, `Stopping`.
- `SpeechRecognizer` / `RecognitionSession` / `SpeechSink`: recognition seam.
- `AudioInput` / `AudioSink` / `AudioFormat`: capture seam.
- `Microphone`: built-in cpal input (non-Windows/Linux; ALSA on Linux).

## Implement a whisper-backed recognizer (sketch)

```rust
use gpui_kit::component::speech::{
    RecognitionSession, SpeechError, SpeechRecognizer, SpeechSink,
};
use std::sync::{Arc, Mutex};

pub struct WhisperRecognizer {
    pub engine: Arc<stt::WhisperCli>,
}

impl SpeechRecognizer for WhisperRecognizer {
    fn start(&self, sink: SpeechSink, cx: &mut gpui_kit::App)
        -> Result<Box<dyn RecognitionSession>, SpeechError>
    {
        sink.ready(cx);
        Ok(Box::new(WhisperSession {
            engine: self.engine.clone(),
            sink,
            samples: Vec::new(),
        }))
    }
}

struct WhisperSession {
    engine: Arc<stt::WhisperCli>,
    sink: SpeechSink,
    samples: Vec<f32>,
}

impl RecognitionSession for WhisperSession {
    fn push_audio(&mut self, samples: &[i16], _cx: &mut gpui_kit::App) {
        // 16 kHz mono i16 -> f32
        self.samples.extend(samples.iter().map(|s| *s as f32 / 32768.0));
    }

    fn finish(&mut self, cx: &mut gpui_kit::App) {
        // whisper-cli is blocking: run it on a background thread and report
        // through the sink when done. Never block the UI thread.
        let engine = self.engine.clone();
        let samples = std::mem::take(&mut self.samples);
        let sink = self.sink.clone();
        std::thread::spawn(move || match engine.transcribe(&samples) {
            Ok(t) => {
                // Sink calls are applied on the next update; safe from here.
                sink.phrase(&t.text, cx); // `cx` is not Send: see note below
                sink.finish(cx);
            }
            Err(e) => sink.error(SpeechError::recognizer(e.to_string()), cx),
        });
    }
}
```

**Important:** `&mut App` is not `Send`, so you cannot move it into
`std::thread::spawn`. Use `cx.spawn`/`cx.background_spawn`, or keep the
recognition synchronous in `finish` (blocking the session, not the frame) only
for short audio. The correct pattern is:

```rust
fn finish(&mut self, cx: &mut App) {
    let engine = self.engine.clone();
    let samples = std::mem::take(&mut self.samples);
    let sink = self.sink.clone();
    cx.spawn(async move |cx| {
        let result = cx.background_spawn(async move { engine.transcribe(&samples) }).await;
        match result {
            Ok(t) => { sink.phrase(&t.text, cx); sink.finish(cx); }
            Err(e) => sink.error(SpeechError::recognizer(e.to_string()), cx),
        }
    })
    .detach();
}
```

`SpeechSink::phrase` expects the *committed phrase* with separators; the state
joins phrases verbatim. Add a space between sentences for Spanish/English.

## When to use the built-in speech vs LocalForge audio_engine

| Requirement | Built-in Speech | audio_engine |
| --- | --- | --- |
| Push-to-talk dictation field | ✅ | possible |
| Live waveform | ✅ (`SpeechWaveform`) | own meter |
| VAD-driven turn taking | ❌ (session-level) | ✅ |
| Barge-in / interrupt TTS | ❌ | ✅ |
| 16 kHz mono pipeline | ✅ | ✅ |
| Full control of device selection | partial | ✅ |

LocalForge v0.1 uses `audio_engine` for the conversation loop and can adopt the
built-in speech later for a "dictate into the composer" button.

## Task patterns

### Debounced config save

```rust
struct SettingsDraft {
    _save_task: Option<Task<()>>,
}

fn schedule_save(&mut self, cx: &mut Context<Self>) {
    self._save_task = Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(Duration::from_millis(400)).await;
        let _ = this.update(cx, |this, _| this.persist());
    }));
}
```

Dropping the previous task cancels the pending save, giving debounce semantics.

### Cancellable engine request

```rust
self.request = Some(cx.spawn(async move |this, cx| {
    // Send the command; poll events in the view's pump instead of awaiting
    // the engine directly. This keeps one source of truth for UI state.
}));
```

For LocalForge, the command goes through `EngineHandle`; the task only drives the
polling loop. Cancel by bumping the generation and sending `Cancel`.

### Background CPU work

```rust
let summary = cx.background_spawn(async move {
    expensive_pure_function(owned_data)
}).await;
this.update(cx, |this, cx| {
    this.summary = summary;
    cx.notify();
});
```

Use `background_spawn` for parsing, resampling, or file scanning; never for
`reqwest`/tokio-I/O unless the future is runtime-agnostic.

## Timers and polling cadence

- UI event pump: 50 ms (20 Hz) is enough for token streaming.
- Waveform: 16–33 ms; use the levels provided by the audio engine.
- Health checks: 1–2 s; the controller owns them, not the UI.
- Avoid `cx.notify()` on every poll if nothing changed; compare state first.

## Testing notes

- Business logic (parsers, state machines) belongs outside views and is tested
  with plain `#[test]`.
- For UI behavior, enable `gpui-kit`'s `test-support` feature and assert on
  accessibility labels and visible text, not pixels.
- A test should not require a GPU, microphone, or a real llama-server. Inject a
  fake `EngineHandle` or call the view's state methods directly.

# Crate contracts

## `shared`

```rust
pub use config::VoxConfig;
pub use event::{EngineCommand, EngineEvent, LlmState};
pub use handle::{EngineHandle, engine_channel};

pub fn for_speech(text: &str) -> String;
```

- `EngineHandle` is `Clone + Send + Sync`. It exposes `send(&self, cmd)`,
  `try_recv(&self) -> Option<EngineEvent>`, and `drain(&self) -> Vec<EngineEvent>`.
- `EngineConfig` is serializable; every struct derives
  `Debug, Clone, PartialEq, Serialize, Deserialize`.
- No `async` in this crate: it must stay usable from GPUI, tokio, and tests.

## `llm_engine`

```rust
pub enum LlmHealth { Down, Loading, Ready }

pub struct ChatMessage { pub role: Role, pub content: String }
pub enum Role { System, User, Assistant }

pub enum ChatDelta {
    Delta(String),
    Done,
    Error(LlmError),
}

impl LlamaManager {
    pub fn new(cfg: &LlmConfig) -> Result<Self, LlmError>;
    pub fn build_args(cfg: &LlmConfig) -> Result<Vec<String>, LlmError>;
    pub async fn start(&mut self) -> Result<(), LlmError>;
    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), LlmError>;
    pub async fn health(&self) -> LlmHealth;
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<mpsc::Receiver<ChatDelta>, LlmError>;
    pub async fn stop(&mut self);
    pub fn last_logs(&self, n: usize) -> Vec<String>;
}
```

- `build_args` is pure: test it without spawning.
- `start` is idempotent: if a child is running, it returns `Ok(())`.
- `wait_ready` polls `/health` every 250 ms; `503` means loading, not failure.
- `chat_stream` returns a channel so the caller can select on cancellation.
- `Drop` calls `start_kill`; `kill_on_drop(true)` is set on the child.

## `stt`

```rust
pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
}

impl WhisperCli {
    pub fn new(cfg: &SttConfig) -> Self;
    pub fn transcribe(&self, samples_16k_mono: &[f32]) -> Result<Transcript, SttError>;
}

pub fn parse_output(stdout: &str) -> String; // pure, unit-tested
```

- Accepts only 16 kHz mono `f32` in `-1.0..=1.0`; clamps on encode.
- Writes a `PCM16` WAV in the OS temp dir and removes it on all paths (use a
  guard struct).
- Non-zero exit is an error and includes the last stderr lines.

## `tts`

```rust
pub struct AudioBuffer { pub samples: Vec<f32>, pub sample_rate: u32 }

pub trait Synthesizer: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, TtsError>;
}

pub struct KokoroCli;
pub struct ExternalCommand;
```

- `KokoroCli` sends text on **stdin** (not argv) to avoid command-line length
  and injection issues.
- It returns `AudioBuffer` at the model's native rate (24 kHz for Kokoro);
  `AudioPlayer` resamples.
- `ExternalCommand` splits the configured command with shell-like quoting and
  substitutes `{out}`, `{voice}`, `{speed}`. Text still goes on stdin.

## `audio_engine`

```rust
pub struct AudioCapture;      // start() -> (Receiver<Vec<f32>>, CaptureHandle)
pub struct AudioPlayer;       // play(AudioBuffer) / stop() / is_playing()
pub struct LinearResampler;
pub struct EndpointDetector<V: VoiceActivity>;

pub trait VoiceActivity {
    fn is_voice(&mut self, frame_16k: &[f32]) -> bool;
}

pub enum EndpointEvent {
    SpeechStart,
    SpeechEnd(Vec<f32>),
}
```

- Capture always yields 16 kHz mono `f32`, regardless of device format.
- `EndpointDetector` is generic over `VoiceActivity` so tests use a mock and
  do not need a microphone or ONNX.
- `AudioPlayer` accepts any input rate and resamples to the device rate.
- cpal `Stream` handles stay in the returned guard. Dropping the guard stops
  the stream.

## `ui`

```rust
pub struct VoxApp; // implements Render
impl VoxApp {
    pub fn new(handle: EngineHandle, cx: &mut Context<Self>) -> Self;
}
```

- Reads config from the `GlobalConfig` global (set by `main`).
- Polls `EngineHandle` every 50 ms in a `Task<()>` stored in the view.
- Never imports engine crates.

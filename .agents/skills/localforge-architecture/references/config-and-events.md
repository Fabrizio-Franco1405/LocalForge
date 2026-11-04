# Config schema and event lifecycle

## Config schema (v0.1)

```toml
[llm]
binary = "llama-server"
model = "/home/me/models/qwen3-4b-q4_k_m.gguf"
host = "127.0.0.1"
port = 9931
gpu_layers = 99
ctx_size = 4096
threads = 8
temperature = 0.7
max_tokens = 512
system_prompt = "Eres un asistente útil. Responde de forma breve y natural."
extra_args = []
auto_start = true
startup_timeout_secs = 90

[stt]
binary = "whisper-cli"
model = "/home/me/models/whisper/ggml-small.bin"
language = "es"
threads = 8
translate = false
extra_args = []

[tts]
backend = "kokoro_cli"          # o "external_command"
python = ".venv-kokoro/bin/python"
worker_script = "tools/kokoro_worker.py"
model_dir = "assets/models/kokoro"
voice = "ef_dora"
speed = 1.0
command = ""                     # solo para external_command
extra_args = []

[audio]
input_device = ""                # vacío = dispositivo por defecto
output_device = ""
sample_rate = 16000
playback_volume = 1.0

[audio.vad]
mode = "quality"                 # low | aggressive | quality
min_speech_ms = 250
min_silence_ms = 650
padding_ms = 300
max_utterance_ms = 30000

[ui]
theme = "dark"
show_debug = false

[scene3d]
enabled = false
backend = "disabled"
endpoint = ""
command = ""
```

Environment overrides:

```bash
LOCALFORGE_LLM__PORT=9940
LOCALFORGE_STT__LANGUAGE=en
LOCALFORGE_AUDIO__VAD__MIN_SILENCE_MS=800
```

## Loading rules

```rust
let config = VoxConfig::load(&VoxConfig::default_path())?;
```

- Missing file → defaults, no error.
- Unknown keys → ignored (`serde` default behavior).
- Missing keys → filled by `#[serde(default)]`.
- Invalid types → `ConfigError` with the TOML location.
- `save` creates the parent directory and writes pretty TOML atomically
  (write to `config.toml.tmp`, then rename).

## Event lifecycle

### Send a message

```
UI: generation += 1
UI: push user message + empty assistant message
UI: send SendMessage { generation, prompt }
UI: status = "Generando…"

Controller:
  ensure LLM started (start + wait_ready)
  push system prompt if history empty
  chat_stream(messages)
  for each delta: send Token { generation, delta }
  on finish: send ReplyDone { generation, full_text }

UI:
  Token with matching generation → append delta to assistant message
  ReplyDone → streaming = false
  Token with old generation → drop
```

### Cancel

```
UI: generation += 1 (invalidates in-flight events), send Cancel { generation }
Controller: abort streaming task, send ReplyDone with partial text
```

Dropping the tokio task closes the SSE response; llama-server aborts the
request. Do not rely on the server finishing.

### Start/stop engine

```
StartLlm:
  state = Starting → start() → wait_ready(90s) → state = Ready
  on error → state = Error(detail) + last logs

StopLlm:
  state = Stopping → stop() → state = Stopped

ReloadConfig:
  replace config; if running, RestartLlm
```

### Errors

`EngineEvent::Error { scope, message }` must be terminal for the operation
that produced it. The UI shows a notification and resets the streaming flag.
Never emit `Error` and then keep emitting deltas for the same generation.

## Generation counter invariants

- Monotonic `u64`, incremented on every user-initiated operation.
- Stored in both UI and controller (controller echoes it).
- Events from `generation < current` are ignored by the UI.
- The controller must not reuse a generation for two operations.

## Persistence

- Config: TOML at `VoxConfig::default_path()`.
- Sessions (M1): JSONL at `dirs::data_dir()/localforge/sessions/<timestamp>.jsonl`.
- Logs: `dirs::data_dir()/localforge/logs/localforge.log` (rotating, 5 files × 10 MB).
- Never store audio by default; STT temp WAVs are deleted after transcription.

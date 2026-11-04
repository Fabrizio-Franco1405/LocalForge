---
name: localforge-architecture
description: >
  Use when adding or changing a feature in LocalForge, deciding which crate owns
  a piece of logic, wiring the GPUI UI to an engine, adding a configuration
  field, defining a new EngineCommand/EngineEvent, or reviewing architecture
  boundaries. Covers the 8-crate workspace, threading model, error strategy,
  and the UI/engine contract.
license: MIT
compatibility: Rust 1.92+, Cargo, GPUI Kit 0.9
metadata:
  author: localforge
  version: "0.1.0"
---

# LocalForge Architecture

## When to use this skill

Load this skill before touching more than one crate, before exposing engine
state to the UI, or when a change "feels like it belongs everywhere". It is the
default entry point for feature work and code review.

Related skills: `gpui-kit-desktop` (views), `vulkan-ai-stack` (inference),
`voice-pipeline` (audio flow), `process-supervision` (child processes).

## Hard rules

1. **The UI never launches or inspects processes.** `crates/ui` must not
   depend on `llm_engine`, `stt`, `tts`, or `audio_engine`. It talks to
   `shared::EngineHandle` only.
2. **Engines never depend on GPUI.** They are plain libraries with `async`
   (LLM/STT) or blocking (TTS) APIs and unit tests that run headless.
3. **`shared` is a leaf.** It must not import any other workspace crate.
4. **One source of truth for config.** No `std::env::var` outside
   `shared::config`; no hardcoded model paths in views.
5. **Every engine operation returns a typed error.** No `unwrap`/`expect`
   outside tests and `main` bootstrapping.
6. **No CUDA/ROCm/PyTorch.** See `vulkan-ai-stack`.
7. **Long work never runs in `render`.** Spawn a task or send a command.

## Crate map and ownership

| Crate | Owns | Must not own |
| --- | --- | --- |
| `shared` | config schema, commands/events, `EngineHandle`, text cleanup, common errors | processes, HTTP, cpal |
| `controller` | command loop, tokio runtime, process/HTTP ownership, generations, diagnostics | GPUI, rendering |
| `llm_engine` | `llama-server` lifecycle, health, SSE parsing, chat history transport | UI state, TTS |
| `stt` | WAV encoding, `whisper-cli` invocation, transcript parsing | microphone, VAD |
| `tts` | Kokoro worker invocation, WAV decode, text-to-speech contract | playback device |
| `audio_engine` | cpal capture/playback, resampling, VAD, endpointing | transcript text, LLM |
| `scene3d` | local 3D engine (stub): contract for scene/mesh generation | LLM, TTS |
| `ui` | windows, sidebar, per-motor settings pages, chat rendering, notifications | any engine internals |
| root `src/` | composition: config load, window creation, controller spawn | feature logic |

Dependency direction:

```
ui ──► shared ◄── llm_engine ◄── controller
 │                 stt              ▲
 │                 tts              │
 └─── main ──────► audio_engine     └── scene3d (stub)
```

## The UI ⇄ engine contract

All cross-boundary interaction goes through two enums in
`crates/shared/src/event.rs`.

```rust
// UI → controller
EngineCommand::StartLlm
EngineCommand::StopLlm
EngineCommand::RestartLlm
EngineCommand::ReloadConfig(Box<VoxConfig>)
EngineCommand::SendMessage { generation: u64, prompt: String }
EngineCommand::Cancel { generation: u64 }
EngineCommand::Transcribe { generation: u64, samples: Arc<[f32]> }
EngineCommand::Speak { generation: u64, text: String }
EngineCommand::SetListening(bool)
EngineCommand::Shutdown
```

```rust
// controller → UI
EngineEvent::Llm { state: LlmState, detail: String }
EngineEvent::Token { generation: u64, delta: String }
EngineEvent::ReplyDone { generation: u64, text: String }
EngineEvent::Transcript { generation: u64, text: String }
EngineEvent::SpeechLevel { level: f32 }
EngineEvent::Speaking(bool)
EngineEvent::Error { scope: &'static str, message: String }
```

Rules:

- Include `generation` in any event that can arrive late. The UI ignores
  events whose generation is not the current one.
- Commands own their payload (`String`, `Arc<[f32]>`), never a reference into
  UI state.
- `EngineEvent` is `Clone + Send + 'static`; never put GPUI types in it.
- Adding a command requires: handler in `crates/controller/src/lib.rs`, match arm in
  `ui`, and a test for the pure part if any.

## Threading model

```
GPUI main thread ──(crossbeam Sender<EngineCommand>)──► controller thread
       ▲                                                     │
       └──(crossbeam Receiver<EngineEvent>, poll 50 ms)◄─────┘
                                                             │ tokio runtime
                                                             ├─ llama-server child
                                                             └─ SSE / health tasks
```

- Never call `blocking_recv`, `std::thread::sleep`, or `reqwest::blocking`
  on the GPUI thread.
- Never hold a `Mutex` across `.await`.
- Audio callbacks only push samples into a bounded channel; processing happens
  in the consumer.
- A GPUI task handle stored in a view is cancelled when dropped; keep it in a
  field (`_pump: Task<()>`).

## Adding a feature: procedure

1. **Write the contract first.** Add command/event variants in `shared`.
2. **Implement the engine method** in its crate, with a pure helper for the
   testable part (argument builder, parser, state machine).
3. **Add a unit test** for the pure helper. `cargo test -p <crate>`.
4. **Handle the command** in `crates/controller/src/lib.rs`. Emit progress and terminal
   events. On error, emit `EngineEvent::Error` and reset state.
5. **Render the state in the UI** from `EngineEvent`s; do not call the engine
   directly.
6. **Add config** in `shared::config` with `#[serde(default)]` and a sensible
   default; expose it in `ui::settings`.
7. **Update docs** if behavior or a public API changed.
8. Run `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.

## Error strategy

| Layer | Type | Behavior |
| --- | --- | --- |
| Pure helpers | `thiserror` enum per crate | return `Err`, never panic |
| Engine public API | crate error enum | include context (path, exit code, stderr tail) |
| Controller | `EngineEvent::Error { scope, message }` | reset state, keep app alive |
| UI | notification / status bar | human text, no backtrace |
| `main` | `expect` only for unrecoverable bootstrap | log and exit |

Use `#![deny(missing_docs)]` only if the crate already documents everything;
otherwise document public items with `///` and keep `//` for *why*.

## Config changes

- Add a field to the relevant struct in `crates/shared/src/config.rs`.
- Give it `#[serde(default = "...")]` if the default is not `Default::default()`.
- Never reorder TOML keys in a way that breaks user files; serde matches by name.
- The settings UI binds to `GlobalConfig` and calls `VoxConfig::save`.
- If the change affects a running process, send `ReloadConfig` and restart.

## Anti-patterns

- ❌ `ui` importing `llm_engine::LlamaManager` "just for the status enum".
  Use `shared::LlmState`.
- ❌ Storing `Child` or `reqwest::Client` in a GPUI `Entity`.
- ❌ Blocking the render thread with `Command::output()`.
- ❌ A `Mutex<Vec<Message>>` shared between UI and controller.
- ❌ Spawning a new `llama-server` per message.
- ❌ Adding a `tokio::runtime::Runtime` per request.
- ❌ `unwrap()` on model paths or health checks.
- ❌ Opening a file dialog with `rfd` (or any blocking modal) from a GPUI
  callback: it re-enters the app and aborts on Windows. Use
  `ui::dialog::pick_paths{,_for}`, which awaits GPUI's `prompt_for_paths`.

## Verification checklist

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Then manually verify:

- [ ] Engine failure shows an error, app stays usable.
- [ ] Closing the window leaves no `llama-server`/`whisper-cli` process.
- [ ] Changing a setting persists after restart.
- [ ] Every new `EngineEvent` is drained by the UI poller (no unbounded queue).

## References

- [`references/crate-contracts.md`](references/crate-contracts.md) — exact
  signatures and ownership of each crate.
- [`references/config-and-events.md`](references/config-and-events.md) — full
  config schema and event lifecycle.
- `docs/ARCHITECTURE.md` — rationale and ADRs.

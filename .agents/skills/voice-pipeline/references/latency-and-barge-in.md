# Latency and barge-in

## Instrumentation

Add spans around each stage:

```rust
let t0 = Instant::now();
let transcript = engine.transcribe(&audio)?;
tracing::info!(elapsed_ms = t0.elapsed().as_millis(), "stt");

let t1 = Instant::now();
let first = stream.next().await;
tracing::info!(ttfb_ms = t1.elapsed().as_millis(), "llm first token");

let t2 = Instant::now();
let wav = tts.synthesize(&sentence)?;
tracing::info!(tts_ms = t2.elapsed().as_millis(), "tts");
```

Log the four numbers per turn. Optimize only the dominant stage.

Typical budget for a 5-second Spanish utterance:

| Stage | p50 | p95 | Lever |
| --- | --- | --- | --- |
| VAD close | 650 ms | 900 ms | `min_silence_ms` |
| STT | 700 ms | 1400 ms | model size, Vulkan, threads |
| LLM first sentence | 700 ms | 1600 ms | model, ctx, prompt |
| TTS first sentence | 450 ms | 900 ms | int8, sentence length |
| Total to audio | ~2.5 s | ~4.8 s | |

## State machine

```
Idle
  └─(mic on)──────────────► Listening
Listening
  ├─(SpeechEnd)───────────► Thinking
  └─(mic off)─────────────► Idle
Thinking
  ├─(first sentence)──────► Speaking   (overlap: still generating)
  ├─(cancel)──────────────► Idle
  └─(error)───────────────► Idle
Speaking
  ├─(generation done + queue empty)──► Listening
  ├─(barge-in validated)──► Listening (abort LLM + clear queue)
  └─(mic off)─────────────► Idle
```

Rules:

- `Thinking` and `Speaking` can overlap; the LLM stream and the audio queue are
  independent.
- Barge-in aborts **both**: cancel the LLM request (generation bump) and clear
  the playback queue.
- After barge-in, discard the partial assistant message or mark it truncated.

## Barge-in validation

```rust
struct BargeIn {
    voiced_ms: u32,
    armed_at: Instant,
}

fn should_interrupt(&mut self, frame_ms: u32, voice: bool, speaking: bool) -> bool {
    if !speaking { return false; }
    if self.armed_at.elapsed() < Duration::from_millis(200) { return false; }
    self.voiced_ms = if voice { self.voiced_ms + frame_ms } else { 0 };
    self.voiced_ms >= 300
}
```

Without echo cancellation, this will still trigger on loud speakers. Add a
config flag `full_duplex` (default `false`) and warn when the output device is
not headphones.

## Audio queue

```rust
struct AudioQueue {
    samples: VecDeque<f32>,
    rate: u32,
    playing: bool,
}
```

- `push(buffer)`: resample to device rate, extend, set `playing = true`.
- `stop()`: clear, `playing = false`, optionally flush the device with silence.
- `is_empty()`: when true and the LLM is done, transition to `Listening`.
- Bound the queue (e.g., 60 s of audio) to avoid unbounded memory if TTS runs
  ahead of playback.

## Sentence splitter

```rust
pub fn split_sentences(buffer: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, ch) in buffer.char_indices() {
        let boundary = matches!(ch, '.' | '!' | '?' | '…' | ';' | ':');
        let next_is_space = buffer[i + ch.len_utf8()..].starts_with(char::is_whitespace);
        if boundary && next_is_space && i + 1 - start >= 24 {
            out.push(buffer[start..=i].trim().to_string());
            start = i + ch.len_utf8();
        }
    }
    if start > 0 { buffer.drain(..start); }
    out.retain(|s| !s.is_empty());
    out
}
```

Also flush at ~160 chars at a word boundary if no punctuation appears, so a
long monologue still starts speaking early.

## Common latency mistakes

- Waiting for the full LLM response before starting TTS.
- Synthesizing the whole reply in one call.
- Restarting whisper per 20 ms frame (batch the utterance).
- Re-encoding the WAV on every chunk instead of once per utterance.
- Loading the STT model per request (whisper-cli loads per process; keep the
  process model in mind — batch utterances, do not spawn per word).
- Polling health too aggressively and competing with inference.
- Blocking the UI while waiting for audio (use events).

## Measuring without a human

- Use a prerecorded 5 s WAV as the "microphone" (an `AudioInput` that streams
  the file at real time).
- Log the four spans; run 10 iterations; report p50/p95.
- Keep the model warm between iterations; the first run includes load time.

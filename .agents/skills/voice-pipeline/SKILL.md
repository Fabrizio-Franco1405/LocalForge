---
name: voice-pipeline
description: >
  Use when implementing or tuning the spoken conversation loop in VoxLocal:
  microphone capture, resampling to 16 kHz, WebRTC/Silero VAD, endpointing,
  whisper.cpp transcription, streaming LLM replies into Kokoro TTS, playback,
  barge-in, latency budgets, and echo/feedback problems.
license: MIT
compatibility: cpal 0.18, whisper.cpp 1.9+, Kokoro ONNX v1.0
metadata:
  author: voxlocal
  version: "0.1.0"
---

# Voice pipeline

## When to use this skill

Anything from "the mic does not work" to "the assistant interrupts itself",
including VAD thresholds, turn-taking, chunking text for TTS, playback queues,
and end-to-end latency.

## Canonical flow

```
capture (device rate, N channels)
  → downmix to mono
  → resample to 16_000 Hz
  → EndpointDetector (20 ms frames, WebRTC VAD)
      ├─ SpeechStart → UI "escuchando"
      └─ SpeechEnd(samples) → STT
  → WhisperCli::transcribe(samples) → text
  → LLM streaming (sentence-level chunks)
  → KokoroCli::synthesize(sentence) → AudioBuffer 24 kHz
  → AudioPlayer (resample to device rate) → speakers
```

## Non-negotiable formats

| Stage | Format |
| --- | --- |
| VAD / STT input | 16 kHz, mono, f32 `[-1, 1]` |
| WAV on disk for whisper | PCM16, mono, 16 kHz |
| Kokoro output | 24 kHz, mono, f32 |
| cpal capture | device native (often 48 kHz, 2 ch) |
| cpal playback | device native |

Convert at the boundaries, never inside the VAD or the STT encoder.

## Endpointing rules that work

- Frame: **20 ms** (320 samples at 16 kHz).
- Start speech after **250 ms** of voiced frames (WebRTC VAD already smooths).
- End speech after **600–800 ms** of silence (Spanish conversation; shorter
  feels rude, longer feels laggy).
- Keep a **pre-roll of 250–350 ms** before the first voiced frame.
- Keep **150–250 ms** of trailing audio after the last voiced frame, so
  fricatives are not cut.
- Cap an utterance at **30 s**; longer means the VAD is stuck on noise.
- If the user speaks while TTS is playing: barge-in (stop playback and
  generation) only after **300 ms** of voiced audio, to ignore coughs/echo.

## VAD choice

| Option | Cost | Quality | When |
| --- | --- | --- | --- |
| WebRTC VAD (`webrtc-vad`) | ~0 | Good for close mic | **Default** |
| Silero VAD ONNX (`silero` crate) | ~10–20 ms/frame CPU | Better in noise | Optional feature |
| Energy threshold | ~0 | Poor | Never in production |

WebRTC modes: `LowBitrate` (aggressive, drops quiet speech),
`Aggressive`, `Quality` (default). Use `Quality` for a desktop mic; use
`Aggressive` in noisy rooms only after testing.

## TTS chunking

Do not synthesize the whole reply at once. Split into sentences/clauses as the
LLM streams:

- Flush at `. ! ? … ; :` followed by whitespace, or at ~160 characters.
- Keep the delimiter (it carries prosody).
- Minimum chunk: 20–30 characters (avoid one-word clips).
- Clean Markdown/emojis with `shared::text::for_speech` before synthesis.
- Speak chunk N while the LLM generates chunk N+1.
- On barge-in, drop the queue and abort the LLM stream.

## Barge-in and echo

The classic failure: the mic hears the assistant's own TTS and interrupts it.

Mitigations, in order:

1. **Half-duplex by default:** stop capturing (or ignore VAD) while speaking;
   enable full-duplex only with headphones.
2. **Echo cancellation:** not provided by cpal; use PipeWire/PulseAudio echo
   cancel or require headphones.
3. **Guard interval:** ignore VAD for the first 200 ms of playback.
4. **Threshold boost:** raise the VAD threshold during playback.
5. **Device hint:** detect the default output as speakers vs headset and warn.

Document the limitation; do not pretend to have AEC.

## Latency budget (target: < 4 s to first audio)

| Stage | Budget |
| --- | --- |
| End-of-speech detection | 0.6–0.8 s |
| STT (5 s audio, small, Vulkan) | 0.4–1.2 s |
| LLM first sentence | 0.5–1.5 s |
| TTS first sentence (CPU) | 0.3–0.9 s |
| Playback start | < 0.1 s |
| **Total** | **2.5–4.5 s** |

Measure each stage with `tracing` spans; optimize the largest one first.
Reducing silence timeout by 200 ms is often cheaper than a faster model.

## Audio thread rules

- The cpal callback must not allocate, lock, log, or block.
- Downmix/resample in the callback only with preallocated buffers; otherwise
  send raw frames to a worker.
- Use a bounded channel; drop oldest audio if the consumer falls behind
  (never grow without limit).
- Never open a second input stream while one is alive; ALSA/PipeWire may fail.

## Error handling

| Failure | Behavior |
| --- | --- |
| No input device | disable mic button, show "sin micrófono" |
| Device busy | retry once, then error |
| Model missing | error before spawning whisper |
| whisper exits non-zero | surface stderr tail |
| Empty transcript | go back to listening, no LLM call |
| TTS worker fails | show text reply, disable audio for the turn |
| Output device disappears | stop playback, keep conversation |

## Testing without hardware

- `EndpointDetector` is generic over `VoiceActivity`; feed a mock that returns
  voice for N frames and assert the emitted `SpeechEnd` length.
- `LinearResampler` is pure: assert length and a sine wave's frequency.
- STT parser is pure: fixture strings, no binary.
- TTS: test the sanitizer and the command builder; mock the worker with a
  script that writes a known WAV.
- cpal device tests are manual, on the target machine.

## References

- [`references/audio-io.md`](references/audio-io.md) — cpal 0.18 capture and
  playback with resampling and format conversion.
- [`references/tts-kokoro.md`](references/tts-kokoro.md) — Kokoro worker,
  voices, and the Python/ONNX setup.
- [`references/latency-and-barge-in.md`](references/latency-and-barge-in.md) —
  measurements, queue design, and interruption state machine.

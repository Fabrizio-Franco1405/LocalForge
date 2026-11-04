# Kokoro TTS (ONNX, CPU)

## Why a worker process

Kokoro is an 82M-parameter ONNX model. In v0.1 it runs through
`tools/kokoro_worker.py` with `kokoro-onnx`, because:

- The Rust crates `tts-rs` and `kokoro-tts` do not compile against
  `ort 2.0.0-rc.15` (API break in `Session::builder`).
- Reimplementing Kokoro's tokenizer/phonemizer in Rust is a separate project.
- ONNX Runtime CPU is fast enough for sentence-level synthesis and keeps VRAM
  free for the LLM.

No PyTorch, no CUDA: `kokoro-onnx` uses `onnxruntime` + `numpy`.

## Files

```text
assets/models/kokoro/
├── kokoro-v1.0.int8.onnx   (88 MB, recommended)
├── kokoro-v1.0.fp16.onnx   (169 MB)
├── kokoro-v1.0.onnx        (310 MB)
└── voices-v1.0.bin         (required for all)
```

Download from `thewh1teagle/kokoro-onnx` or `taylorchu/kokoro-onnx` releases
(`model-files-v1.0`).

## Setup

```bash
python3 -m venv .venv-kokoro
.venv-kokoro/bin/pip install --upgrade pip
.venv-kokoro/bin/pip install kokoro-onnx

BASE=https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0
mkdir -p assets/models/kokoro
curl -L -o assets/models/kokoro/kokoro-v1.0.int8.onnx "$BASE/kokoro-v1.0.int8.onnx"
curl -L -o assets/models/kokoro/voices-v1.0.bin "$BASE/voices-v1.0.bin"
```

`tools/setup_kokoro.sh` automates this and verifies the files.

## Worker contract

```bash
.venv-kokoro/bin/python tools/kokoro_worker.py \
  --model-dir assets/models/kokoro \
  --voice ef_dora \
  --speed 1.0 \
  --out /tmp/out.wav
# text is read from stdin
```

- Writes a PCM16 WAV at the model's native 24 kHz.
- Prints one JSON line to stdout: `{"ok":true,"samples":123456,"sample_rate":24000}`.
- On error: non-zero exit + human message on stderr.
- Never accepts text in argv (avoids length limits and shell quoting).

## Voice mapping

Kokoro encodes language and gender in the voice prefix:

| Prefix | Language | Example |
| --- | --- | --- |
| `af` / `am` | English (US) | `af_heart`, `am_michael` |
| `bf` / `bm` | English (UK) | `bf_emma` |
| `ef` / `em` | Spanish | `ef_dora`, `em_alex`, `em_santa` |
| `ff` | French | `ff_siwis` |
| `hf` / `hm` | Hindi | `hf_alpha` |
| `if` / `im` | Italian | `if_sara` |
| `jf` / `jm` | Japanese | `jf_alpha` |
| `pf` / `pm` | Portuguese (BR) | `pf_dora` |
| `zf` / `zm` | Chinese | `zf_xiaobei` |

The worker infers `lang` from the prefix; `--lang` can override it.

## Text preparation

`shared::text::for_speech` must run before synthesis:

- Remove fenced code blocks and inline code.
- Convert Markdown links `[text](url)` → `text`.
- Strip `#`, `*`, `_`, backticks, list markers.
- Remove emojis and zero-width joiners.
- Keep punctuation (`. , ? ! ; :`) for prosody.
- Collapse whitespace; cap to a sane length per chunk.

Kokoro mispronounces numbers, URLs, and abbreviations. Normalize common cases
(`3.5` → "tres coma cinco" for Spanish) or accept the imperfection in v1.

## Streaming playback

1. LLM streams tokens.
2. Accumulate until a sentence boundary or ~160 chars.
3. Sanitize the chunk.
4. Synthesize (0.3–0.9 s for a short sentence).
5. Enqueue the `AudioBuffer` in `AudioPlayer`.
6. Repeat while the LLM generates the next sentence.

The player resamples 24 kHz → device rate. Overlap synthesis with playback;
do not wait for the whole reply.

## Failure modes

| Symptom | Cause | Fix |
| --- | --- | --- |
| `ModuleNotFoundError: kokoro_onnx` | wrong venv | point `tts.python` to `.venv-kokoro/bin/python` |
| `voices-v1.0.bin not found` | wrong `model_dir` | check filenames |
| Robotic/clipped audio | int8 model + high speed | use fp16, speed ≤ 1.1 |
| Long silence at start | sentence too short / model warmup | pre-warm with a short phrase at startup |
| Wrong language | voice prefix mismatch | choose an `ef_`/`em_` voice |
| Worker hangs | model download/corrupt | validate file size, run manually |

## Future: native Rust backend

When `ort` + a Kokoro crate compile together:

- Implement `Synthesizer` with `ort` directly.
- Use `espeak-ng` (pure Rust port available) for phonemes.
- Keep the same `AudioBuffer` contract so the controller does not change.
- Do not add `ort/cuda` or `ort/rocm` features.

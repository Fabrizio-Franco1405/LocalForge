---
name: vulkan-ai-stack
description: >
  Use when building, configuring, debugging, or choosing local inference
  backends for LocalForge: llama.cpp/llama-server, whisper.cpp/whisper-cli,
  Vulkan on RDNA 1 (RX 5700 XT), VRAM budgeting for 8 GB, model selection, or
  diagnosing "runs on CPU", OOM, and driver problems. Enforces the no-CUDA /
  no-ROCm / no-PyTorch constraint.
license: MIT
compatibility: Linux (Mesa RADV), Windows (AMD/NVIDIA/Intel Vulkan), macOS via MoltenVK
metadata:
  author: localforge
  version: "0.1.0"
---

# Vulkan AI stack

## When to use this skill

Any task that involves compiling or invoking llama.cpp / whisper.cpp, picking a
model that fits 8 GB of VRAM, interpreting `llama-server` logs, or changing GPU
offload. Also load it when a dependency suggestion would pull CUDA, ROCm,
PyTorch, TensorRT or vLLM.

## Non-negotiables

- Backend GPU: **Vulkan only**. Never add `-DGGML_CUDA`, `-DGGML_HIP`,
  `-DGGML_SYCL`, `-DGGML_VULKAN=OFF`, or CUDA-only crates.
- No PyTorch (`torch`, `transformers`, `candle` with CUDA, `tch`).
- ONNX models run on **CPU** with ONNX Runtime; there is no supported Vulkan EP.
- Keep `llama-server` bound to `127.0.0.1`.

## Decision table

| Need | Use | Why |
| --- | --- | --- |
| Chat LLM | `llama-server` + GGUF, `-ngl` | Vulkan stable on RDNA 1 |
| STT | `whisper-cli` + `ggml-*.bin` | Official Vulkan build, simple CLI |
| VAD | WebRTC VAD (Rust) | No model, negligible CPU |
| TTS | Kokoro ONNX on CPU | Preserves VRAM for LLM |
| Embeddings | `llama-server --embeddings` | Same backend, no extra runtime |
| Vision | `llama-server` + mmproj (experimental) | Optional, not v1 |

## Build recipes

### llama.cpp

```bash
cmake -S llama.cpp -B llama.cpp/build \
  -DGGML_VULKAN=ON \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_CURL=OFF
cmake --build llama.cpp/build -j"$(nproc)" --target llama-server
```

Verify:

```bash
llama-server --version
llama-server --list-devices
# Vulkan0: AMD Radeon RX 5700 XT (RADV NAVI10) (8176 MiB, ...)
```

### whisper.cpp

```bash
cmake -S whisper.cpp -B whisper.cpp/build \
  -DGGML_VULKAN=1 \
  -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j"$(nproc)" --target whisper-cli
```

### Windows

Use the MSVC toolchain, install the Vulkan SDK (or at least the loader and
driver), and build from a "x64 Native Tools" prompt. Do not use the GNU
toolchain for GPUI Kit either.

### macOS

Vulkan is provided by MoltenVK; ggml Vulkan works but is slower than Metal.
For macOS, building llama.cpp with Metal is acceptable (it is not CUDA/ROCm),
but LocalForge's supported default remains Vulkan.

## Serving flags used by LocalForge

```text
--model <gguf>          -m
--host 127.0.0.1
--port 9931
--ctx-size 4096         -c
--gpu-layers 99         -ngl (or 24..28 for 8B on 8 GB)
--threads 8             -t (leave 2–4 cores for audio/UI)
--jinja                 respect the model chat template
--no-webui              disable the embedded UI
```

Health:

- `GET /health` → `503 {"error":{"message":"Loading model"}}` while loading.
- `GET /health` → `200 {"status":"ok"}` when ready.
- `/v1/health` is equivalent.
- Chat: `POST /v1/chat/completions` (OpenAI-compatible, supports `stream`).

## VRAM budgeting (8 GB)

```text
budget = 8192 MiB
  - desktop/compositor        ~700 MiB
  - whisper small (peak)      ~700 MiB
  - KV cache (4096 ctx, f16)  ~900 MiB
  - safety margin             ~400 MiB
  = available for weights    ~5.4 GiB
```

- 4B Q4_K_M (~2.5 GB): comfortable with 8k ctx.
- 8B Q4_K_M (~5.0 GB): use 2k–4k ctx and `-ngl 24..28`, or Q3/IQ3.
- Never run llama-server and whisper simultaneously at full offload on 8 GB if
  the model is 8B.
- Model + KV must fit together; `-ngl` offloads layers, not KV per se.

## Model selection rules

- Prefer **Q4_K_M**; it is the quality/size sweet spot for ggml Vulkan.
- Avoid Q8/f16 on 8 GB unless the model is ≤3B.
- Prefer instruction-tuned models with a chat template (`--jinja`).
- For Spanish, Qwen3-4B/8B, Gemma-3-4B, Llama-3.1-8B are good defaults.
- Whisper: `small` is the best quality/latency tradeoff; `base` for slow GPUs.

## Diagnosis workflow

1. `vulkaninfo --summary` — is the discrete GPU present and using RADV?
2. `llama-server --list-devices` — does llama.cpp see Vulkan0?
3. Start manually with `-ngl 99 -c 4096` and read the load log.
4. If tokens/s is CPU-like (< 3 tok/s for 4B), the binary lacks Vulkan or the
   model did not offload. Check for `ggml_vulkan: Found ... devices` and
   `offloaded N/N layers to GPU`.
5. On OOM, lower `-ngl` first, then `-c`, then quantization.
6. Compare with `radeontop` / `nvtop` to confirm GPU utilization.

## Common failure messages

| Log | Meaning | Fix |
| --- | --- | --- |
| `no usable Vulkan devices` | driver/ICD missing | install `mesa-vulkan-drivers`; check `VK_ICD_FILENAMES` |
| `failed to allocate Vulkan buffer` | VRAM exhaustion | lower `-ngl`/`-c` |
| `unknown model architecture` | llama.cpp too old | update build for the GGUF |
| `failed to load model` + mmap | file truncated / wrong path | re-download, check SHA |
| whisper `error: failed to initialize whisper context` | wrong/corrupt model | use `ggml-*.bin`, not a HF safetensors |
| whisper output `[BLANK_AUDIO]` | silence or bad VAD | adjust VAD; filter markers |

## Anti-patterns

- ❌ `pip install torch` "just for preprocessing".
- ❌ Adding `llama-cpp-rs` with default CUDA features.
- ❌ Shipping one binary per GPU vendor in v1.
- ❌ Running whisper and LLM at `-ngl 99` simultaneously on 8 GB.
- ❌ Parsing human-readable logs for health; use `/health`.

## References

- [`references/llama-server.md`](references/llama-server.md) — full flag and
  API reference for the version targeted by LocalForge.
- [`references/vram-and-models.md`](references/vram-and-models.md) — sizing
  tables and measured expectations.
- [`references/troubleshooting.md`](references/troubleshooting.md) — step-by-step
  GPU/audio diagnosis.
- `docs/HARDWARE-VULKAN.md`, `docs/MODELS.md`.

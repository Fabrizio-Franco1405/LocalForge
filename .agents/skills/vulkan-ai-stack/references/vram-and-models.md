# VRAM and model sizing

## Target machine

- GPU: RX 5700 XT, 8 GB GDDR6, RDNA 1 (gfx1010), RADV.
- iGPU: Vega (Renoir) shares system RAM; not used for inference by default.
- CPU: Ryzen 7 5700G, 8C/16T, AVX2.
- RAM: 32 GB DDR4-3200.

`llama-server --list-devices` reports both Vulkan devices. Use the discrete GPU
and, if needed, `--tensor-split` or the device selection flag accepted by the
build to avoid the iGPU.

## Measured expectations (order of magnitude)

| Workload | Hardware | Expectation |
| --- | --- | --- |
| Qwen3-4B Q4_K_M, 4k ctx | 5700 XT Vulkan | 25–45 tok/s generation |
| Llama-3.1-8B Q4_K_M, 2k ctx | 5700 XT Vulkan | 12–22 tok/s |
| Qwen3-4B Q4_K_M | CPU only (8 threads) | 3–6 tok/s |
| whisper small, 5 s audio | 5700 XT Vulkan | 0.4–1.2 s |
| whisper small, 5 s audio | CPU only | 1.5–4 s |
| Kokoro int8, 1 sentence | 5700G CPU | 0.3–0.9 s |

Numbers vary with driver, Mesa version, thermals, and desktop load. Always
measure on the target machine; do not tune from internet benchmarks.

## Sizing formula

```text
weights_gib  ≈ params_b × bits_per_weight / 8
             (Q4_K_M ≈ 4.8 bits/weight → 0.6 GiB per 1B params)
kv_cache_gib ≈ 2 × layers × ctx × hidden × bytes_per_elem
             (roughly 0.2–0.3 GiB per 1k ctx for a 7–8B model at f16)
```

For an 8B model at 4k f16 KV: ~5.0 GiB weights + ~1.0 GiB KV = 6.0 GiB, leaving
~2 GiB for desktop + whisper peak. That is tight but workable if whisper is
not running concurrently at full offload.

## Mitigations when it does not fit

In order of least quality loss:

1. Reduce `-c` (4096 → 2048).
2. Reduce `-ngl` (offload fewer layers; CPU handles the rest).
3. Quantize KV cache: `--cache-type-k q8_0 --cache-type-v q8_0`.
4. Use a smaller quantization (Q4_K_M → IQ4_XS → Q3_K_M).
5. Use a smaller model (8B → 4B → 3B).
6. Close the browser/compositor effects.

## Concurrency policy

LocalForge serializes heavy inference:

- While the LLM generates, whisper is idle (unless VAD detected a barge-in).
- TTS runs on CPU, overlapped with generation only for completed sentences.
- Only one whisper process at a time; requests are queued in the controller.
- `--parallel 1` on llama-server; a voice assistant has one user.

## Checking actual usage

```bash
# AMD
radeontop -d /dev/dri/renderD128

# NVIDIA
nvidia-smi dmon

# Generic
watch -n0.5 'vulkaninfo --summary | grep -A3 GPU0'
```

Also watch `MemAvailable`, swap, and CPU per core (`htop`). If the model is
swapping, generation collapses even if VRAM looks fine.

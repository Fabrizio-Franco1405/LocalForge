# Modelos recomendados

Todos los modelos son locales y en formato abierto (GGUF / ONNX / bin de
whisper.cpp). No se descarga nada automáticamente en v0.1.

## LLM (GGUF, Q4_K_M salvo nota)

| Modelo | Tamaño aprox. | VRAM con 4k ctx | Uso |
| --- | --- | --- | --- |
| Qwen3-4B-Instruct | ~2,5 GB | ~3,5 GB | Español correcto, rápido; **recomendado por defecto** |
| Llama-3.2-3B-Instruct | ~2,0 GB | ~3,0 GB | Muy rápido, respuestas cortas |
| Gemma-3-4B-it | ~2,7 GB | ~3,8 GB | Buena calidad multilingüe |
| Qwen3-8B-Instruct | ~5,0 GB | ~6,5 GB | Mejor razonamiento; contexto 2k si hay apuros |
| Mistral-7B-Instruct v0.3 | ~4,4 GB | ~5,8 GB | Alternativa clásica |
| Llama-3.1-8B-Instruct | ~4,9 GB | ~6,5 GB | Buena calidad, más lento |

**Evita** modelos > 8B en Q4 para 8 GB de VRAM, y evita `Q8`/`f16` salvo
pruebas de calidad.

Comandos de ejemplo:

```bash
# Qwen3 4B (elige el repo vigente en Hugging Face)
huggingface-cli download Qwen/Qwen3-4B-Instruct-2507-GGUF \
  --include '*Q4_K_M.gguf' --local-dir assets/models/llm

# Si no usas huggingface-cli:
curl -L -o assets/models/llm/qwen3-4b-q4_k_m.gguf '<URL del GGUF>'
```

## STT (whisper.cpp)

| Modelo | Tamaño | Velocidad | Calidad español |
| --- | --- | --- | --- |
| `ggml-base.bin` | 142 MB | Muy rápida | Aceptable |
| `ggml-small.bin` | 466 MB | Rápida | **Recomendado** |
| `ggml-medium.bin` | 1,5 GB | Lenta en Vulkan 8 GB | Buena, pero compite por VRAM |

```bash
curl -L -o assets/models/whisper/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

Para español, `-l es`. Si el modelo es `.en`, no lo uses para español.

## TTS (Kokoro v1.0 ONNX)

Archivos necesarios en `assets/models/kokoro/`:

| Archivo | Tamaño | Nota |
| --- | --- | --- |
| `kokoro-v1.0.int8.onnx` | ~88 MB | **Recomendado**: rápido en CPU |
| `kokoro-v1.0.fp16.onnx` | ~169 MB | Más calidad, más lento |
| `kokoro-v1.0.onnx` | ~310 MB | Referencia f32 |
| `voices-v1.0.bin` | ~27 MB | Obligatorio para todas las variantes |

Descarga (release de `thewh1teagle/kokoro-onnx` o `taylorchu/kokoro-onnx`):

```bash
BASE=https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0
curl -L -o assets/models/kokoro/kokoro-v1.0.int8.onnx "$BASE/kokoro-v1.0.int8.onnx"
curl -L -o assets/models/kokoro/voices-v1.0.bin       "$BASE/voices-v1.0.bin"
```

Voces útiles en español: `ef_dora`, `em_alex`, `em_santa`.
Voces inglesas: `af_heart`, `af_bella`, `am_michael`.
Lista completa: <https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md>

## VAD

El VAD por defecto es WebRTC (incluido en el binario, sin modelo). Si más
adelante se usa Silero, el modelo es `silero_vad.onnx` (~2 MB) y corre en CPU
con ONNX Runtime.

## Estructura de `assets/`

```text
assets/
├── bin/            # binarios locales opcionales (llama-server, whisper-cli)
├── models/
│   ├── llm/*.gguf
│   ├── whisper/ggml-small.bin
│   └── kokoro/{kokoro-v1.0.int8.onnx,voices-v1.0.bin}
└── scripts/        # utilidades
```

`assets/` no se versiona con modelos grandes: usa `.gitignore` y documenta las
descargas.

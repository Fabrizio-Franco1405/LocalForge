# Hardware y Vulkan

## 1. La restricción que define el proyecto

RDNA 1 (gfx1010, RX 5700 XT) **no** tiene soporte oficial estable en ROCm y no
tiene CUDA. El único camino GPU que funciona hoy sin parches de driver es
**Vulkan** (Mesa RADV en Linux).

Esto implica:

- Nada de `-DGGML_CUDA=ON`, `-DGGML_HIP=ON`, `-DGGML_SYCL=ON` ni TensorRT.
- Nada de `torch`, `transformers`, `bitsandbytes`, `vLLM` ni `exllama`.
- Los binarios se compilan con `-DGGML_VULKAN=ON` / `-DGGML_VULKAN=1`.
- ONNX Runtime para VAD/TTS corre en **CPU** (no existe un EP Vulkan maduro y
  estable para ONNX Runtime).

## 2. Verificación del entorno

```bash
# Instancia y dispositivos Vulkan
vulkaninfo --summary

# Debe mostrar algo como:
# GPU0:
#   deviceName  = AMD Radeon RX 5700 XT (RADV NAVI10)
#   driverName  = radv
#   apiVersion  = 1.4.x

# llama.cpp debe enumerar la GPU
llama-server --list-devices

# whisper.cpp debe cargar el modelo sin fallback silencioso a CPU
whisper-cli -m ggml-small.bin -f muestra.wav -l es -pp
```

En Windows el driver AMD moderno incluye Vulkan; en macOS, MoltenVK traduce a
Metal y el rendimiento es menor, pero funciona.

Checklist manual paso a paso (local, no versionado): `test/docs/hardware.md`.

## 3. Presupuesto de VRAM (8 GB)

| Uso | VRAM aproximada |
| --- | --- |
| Escritorio / compositor | 0,5–1,0 GB |
| Whisper `small` en Vulkan | 0,5–1,0 GB (pico) |
| LLM Q4_K_M 4B | ~2,8 GB (modelo) + KV cache |
| LLM Q4_K_M 8B | ~5,2 GB (modelo) + KV cache |
| KV cache 4k ctx (8B, f16) | ~0,8–1,2 GB |
| Margen recomendado | 0,5 GB |

**Regla práctica:** para 8 GB, un 4B Q4_K_M con 4k de contexto es cómodo; un 8B
Q4_K_M exige contexto 2k o cuantización Q3/IQ3 y cerrar el navegador. Whisper
compite por VRAM cuando se ejecuta; usa `small` como máximo.

`-ngl` (capas en GPU):

- `-ngl 99` o `-ngl auto`: intenta todo. Si el driver no cabe, baja.
- `-ngl 24` / `-ngl 28`: híbrido para 8B en 8 GB.
- `-ngl 0`: diagnóstico CPU.

## 4. Compilación de llama.cpp con Vulkan

```bash
cmake -S llama.cpp -B llama.cpp/build \
  -DGGML_VULKAN=ON \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_CURL=OFF
cmake --build llama.cpp/build -j"$(nproc)" --target llama-server
```

Notas:

- `-DLLAMA_CURL=OFF` evita depender de libcurl si los modelos son locales.
- En Linux, `VK_ICD_FILENAMES` solo es necesario si hay varias ICDs; no lo
  fijes a ciegas.
- Comprueba la build con `llama-server --version` y `--list-devices`.

Flags de servicio relevantes (v0.8+):

| Flag | Uso en VoxLocal |
| --- | --- |
| `-m, --model` | GGUF local |
| `-c, --ctx-size` | 4096 por defecto; subir solo si hay VRAM |
| `-ngl, --gpu-layers` | capas en Vulkan |
| `-t, --threads` | hilos CPU (deja 2–4 libres para audio/UI) |
| `--host` / `--port` | `127.0.0.1:9931` por defecto |
| `--jinja` | respeta la plantilla de chat del GGUF |
| `--no-webui` | desactiva la UI web si no se usa |

`GET /health` devuelve **503** mientras carga y **200** con `{"status":"ok"}`
cuando está listo. Es el único endpoint necesario para el arranque.

## 5. Compilación de whisper.cpp con Vulkan

```bash
cmake -S whisper.cpp -B whisper.cpp/build \
  -DGGML_VULKAN=1 \
  -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j"$(nproc)" --target whisper-cli
```

Invocación:

```bash
whisper-cli -m assets/models/whisper/ggml-small.bin \
  -f turno.wav -l es -t 8 -np -nt
```

- `-np` reduce ruido de logs; `-nt` quita timestamps.
- El WAV debe ser **PCM16, mono, 16 kHz**. `audio_engine` lo garantiza.
- Si el modelo no existe, whisper-cli falla con código ≠ 0; no asumas salida
  vacía como éxito.

## 6. Diagnóstico rápido

| Síntoma | Causa probable | Acción |
| --- | --- | --- |
| `no usable Vulkan device` | driver/ICD ausente | instalar `mesa-vulkan-drivers`, revisar `vulkaninfo` |
| Carga en CPU a 2 tok/s | binario sin Vulkan | recompilar con `-DGGML_VULKAN=ON`; revisar log de devices |
| `VK_ERROR_OUT_OF_DEVICE_MEMORY` | modelo/contexto no cabe | bajar `-ngl`, `-c`, o cuantización |
| Primer token > 30 s | modelo frío / swap | precargar, `--mlock` solo si hay RAM |
| Whisper inventa texto en silencio | VAD mal ajustado | subir `min_silence_ms`, filtrar `[BLANK_AUDIO]` |
| Sonido entrecortado en TTS | competencia CPU/GPU | hilos del LLM a 6–8, TTS por frases |

## 7. Alternativas descartadas

- **ROCm/HIP:** inestable en gfx1010; requiere variables y versiones concretas.
- **Vulkano/ash para inferencia propia:** reimplementar kernels no aporta nada
  frente a ggml Vulkan.
- **ONNX Runtime Vulkan EP:** no existe soporte oficial mantenido.
- **PyTorch + DirectML (Windows):** viola la regla de no PyTorch y no cubre
  Linux.

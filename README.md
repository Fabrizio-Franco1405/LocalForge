# VoxLocal

Asistente de voz local y multiplataforma escrito en Rust. Habla con modelos de lenguaje
que corren **en tu propia máquina**, sin nube, sin PyTorch y sin CUDA.

```
Micrófono ──► VAD ──► whisper.cpp (Vulkan) ──► llama.cpp (Vulkan) ──► Kokoro ONNX (CPU) ──► Altavoces
              │                │                        │                     │
              └── audio_engine └── stt                  └── llm_engine        └── tts
```

## Hardware objetivo

| Componente | Objetivo | Notas |
| --- | --- | --- |
| GPU | AMD RX 5700 XT (RDNA 1, gfx1010, 8 GB) | Vulkan vía Mesa RADV; también NVIDIA/Intel con Vulkan |
| CPU | AMD Ryzen 7 5700G (8C/16T, sin NPU) | AVX2; suficiente para VAD, TTS ONNX y orquestación |
| RAM | 32 GB DDR4 | Presupuesto típico: 12–16 GB para el LLM |
| Backend | **Vulkan** | Único backend soportado de forma nativa en RDNA 1 |

> **Regla de oro:** si una dependencia exige CUDA, ROCm o PyTorch, se descarta. Vulkan o CPU.

## Estado

- [x] Workspace de crates y shell GPUI Kit
- [x] Configuración segmentada y persistente (`shared`)
- [x] Gestión del proceso `llama-server` con health-check y chat en streaming (`llm_engine`)
- [x] Wrapper de `whisper-cli` con WAV 16 kHz y parser de salida (`stt`)
- [x] TTS Kokoro ONNX vía worker aislado + limpieza de texto (`tts`)
- [x] Captura/playback cpal, VAD WebRTC y endpointing (`audio_engine`)
- [x] UI de chat y ajustes (`ui`)
- [ ] Conversación manos libres completa (barge-in, streaming TTS)
- [ ] Empaquetado e instaladores

Consulta [docs/ROADMAP.md](docs/ROADMAP.md) para el plan por hitos.

## Inicio rápido

### 1. Dependencias del sistema

```bash
# Arch / CachyOS
sudo pacman -S --needed base-devel clang cmake vulkan-icd-loader alsa-lib libxkbcommon wayland

# Debian / Ubuntu
sudo apt install -y build-essential clang cmake libasound2-dev libfontconfig-dev \
  libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libvulkan1 mesa-vulkan-drivers
```

Comprueba la GPU:

```bash
vulkaninfo --summary | grep -E 'deviceName|driverName'
```

### 2. Compilar llama.cpp y whisper.cpp con Vulkan

```bash
# llama.cpp
git clone https://github.com/ggml-org/llama.cpp
cmake -S llama.cpp -B llama.cpp/build -DGGML_VULKAN=ON -DCMAKE_BUILD_TYPE=Release
cmake --build llama.cpp/build -j --target llama-server

# whisper.cpp
git clone https://github.com/ggml-org/whisper.cpp
cmake -S whisper.cpp -B whisper.cpp/build -DGGML_VULKAN=1 -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j --target whisper-cli
```

Verifica que llama.cpp ve la GPU:

```bash
llama-server --list-devices
# Vulkan0: AMD Radeon RX 5700 XT (RADV NAVI10) (8176 MiB, ...)
```

### 3. Modelos

```bash
mkdir -p assets/models/{llm,whisper,kokoro}

# LLM: un modelo pequeño/mediano en Q4_K_M para 8 GB de VRAM
# (Qwen3-4B/8B, Llama-3.2-3B, Gemma-3-4B...)
# Whisper: base o small para español
curl -L -o assets/models/whisper/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

### 4. Kokoro (TTS)

```bash
./tools/setup_kokoro.sh
```

Crea un entorno virtual aislado en `.venv-kokoro/` con `kokoro-onnx` (ONNX Runtime, no
PyTorch) y descarga los modelos en `assets/models/kokoro/`.

### 5. Ejecutar

```bash
cargo run
```

La primera vez abre `Ajustes` para indicar las rutas de binarios y modelos. La
configuración se guarda en:

- Linux: `~/.config/voxlocal/config.toml`
- Windows: `%APPDATA%\voxlocal\config.toml`
- macOS: `~/Library/Application Support/voxlocal/config.toml`

## Documentación

| Documento | Contenido |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, flujo de datos, hilos, decisiones de diseño |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Hitos y criterios de aceptación |
| [docs/HARDWARE-VULKAN.md](docs/HARDWARE-VULKAN.md) | RDNA 1, presupuestos de VRAM, builds y diagnóstico |
| [docs/MODELS.md](docs/MODELS.md) | Modelos recomendados y descargas |
| [AGENTS.md](AGENTS.md) | Contexto del proyecto y reglas para agentes |

## Skills para agentes

Las skills locales viven en `.agents/skills/` y son la guía operativa del proyecto:

- `voxlocal-architecture` — límites entre crates y contratos
- `vulkan-ai-stack` — llama.cpp/whisper.cpp en Vulkan, RDNA 1 y VRAM
- `gpui-kit-desktop` — GPUI Kit 0.9: Entity, Settings, tareas y tests
- `voice-pipeline` — VAD, STT, LLM, TTS y latencia
- `process-supervision` — procesos hijos, health-checks y limpieza
- `voxlocal-workflow` — build, modelos, verificación y releases

## Licencia

MIT OR Apache-2.0

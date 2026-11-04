# LocalForge

Harness local de IA para tu máquina. Conecta tus propios motores —LLM, voz y,
próximamente, 3D— sin nube, sin PyTorch y sin CUDA. Tú pones los binarios y los
modelos; LocalForge orquesta procesos, health-checks y configuración.

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

## Secciones

El sidebar agrupa la configuración por motor, aislada entre sí. La conversación
es la vista de inicio (se abre desde la marca o el botón «Conversación»).

| Sección | Contenido |
| --- | --- |
| **Motor LLM** | Local (`llama-server` + GGUF + `-ngl` + host/puerto) o Externo (endpoint, model id, API key). Cada modo muestra solo sus campos |
| **Motor Audio** | Whisper/STT, Kokoro/TTS, micrófono, VAD y playback |
| **Motor 3D** | Reservado; visible como «próximamente», sin backend todavía |
| **Ajustes** | Cambios visuales (tema, color de acento, radio, tipografía), rutas y diagnóstico |

El cambio Local ⇄ Externo reconstruye la página al instante: en Local no verás
endpoints y en Externo no verás rutas de `llama-server`. «Probar conexión»
aplica el borrador y sondea el servidor, así puedes pegar `host:puerto` y
detectarlo sin reiniciar la app. En Local la cabecera ofrece **Encender**,
**Apagar** y **Reiniciar**; el botón de aplicar solo aparece cuando hay cambios.

El pie del sidebar muestra **CPU, RAM del sistema y memoria de la app** cada dos
segundos. El muestreo vive en `crates/controller`, no en la UI.

## Core agnóstico a la UI

`shared`, `llm_engine`, `controller`, `stt`, `tts`, `audio_engine` y `scene3d`
no dependen de GPUI. La UI solo envía `EngineCommand` y pinta `EngineEvent` a
través de `EngineHandle`. Cambiar a JS/TS, Python o una web implica implementar
ese protocolo (o un adaptador IPC sobre el handle), no reescribir los motores.
La suite de pruebas verifica esa frontera.

## Estado

- [x] Workspace modular: `shared`, `llm_engine`, `controller`, `ui` (+ stubs por motor)
- [x] Configuración segmentada y persistente (`shared`: TOML, `LOCALFORGE_*`, tools)
- [x] Gestión de `llama-server` con health-check, SSE y `-ngl` (`llm_engine`)
- [x] Modo externo: detecta un servidor OpenAI local y usa el modelo de `/v1/models`
- [x] Controlador como crate independiente (`crates/controller`)
- [x] UI por secciones con iconos: sidebar LLM/Audio/3D/Ajustes, chat, diagnóstico y temas
- [x] Monitor de CPU/RAM de la app y del host en tiempo real (muestreado en el core)
- [x] Encender / apagar / reiniciar el motor local desde la cabecera
- [x] STT con `whisper-cli` (`stt`): WAV 16 kHz, timeout, parser tolerante
- [x] TTS Kokoro ONNX (`tts`): worker `tools/kokoro_worker.py`, comando externo y WAV 24 kHz
- [x] Captura cpal, VAD WebRTC, endpointing y playback (`audio_engine`)
- [ ] Motor 3D (`scene3d`) — stub, sin fecha
- [ ] Conversación manos libres completa (barge-in, streaming TTS)
- [ ] Empaquetado e instaladores

Los crates sin implementar compilan como placeholders con su contrato en el
docstring; no fingen funcionalidad. Consulta [docs/ROADMAP.md](docs/ROADMAP.md).

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

Crea `.venv-kokoro/` con `kokoro-onnx` (ONNX Runtime CPU, sin PyTorch) y descarga
`kokoro-v1.0.int8.onnx` + `voices-v1.0.bin` en `assets/models/kokoro/`. Después
configura en **Ajustes → Motor Audio**: Python (`.venv-kokoro/bin/python`),
worker (`tools/kokoro_worker.py`), carpeta del modelo y voz (`ef_dora`).

### 5. Ejecutar

```bash
cargo run
```

La primera vez abre **Motor LLM** para indicar el binario, el GGUF o el endpoint
externo. La configuración se guarda en:

- Linux: `~/.config/localforge/config.toml`
- Windows: `%APPDATA%\localforge\config.toml`
- macOS: `~/Library/Application Support/localforge/config.toml`

Si vienes de VoxLocal, se migra una vez desde `voxlocal/config.toml`; el archivo
antiguo no se borra.

## Documentación

| Documento | Contenido |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, flujo de datos, hilos, contratos y ADRs |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Hitos y criterios de aceptación |
| [docs/HARDWARE-VULKAN.md](docs/HARDWARE-VULKAN.md) | RDNA 1, presupuestos de VRAM, builds y diagnóstico |
| [docs/MODELS.md](docs/MODELS.md) | Modelos recomendados y descargas |
| [AGENTS.md](AGENTS.md) | Contexto del proyecto y reglas para agentes |

## Pruebas

La suite vive en `test/`, es un crate propio **fuera del workspace** y está
ignorada por git: el repositorio solo versiona el código de la app.

```bash
cd test && cargo test        # 64 tests: shared, llm_engine, system, audio, ui, controller, boundary
```

No necesita GPU, micrófono ni modelos. El detalle (cobertura, fixtures y
checklist manual en la máquina objetivo) está en `test/README.md`,
`test/docs/plan.md` y `test/docs/hardware.md`, que por diseño no se versionan.

## Skills para agentes

Las skills locales viven en `.agents/skills/` y son la guía operativa del proyecto:

- `localforge-architecture` — límites entre crates y contratos
- `vulkan-ai-stack` — llama.cpp/whisper.cpp en Vulkan, RDNA 1 y VRAM
- `gpui-kit-desktop` — GPUI Kit 0.9: Entity, Settings, tareas y tests
- `voice-pipeline` — VAD, STT, LLM, TTS y latencia
- `process-supervision` — procesos hijos, health-checks y limpieza
- `localforge-workflow` — build, modelos, verificación y releases
- `rust-best-practices` — borrowing, clippy, errores, tests y type-state

## Licencia

MIT OR Apache-2.0

## Rendimiento

- Para uso real compila en release: `cargo run --release`. El perfil incluye
  `lto = "thin"`, `codegen-units = 1` y `strip`; el build de debug es mucho más
  lento y no representa el rendimiento final.
- La app ronda los **200 MB**. El footer muestra `LLM X MB`: esa es la memoria
  del proceso `llama-server`.
- Ver 13 GB de RAM usada con un 8B Q4 no es la UI: es el modelo + KV cache. En
  8 GB de VRAM, un 8B con `-ngl 99` y `ctx 32768` hace swap y arrastra todo el
  sistema. Usa `ctx 4096–8192` y `-ngl 24–32`, o un 4B Q4_K_M.
- El escaneo de modelos corre en `spawn_blocking`, el muestreo de recursos en su
  propia tarea y la captura de audio en un hilo con callback sin locks: nada de
  eso bloquea el hilo de GPUI.
- La biblioteca de modelos busca por defecto en `~/.cache/huggingface/hub`
  cuando no hay carpeta configurada, recorriendo `models--*/snapshots/*/`.

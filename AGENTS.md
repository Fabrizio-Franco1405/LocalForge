# AGENTS.md - VoxLocal Project Context

## 1. Project Overview
VoxLocal es un asistente de voz local y multiplataforma (Linux, Windows, macOS) construido en Rust con GPUI Kit. El objetivo es ofrecer una experiencia de conversación por voz 100% offline (VAD → STT → LLM → TTS), donde el usuario pueda configurar visualmente cada componente (rutas de modelos, puertos, argumentos de GPU, voces) y la app gestione el ciclo de vida de los procesos externos (llama-server, whisper-cli) de forma transparente.

## 2. Hardware Target & Constraints (CRÍTICO)
Este proyecto está diseñado **exclusivamente para hardware de gama media/antigua** que no tiene acceso a los ecosistemas propietarios de NVIDIA (CUDA) o al stack moderno de AMD (ROCm). El agente **NO DEBE** sugerir, instalar ni escribir código que dependa de CUDA o ROCm.
- **GPU Objetivo:** AMD RX 5700 XT (RDNA 1 / gfx1010) con 8 GB de VRAM. También debe funcionar en GPUs NVIDIA e Intel a través de Vulkan.
- **CPU Objetivo:** AMD Ryzen 7 5700G (8 núcleos, 16 hilos, sin NPU).
- **RAM Objetiva:** 32 GB DDR4.
- **Backend Obligatorio:** **Vulkan**. Es el único backend que funciona de forma nativa y estable en RDNA 1 sin necesidad de modificar drivers ni instalar ROCm.
- **Regla de Oro:** Si una librería o herramienta requiere CUDA o ROCm, debe ser descartada o sustituida por una alternativa basada en Vulkan o CPU.

## 3. Arquitectura: Feature Crates
El proyecto sigue el patrón de **Feature Crates** de GPUI Kit. La raíz es un Workspace con un `main.rs` (shell) y `lib.rs` (controlador testeable) en `src/`, y los módulos de lógica en `crates/`:
- `src/main.rs`: Shell de GPUI. Solo compone la ventana, carga configuración y arranca el controlador.
- `src/lib.rs`: Expone `controller` para poder probarlo desde fuera del binario.
- `src/controller.rs`: Hilo del controlador (comandos, procesos, HTTP, generaciones).
- `crates/llm_engine`: Gestión del proceso `llama-server` (spawn, kill, health-check, `-ngl`, puerto, SSE) y modo externo.
- `crates/audio_engine`: Captura de micrófono, VAD y reproducción de audio. **Stub (M2/M4).**
- `crates/stt`: Wrapper para `whisper-cli`. **Stub (M2).**
- `crates/tts`: Wrapper para `kokoro-onnx`. **Stub (M3).**
- `crates/ui`: Componentes visuales de GPUI Kit (chat, Settings, herramientas).
- `crates/shared`: Tipos comunes, configuración, eventos y utilidades de texto/rutas. Es hoja.

**Regla de oro:** La UI (`crates/ui`) no debe saber cómo se lanza `llama-server`. Solo emite eventos o llama a métodos expuestos por `crates/llm_engine`. La lógica de negocio no debe depender de GPUI.

Nota de estado: `llm_engine`, `shared`, `ui` y el controlador están implementados; `stt`,
`tts` y `audio_engine` compilan como placeholders con su contrato en el docstring.
No añadas funcionalidad simulada: implementa el crate o deja el stub honesto.

## 4. Stack Tecnológico y Documentación Oficial (Solo Vulkan)

### 4.1 Framework de UI: GPUI Kit
- **Lenguaje:** Rust (edición 2024).
- **Documentación:** https://gpui-kit.com
- **Guía de Arquitectura:** https://github.com/longbridge/gpui-component/blob/main/website/docs/index.md
- **Reglas:**
  - Usa `gpui-kit` como única dependencia principal para la UI.
  - Prefiere componentes de `gpui-component` (75+ componentes documentados) antes de crear controles desde cero.
  - Sigue el modelo de renderizado `Render` y `RenderOnce`.
  - Los estados deben manejarse con `Entity` y `Context`.

### 4.2 Motor LLM: llama.cpp (llama-server) con Vulkan
- **Repositorio:** https://github.com/ggml-org/llama.cpp
- **Documentación del Servidor:** https://github.com/ggml-org/llama.cpp/tree/master/tools/server
- **Documentación del Backend Vulkan:** https://deepwiki.com/ggml-org/llama.cpp/5.3-example-applications (Sección: Vulkan Backend)
- **API:** Compatible con OpenAI (`/v1/chat/completions`).
- **Reglas:**
  - El proceso se lanza con `tokio::process::Command`.
  - La configuración (`LlmConfig`) incluye `model`, `mode` (`Managed`/`External`), `host`, `port`, `gpu_layers`, `ctx_size`, `threads`, `extra_args`.
  - El `LlamaManager` expone `start()`, `stop()`, `health()`, `wait_ready()` y `chat_stream()`.
  - El proceso hijo se elimina en `Drop` (`start_kill`) y con `kill_on_drop(true)`.
  - En modo `External` no se lanza ningún proceso: se sondea `/v1/models`, se usa el `model` reportado y `/health` solo si existe.
  - **Obligatorio:** Compilar `llama.cpp` con `-DGGML_VULKAN=ON`.

### 4.3 STT: whisper.cpp (whisper-cli) con Vulkan
- **Repositorio:** https://github.com/ggml-org/whisper.cpp
- **Documentación del Backend Vulkan:** https://deepwiki.com/ggml-org/whisper.cpp/2.4.1-cuda-backend (El contenido de esta página es sobre Vulkan, aunque la URL mencione CUDA).
- **Reglas:**
  - El binario `whisper-cli` se invoca como subproceso con argumentos: `-m <model_path> -f <audio.wav> -l <language>`.
  - El audio de entrada debe ser WAV mono a 16 kHz.
  - La salida estándar se parsea de forma tolerante: líneas con `[timestamp]` o texto
    plano (se invoca con `-nt`, que quita timestamps); descarta `[BLANK_AUDIO]` y similares.
  - **Obligatorio:** Compilar `whisper.cpp` con `-DGGML_VULKAN=1`.

### 4.4 TTS: kokoro-onnx (CPU / ONNX Runtime)
- **Estado:** pendiente (M3); `crates/tts` es un stub y `tools/` aún no existe.
- **Repositorio:** https://github.com/xieren58/kokoro-onnx
- **Modelo y Voces:** https://github.com/thewh1teagle/kokoro-onnx/releases/tag/model-files-v1.0
- **Voces disponibles:** https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md
- **Reglas:**
  - Kokoro se ejecuta en CPU a través de ONNX Runtime para no competir por la VRAM con el LLM y Whisper.
  - El texto debe limpiarse de emojis y símbolos Markdown (`shared::text::for_speech`) antes de enviarse a Kokoro.
  - La voz y el idioma deben ser configurables desde la UI.

### 4.5 Pipeline de Voz (Referencia): speech-to-speech
- **Repositorio:** https://github.com/huggingface/speech-to-speech
- **Uso:** Este framework es la referencia conceptual para la arquitectura en cascada (VAD → STT → LLM → TTS). No es una dependencia directa, pero sus patrones de diseño (colas, hilos) deben inspirar la implementación del `audio_engine`.

## 5. Configuración Segmentada
- Las estructuras `serde` (`LlamaConfig`, `SttConfig`, `TtsConfig`, `AudioConfig`, `UiConfig`) viven en `crates/shared/src/config.rs` y se agrupan en `VoxConfig`.
- La carga se centraliza en `shared::config` con `figment` (TOML + `VOXLOCAL_*`); no leas `std::env::var` fuera de ahí.
- La UI de configuración usa el componente `Settings` de GPUI Kit y persiste con `VoxConfig::save`.

## 6. Pruebas (aisladas)

- La suite vive en `test/`, un crate propio con `[workspace]` (fuera del workspace de la app).
- `test/` está en `.gitignore`: el repositorio solo versiona el código de la aplicación.
- No agregues `#[cfg(test)]` ni `mod tests` en `src/` ni en `crates/`; los tests van a
  `test/tests/`.
- Para probar lógica privada, expón una API pública pequeña y documentada (ej.
  `shared::text::truncate_chars`, `ui::theme::parse_hex`), no dejes el test dentro del crate.
- Comando: `cd test && cargo test` (42 tests; sin GPU, micrófono, modelos ni red externa).
- Lo que requiera hardware, modelos o latencia real va a `test/docs/hardware.md` como
  checklist manual, no como test automatizado.

## 7. Comandos de Desarrollo
```bash
# Compilar todo el workspace
cargo build

# Ejecutar la aplicación
cargo run

# Verificar linting
cargo clippy --all-targets --all-features -- -D warnings

# Formatear código
cargo fmt --all

# Pruebas (crate aislado, fuera del workspace y de git)
cd test && cargo test
```


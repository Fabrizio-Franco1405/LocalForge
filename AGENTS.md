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
El proyecto sigue el patrón de **Feature Crates** de GPUI Kit. La raíz es un Workspace con un `main.rs` en `src/` y los módulos de lógica en `crates/`:
- `crates/app` (o raíz `src/main.rs`): Shell de GPUI. Solo compone ventanas, inicializa el runtime y conecta la UI con los servicios.
- `crates/llm_engine`: Gestión del proceso `llama-server` (spawn, kill, health-check, configuración de `-ngl`, puerto, etc.).
- `crates/audio_engine`: Captura de micrófono, detección de actividad de voz (VAD) y reproducción de audio.
- `crates/stt`: Wrapper para `whisper-cli` (transcripción de audio a texto).
- `crates/tts`: Wrapper para `kokoro-onnx` (síntesis de texto a voz).
- `crates/ui`: Componentes visuales reutilizables de GPUI Kit (Settings, formularios, etc.).
- `crates/shared`: Tipos comunes, errores, eventos y estructuras de configuración global.

**Regla de oro:** La UI (`crates/ui`) no debe saber cómo se lanza `llama-server`. Solo emite eventos o llama a métodos expuestos por `crates/llm_engine`. La lógica de negocio no debe depender de GPUI.

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
  - La configuración (`LlamaConfig`) debe incluir: `model_path`, `port`, `ngl` (capas en GPU), `threads`.
  - El `LlamaManager` debe exponer métodos `start()`, `stop()` y `health_check()` (usando `GET /health`).
  - El proceso hijo debe ser eliminado en el `Drop` trait para evitar zombis.
  - **Obligatorio:** Compilar `llama.cpp` con `-DGGML_VULKAN=ON`.

### 4.3 STT: whisper.cpp (whisper-cli) con Vulkan
- **Repositorio:** https://github.com/ggml-org/whisper.cpp
- **Documentación del Backend Vulkan:** https://deepwiki.com/ggml-org/whisper.cpp/2.4.1-cuda-backend (El contenido de esta página es sobre Vulkan, aunque la URL mencione CUDA).
- **Reglas:**
  - El binario `whisper-cli` se invoca como subproceso con argumentos: `-m <model_path> -f <audio.wav> -l <language>`.
  - El audio de entrada debe ser WAV mono a 16 kHz.
  - La salida estándar debe parsearse para extraer el texto transcrito (buscar líneas con `[timestamp]`).
  - **Obligatorio:** Compilar `whisper.cpp` con `-DGGML_VULKAN=1`.

### 4.4 TTS: kokoro-onnx (CPU / ONNX Runtime)
- **Repositorio:** https://github.com/xieren58/kokoro-onnx
- **Modelo y Voces:** https://github.com/thewh1teagle/kokoro-onnx/releases/tag/model-files-v1.0
- **Voces disponibles:** https://huggingface.co/hexgrad/Kokoro-82M/blob/main/VOICES.md
- **Reglas:**
  - Kokoro se ejecuta en CPU a través de ONNX Runtime para no competir por la VRAM con el LLM y Whisper.
  - El texto debe limpiarse de emojis y símbolos Markdown antes de enviarse a Kokoro.
  - La voz y el idioma deben ser configurables desde la UI.

### 4.5 Pipeline de Voz (Referencia): speech-to-speech
- **Repositorio:** https://github.com/huggingface/speech-to-speech
- **Uso:** Este framework es la referencia conceptual para la arquitectura en cascada (VAD → STT → LLM → TTS). No es una dependencia directa, pero sus patrones de diseño (colas, hilos) deben inspirar la implementación del `audio_engine`.

## 5. Configuración Segmentada
- Cada crate define su propia estructura de configuración con `serde` (ej. `LlamaConfig`, `TtsConfig`, `SttConfig`).
- La carga se centraliza en `crates/shared` usando `figment` (o `config`) para combinar archivos TOML y variables de entorno.
- La UI de configuración debe usar el componente `Settings` de GPUI Kit, generando pestañas automáticamente a partir de las estructuras.

## 6. Comandos de Desarrollo
```bash
# Compilar todo el workspace
cargo build

# Ejecutar la aplicación
cargo run

# Verificar linting
cargo clippy -- -D warnings

# Formatear código
cargo fmt

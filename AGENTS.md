# AGENTS.md - LocalForge Project Context

## 1. Project Overview
LocalForge es un **harness local de IA** multiplataforma (Linux, Windows, macOS) construido en Rust con GPUI Kit. Conecta motores que corren en tu máquina (LLM, voz y, próximamente, 3D) sin nube, sin PyTorch y sin CUDA. El usuario configura cada motor visualmente (rutas de binarios y modelos, puertos, argumentos de GPU, voces) y la app gestiona el ciclo de vida de los procesos externos (`llama-server`, `whisper-cli`, worker TTS) de forma transparente.

## 2. Hardware Target & Constraints (CRÍTICO)
Este proyecto está diseñado **exclusivamente para hardware de gama media/antigua** que no tiene acceso a los ecosistemas propietarios de NVIDIA (CUDA) o al stack moderno de AMD (ROCm). El agente **NO DEBE** sugerir, instalar ni escribir código que dependa de CUDA o ROCm.
- **GPU Objetivo:** AMD RX 5700 XT (RDNA 1 / gfx1010) con 8 GB de VRAM. También debe funcionar en GPUs NVIDIA e Intel a través de Vulkan.
- **CPU Objetivo:** AMD Ryzen 7 5700G (8 núcleos, 16 hilos, sin NPU).
- **RAM Objetiva:** 32 GB DDR4.
- **Backend Obligatorio:** **Vulkan**. Es el único backend que funciona de forma nativa y estable en RDNA 1 sin necesidad de modificar drivers ni instalar ROCm.
- **Regla de Oro:** Si una librería o herramienta requiere CUDA o ROCm, debe ser descartada o sustituida por una alternativa basada en Vulkan o CPU.

## 3. Arquitectura: crates y módulos
El workspace tiene un binario mínimo (`src/main.rs`) y crates con responsabilidades separadas:

- `crates/controller`: hilo del controlador (comandos, procesos, HTTP, generaciones, diagnóstico), muestreo CPU/RAM/VRAM (`system.rs`), hardware (`hardware.rs`) y biblioteca de GGUF (`models.rs`). Librería pura, sin GPUI.
- `crates/shared`: contrato UI ⇄ motores, configuración, texto y resolución de rutas. Es hoja.
- `crates/llm_engine`: `llama-server` gestionado y modo externo OpenAI (`/v1/models`, SSE).
- `crates/stt`: wrapper de `whisper-cli` (WAV 16 kHz, timeout, parser). **Implementado.**
- `crates/tts`: Kokoro worker/comando externo + lectura WAV. **Implementado.**
- `crates/audio_engine`: captura cpal, VAD WebRTC, endpointing y playback. **Implementado.**
- `crates/scene3d`: motor 3D. **Stub (futuro).**
- `crates/ui`: vista GPUI componentizada:
  - `app/mod.rs` — shell, `WorkspaceMode`, `SettingsTab`, pump y acciones.
  - `app/sidebar.rs` — marca + 4 workspaces (Chat, Llamada, Estudio 3D, Ajustes).
  - `app/chat.rs`, `app/call.rs`, `app/studio.rs` — los tres workspaces.
  - `app/settings_page.rs` + `app/system.rs` — pestañas de Ajustes y panel de hardware.
  - `settings/mcp.rs` — pestaña MCP (cliente pendiente).
  - `app/footer.rs` — estado del LLM, CPU/RAM/VRAM, micrófono y altavoz.
  - `settings/mod.rs` + `settings/{fields,llm,audio,scene3d,general}.rs` — una página por motor.
  - `settings/llm.rs` decide los campos según `LlmMode`: Local muestra binario/GGUF/host; Externo muestra endpoint/model id/API key. Nunca ambos.
  - `dialog.rs` — únicos selectores de archivos: `prompt_for_paths` de GPUI con callback asíncrono (`Pick::File/Files/Folder`, `pick_paths`, `pick_paths_for`).

**Regla de oro:** La UI (`crates/ui`) no debe saber cómo se lanza `llama-server`. Solo envía `EngineCommand` y pinta `EngineEvent`. La lógica de negocio no depende de GPUI.

**Regla de tamaño:** ningún archivo de UI debe volver a pasar de ~500 líneas; una página o grupo de widgets por módulo.

**Regla de frontera:** el core no importa GPUI. La UI consume `EngineCommand`/`EngineEvent`; sustituirla por JS/TS o Python es implementar ese protocolo, no tocar los motores. `test/tests/boundary.rs` lo verifica.

**Regla de rendimiento:** nada pesado en el hilo de GPUI ni en el callback de audio. Escaneos, STT y TTS van a `spawn_blocking`/hilos; la captura usa un canal acotado y el callback solo convierte y reenvía. Para medir, usa `--release`.

**Regla multiplataforma:** nada de `/proc` obligatorio, rutas POSIX asumidas ni `unsafe`. Usa `sysinfo`, `PathBuf`, `shared::tools::resolve_binary` (PATHEXT) y los selectores nativos de GPUI (`cx.prompt_for_paths` vía `ui::dialog`). Linux, Windows 11 y macOS deben compilar y funcionar.

**Regla de diálogos nativos:** prohibido `rfd` y cualquier diálogo modal bloqueante dentro de un callback de la UI. `rfd::FileDialog::pick_file()` corre un bucle modal propio que en Windows reentra en la ventana de GPUI mientras el `App` está prestado y mata el proceso con `RefCell already borrowed` / `0xc0000409`. Usa `crate::dialog::pick_paths`/`pick_paths_for`; `test/tests/ui.rs` falla si `rfd` reaparece en el código o en el manifiesto.

Nota de estado: `llm_engine`, `shared`, `ui`, `controller`, `stt`, `tts` y `audio_engine` están implementados; solo `scene3d` sigue como placeholder. No añadas funcionalidad simulada: implementa el crate o deja el stub honesto.

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
- **Estado:** implementado (`crates/tts` + `tools/kokoro_worker.py` + `tools/setup_kokoro.sh`).
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
- La carga se centraliza en `shared::config` con `figment` (TOML + `LOCALFORGE_*`; se acepta el prefijo antiguo `VOXLOCAL_*`); no leas `std::env::var` fuera de ahí.
- La config vive en `dirs::config_dir()/localforge/config.toml`. Si existe la ruta antigua `voxlocal/config.toml`, `VoxConfig::load_default()` la migra una vez.
- La UI de configuración usa el componente `Settings` de GPUI Kit y persiste con `VoxConfig::save`.

## 6. Pruebas (aisladas)

- La suite vive en `test/`, un crate propio con `[workspace]` (fuera del workspace de la app).
- `test/` está en `.gitignore`: el repositorio solo versiona el código de la aplicación.
- No agregues `#[cfg(test)]` ni `mod tests` en `src/` ni en `crates/`; los tests van a
  `test/tests/`.
- Para probar lógica privada, expón una API pública pequeña y documentada (ej.
  `shared::text::truncate_chars`, `ui::theme::parse_hex`), no dejes el test dentro del crate.
- Comando: `cd test && cargo test` (64 tests; sin GPU, micrófono, modelos ni red externa).
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


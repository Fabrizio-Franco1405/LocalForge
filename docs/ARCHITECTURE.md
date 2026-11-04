# Arquitectura de VoxLocal

## 1. Objetivo y no-objetivos

**Objetivo:** un asistente de voz 100 % local y multiplataforma que encadene
VAD → STT → LLM → TTS con modelos configurables, sobre GPUs sin CUDA/ROCm
(objetivo principal: RX 5700 XT, RDNA 1, 8 GB) y CPUs sin NPU.

**No objetivos (v1):**

- Entrenamiento o fine-tuning.
- Ejecución de modelos PyTorch.
- Backends CUDA, ROCm, TensorRT o DirectML.
- Servicios en la nube obligatorios (todo debe funcionar offline).
- WASM/móvil.

## 2. Principios

1. **La UI no sabe arrancar motores.** `ui` solo envía comandos y pinta eventos.
2. **Los motores no dependen de GPUI.** `llm_engine`, `stt`, `tts` y
   `audio_engine` son librerías Rust testeables sin ventana.
3. **Un proceso por motor pesado.** llama.cpp y whisper.cpp se ejecutan como
   procesos hijos; el estado y los modelos no viven en el heap de la UI.
4. **Vulkan o CPU.** No hay rutas de compilación alternativas con CUDA/ROCm.
5. **Todo configurable sin recompilar.** Rutas, puertos, capas GPU, voces y
   umbrales viven en `config.toml`.
6. **Fallo ruidoso y recuperable.** Un motor caído se reporta con su último log
   y se puede reiniciar sin cerrar la app.

## 3. Mapa de crates

```text
voxlocal (src/main.rs)        shell GPUI: ventana, configuración y arranque
voxlocal (src/lib.rs)         fachada de librería: `pub mod controller`
├── src/controller.rs         hilo controlador: comandos, procesos, HTTP, SSE
├── crates/ui                 vistas, ajustes, chat, herramientas (solo gpui-kit + shared)
├── crates/shared             config, tipos, eventos, errores, texto y tools (hoja)
├── crates/llm_engine         llama-server: spawn, health, SSE, modo externo
├── crates/stt                whisper-cli: WAV, proceso, parser          (stub, M2)
├── crates/tts                Kokoro ONNX: worker, limpieza de texto     (stub, M3)
└── crates/audio_engine       cpal: captura, VAD, endpointing, playback  (stub, M2/M4)
```

El paquete `voxlocal` es **bin + lib**: `main.rs` solo compone la ventana y
`lib.rs` expone `controller` para que la suite aislada en `test/` pueda
ejercitarlo sin display.

### Estado de implementación

| Componente | Estado | Detalle |
| --- | --- | --- |
| `shared` | Implementado | Config TOML/`VOXLOCAL_*`, eventos, `for_speech`, `truncate_chars`, resolución de binarios |
| `llm_engine` | Implementado | `build_args`, spawn/kill, health, SSE, `usage`, modo `External` con `/v1/models` |
| `ui` | Implementado | Chat, ajustes por pestañas, herramientas/diagnóstico, temas |
| `controller` | Implementado | Comandos, generaciones, probe, historial, apagado |
| `stt` | Stub (M2) | Contrato documentado en `crates/stt/src/lib.rs` |
| `tts` | Stub (M3) | Contrato documentado en `crates/tts/src/lib.rs` |
| `audio_engine` | Stub (M2/M4) | Contrato documentado en `crates/audio_engine/src/lib.rs` |

Un stub **no simula** funcionalidad: compila, documenta su contrato y la UI lo
muestra deshabilitado o ausente. No se aceptan implementaciones a medias que
finjan funcionar.

### Dependencias permitidas

```text
ui           -> gpui-kit, shared
llm_engine   -> shared, tokio, reqwest
stt          -> shared, tokio, hound          (cuando se implemente)
tts          -> shared, hound                 (cuando se implemente)
audio_engine -> shared, cpal, webrtc-vad      (cuando se implemente)
shared       -> serde, figment, toml, dirs, thiserror
main/lib     -> todos
```

`shared` no depende de ningún otro crate del workspace. Esto evita ciclos y
permite testear la lógica sin GPUI.

## 4. Flujo de datos

### 4.1 Conversación

```
UI (GPUI, hilo principal)
  │ EngineCommand::SendMessage { generation, text }
  ▼
Controller (hilo dedicado + runtime tokio)
  │ LlamaManager::chat_stream()
  ▼
llama-server :9931 (Vulkan)
  │ SSE delta tokens
  ▼
EngineEvent::Token { generation, delta }
  │
  ▼
UI actualiza el último mensaje del asistente
```

El controlador soporta dos modos de LLM:

- `Managed`: lanza `llama-server` y habla con `127.0.0.1:<port>`.
- `External`: no lanza ningún proceso; sondea `/v1/models`, usa el `model`
  reportado y sirve para Ollama, LM Studio o un `llama-server` ya arrancado.

El pipeline de voz (sustituir el texto por el transcript de `stt` y enviar la
respuesta limpia a `tts`) está **diseñado pero pendiente**: `stt`, `tts` y
`audio_engine` son stubs hasta M2/M3/M4, así que hoy la conversación es por texto.

### 4.2 Voz manos libres

```
cpal capture (48 kHz estéreo)
  → downmix + remuestreo a 16 kHz mono
  → EndpointDetector (WebRTC VAD, 20 ms)
  → SpeechEnd(samples)
  → WhisperCli::transcribe()
  → Controller → LLM → Kokoro → AudioPlayer
```

El detector mantiene un *pre-roll* de 250–400 ms para no cortar la primera
sílaba y un silencio mínimo de 500–800 ms para cerrar el turno.

## 5. Modelo de hilos y concurrencia

| Hilo | Responsabilidad | Regla |
| --- | --- | --- |
| Hilo GPUI | Render, eventos de UI | Nunca bloquear con I/O o procesos |
| `voxlocal-controller` | Comandos, procesos, HTTP | Runtime tokio multi-hilo |
| Workers tokio | Streaming SSE, health-checks | Tareas cancelables |
| Hilo de audio (cpal) | Callback de captura/playback | Solo copiar muestras a un canal; sin locks largos |
| Workers de cpal | Remuestreo/mezcla | Buffers preasignados |

**Comunicación UI ⇄ controlador:** canales.

- `EngineCommand`: `crossbeam_channel::Sender` (UI → controlador).
- `EngineEvent`: `crossbeam_channel::Receiver` (controlador → UI), sondeado
  cada 50 ms desde una tarea GPUI.

No se comparten `Arc<Mutex<...>>` entre UI y motores salvo para estado
inmutable o contadores de nivel de audio.

## 6. Contratos entre crates

### `shared`

- `VoxConfig`: raíz serializable con secciones `llm`, `stt`, `tts`, `audio`,
  `ui`.
- `EngineCommand` / `EngineEvent`: el único contrato UI ⇄ motores.
- `EngineHandle`: fachada clonable con `send`, `try_recv`, `recv_timeout` y
  `drain`.
- `ConfigError`: errores de configuración (I/O, TOML inválido, serialización).
- `text::for_speech` y `text::truncate_chars`: limpieza de Markdown/emojis para
  TTS y recorte seguro en UTF-8.
- `tools::{expand_path, resolve_binary, file_status, command_program}`:
  resolución de binarios y rutas configuradas.

### `llm_engine`

```rust
pub struct LlamaManager { /* ... */ }

impl LlamaManager {
    pub fn new(config: LlmConfig, startup_timeout: Duration) -> Result<Self, LlmError>;
    pub fn build_args(config: &LlmConfig) -> Result<Vec<String>, LlmError>;
    pub async fn start(&mut self) -> Result<(), LlmError>;
    pub async fn wait_ready(&mut self) -> Result<(), LlmError>;
    pub async fn wait_ready_for(&mut self, timeout: Duration) -> Result<(), LlmError>;
    pub async fn health(&self) -> LlmHealth;
    pub async fn probe(&self) -> ServerProbe;
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<mpsc::Receiver<ChatDelta>, LlmError>;
    pub async fn stop(&mut self);
    pub fn last_logs(&self, n: usize) -> Vec<String>;
}
```

`Drop` mata el hijo (`start_kill`) y `kill_on_drop(true)` evita zombis incluso
si el runtime se desmonta de golpe. En modo `External` `start()` es un no-op y
`probe()` es la fuente de verdad del estado.

### `stt`

**Estado: stub (M2).** El contrato siguiente es el objetivo, no código actual.

```rust
pub struct WhisperCli { /* ... */ }
impl WhisperCli {
    pub fn transcribe(&self, samples_16k_mono: &[f32]) -> Result<Transcript, SttError>;
}
```

- Escribe WAV PCM16 mono 16 kHz en un temporal.
- Ejecuta `whisper-cli -m <model> -f <wav> -l <lang> -t <n> -np -nt`.
- Parsea stdout: acepta líneas con `[00:00:00.000 --> ...]` y texto plano.
- Descarta marcadores como `[BLANK_AUDIO]`, `[MUSIC]`, `(silencio)`.

### `tts`

**Estado: stub (M3).** El contrato siguiente es el objetivo; `tools/kokoro_worker.py`
todavía no existe.

```rust
pub trait Synthesizer: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, TtsError>;
}

pub struct KokoroCli { /* ... */ }
```

Kokoro correrá en **CPU con ONNX Runtime** para no competir por VRAM. El plan
(ADR-004) es llamar a un worker Python aislado (`.venv-kokoro`) porque los
crates Rust de Kokoro (`tts-rs`, `kokoro-tts`) están rotos contra
`ort 2.0.0-rc.15`. La interfaz permitirá sustituirlo por un backend nativo
cuando `ort` estabilice.

### `audio_engine`

**Estado: stub (M2/M4).** El contrato siguiente es el objetivo.

```rust
pub struct AudioCapture;   // cpal input -> Receiver<Vec<f32>> 16 kHz mono
pub struct AudioPlayer;    // cola -> cpal output
pub struct EndpointDetector<V: VoiceActivity>; // VAD + histéresis
pub struct LinearResampler; // 48 kHz -> 16 kHz sin dependencias C
```

## 7. Configuración

Precedencia (mayor gana):

1. Argumentos de línea de comandos (futuro).
2. Variables de entorno `VOXLOCAL_*` (doble guion bajo para anidar:
   `VOXLOCAL_LLM__PORT=9931`).
3. `config.toml` del usuario.
4. Valores por defecto del código.

`VoxConfig::load` usa `figment` y `#[serde(default)]`, por lo que un TOML
parcial es válido y las claves nuevas no rompen configuraciones antiguas.

Rutas por defecto:

| Recurso | Ruta |
| --- | --- |
| Config | `dirs::config_dir()/voxlocal/config.toml` |
| Datos/logs | `dirs::data_dir()/voxlocal/` |
| Binarios | `assets/bin/` o `PATH` |
| Modelos | `assets/models/{llm,whisper,kokoro}` |

## 8. Errores y logs

- Cada crate define su error con `thiserror`; `shared` ofrece `ConfigError` para
  el límite de la aplicación.
- La UI nunca hace `unwrap` sobre errores de motor: los muestra como
  notificación o en la barra de estado.
- Los stderr de `llama-server` y `whisper-cli` se guardan en un ring buffer
  (últimas 200 líneas) para diagnosticar fallos de carga de modelo.
- `tracing` es el logger único; `RUST_LOG=voxlocal=debug` activa detalle.

## 9. Decisiones (ADR)

### ADR-001 — Vulkan como único backend GPU

**Contexto:** RDNA 1 (gfx1010) no tiene soporte estable en ROCm; CUDA no aplica.
**Decisión:** compilar llama.cpp con `-DGGML_VULKAN=ON` y whisper.cpp con
`-DGGML_VULKAN=1`. No incluir flags CUDA/HIP/ROCm.
**Consecuencia:** funciona en AMD, NVIDIA e Intel con driver Vulkan; el
rendimiento depende del driver (Mesa RADV es el objetivo).

### ADR-002 — Procesos hijos para LLM y STT

**Contexto:** los bindings Rust in-process acoplan versiones y complican la
recuperación ante crashes.
**Decisión:** `llama-server` y `whisper-cli` como procesos gestionados.
**Consecuencia:** aislamiento, reinicio barato y logs claros; coste de IPC
(HTTP/stdio) despreciable frente a la inferencia.

### ADR-003 — TTS en CPU

**Contexto:** 8 GB de VRAM compartidos entre LLM y Whisper.
**Decisión:** Kokoro ONNX en CPU.
**Consecuencia:** más latencia (0,3–1,5 s por frase), pero el LLM conserva
VRAM. Se puede solapar síntesis y reproducción por frases.

### ADR-004 — Worker Python aislado para Kokoro (temporal)

**Contexto:** `tts-rs 2026.2.3` y `kokoro-tts 0.3.3` no compilan con
`ort 2.0.0-rc.15` (API de `Session::builder` cambió). Escribir el tokenizador
de Kokoro desde cero es un proyecto aparte.
**Decisión:** worker Python mínimo (`tools/kokoro_worker.py`) con
`kokoro-onnx` y ONNX Runtime CPU, en un venv propio. **Pendiente de
implementar (M3):** hoy `tools/` no existe y `crates/tts` es un stub.
**Consecuencia:** cero PyTorch/CUDA y modelos ONNX; se paga una dependencia de
Python en runtime. La trait `Synthesizer` permite reemplazarlo por un backend
nativo cuando `ort` y los crates Kokoro vuelvan a compilar.

### ADR-005 — Canales en vez de compartir estado

**Contexto:** GPUI es single-thread; tokio y cpal viven en otros hilos.
**Decisión:** comandos por canal y eventos por canal; la UI sondea cada 50 ms.
**Consecuencia:** no hay data races ni locks entre UI y motores; el coste es
una latencia de UI acotada a 50 ms.

## 10. Seguridad y privacidad

- Todo el audio y los prompts se quedan en `localhost`.
- `llama-server` escucha en `127.0.0.1` por defecto. Nunca `0.0.0.0` salvo
  decisión explícita del usuario.
- Los logs no incluyen audio ni prompts completos por defecto.
- El worker de TTS se lanza sin shell, con argumentos separados (sin
  inyección de comandos); el texto va por stdin.

## 11. Pruebas

- La suite vive en `test/`: crate propio con `[workspace]`, **fuera del
  workspace** y **fuera de git** (`.gitignore` ignora `/test/`). El repositorio
  solo versiona el código de la aplicación.
- No hay módulos `#[cfg(test)]` en `src/` ni en `crates/`; si algo necesita
  test, se expone una API pública mínima y documentada
  (`shared::text::truncate_chars`, `llm_engine::{take_sse_payloads, parse_delta}`,
  `ui::theme::parse_hex`).
- Comando: `cd test && cargo test`. Cubre `shared` (23), `llm_engine` (10),
  `ui` (7) y el controlador (2): 42 tests sin GPU, micrófono, modelos ni red
  externa. El controlador se prueba contra un servidor OpenAI falso en loopback.
- Lo que requiere hardware, modelos o latencia real es checklist manual
  (`test/docs/hardware.md`), no un test automatizado.
- Consecuencia asumida: al no versionarse, la suite no protege PRs remotos; es
  una red de seguridad local. Para CI con tests habría que versionar un espejo
  o reconstruir la carpeta en el workflow.

# Arquitectura de LocalForge

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
localforge (src/main.rs)     shell GPUI: ventana, configuración y arranque
├── crates/controller        hilo controlador: comandos, procesos, HTTP, SSE
├── crates/ui                vistas por secciones (solo gpui-kit + shared)
├── crates/shared            config, eventos, texto y tools (hoja)
├── crates/llm_engine        llama-server gestionado + modo externo
├── crates/stt               whisper-cli                  (stub, M2)
├── crates/tts               Kokoro ONNX                  (stub, M3)
├── crates/audio_engine      cpal: captura/VAD/playback   (stub, M2/M4)
└── crates/scene3d           motor 3D                     (stub, futuro)
```

El binario `localforge` solo compone la ventana y el arranque. El controlador
vive en `crates/controller`, así que la suite aislada de `test/` puede
ejercitarlo sin display.

### Estado de implementación

| Componente | Estado | Detalle |
| --- | --- | --- |
| `shared` | Implementado | Config TOML/`LOCALFORGE_*` (legado `VOXLOCAL_*`), eventos, `for_speech`, `truncate_chars`, binarios |
| `llm_engine` | Implementado | `build_args`, spawn/kill, health, SSE, `usage`, modo `External` con `/v1/models` |
| `controller` | Implementado | Comandos, generaciones, probe, historial, diagnóstico, apagado y muestreo CPU/RAM (`system.rs`) |
| `ui` | Implementado | Workspaces Chat/Llamada/Estudio 3D/Ajustes, footer de recursos y Ajustes por pestañas |
| `stt` | Stub (M2) | Contrato documentado en `crates/stt/src/lib.rs` |
| `tts` | Stub (M3) | Contrato documentado en `crates/tts/src/lib.rs` |
| `audio_engine` | Stub (M2/M4) | Contrato documentado en `crates/audio_engine/src/lib.rs` |
| `scene3d` | Stub (futuro) | Contrato documentado en `crates/scene3d/src/lib.rs` |

Un stub **no simula** funcionalidad: compila, documenta su contrato y la UI lo
muestra deshabilitado o ausente. No se aceptan implementaciones a medias que
finjan funcionar.

### Dependencias permitidas

```text
ui           -> gpui-kit, shared
controller   -> shared, llm_engine, crossbeam-channel, tokio
llm_engine   -> shared, tokio, reqwest
stt          -> shared, tokio, hound          (cuando se implemente)
tts          -> shared, hound                 (cuando se implemente)
audio_engine -> shared, cpal, webrtc-vad      (cuando se implemente)
scene3d      -> shared                        (stub)
shared       -> serde, figment, toml, dirs, thiserror
main         -> controller, shared, ui, gpui-kit
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
| `localforge-controller` | Comandos, procesos, HTTP | Runtime tokio multi-hilo |
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

### `controller`

```rust
pub fn spawn(config: VoxConfig) -> EngineHandle;
```

- Owns the OS thread, the tokio runtime, the child processes and the HTTP clients.
- `ReloadConfig` restarts the engine with the new values; `ProbeLlm` and
  `ProbeTools` force a probe.
- External mode: `ProbeLlm` reads `/v1/models`, adopts the reported model id and
  detects a server started outside LocalForge. With an empty endpoint it
  reports a clear error instead of probing the local port.
- `ProbeHardware` collects OS/CPU/RAM/GPU/tools and emits `EngineEvent::Hardware`.
- `SystemUsage` (CPU, RAM, VRAM, app memory) is sampled every 2 s in an
  independent task, so values are visible even while a model is loading.
- Emits only [`EngineEvent`]; the UI never calls an engine directly.

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
2. Variables de entorno `LOCALFORGE_*` (doble guion bajo para anidar:
   `LOCALFORGE_LLM__PORT=9931`). El prefijo antiguo `VOXLOCAL_*` se sigue
   leyendo para no romper configuraciones existentes.
3. `config.toml` del usuario.
4. Valores por defecto del código.

`VoxConfig::load` usa `figment` y `#[serde(default)]`, por lo que un TOML
parcial es válido y las claves nuevas no rompen configuraciones antiguas.
`VoxConfig::load_default` migra una vez `voxlocal/config.toml` a
`localforge/config.toml` (deja el archivo viejo intacto).

Rutas por defecto:

| Recurso | Ruta |
| --- | --- |
| Config | `dirs::config_dir()/localforge/config.toml` |
| Datos/logs | `dirs::data_dir()/localforge/` |
| Binarios | `assets/bin/` o `PATH` |
| Modelos | `assets/models/{llm,whisper,kokoro}` |

## 8. Errores y logs

- Cada crate define su error con `thiserror`; `shared` ofrece `ConfigError` para
  el límite de la aplicación.
- La UI nunca hace `unwrap` sobre errores de motor: los muestra como
  notificación o en la barra de estado.
- Los stderr de `llama-server` y `whisper-cli` se guardan en un ring buffer
  (últimas 200 líneas) para diagnosticar fallos de carga de modelo.
- `tracing` es el logger único; `RUST_LOG=localforge=debug` activa detalle.

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

### ADR-006 — UI por secciones y controlador como crate

**Contexto:** `settings.rs` y `app.rs` superaban las 800 líneas, y el
controlador vivía en el binario, lo que impedía probarlo desde la suite externa.
**Decisión:** extraer `crates/controller`; dividir la UI en `app/`
(shell, sidebar, chat, tools, status) y `settings/` (campos + una página por
motor); sidebar con cuatro secciones aisladas (LLM, Audio, 3D, General) y el
chat como inicio; el modo LLM decide qué campos se montan.
**Consecuencia:** archivos pequeños, páginas independientes y un contrato
estable para reutilizar el core. Coste: más módulos y un `Page` con estado de
navegación.

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
- Comando: `cd test && cargo test`. Cubre `shared` (24), `llm_engine` (11), `system` (6),
  `ui` (8), el controlador (2), sistema (6) y frontera (2): 53 tests sin GPU, micrófono, modelos ni red
  externa. El controlador se prueba contra un servidor OpenAI falso en loopback.
- Lo que requiere hardware, modelos o latencia real es checklist manual
  (`test/docs/hardware.md`), no un test automatizado.
- Consecuencia asumida: al no versionarse, la suite no protege PRs remotos; es
  una red de seguridad local. Para CI con tests habría que versionar un espejo
  o reconstruir la carpeta en el workflow.

## 12. Frontera UI ⇄ core (UI-agnóstica)

El core (`shared`, `llm_engine`, `controller`, `stt`, `tts`, `audio_engine`,
`scene3d`) no depende de GPUI. La única superficie entre la UI y el core es:

```text
EngineHandle::send(EngineCommand)  ->  controller thread  ->  motores
EngineHandle::drain()/try_recv()   <-  EngineEvent
```

Consecuencias:

- Cambiar la UI (JS/TS, Python, web) es implementar ese protocolo, no tocar los
  motores. Un adaptador IPC (JSON Lines o WebSocket) sobre `EngineHandle`
  habilita cualquier front-end.
- `EngineEvent` es `Clone + Send + 'static` y solo contiene primitivas (`u64`,
  `String`, `f32`, `Option<...>`), nunca tipos de GPUI.
- Los recursos del host viajan como `EngineEvent::SystemUsage`, muestreados en
  `controller::system` con una tarea propia: la UI solo pinta.
- `test/tests/boundary.rs` falla si un crate core añade `gpui` a su manifiesto o
  importa GPUI en el código. Es una regla ejecutable, no una convención.

Si en el futuro se extrae el core a otro lenguaje, el contrato a conservar es
exactamente `EngineCommand`/`EngineEvent` más los tipos de `shared::config`.

## 13. Workspaces y multiplataforma

La UI se organiza en `WorkspaceMode` (`Chat`, `Call`, `Studio3d`, `Settings`).
Dentro de `Settings` hay `SettingsTab` (`Llm`, `Audio`, `Scene3d`, `General`,
`System`). Añadir un workspace es añadir una variante, una entrada al sidebar y
una vista; el core no cambia.

Multiplataforma:

- `sysinfo` da CPU/RAM/SO en Linux, Windows 11 y macOS; no hay dependencia de
  `/proc` en el camino principal.
- VRAM: sysfs AMD en Linux y `nvidia-smi` si existe; AMD en Windows queda como
  `n/d` hasta integrar DXGI/ADL.
- `rfd` abre los selectores nativos en las tres plataformas.
- `resolve_binary` respeta `PATHEXT` en Windows; los hijos usan `kill_on_drop`.
- Prohibido `unsafe`: el proyecto no enlaza APIs de driver.

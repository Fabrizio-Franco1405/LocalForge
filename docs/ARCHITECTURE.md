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

```
voxlocal (src/main.rs)        shell GPUI + controlador + wiring
├── crates/ui                 vistas, ajustes, chat (solo gpui-kit + shared)
├── crates/shared             config, tipos, eventos, errores, texto
├── crates/llm_engine         llama-server: spawn, health, SSE
├── crates/stt                whisper-cli: WAV, proceso, parser
├── crates/tts                Kokoro ONNX: worker, limpieza de texto
└── crates/audio_engine       cpal: captura, VAD, endpointing, playback
```

### Dependencias permitidas

```text
ui           -> gpui-kit, shared
llm_engine   -> shared, tokio, reqwest
stt          -> shared, tokio, hound
tts          -> shared, hound
audio_engine -> shared, cpal, webrtc-vad
shared       -> serde, figment, toml, dirs, thiserror
main         -> todos
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

Cuando el pipeline de voz está activo, el controlador sustituye el texto de
entrada por el transcript de `stt` y, al terminar la respuesta, envía el texto
limpio a `tts` y reproduce el audio.

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
- `EngineHandle`: fachada clonable con `send` y `try_recv`.
- `VoxError`: errores de configuración y de canal.
- `text::for_speech`: limpieza de Markdown/emojis para TTS.

### `llm_engine`

```rust
pub struct LlamaManager { /* ... */ }

impl LlamaManager {
    pub fn new(cfg: &LlmConfig) -> Result<Self, LlmError>;
    pub fn build_args(cfg: &LlmConfig) -> Result<Vec<String>, LlmError>;
    pub async fn start(&mut self) -> Result<(), LlmError>;
    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), LlmError>;
    pub async fn health(&self) -> LlmHealth;
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<mpsc::Receiver<ChatDelta>, LlmError>;
    pub async fn stop(&mut self);
}
```

`Drop` mata el hijo (`start_kill`) y `kill_on_drop(true)` evita zombis incluso
si el runtime se desmonta de golpe.

### `stt`

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

```rust
pub trait Synthesizer: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<AudioBuffer, TtsError>;
}

pub struct KokoroCli { /* ... */ }
```

Kokoro corre en **CPU con ONNX Runtime** para no competir por VRAM. El crate
llama a un worker Python aislado (`.venv-kokoro`) porque los crates Rust de
Kokoro (`tts-rs`, `kokoro-tts`) están rotos contra `ort 2.0.0-rc.15` (ver
ADR-004). La interfaz permite sustituirlo por un backend nativo cuando `ort`
estabilice.

### `audio_engine`

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

- Cada crate define su error con `thiserror`; `shared` ofrece `VoxError` para
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
`kokoro-onnx` y ONNX Runtime CPU, en un venv propio.
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

# Roadmap

Cada hito debe terminar con `cargo fmt`, `cargo clippy -- -D warnings` y
`cargo test` en verde. Los criterios de aceptación se prueban en la máquina
objetivo (RX 5700 XT + 5700G).

## M0 — Fundaciones (completado en este scaffold)

- Workspace de 6 crates + shell GPUI Kit.
- `VoxConfig` con TOML, variables de entorno y valores por defecto.
- Contratos `EngineCommand` / `EngineEvent` y `EngineHandle`.
- `LlamaManager` con spawn, health-check, streaming SSE y limpieza.
- `WhisperCli` con WAV 16 kHz y parser tolerante.
- `KokoroCli` + worker Python aislado + limpieza de texto.
- `AudioCapture`, `AudioPlayer`, `EndpointDetector` y `LinearResampler`.
- UI: chat + ajustes + barra de estado.

**Criterio:** `cargo run` abre la app, permite guardar ajustes y conversar con
un GGUF local por texto.

## M1 — Conversación por texto sólida

- [ ] Historial persistente por sesión (JSONL en `data_dir`).
- [ ] Cancelación cooperativa del streaming (AbortHandle + generación).
- [ ] Reintento automático si `llama-server` muere a mitad de respuesta.
- [ ] Mostrar métricas: tokens/s, tiempo hasta el primer token, VRAM usada.
- [ ] Selector de modelos GGUF escaneando `assets/models/llm`.
- [ ] Plantillas de chat y `--jinja` validadas contra el modelo.

**Criterio:** 20 turnos seguidos sin fugas de memoria ni procesos zombis.

## M2 — Dictado (STT)

- [ ] Botón de micrófono con push-to-talk y waveform.
- [ ] Captura multidispositivo y selección persistente.
- [ ] VAD WebRTC con umbral y silencio configurables en caliente.
- [ ] Transcripción en < 1,5 s para 5 s de audio con `ggml-small`.
- [ ] Manejo de errores: modelo ausente, binario sin Vulkan, audio vacío.

**Criterio:** dictar una frase en español y verla en el input sin tocar el teclado.

## M3 — Respuesta hablada (TTS)

- [ ] Reproducción por frases solapada con la generación del LLM.
- [ ] Cola de audio con cancelación (barge-in).
- [ ] Voces Kokoro por idioma y velocidad configurables.
- [ ] Cache de frases frecuentes (opcional, disco).
- [ ] Medición de RTF (real-time factor) en CPU.

**Criterio:** el asistente responde en voz alta en < 4 s desde el fin del turno.

## M4 — Manos libres

- [ ] Máquina de estados `Idle → Listening → Thinking → Speaking → Idle`.
- [ ] Barge-in: el usuario habla y se corta la reproducción + generación.
- [ ] Detección de fin de turno con VAD + timeout adaptativo.
- [ ] Wake word opcional (fuera de alcance inicial).
- [ ] Modo "solo auriculares" y selección de salida.

**Criterio:** conversación de 5 turnos sin pulsar botones, con interrupción
funcional.

## M5 — Distribución

- [ ] `build.rs`/script que compile llama.cpp y whisper.cpp con Vulkan.
- [ ] Empaquetado: `.deb`/AppImage, ZIP Windows, `.dmg` macOS.
- [ ] Primer arranque guiado (asistente de modelos).
- [ ] Descarga de modelos con verificación SHA-256.
- [ ] CI: `fmt`, `clippy`, `test`, build Linux; builds cruzados opcionales.
- [ ] Documentación de requisitos Vulkan por plataforma.

## Ideas posteriores

- Backend STT in-process con `whisper-rs` y feature Vulkan.
- Backend TTS nativo con `ort` cuando los crates Kokoro estabilicen.
- Traducción local opcional (NLLB/M2M ONNX) antes del LLM.
- Perfiles por aplicación (juegos, oficina) para `-ngl` y contexto.
- Telemetría local (nunca remota) para ajustar umbrales de VAD.

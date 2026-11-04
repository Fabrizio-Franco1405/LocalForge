# Plan de pruebas

## Pirámide usada

| Capa | Dónde | Qué entra | Coste |
| --- | --- | --- | --- |
| Unitaria pura | `tests/shared.rs`, `tests/ui.rs`, `tests/llm_engine.rs` | Parsers, defaults, estado del chat, flags | Milisegundos |
| Integración loopback | `tests/controller.rs` | Controlador real + servidor falso | < 1 s |
| Manual de hardware | `docs/hardware.md` | GPU, micrófono, modelos, latencia | Minutos |

Regla: si se puede decidir mirando datos de entrada y salida, es automatizable.
Si necesita un dispositivo o un modelo, es manual.

## Cobertura actual por crate

| Crate | Estado | Tests | Pendiente |
| --- | --- | --- | --- |
| `shared` | Implementado | 23 | Reload de config desde entorno (`LOCALFORGE_*`), `save` atómico |
| `llm_engine` | Implementado | 12 | Parseo de `usage`/tokens, timeout de stream, reinicio tras crash |
| `ui` | Implementado | 12 | Estados de la barra, notificaciones, settings (requieren ventana) |
| `localforge` (controlador) | Implementado | 2 | Cancelación de generación, `ReloadConfig`, diagnóstico de tools |
| `stt` | Implementado | 3 | WAV PCM16, parser de stdout, códigos de error |
| `tts` | Implementado | 1 | Sanitizador, plantilla de comando, WAV 24 kHz |
| `audio_engine` | Implementado | 5 | Remuestreo lineal, endpoint detector con VAD falso |

## Tests a escribir cuando exista cada crate

### `stt` (M2)

1. `parse_output` acepta líneas con `[00:00:00.000 --> ...]` y texto plano.
2. `parse_output` descarta `[BLANK_AUDIO]`, `[MUSIC]`, `(silencio)`.
3. El WAV generado es PCM16, mono, 16 kHz (cabecera comprobada con `hound`).
4. Un WAV temporal se borra tras éxito y tras error.
5. Exit code ≠ 0 devuelve stderr (últimas líneas) en el error.
6. Timeout de 60 s mata el proceso.

### `tts` (M3)

1. `for_speech` ya cubierto; el backend no debe recibir Markdown.
2. El comando externo sustituye `{out}`, `{voice}`, `{speed}` y no usa shell.
3. Texto por stdin (nunca argv).
4. WAV de salida a 24 kHz mono; error si el worker falla.
5. Timeout por frase y limpieza del temporal.

### `audio_engine` (M2/M4)

1. `LinearResampler` 48 → 16 kHz conserva la frecuencia de una sinusoide.
2. `EndpointDetector` con `VoiceActivity` falso: emite `SpeechEnd` con la
   longitud esperada, respeta pre-roll y silencio mínimo.
3. Corte por `max_utterance_ms` (30 s).
4. Barge-in solo tras 300 ms de voz y 200 ms de guarda.

## Criterios de aceptación por hito

| Hito | Criterio | Cómo se comprueba |
| --- | --- | --- |
| M1 texto | 20 turnos sin fugas ni zombis | `docs/hardware.md` §Sesión larga |
| M2 dictado | frase en español < 1,5 s para 5 s de audio | manual + `tracing` |
| M3 voz | primera frase audible < 4 s | manual con cronómetro |
| M4 manos libres | 5 turnos sin botones, barge-in funcional | checklist manual |

## Riesgos que la suite debe cubrir primero

1. **SSE mal parseado** → respuestas truncadas o duplicadas (ya cubierto).
2. **Modelo equivocado** en modo externo → `/v1/models` ignorado (ya cubierto).
3. **Config parcial** que rompe al añadir campos (ya cubierto).
4. **Zombis** de `llama-server`/`whisper-cli` (manual: `pgrep`).
5. **TTS leyendo Markdown** (cubierto en `shared`).
6. **Barge-in que se dispara con el propio TTS** (manual, sin AEC real).
7. **Selector de archivos que cierra la app en Windows**: un diálogo modal
   bloqueante reentra en GPUI sobre el `&mut App` prestado (`RefCell already
   borrowed`, `0xc0000409`). Cubierto en `tests/ui.rs`: opciones del prompt
   nativo y prohibición de `rfd` en el código y el manifiesto de `ui`.

# Fixtures y datos de prueba

Nada de esto se versiona. Los modelos son grandes y con licencias propias.

## Audio sintético (sin micrófono)

Genera WAV de prueba con `ffmpeg` o `sox`; sirven para STT manual y para
comprobar el formato PCM16 16 kHz.

```bash
mkdir -p /tmp/localforge-fixtures

# 3 s de silencio
ffmpeg -f lavfi -i anullsrc=r=16000:cl=mono -t 3 -c:a pcm_s16le \
  /tmp/localforge-fixtures/silencio.wav

# 5 s de tono (no es voz: debe producir transcript vacío o [BLANK_AUDIO])
ffmpeg -f lavfi -i sine=frequency=440:sample_rate=16000 -t 5 -c:a pcm_s16le \
  /tmp/localforge-fixtures/tono.wav

# Voz real grabada (la mejor fixture para español)
ffmpeg -f pulse -i default -t 6 -ar 16000 -ac 1 -c:a pcm_s16le \
  /tmp/localforge-fixtures/frase.wav
```

Comprobación de formato:

```bash
ffprobe -v error -show_entries stream=codec_name,sample_rate,channels \
  -of default=nw=1 /tmp/localforge-fixtures/frase.wav
# codec_name=pcm_s16le  sample_rate=16000  channels=1
```

Con `sox`:

```bash
sox -n -r 16000 -c 1 -b 16 /tmp/localforge-fixtures/silencio.wav trim 0 3
```

## Modelos para pruebas manuales

| Recurso | Ruta esperada | Tamaño | Nota |
| --- | --- | --- | --- |
| GGUF 4B Q4_K_M | `assets/models/llm/*.gguf` | ~2,5 GB | Cualquiera instruct; el ID lo reporta `/v1/models` |
| Whisper `ggml-small.bin` | `assets/models/whisper/` | 466 MB | Español; no usar `.en` |
| Kokoro int8 + voces | `assets/models/kokoro/` | ~115 MB | Solo cuando exista M3 |

En tests automatizados **no** se usan modelos: `llm_engine` prueba argumentos y
parsers, y el controlador usa el servidor falso.

## Servidor falso

`src/mock_server.rs` responde:

- `GET /v1/models` → `{"data":[{"id":"mock-qwen"}]}`
- `POST /v1/chat/completions` → SSE con `Hola` + ` mundo` + `[DONE]`
- Guarda el último body para asserts.

Se usa en `tests/controller.rs`; no requiere red externa ni permisos.

## Variables de entorno útiles

```bash
RUST_LOG=localforge=debug        # trazas del controlador
RUST_BACKTRACE=1               # pánico con backtrace
LOCALFORGE_LLM__PORT=9940        # override puntual de config
```

## Reglas

- Fixtures grandes → `/tmp`, nunca en el repo.
- Nada de audio de personas reales sin permiso.
- Los WAV temporales de STT se borran tras cada transcripción (probar también
  el camino de error).
- Los tests no descargan nada de Hugging Face.

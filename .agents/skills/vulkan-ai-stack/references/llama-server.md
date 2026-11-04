# llama-server reference (target: llama.cpp b12003+)

## Endpoints used by LocalForge

### Health

```http
GET /health
```

- `200 {"status":"ok"}` — model loaded, ready.
- `503 {"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}`
  — still loading.
- Connection refused — process not started or crashed.

`/v1/health` behaves the same. Health is public (no API key).

### Chat completions (OpenAI-compatible)

```http
POST /v1/chat/completions
Content-Type: application/json
```

```json
{
  "model": "local",
  "messages": [
    {"role": "system", "content": "Eres un asistente útil."},
    {"role": "user", "content": "Hola"}
  ],
  "stream": true,
  "temperature": 0.7,
  "max_tokens": 512,
  "cache_prompt": true
}
```

Streaming response (SSE):

```text
data: {"choices":[{"delta":{"role":"assistant"}}]}

data: {"choices":[{"delta":{"content":"Ho"}}]}

data: {"choices":[{"delta":{"content":"la"}}]}

data: [DONE]
```

Parse rules:

- Split on `\n\n`; a chunk may be split across TCP reads (keep a buffer).
- Ignore comment lines (`: keep-alive`) and empty `data:` payloads.
- `choices[0].delta.content` may be absent (role-only chunk).
- Stop at `data: [DONE]`.
- `choices[0].finish_reason` is non-null on the last content chunk.

Non-streaming: `choices[0].message.content`; usage in `usage`.

## Flags

| Flag | Default | LocalForge |
| --- | --- | --- |
| `-m, --model` | — | required |
| `-c, --ctx-size` | model | 4096 |
| `-ngl, --gpu-layers` | auto | 99 (or 24–28 for 8B/8 GB) |
| `-t, --threads` | auto | 8 |
| `--host` | 127.0.0.1 | 127.0.0.1 |
| `--port` | 9931 | 9931 |
| `--jinja` | off | on |
| `--no-webui` | off | on |
| `--mlock` | off | only if RAM allows |
| `--no-mmap` | off | avoid on slow disks |
| `-fa, --flash-attn` | auto | try `on` if supported |
| `--cache-type-k/v` | f16 | q8_0 to save VRAM on 8B |
| `--parallel` | 1 | 1 for a voice assistant |
| `--ubatch-size` | 512 | 256–512 |

## Process behavior

- Prints model metadata and device assignment at startup.
- Exits non-zero on invalid model/port already in use.
- Handles SIGTERM/SIGINT gracefully; still call `kill` from the app.
- On Windows, child processes do not die with the parent by default → use a job
  object or an explicit shutdown command.

## Minimal smoke test

```bash
llama-server -m model.gguf -ngl 99 -c 4096 --host 127.0.0.1 --port 9931 &
until curl -sf http://127.0.0.1:9931/health; do sleep 0.5; done
curl -s http://127.0.0.1:9931/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Di hola"}],"max_tokens":32}' | jq -r '.choices[0].message.content'
kill %1
```

## Version caveats

- Default port changed to **9931** in recent builds; do not assume 8080.
- `-ngl` accepts `auto` and `all` in new builds.
- `--jinja` is required for models whose template uses Jinja features.
- `/completion` is *not* OpenAI-compatible; use `/v1/completions` or
  `/v1/chat/completions`.

# Troubleshooting

## Step 0 — Is it a Vulkan problem or an app problem?

```bash
vulkaninfo --summary          # driver / device present
llama-server --list-devices   # ggml sees Vulkan
whisper-cli --help            # binary runs
```

If any of these fail, fix the environment before touching LocalForge code.

## llama-server will not start

| Check | Command | Fix |
| --- | --- | --- |
| Binary exists | `command -v llama-server` | set `llm.binary` to an absolute path |
| Model path | `ls -lh <model>` | absolute path, `.gguf` |
| Port free | `ss -ltnp \| grep 9931` | change `llm.port` or kill the stale process |
| Model loads | run the same args manually | read the first error line |
| Vulkan | `--list-devices` | recompile with Vulkan |

Capture the first 30 lines of stderr; the failure reason is almost always
there (`unknown model architecture`, `failed to load`, `failed to allocate`).

## Health stays 503

- Large model on a slow disk: loading can take 30–120 s the first time.
- Check RAM/swap: mmap may be thrashing.
- Increase `startup_timeout_secs`; do not poll faster than 250 ms.
- If the child exited, `/health` refuses connections; inspect the child status
  instead of treating it as "loading".

## Tokens/s is CPU-like

- Confirm the binary was built with Vulkan, not a distro CPU build.
- Look for `offloaded N/N layers to GPU` in the log.
- If `N` is 0, `-ngl` is missing or the model is not offloadable.
- On Linux, check `MESA_VK_DEVICE_SELECT` is not forcing the iGPU.
- Compare with a manual run and `radeontop`.

## OOM (`VK_ERROR_OUT_OF_DEVICE_MEMORY`)

The compositor and browser consume VRAM too. Free 500–1000 MiB before testing.
Then reduce `-ngl`, `-c`, KV precision, or model size (in that order).

## Whisper problems

| Symptom | Fix |
| --- | --- |
| `failed to initialize` | model is not `ggml-*.bin` or is corrupt |
| empty output | check WAV is 16 kHz mono PCM16; test with `samples/jfk.wav` |
| repeats/hallucinates | lower `--temperature 0`, set `-nth 0.6`, improve VAD |
| slow | `-t 8`, use `small`, ensure Vulkan build |
| `[BLANK_AUDIO]` | normal for silence; filter it in the parser |
| accents wrong | `-l es`, use a multilingual model (not `.en`) |

## Audio devices

```bash
# ALSA
aplay -l
arecord -l

# PipeWire
pw-cli list-objects | grep node.name

# Test capture
arecord -f S16_LE -r 16000 -c 1 -d 3 /tmp/test.wav && aplay /tmp/test.wav
```

- cpal uses ALSA on Linux; PipeWire is reached through its ALSA/Pulse bridge.
- If the default device changes while running, rebuild the stream (cpal emits
  a device error; the app should recover on next capture).
- Bluetooth headsets often expose 8/16 kHz mono; the resampler handles it.

## GPUI window problems

- `failed to open window` with no display → run inside a Wayland/X11 session.
- Missing `libxkbcommon`/`wayland` → install dev packages (see README).
- Blank window on NVIDIA → check the proprietary driver's Vulkan ICD.
- The app builds but panics on start → run with `RUST_BACKTRACE=1` and
  `RUST_LOG=debug`.

## Process leaks

```bash
pgrep -af 'llama-server|whisper-cli'
```

- `LlamaManager::Drop`, `kill_on_drop`, and `Shutdown` must all kill the child.
- On Windows, use a Job Object if the parent can crash.
- whisper-cli is short-lived; a leak means `wait()` was never called.

## Clean-room test sequence

```bash
# 1. engine alone
llama-server -m model.gguf -ngl 99 -c 2048 --port 9931 &
curl -sf http://127.0.0.1:9931/health && echo OK
kill %1

# 2. STT alone
whisper-cli -m ggml-small.bin -f /tmp/test.wav -l es -np -nt

# 3. TTS alone
.venv-kokoro/bin/python tools/kokoro_worker.py \
  --model-dir assets/models/kokoro --voice ef_dora --out /tmp/tts.wav <<< 'Hola, esto es una prueba.'

# 4. app
RUST_LOG=debug cargo run
```

# Checklist manual de hardware

Solo para la máquina objetivo (RX 5700 XT + Ryzen 7 5700G) o equivalente con
Vulkan. No automatizable en CI pública: requiere GPU, micrófono y modelos.

## 0. Prerrequisitos

```bash
rustc --version                  # 1.92+
vulkaninfo --summary | grep -E 'deviceName|driverName'
# deviceName = AMD Radeon RX 5700 XT (RADV NAVI10)
# driverName = radv

llama-server --version
llama-server --list-devices      # debe listar Vulkan0
whisper-cli --help
```

- [ ] Vulkan funciona y usa RADV (no `llvmpipe`, no CPU).
- [ ] `llama-server --list-devices` muestra la GPU discreta, no solo la iGPU.
- [ ] `assets/models/llm/*.gguf` y `assets/models/whisper/ggml-small.bin` existen.

## 1. Motor LLM solo

```bash
llama-server -m assets/models/llm/<modelo>.gguf -ngl 99 -c 4096 \
  --host 127.0.0.1 --port 9931 &
curl -s http://127.0.0.1:9931/health
# 200 {"status":"ok"}   (503 mientras carga)
curl -s http://127.0.0.1:9931/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Di hola"}],"max_tokens":32}' \
  | jq -r '.choices[0].message.content'
kill %1
```

- [ ] `GET /health` llega a 200.
- [ ] El log contiene `offloaded N/N layers to GPU`.
- [ ] Generación > 10 tok/s en 4B; si no, el binario no tiene Vulkan.

## 2. App con motor gestionado

```bash
RUST_LOG=localforge=debug cargo run
```

- [ ] La ventana abre y muestra el estado del motor.
- [ ] Iniciar → `Listo`; Detener → `Stopped`.
- [ ] Cambiar `-ngl` o puerto en Ajustes y reiniciar aplica el cambio.
- [ ] Cerrar la ventana no deja procesos:

```bash
pgrep -af 'llama-server|whisper-cli'    # debe salir vacío
ss -ltnp | grep 9931                     # debe salir vacío
```

- [ ] Repetir iniciar/detener 10 veces: sin `address already in use`, sin zombis.

## 3. Modo externo (sin GGUF configurado)

Con un servidor OpenAI local (Ollama, LM Studio, llama-server aparte):

```bash
ollama serve &            # ejemplo, puerto 11434
```

- [ ] Ajustes → modo `Externo` → endpoint `http://127.0.0.1:11434`.
- [ ] Pulsar Probar/Iniciar detecta el servidor y muestra el modelo reportado.
- [ ] Enviar mensaje: la petición usa el `model` de `/v1/models`.

## 4. STT (cuando M2 esté implementado)

```bash
whisper-cli -m assets/models/whisper/ggml-small.bin \
  -f /tmp/localforge-fixtures/frase.wav -l es -t 8 -np -nt
```

- [ ] Reconoce una frase en español sin timestamps en la salida.
- [ ] Audio de 5 s transcribe en < 1,5 s.
- [ ] Silencio → sin texto (se filtran `[BLANK_AUDIO]`).
- [ ] Modelo ausente → error visible, no crash.

## 5. Micrófono y VAD

```bash
arecord -f S16_LE -r 16000 -c 1 -d 3 /tmp/mic.wav && aplay /tmp/mic.wav
```

- [ ] El dispositivo de entrada aparece en Ajustes y persiste tras reiniciar.
- [ ] Hablar → `SpeechStart` en < 300 ms; callar → cierre en 600–800 ms.
- [ ] No se corta la primera sílaba (pre-roll 250–350 ms).
- [ ] Frase larga se corta a los 30 s como máximo.

## 6. TTS (cuando M3 esté implementado)

```bash
cd /ruta/LocalForge
.venv-kokoro/bin/python tools/kokoro_worker.py \
  --model-dir assets/models/kokoro --voice ef_dora --out /tmp/tts.wav \
  <<< 'Hola, esto es una prueba.'
```

- [ ] WAV a 24 kHz, voz española (`ef_dora`/`em_alex`).
- [ ] El texto con Markdown llega limpio (sin `#`, `*`, enlaces).
- [ ] Frase corta sintetizada en 0,3–0,9 s en CPU.

## 7. Latencia de extremo a extremo (< 4 s)

Medir por turno con `tracing`:

- [ ] Fin de voz → STT: 0,4–1,2 s.
- [ ] STT → primer token: 0,5–1,5 s.
- [ ] Primer token → primera frase TTS: 0,3–0,9 s.
- [ ] Total hasta audio: ≤ 4 s (p95).

## 8. Barge-in y eco

- [ ] Con auriculares: hablar mientras responde corta audio y generación.
- [ ] Con altavoces: documentar que se dispara solo (no hay AEC real).
- [ ] El corte no ocurre antes de 200 ms de playback ni con < 300 ms de voz.

## 9. Recursos y cierre

```bash
radeontop -d /dev/dri/renderD128     # GPU ocupada durante el LLM
watch -n0.5 'free -h'                # sin swap durante la carga
```

- [ ] VRAM < 8 GB; sin `VK_ERROR_OUT_OF_DEVICE_MEMORY`.
- [ ] Cerrar la app durante streaming: sin pánico y sin procesos huérfanos.
- [ ] `kill -9` al padre: en Linux el hijo muere; en Windows documentar la
  limitación (Job Object pendiente).

## 10. Diálogos nativos (Linux, Windows, macOS)

Regresión del cierre en Windows: `rfd::FileDialog` abría un diálogo modal
bloqueante dentro del callback de GPUI, reentraba en el `&mut App` prestado y
mataba el proceso con `RefCell already borrowed` / `0xc0000409`. Ahora todo
selector pasa por `ui::dialog` → `prompt_for_paths`. Probar en las tres
plataformas:

- [ ] Ajustes → Motor LLM → «Elegir archivo…» (binario `llama-server`) abre el
      explorador nativo y la app sigue viva.
- [ ] Ajustes → Motor LLM → «Elegir carpeta…» y Cancelar no cambia la ruta.
- [ ] Ajustes → Motor Audio → modelo ggml, script de Kokoro y carpeta del modelo.
- [ ] Chat → «Adjuntar» permite multiselección; los adjuntos aparecen como chips.
- [ ] Estudio 3D → «Elegir binario», «Modelos» y «Elegir imagen».
- [ ] Abrir el mismo selector dos veces seguidas: sin pánico ni diálogos
      huérfanos.
- [ ] El diálogo queda modal a la ventana y por delante de ella.
- [ ] Linux sin `xdg-desktop-portal`: aparece un `warn` en el log y la app
      sigue usable (no hay pánico).

## Registro de resultados

Anota por ejecución: fecha, versión de Mesa, modelo, `-ngl`, contexto, tok/s,
latencia y cualquier error del ring buffer. Sin esos datos, un "va lento" no es
accionable.

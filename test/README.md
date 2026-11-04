# Test suite de LocalForge

Suite **aislada y local**. No forma parte del workspace de la app, no se
versiona (`.gitignore` ignora `/test/`) y no viaja en releases: el repositorio
solo contiene el código de la aplicación.

```text
test/
├── Cargo.toml            # crate propio con [workspace] (fuera del workspace)
├── .cargo/config.toml    # reutiliza ../target para no recompilar GPUI
├── src/
│   ├── lib.rs            # helpers compartidos por la suite
│   └── mock_server.rs    # servidor OpenAI falso para el controlador
├── tests/
│   ├── shared.rs         # config, eventos, texto, tools        (24 tests)
│   ├── llm_engine.rs     # args, SSE, health, modo externo      (11 tests)
│   ├── ui.rs             # ChatLog, workspaces, colores         (8 tests)
│   ├── controller.rs     # controlador real + servidor falso      (2 tests)
│   ├── system.rs         # sampler, VRAM, escaneo y GPU           (9 tests)
│   ├── audio.rs          # remuestreo, parser STT y WAV TTS        (4 tests)
│   └── boundary.rs       # el core no depende de GPUI            (2 tests)
└── docs/
    ├── plan.md           # matriz de cobertura y qué falta
    ├── fixtures.md       # WAV, modelos y datos de prueba
    └── hardware.md       # checklist manual en la máquina objetivo
```

## Cómo ejecutar

Desde `test/` (importante: así cargo lee `.cargo/config.toml` y comparte el
`target` del proyecto):

```bash
cd test
cargo test                    # 62 tests, sin GPU, sin micrófono, sin modelos
cargo test --test shared      # solo un archivo
cargo test truncate           # filtra por nombre
cargo test -- --nocapture     # muestra stdout
```

Desde la raíz también funciona, pero recompila en `test/target`:

```bash
cargo test --manifest-path test/Cargo.toml
```

## Requisitos

- Rust 1.92+ (edición 2024).
- Nada más para los 62 tests actuales: ni Vulkan, ni `llama-server`, ni
  `whisper-cli`, ni micrófono.
- La primera compilación tarda porque construye `ui`/GPUI Kit; después usa la
  caché de `../target`.

## Qué cubre hoy

| Archivo | Qué prueba | Riesgo que protege |
| --- | --- | --- |
| `shared.rs` | Defaults, TOML parcial, round-trip, `expand_path`, `resolve_binary`, `for_speech`, `truncate_chars`, canal de eventos | Config rota, TTS leyendo Markdown, eventos perdidos |
| `llm_engine.rs` | `build_args`, `extra_args`, parser SSE (cola parcial, comentarios, `[DONE]`), health `Down`, URLs externas con/sin `/v1` | Arrancar llama-server con flags incorrectos, romper el streaming |
| `ui.rs` | `ChatLog` (deltas, cancelación, burbuja vacía), etiquetas de página, `parse_hex` | Burbujas duplicadas, texto perdido al cancelar, tema ilegible |
| `controller.rs` | Detección de servidor externo sin modelo, chat completo vía SSE, `model` reportado por `/v1/models` | Controlador que no arranca sin GGUF o que ignora el modelo del servidor |
| `system.rs` | Sampler `sysinfo`, parser de `nvidia-smi`, dispositivos Vulkan y `llama-server --list-devices` | Monitor de recursos o hardware que miente |
| `boundary.rs` | Los crates core no dependen ni importan GPUI | Que el "core" se acople a la UI y no se pueda portar |

Total: **62 tests**. Todos puros o con loopback; ninguno abre red externa.

## Convenciones

- Nombre descriptivo: `accion_condicion_resultado`
  (`deltas_after_finish_are_ignored`).
- Un comportamiento por test; varios asserts solo si describen lo mismo.
- Nada de `sleep` fijos: `recv_timeout` con deadline.
- Nada de depender del orden de `PATH` salvo lo imprescindible (`sh` en Unix).
- El servidor falso vive en `src/mock_server.rs`; no se copia en cada test.
- Los tests usan **API pública**. Si algo privado necesita test, se diseña una
  API pública pequeña y documentada (p. ej. `truncate_chars`, `parse_hex`).

## Cómo añadir un test

1. Crea o edita un archivo en `tests/`.
2. Importa desde `shared`, `llm_engine`, `ui` o `localforge`.
3. Si necesitas procesos, modelos o audio reales, eso no es un test
   automatizado: va a `docs/hardware.md` como checklist manual.
4. Ejecuta `cd test && cargo test`.
5. No lo muevas al workspace: el contrato es que el código de la app no
   contiene tests.

## Cuándo NO usar esta carpeta

- Benchmarks de latencia: usa `--release` y mide en la máquina objetivo
  (`docs/hardware.md`).
- Tests que requieren GPU, micrófono o modelos: checklist manual.
- Pruebas de UI con ventana real: GPUI necesita un display; quedan fuera de
  esta suite por ahora.

## Limitación consciente

Al no versionarse, esta suite **no protege pull requests** en un remoto: es una
red de seguridad local. Antes de cada commit relevante:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cd test && cargo test
```

Si algún día se quiere CI con tests, hay que decidir explícitamente versionar
un espejo de esta carpeta o un workflow que la reconstruya.

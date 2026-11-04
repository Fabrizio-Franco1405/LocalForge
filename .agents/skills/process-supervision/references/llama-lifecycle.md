# llama-server lifecycle state machine

## States

```
Stopped ──start()──► Starting ──health 200──► Ready
   ▲                    │                      │
   │                    ├─timeout──► Error     │
   │                    └─exit─────► Error     │
   └────stop()──────────┴──────────────────────┘
```

- `Starting`: child spawned, health not yet 200. Emit `LlmState::Starting`.
- `Ready`: health 200. Emit `LlmState::Ready` with device/model summary if
  parsed.
- `Error`: child exited, timeout, or spawn failure. Keep the last logs.
- `Stopped`: no child. Normal after `stop` or before `start`.

## Idempotency

```rust
pub async fn start(&mut self) -> Result<(), LlmError> {
    if self.child.is_some() {
        return Ok(()); // already managed by this instance
    }
    // spawn, capture logs, store child
}
```

Do not "reuse" a process you did not spawn unless the user explicitly chooses
to attach to a running server. A foreign process may be a different model.

## Startup sequence

1. Validate `model_path` exists and ends in `.gguf` (warn otherwise).
2. Build args (pure function, unit-tested).
3. Spawn with piped output, `kill_on_drop(true)`.
4. Start log readers immediately (both stdout and stderr).
5. Poll `/health` until 200, checking `child.try_wait()` on each miss.
6. On success, emit `Ready`; on failure, kill and emit `Error` with logs.

Never sleep a fixed 30 s. A small model may be ready in 2 s; a 8B on a cold
disk can take 60 s.

## Errors and messages

| Error | Message to user | Log hint |
| --- | --- | --- |
| `BinaryNotFound` | "No encuentro llama-server. Revisa Ajustes → Modelo." | path |
| `ModelMissing` | "El modelo GGUF no existe." | path |
| `PortInUse` | "El puerto 9931 está ocupado." | `ss -ltnp` |
| `StartTimeout` | "El modelo tardó demasiado en cargar." | last 20 lines |
| `Exited(status)` | "llama-server terminó con código X." | last 20 lines |
| `Spawn(err)` | OS error | — |
| `Http(err)` | "No se pudo hablar con el motor." | URL |

## Shutdown ordering

1. Set state `Stopping`.
2. Abort streaming tasks (drop receivers).
3. Send kill (`start_kill`), wait with timeout.
4. Drop the manager; `Drop` is a no-op if the child is `None`.
5. Set state `Stopped`.

If the app is closing, the controller should wait for this sequence (max 5 s)
before the process exits; otherwise the OS reaps the child anyway on Unix.

## Signals

- Unix: SIGTERM for graceful shutdown, SIGKILL after a timeout.
- Windows: `TerminateProcess` via `kill`; graceful shutdown is not exposed by
  llama-server; job objects handle orphans.
- macOS: same as Unix.

`tokio::process::Child::start_kill` sends SIGKILL on Unix. If you need SIGTERM:

```rust
#[cfg(unix)]
unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGTERM); }
```

Then wait 2–3 s and call `start_kill` if still alive.

## Health-check cost

A health request is cheap but not free. Poll every 250 ms only during
`Starting`; once `Ready`, poll every 5 s (or rely on request errors) to detect
crashes. Do not poll from the UI thread.

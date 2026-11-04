---
name: process-supervision
description: >
  Use when spawning, monitoring, restarting, or shutting down external
  processes in LocalForge: llama-server, whisper-cli, the Kokoro worker, port
  allocation, health checks, stderr capture, zombie prevention, cancellation,
  and cross-platform process cleanup (tokio and std).
license: MIT
compatibility: tokio 1.x, Rust 1.92+, Linux/Windows/macOS
metadata:
  author: localforge
  version: "0.1.0"
---

# Process supervision

## When to use this skill

Any change to `LlamaManager`, the whisper invocation, the Kokoro worker, app
shutdown, restart handling, or a bug where a process survives the app or a
second instance cannot bind the port.

## Rules

1. Every child has an **owner** and a **kill path** on all exits, including
   `Drop` and panic.
2. Every child has `stdin(null)`, piped stdout/stderr, and a bounded log ring.
3. Read stdout/stderr continuously; an unread pipe fills and deadlocks the
   child.
4. Health is determined by the child status **and** the protocol (HTTP/exit
   code), never by a fixed sleep.
5. A restart is explicit: stop (kill + wait) before start. Never spawn over a
   live child.
6. One instance per port. Check or fail with a clear message.
7. Never `shell = true` with a user-controlled model path or prompt.

## llama-server lifecycle

```rust
use tokio::process::{Child, Command};
use tokio::process::ChildStdout;
use tokio::io::{AsyncBufReadExt, BufReader};

let mut child = Command::new(&cfg.binary)
    .args(LlamaManager::build_args(cfg)?)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true)
    .spawn()?;
```

- `kill_on_drop(true)` protects against a leaked child if the manager is
  dropped, but it does **not** run on Windows if the parent is killed; see the
  job-object note below.
- Capture logs:

```rust
if let Some(stdout) = child.stdout.take() {
    let logs = self.logs.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            push_log(&logs, line);
        }
    });
}
```

Same for stderr (usually where llama.cpp writes load errors).

## Readiness

```rust
pub async fn wait_ready(&self, timeout: Duration) -> Result<(), LlmError> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() > deadline {
            return Err(LlmError::StartTimeout(self.last_logs(20)));
        }
        match self.health().await {
            LlmHealth::Ready => return Ok(()),
            LlmHealth::Loading => tokio::time::sleep(Duration::from_millis(250)).await,
            LlmHealth::Down => {
                if self.child_exited().await? {
                    return Err(LlmError::Exited(self.last_logs(20)));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}
```

- 503 is *loading*, not failure.
- Connection refused can be "not listening yet" or "crashed"; check
  `child.try_wait()` before treating it as fatal.
- Health request timeout: 1–2 s. The first load may block the server's event
  loop briefly; do not mark it dead on one slow response.

## Shutdown

```rust
pub async fn stop(&mut self) {
    if let Some(mut child) = self.child.take() {
        let _ = child.start_kill();
        // Wait up to 3 s for a graceful exit, then SIGKILL.
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    }
}
```

- `start_kill` is synchronous and safe to call from `Drop`.
- `wait` reaps the zombie; without it, the PID stays.
- On Unix, you may send SIGTERM first for a clean server shutdown; `start_kill`
  sends SIGKILL. Prefer SIGTERM via `libc`/`nix` if you need clean logs, then
  fall back to kill.
- Keep the manager's `Drop` as a last resort; the controller's `Shutdown`
  command is the normal path.

## Port handling

```rust
let addr = format!("{}:{}", cfg.host, cfg.port);
if std::net::TcpListener::bind(&addr).is_ok() {
    // free
}
```

- Binding to check is racy; the authoritative test is the child's exit/health.
- If the port is busy, probe `GET /health` on it: it may be an instance from a
  previous run. Offer "reuse" or "kill stale" explicitly, never silently kill
  an arbitrary process.
- Bind to `127.0.0.1`, not `0.0.0.0`, unless the user opts in.

## whisper-cli lifecycle

- Short-lived: spawn per utterance, write WAV, `output().await`, parse stdout.
- Apply a timeout (e.g., 60 s for a 30 s utterance) and kill on timeout.
- `-np -nt` keeps stdout clean; logs go to stderr.
- Do not spawn one process per 20 ms frame.
- On non-zero exit, return stderr (last 20 lines) in the error.

```rust
let output = tokio::time::timeout(
    Duration::from_secs(60),
    tokio::process::Command::new(&cfg.binary)
        .args(args)
        .output(),
).await??;
if !output.status.success() { /* include stderr */ }
```

## Kokoro worker lifecycle

- Blocking HTTP-free subprocess; run it in `spawn_blocking` from async code or
  on a dedicated TTS thread.
- Text goes via stdin; close stdin after writing so the worker does not wait.
- Timeout: 30 s per chunk; kill on timeout.
- Read stdout for the JSON result and stderr for the traceback.

## Windows: job objects

`kill_on_drop` does not fire if the parent process is terminated (Task Manager,
crash). For production, assign the child to a Job Object with
`KILL_ON_JOB_CLOSE`:

```rust
// windows crate: CreateJobObjectW + SetInformationJobObject +
// AssignProcessToJobObject, using child.raw_handle() via AsRawHandle.
```

Add this behind `#[cfg(windows)]` in the controller startup, not in the UI.
Until then, document the limitation and provide a "detener motores" action.

## Observability

- Ring buffer: last 200 lines per child, shared `Arc<Mutex<VecDeque<String>>>`.
- Log lines also go to `tracing` with `source = "llama-server"`.
- On failure, include the last 20 lines in the `EngineEvent::Error`.
- Never log prompts or audio; model paths are okay (may reveal username, so
  truncate home directory if logs are shared).

## Restart strategy

| Situation | Action |
| --- | --- |
| Start fails | report, leave `Stopped`, user can retry |
| Crashes while idle | report, optional auto-restart once |
| Crashes mid-stream | emit error, `ReplyDone(partial)`, allow retry |
| Config changed | explicit restart, not hot reload |
| App closing | `Shutdown`, wait up to 5 s, then force |

Avoid restart loops: track attempts and back off (1 s, 5 s, 30 s), stop after
3 failures until user action.

## Verification

```bash
# no orphans
pgrep -af 'llama-server|whisper-cli|kokoro_worker'

# port
ss -ltnp | grep 9931

# simulate crash
kill -9 $(pgrep llama-server)   # app should report Error
```

- Start/stop 10 times: no zombie, no "address already in use".
- Kill the app with SIGKILL: verify the child dies (or document the Windows
  exception).
- Close the window while streaming: child stops, no panic.

## References

- [`references/llama-lifecycle.md`](references/llama-lifecycle.md) — detailed
  state machine and error messages.
- [`references/logs-and-health.md`](references/logs-and-health.md) — parsing,
  timeouts, and diagnostics.

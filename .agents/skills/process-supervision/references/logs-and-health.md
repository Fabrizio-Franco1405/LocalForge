# Logs and health diagnostics

## Ring buffer

```rust
const MAX_LOG_LINES: usize = 200;

pub fn push_log(logs: &Arc<Mutex<VecDeque<String>>>, line: String) {
    tracing::debug!(target: "llama", "{line}");
    let mut logs = logs.lock().unwrap();
    if logs.len() == MAX_LOG_LINES {
        logs.pop_front();
    }
    logs.push_back(line);
}
```

- Truncate individual lines to 2 KB to avoid a pathological log eating RAM.
- Strip ANSI escapes before storing/showing.
- Keep stdout and stderr in one ring with a `[err]`/`[out]` prefix.

## What to grep in llama.cpp logs

| Pattern | Meaning |
| --- | --- |
| `ggml_vulkan: Found N Vulkan devices` | Vulkan backend active |
| `ggml_vulkan: 0 = AMD Radeon RX 5700 XT` | correct device |
| `load_tensors: offloaded N/N layers to GPU` | offload succeeded |
| `load_tensors: offloading ... to GPU` | partial offload |
| `failed to allocate Vulkan buffer` | OOM |
| `error loading model` | wrong/corrupt GGUF |
| `unknown model architecture` | llama.cpp too old |
| `port 9931 is already in use` | stale instance |
| `HTTP server listening` | ready to serve (health should be 200) |

## Health probe

```rust
pub async fn health(&self) -> LlmHealth {
    let url = format!("{}/health", self.base_url());
    match self.client.get(url).timeout(Duration::from_secs(2)).send().await {
        Ok(resp) if resp.status().is_success() => LlmHealth::Ready,
        Ok(resp) if resp.status().as_u16() == 503 => LlmHealth::Loading,
        Ok(_) => LlmHealth::Down,
        Err(_) => LlmHealth::Down,
    }
}
```

`base_url` must use `127.0.0.1` when the configured host is `0.0.0.0` or `::`.

## Distinguishing "not yet" from "dead"

```rust
match self.health().await {
    LlmHealth::Down => {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(status)) => return Err(LlmError::Exited(status.code())),
                Ok(None) => { /* still starting */ }
                Err(e) => return Err(LlmError::Io(e)),
            }
        }
    }
    _ => {}
}
```

A connection refusal with a live child is normal during the first second.

## STDERR tail in errors

Include the last 20 lines, but redact:

- Home directory (`/home/<user>` → `~`).
- Anything that looks like an API key (`sk-...`, `Bearer ...`).
- Prompt text if it ever appears in a request log.

## Client-side timeouts

| Call | Timeout |
| --- | --- |
| health | 2 s |
| chat request (connect) | 5 s |
| chat stream idle | 120 s (first token can be slow) |
| stop/wait | 3–5 s |

For streaming, reqwest has no per-chunk timeout by default. Wrap the whole
stream in a watchdog: if no delta for 120 s, cancel and report.

## Log locations

- Console: `tracing_subscriber::fmt` with `RUST_LOG=voxlocal=debug`.
- File (M1): `data_dir()/voxlocal/logs/voxlocal.log`, 10 MB × 5.
- Child logs: ring buffer + `tracing` target `llama` / `whisper` / `tts`.

## Useful one-liners

```bash
RUST_LOG=voxlocal=debug,llama=debug cargo run 2>&1 | tee /tmp/voxlocal.log

# what did the engine say before dying?
grep -E 'error|failed|Vulkan|offloaded' /tmp/voxlocal.log | tail -40

# who is listening?
ss -ltnp | grep 9931

# who is using the GPU?
radeontop -d /dev/dri/renderD128
```

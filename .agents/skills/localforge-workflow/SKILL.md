---
name: localforge-workflow
description: >
  Use for day-to-day work on LocalForge: build/run/test commands, adding a
  workspace crate or dependency, model and binary layout, environment setup,
  formatting/linting gates, CI expectations, release packaging, and
  troubleshooting the development environment (ALSA, Vulkan, GPUI, Rust 2024).
license: MIT
compatibility: Rust 1.92+, Cargo, Linux/Windows/macOS
metadata:
  author: localforge
  version: "0.1.0"
---

# LocalForge development workflow

## Commands

```bash
cargo build                      # workspace
cargo run                        # app (GPUI window)
cargo fmt --all                  # format
cargo clippy --all-targets --all-features -- -D warnings
cd test && cargo test            # suite aislada (fuera del workspace y de git)
```

- Always `cargo fmt` before finishing a task.
- `clippy -D warnings` is a gate, not a suggestion.
- Tests live in `test/`, never as `#[cfg(test)]` inside `src/` or `crates/`.
- Tests must not require a GPU, microphone, model files, or network.
- Use `--release` for any latency measurement.

## Repository layout

```text
.
├── Cargo.toml              workspace + app package
├── src/
│   └── main.rs             GPUI bootstrap (window only)
├── crates/
│   ├── controller/         command loop + tokio runtime
│   ├── shared/             config, events, text, tools
│   ├── llm_engine/         llama-server (managed + external)
│   ├── stt/                whisper-cli            (stub, M2)
│   ├── tts/                Kokoro worker          (stub, M3)
│   ├── audio_engine/       cpal + VAD             (stub, M2/M4)
│   ├── scene3d/            local 3D engine        (stub)
│   └── ui/                 app/ (chat, call, studio, system, footer) + settings/
├── assets/
│   ├── bin/                local binaries         (ignored)
│   └── models/             model files            (ignored)
├── docs/
├── test/                   isolated suite         (ignored, own workspace)
│   ├── Cargo.toml
│   ├── tests/
│   └── docs/
├── tools/                  Kokoro worker/scripts  (planned, M3)
└── .agents/skills/
```

## Adding a crate

1. `mkdir -p crates/foo/src` and add `crates/foo/Cargo.toml` with
   `edition = "2024"` and `[lints] workspace = true`.
2. Add to `members` and `[workspace.dependencies]` in the root manifest.
3. Use `foo = { workspace = true }` from consumers.
4. Keep the dependency direction: `foo -> shared` only (unless it is `main`).
5. Add a `#![doc = "..."]` module doc and its tests in `test/tests/`, not in the
   crate.

## Adding a dependency

Before adding:

- Does it require CUDA/ROCm/PyTorch? Reject.
- Is it maintained and cross-platform? Check last release and license.
- Is it already transitively available via GPUI Kit? Do not duplicate `gpui`.
- Does it need a system library? Document it in the README and CI.
- Pin pre-release versions exactly (`=2.0.0-rc.15`), never `^`.

Then run `cargo tree -d` and avoid duplicate majors when practical.

## Model and binary resolution

Priority:

1. Absolute path in config.
2. `assets/bin/<name>` next to the executable.
3. `PATH`.

Never assume the current working directory; when running from a desktop
launcher it is `/`. Resolve relative paths against the executable directory or
the config file's directory, and document the choice.

## Environment checklist

```bash
rustc --version                  # 1.92+
cargo --version
pkg-config --modversion alsa     # Linux audio
vulkaninfo --summary             # GPU
llama-server --list-devices      # Vulkan in ggml
whisper-cli --help
python3 --version                # only for the TTS worker
```

Install GPUI Linux deps (Ubuntu):

```bash
sudo apt install -y gcc g++ clang libfontconfig-dev libwayland-dev \
  libwebkit2gtk-4.1-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev \
  libzstd-dev libasound2-dev vulkan-validationlayers libvulkan1
```

## Commit and PR hygiene

- One logical change per commit; imperative subject (`stt: parse plain output`).
- Update docs/skills when a public contract changes.
- Include the commands you ran in the PR description.
- Never commit models, `.venv-kokoro/`, logs, `target/` or `test/` (the test
  suite is local by design).
- Generated code goes in `target/` or is checked in with a header saying how
  to regenerate it.

## CI expectations

```yaml
- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo build --release   # Linux
```

`cargo test --workspace` runs no tests: the suite lives in the ignored `test/`
crate. Run it locally with `cd test && cargo test`. If CI ever needs tests,
version a mirror of that crate or rebuild it in the workflow — do not silently
assume the gate is covered.

Add a smoke job that runs `llama-server --list-devices` and asserts the output
contains `Vulkan0` on a self-hosted GPU runner (optional, not on public CI).

## Release

- Build on each target OS; do not cross-compile GPUI.
- Bundle: binary, `tools/kokoro_worker.py` (once M3 exists), default config
  template, licenses.
- Do not bundle models by default; offer a first-run downloader with SHA-256.
- Linux: `.deb`/AppImage + desktop entry; Windows: ZIP/Inno Setup; macOS:
  `.app` in a DMG with microphone usage strings.
- Record the llama.cpp/whisper.cpp commits used for the Vulkan builds.

## Troubleshooting the dev environment

| Symptom | Fix |
| --- | --- |
| `alsa-sys` build fails | install `libasound2-dev`/`alsa-lib` |
| `failed to open window` | run inside Wayland/X11; install xkbcommon/wayland |
| `no such command: clippy` | `rustup component add clippy` |
| `edition2024` unsupported | update Rust (1.92+) |
| GPUI link errors | install `libfontconfig-dev`, `libzstd-dev`, `libssl-dev` |
| Wayland crash on NVIDIA | try `WAYLAND_DISPLAY=` (X11) and check the driver |
| `cargo` picks old lockfile | `cargo update -p <crate>`, commit `Cargo.lock` |

## Definition of done

- [ ] `cargo fmt`, `clippy -D warnings` and `cd test && cargo test` green.
- [ ] No new CUDA/ROCm/PyTorch dependency.
- [ ] Config change is backward compatible.
- [ ] Error path leaves the app usable.
- [ ] No process orphan after close.
- [ ] Docs/skills updated if contracts changed.

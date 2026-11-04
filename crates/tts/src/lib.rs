//! Text-to-speech wrapper (Kokoro ONNX on CPU).
//!
//! **Not implemented yet (milestone M3).** This crate is the placeholder for
//! the contract described in `docs/ARCHITECTURE.md` (ADR-003, ADR-004):
//!
//! - `Synthesizer::synthesize(&str) -> Result<AudioBuffer, TtsError>`, with
//!   `AudioBuffer` carrying 24 kHz mono `f32` samples.
//! - `KokoroCli` writes the text to the worker's stdin (never argv) and reads
//!   the WAV path from stdout.
//! - `ExternalCommand` splits a user-provided command template and substitutes
//!   `{out}`, `{voice}` and `{speed}`.
//! - Text must be cleaned with `shared::text::for_speech` before synthesis.
//!
//! ONNX Runtime runs on CPU; do not enable CUDA/ROCm execution providers.

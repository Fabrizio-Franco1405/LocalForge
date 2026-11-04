//! Speech-to-text wrapper for `whisper-cli`.
//!
//! **Not implemented yet (milestone M2).** This crate is the placeholder for
//! the contract described in `docs/ARCHITECTURE.md`:
//!
//! - Encode 16 kHz mono `f32` samples as a PCM16 WAV in the temp directory.
//! - Spawn `whisper-cli -m <model> -f <wav> -l <lang> -t <n> -np -nt`.
//! - Parse stdout tolerantly (timestamps or plain text) and drop markers such
//!   as `[BLANK_AUDIO]`.
//! - Return the last stderr lines when the process exits non-zero.
//!
//! The implementation must stay CPU/Vulkan only: never link CUDA or ROCm.

//! Microphone capture, voice activity detection and playback.
//!
//! **Not implemented yet (milestones M2 and M4).** This crate is the
//! placeholder for the contract described in `docs/ARCHITECTURE.md`:
//!
//! - `AudioCapture` exposes 16 kHz mono `f32` frames regardless of the device
//!   format (downmix + resample outside the cpal callback).
//! - `EndpointDetector<V: VoiceActivity>` implements the 20 ms frame state
//!   machine with pre-roll, minimum silence and a 30 s cap.
//! - `AudioPlayer` resamples any input rate to the output device rate and
//!   supports `stop()` for barge-in.
//! - `LinearResampler` is pure and unit-testable without a device.
//!
//! cpal 0.18 uses `StreamTrait::start()`; the callback must not allocate, lock
//! or log.

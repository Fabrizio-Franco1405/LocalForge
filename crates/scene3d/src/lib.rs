//! Local 3D generation engine (placeholder).
//!
//! **Not implemented yet.** This crate reserves the third motor of LocalForge:
//! local scene/mesh generation (for example a ComfyUI graph, a Blender CLI or a
//! TRELLIS-style pipeline). The contract will mirror the other engines:
//!
//! - `Scene3dEngine::start()`, `stop()`, `health()` and `generate(prompt)`.
//! - A `Scene3dConfig` owned by `shared` (already present) with backend,
//!   endpoint/command, model directory and extra arguments.
//! - No CUDA/ROCm requirement: Vulkan, CPU or an HTTP backend on loopback.
//!
//! The UI ships the section disabled ("Próximamente") until this crate has a
//! real implementation.

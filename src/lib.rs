//! VoxLocal application library.
//!
//! The binary ([`main`](../main.rs)) is a thin GPUI shell: it loads the
//! configuration, starts the controller and opens the window. Everything that
//! can be exercised without a window lives here, so the external test crate in
//! `test/` can drive the controller through its public API.
//!
//! Boundary: the UI never imports this module; it only sends
//! [`shared::EngineCommand`] and reads [`shared::EngineEvent`] values.

pub mod controller;

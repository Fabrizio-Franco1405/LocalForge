//! Helpers for the isolated LocalForge test suite.
//!
//! This crate lives outside the application workspace. It only uses the public
//! API of `shared`, `llm_engine`, `ui` and the `localforge` library, so it can be
//! versioned or discarded independently of the app.

pub mod mock_server;

//! VVW Core — platform-independent types, math, and data structures
//!
//! This crate contains everything that can be shared between the desktop app,
//! WASM web player, and backend server. No `std::fs`, no `dirs`, no `kira`,
//! no platform-specific code.

pub mod lighting;
pub mod maze;
pub mod mazegen;
pub mod project;
pub mod spatial;
pub mod tiles;

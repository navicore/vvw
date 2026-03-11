//! VVW Core — platform-independent types, math, and data structures
//!
//! This crate contains everything shared between the web player, deploy CLI,
//! and game plugin. No `std::fs`, no `dirs`, no platform-specific code.

pub mod audio;
pub mod lighting;
pub mod maze;
pub mod mazegen;
pub mod physics;
pub mod project;
pub mod spatial;
pub mod tiles;

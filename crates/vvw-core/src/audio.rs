//! Platform-independent audio trait
//!
//! [`TrackHandle`] abstracts over kira (native) and future web-audio (WASM)
//! backends so the game layer never depends on a specific audio engine.

/// Platform-independent trait for controlling a playing audio track.
/// Implemented by `GameTrack` (kira, native) and future web-audio (WASM).
pub trait TrackHandle: Send + Sync {
    fn set_volume(&mut self, amplitude: f32);
    fn set_panning(&mut self, pan: f32);
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
}

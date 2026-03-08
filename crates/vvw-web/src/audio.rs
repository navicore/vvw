//! Web Audio API engine: `AudioContext` + per-track gain/panner node chains

use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, GainNode, StereoPannerNode};

/// Per-track audio node chain: source(loop) -> gain -> panner -> destination
struct WebTrack {
    _source: AudioBufferSourceNode,
    gain_node: GainNode,
    panner_node: StereoPannerNode,
}

/// Manages the Web Audio API context and all track playback
pub struct WebAudioEngine {
    ctx: AudioContext,
    tracks: HashMap<usize, WebTrack>,
}

impl WebAudioEngine {
    /// Create a new engine. The `AudioContext` starts suspended (browser policy).
    pub fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        Ok(Self {
            ctx,
            tracks: HashMap::new(),
        })
    }

    /// Decode audio bytes and create a looping source -> gain -> panner -> destination chain
    pub async fn add_track(&mut self, id: usize, bytes: &[u8]) -> Result<(), JsValue> {
        let buffer = self.decode_audio(bytes).await?;
        let source = self.ctx.create_buffer_source()?;
        source.set_buffer(Some(&buffer));
        source.set_loop(true);

        let gain_node = self.ctx.create_gain()?;
        gain_node.gain().set_value(0.0);

        let panner_node = StereoPannerNode::new(&self.ctx)?;
        panner_node.pan().set_value(0.0);

        // Wire: source -> gain -> panner -> destination
        source.connect_with_audio_node(&gain_node)?;
        gain_node.connect_with_audio_node(&panner_node)?;
        panner_node.connect_with_audio_node(&self.ctx.destination())?;

        // Start the source (it won't produce sound until AudioContext is resumed)
        source.start()?;

        self.tracks.insert(
            id,
            WebTrack {
                _source: source,
                gain_node,
                panner_node,
            },
        );

        Ok(())
    }

    /// Set volume for a track (0.0 = silent, 1.0 = full)
    pub fn set_volume(&self, id: usize, amplitude: f32) {
        if let Some(track) = self.tracks.get(&id) {
            track.gain_node.gain().set_value(amplitude);
        }
    }

    /// Set stereo pan for a track (-1.0 = left, 0.0 = center, 1.0 = right)
    pub fn set_panning(&self, id: usize, pan: f32) {
        if let Some(track) = self.tracks.get(&id) {
            track.panner_node.pan().set_value(pan);
        }
    }

    /// Resume the `AudioContext` (must be called from a user gesture handler).
    /// Returns a promise that resolves when the context is running.
    pub fn resume(&self) -> Result<js_sys::Promise, JsValue> {
        self.ctx.resume()
    }

    async fn decode_audio(&self, bytes: &[u8]) -> Result<AudioBuffer, JsValue> {
        // Copy bytes into an ArrayBuffer
        let uint8_array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        uint8_array.copy_from(bytes);
        let array_buffer = uint8_array.buffer();

        let promise = self.ctx.decode_audio_data(&array_buffer)?;
        let result = JsFuture::from(promise).await?;
        result.dyn_into()
    }
}

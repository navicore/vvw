//! Web Audio API engine: streaming `<audio>` elements routed through gain/panner nodes
//!
//! Uses `MediaElementAudioSourceNode` so the browser streams and decodes audio
//! incrementally — no need to download entire files before playback begins.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, GainNode, HtmlAudioElement, StereoPannerNode};

/// Per-track audio node chain: `<audio>`(loop) -> media element source -> gain -> panner -> dest
struct WebTrack {
    audio_el: HtmlAudioElement,
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

    /// Create a streaming audio node chain for a track URL.
    /// The browser streams and decodes the file — no full download required.
    pub fn add_track(&mut self, id: usize, url: &str) -> Result<(), JsValue> {
        let audio_el = HtmlAudioElement::new_with_src(url)?;
        audio_el.set_loop(true);
        audio_el.set_preload("auto");
        // Allow streaming from same origin without CORS issues
        audio_el.set_cross_origin(None);

        let source = self.ctx.create_media_element_source(&audio_el)?;

        let gain_node = self.ctx.create_gain()?;
        gain_node.gain().set_value(0.0);

        let panner_node = StereoPannerNode::new(&self.ctx)?;
        panner_node.pan().set_value(0.0);

        // Wire: <audio> -> source -> gain -> panner -> destination
        source.connect_with_audio_node(&gain_node)?;
        gain_node.connect_with_audio_node(&panner_node)?;
        panner_node.connect_with_audio_node(&self.ctx.destination())?;

        self.tracks.insert(
            id,
            WebTrack {
                audio_el,
                gain_node,
                panner_node,
            },
        );

        Ok(())
    }

    /// Start playback on all tracks. Call after `AudioContext` is resumed.
    pub fn play_all(&self) {
        for track in self.tracks.values() {
            let _ = track.audio_el.play();
        }
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
}

//! Web Audio API engine: streaming `<audio>` elements routed through gain/panner nodes
//!
//! Uses `MediaElementAudioSourceNode` so the browser streams and decodes audio
//! incrementally — no need to download entire files before playback begins.
//!
//! Safari compatibility:
//! - Audio elements are created eagerly (to start buffering), but NOT connected
//!   to `createMediaElementSource()` until the user clicks. Safari throws
//!   `NotSupportedError` on `play()` for elements already captured by Web Audio.
//! - `StereoPannerNode` fallback for older Safari (pre-14.1).

use std::collections::HashMap;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioContextState, GainNode, HtmlAudioElement};

/// Panner abstraction: `StereoPannerNode` where supported, no-op otherwise.
enum Panner {
    Stereo(web_sys::StereoPannerNode),
    None,
}

impl Panner {
    fn set_pan(&self, value: f32) {
        if let Self::Stereo(node) = self {
            node.pan().set_value(value);
        }
    }
}

/// Per-track audio node chain (after activation):
/// `<audio>`(loop) -> media element source -> gain -> panner -> dest
struct WebTrack {
    gain_node: GainNode,
    panner: Panner,
}

/// A track that hasn't been wired into the Web Audio graph yet.
struct PendingTrack {
    id: usize,
    audio_el: HtmlAudioElement,
}

/// Manages the Web Audio API context and all track playback.
///
/// Stored as a Bevy `NonSend` resource — web-sys types are `!Send`.
pub struct WebAudioEngine {
    ctx: AudioContext,
    tracks: HashMap<usize, WebTrack>,
    /// Tracks waiting to be connected (before user gesture).
    pending: Vec<PendingTrack>,
}

impl WebAudioEngine {
    /// Create a new engine. The `AudioContext` starts suspended (browser policy).
    pub fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        Ok(Self {
            ctx,
            tracks: HashMap::new(),
            pending: Vec::new(),
        })
    }

    /// Register a track URL. Creates the `<audio>` element (starts buffering)
    /// but does NOT connect to Web Audio yet — that happens in `activate()`.
    pub fn add_track(&mut self, id: usize, url: &str) -> Result<(), JsValue> {
        let audio_el = HtmlAudioElement::new_with_src(url)?;
        audio_el.set_loop(true);
        audio_el.set_preload("auto");
        audio_el.set_cross_origin(Some("anonymous"));
        audio_el.set_attribute("playsinline", "")?;
        self.pending.push(PendingTrack { id, audio_el });
        Ok(())
    }

    /// Wire all pending tracks into the Web Audio graph and start playback.
    /// Must be called from within a user gesture (click handler).
    pub fn activate(&mut self) -> Result<(), JsValue> {
        for pending in self.pending.drain(..) {
            // Call play() BEFORE createMediaElementSource — Safari requirement
            if let Err(e) = pending.audio_el.play() {
                web_sys::console::error_1(
                    &format!("track {} play() failed: {e:?}", pending.id).into(),
                );
            }

            let source = self.ctx.create_media_element_source(&pending.audio_el)?;

            let gain_node = self.ctx.create_gain()?;
            gain_node.gain().set_value(0.0);

            let panner = if let Ok(panner_node) = web_sys::StereoPannerNode::new(&self.ctx) {
                panner_node.pan().set_value(0.0);
                source.connect_with_audio_node(&gain_node)?;
                gain_node.connect_with_audio_node(&panner_node)?;
                panner_node.connect_with_audio_node(&self.ctx.destination())?;
                Panner::Stereo(panner_node)
            } else {
                source.connect_with_audio_node(&gain_node)?;
                gain_node.connect_with_audio_node(&self.ctx.destination())?;
                Panner::None
            };

            self.tracks
                .insert(pending.id, WebTrack { gain_node, panner });
        }
        Ok(())
    }

    /// Clone the `AudioContext` reference for the overlay click handler.
    pub fn ctx(&self) -> AudioContext {
        self.ctx.clone()
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
            track.panner.set_pan(pan);
        }
    }

    /// Returns true if the `AudioContext` needs resuming (suspended, or iOS
    /// Safari's non-standard "interrupted" state). We check for "not running
    /// and not closed" rather than specifically `Suspended` so that unknown
    /// states (like `interrupted`) are also caught.
    pub fn needs_resume(&self) -> bool {
        let state = self.ctx.state();
        state != AudioContextState::Running && state != AudioContextState::Closed
    }

    /// Resume a suspended `AudioContext`. Must be called from a user gesture.
    pub fn resume(&self) {
        match self.ctx.resume() {
            Ok(promise) => {
                let on_err = Closure::once(move |e: JsValue| {
                    web_sys::console::error_1(
                        &format!("AudioContext resume rejected: {e:?}").into(),
                    );
                });
                let _ = promise.catch(&on_err);
                on_err.forget();
            }
            Err(e) => {
                web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
            }
        }
    }
}

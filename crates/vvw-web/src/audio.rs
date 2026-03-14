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
    audio_el: HtmlAudioElement,
    gain_node: GainNode,
    panner: Panner,
    /// True when intentionally paused to save bandwidth (not browser-suspended).
    paused_for_distance: bool,
    /// Seconds the track has been in the silent zone. Pause after debounce threshold.
    silent_secs: f32,
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
    /// Set after `activate()` — skip per-frame FFI state checks until then.
    activated: bool,
}

impl WebAudioEngine {
    /// Create a new engine. The `AudioContext` starts suspended (browser policy).
    pub fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        Ok(Self {
            ctx,
            tracks: HashMap::new(),
            pending: Vec::new(),
            activated: false,
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

            self.tracks.insert(
                pending.id,
                WebTrack {
                    audio_el: pending.audio_el,
                    gain_node,
                    panner,
                    paused_for_distance: false,
                    silent_secs: 0.0,
                },
            );
        }
        self.activated = true;
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

    /// Update streaming state for a track based on distance from the player.
    ///
    /// Tracks beyond `prefetch_distance` are paused after a debounce period to
    /// save bandwidth. Tracks within range are resumed immediately.
    /// The Web Audio graph stays wired — only the `<audio>` element pauses/plays.
    pub fn update_streaming(&mut self, id: usize, distance: f32, dt: f32) {
        /// Margin beyond `DEFAULT_MAX_DISTANCE` to start pre-fetching.
        const PREFETCH_MARGIN: f32 = 5.0;
        const PREFETCH_DISTANCE: f32 = vvw_core::spatial::DEFAULT_MAX_DISTANCE + PREFETCH_MARGIN;
        /// Seconds a track must stay beyond the threshold before pausing.
        const PAUSE_DEBOUNCE_SECS: f32 = 2.0;

        if !self.activated {
            return;
        }

        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };

        if distance < PREFETCH_DISTANCE {
            // Within range: resume immediately if paused for distance
            track.silent_secs = 0.0;
            if track.paused_for_distance {
                track.paused_for_distance = false;
                // NOTE: play() here runs outside a user gesture. Browsers
                // generally permit this on an already-activated element after
                // a script-initiated pause(), but this is not guaranteed by
                // the autoplay spec. If a platform rejects it, the error is
                // logged and the track stays silent until the next gesture.
                Self::play_with_rejection_handler(&track.audio_el, id);
            }
        } else {
            // Beyond range: accumulate silence time, pause after debounce
            track.silent_secs += dt;
            if !track.paused_for_distance && track.silent_secs >= PAUSE_DEBOUNCE_SECS {
                track.paused_for_distance = true;
                track.silent_secs = 0.0;
                track.audio_el.pause().ok();
            }
        }
    }

    /// Call `play()` on an audio element and handle the returned Promise rejection.
    /// Uses `spawn_local` to await the promise without leaking a closure.
    fn play_with_rejection_handler(audio_el: &HtmlAudioElement, id: usize) {
        match audio_el.play() {
            Ok(promise) => {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
                        web_sys::console::error_1(
                            &format!("track {id} play() rejected: {e:?}").into(),
                        );
                    }
                });
            }
            Err(e) => {
                web_sys::console::error_1(&format!("track {id} play() failed: {e:?}").into());
            }
        }
    }

    /// Returns true if audio needs resuming: either the `AudioContext` is
    /// suspended (or iOS Safari's "interrupted" state), or any `<audio>`
    /// elements have been paused by the browser (e.g. bfcache restore on Android).
    pub fn needs_resume(&self) -> bool {
        if !self.activated {
            return false;
        }
        let state = self.ctx.state();
        let ctx_suspended =
            state != AudioContextState::Running && state != AudioContextState::Closed;
        // A browser-paused element is one that's paused but NOT intentionally
        // paused for distance. However, if the AudioContext itself is suspended
        // (e.g. bfcache, tab suspend), always trigger resume — the context
        // suspension affects all tracks regardless of distance state.
        if ctx_suspended {
            return true;
        }
        self.tracks.values().any(|t| t.audio_el.paused())
    }

    /// Resume a suspended `AudioContext` and restart any paused `<audio>`
    /// elements. Must be called from a user gesture. Handles bfcache restore
    /// on Android where both the context and elements may have stopped.
    pub fn resume(&mut self) {
        // Re-play audio elements that stopped during backgrounding.
        // Skip tracks intentionally paused for distance — update_streaming
        // will resume them if the player is close enough.
        for (&id, track) in &mut self.tracks {
            if track.audio_el.paused() && !track.paused_for_distance {
                Self::play_with_rejection_handler(&track.audio_el, id);
            }
        }
        match self.ctx.resume() {
            Ok(promise) => {
                let on_err = Closure::once(move |e: JsValue| {
                    web_sys::console::error_1(
                        &format!("AudioContext resume rejected: {e:?}").into(),
                    );
                });
                let _ = promise.catch(&on_err);
                // NOTE: leaks ~few bytes of WASM linear memory per call.
                // Acceptable here — resume() only fires on rare user gestures.
                on_err.forget();
            }
            Err(e) => {
                web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
            }
        }
    }
}

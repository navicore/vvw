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

use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioContextState, GainNode, HtmlAudioElement, MediaElementAudioSourceNode,
};

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
    /// The `MediaElementAudioSourceNode` — kept so forks can connect directly
    /// to the raw audio signal, bypassing this track's gain node.
    source_node: Option<MediaElementAudioSourceNode>,
    gain_node: GainNode,
    panner: Panner,
    /// Original audio URL, stored so we can restore it after clearing src.
    url: String,
    /// True when intentionally paused to save bandwidth (not browser-suspended).
    paused_for_distance: bool,
    /// Seconds the track has been in the silent zone. Pause after debounce threshold.
    silent_secs: f32,
    /// True for pipe speaker entries — they share the source's `<audio>` element
    /// and must NOT call `play`/`pause`/`set_src`/`load` on it.
    is_fork: bool,
    /// For fork entries: the source track ID whose element feeds this fork's gain.
    source_id: Option<usize>,
}

/// A track that hasn't been wired into the Web Audio graph yet.
struct PendingTrack {
    id: usize,
    audio_el: HtmlAudioElement,
}

/// A fork request buffered until the engine is activated.
struct PendingFork {
    source_id: usize,
    new_id: usize,
}

/// Manages the Web Audio API context and all track playback.
///
/// Stored as a Bevy `NonSend` resource — web-sys types are `!Send`.
pub struct WebAudioEngine {
    ctx: AudioContext,
    tracks: HashMap<usize, WebTrack>,
    /// Tracks waiting to be connected (before user gesture).
    pending: Vec<PendingTrack>,
    /// Fork requests waiting for activation.
    pending_forks: Vec<PendingFork>,
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
            pending_forks: Vec::new(),
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

            let url = pending.audio_el.src();
            self.tracks.insert(
                pending.id,
                WebTrack {
                    audio_el: pending.audio_el,
                    source_node: Some(source),
                    gain_node,
                    panner,
                    url,
                    paused_for_distance: false,
                    silent_secs: 0.0,
                    is_fork: false,
                    source_id: None,
                },
            );
        }
        self.activated = true;

        // Drain any fork requests that arrived before activation
        let forks: Vec<_> = self.pending_forks.drain(..).collect();
        for pf in forks {
            if let Err(e) = self.fork_track(pf.source_id, pf.new_id) {
                web_sys::console::error_1(
                    &format!(
                        "Pending fork {} → {} failed: {e:?}",
                        pf.source_id, pf.new_id
                    )
                    .into(),
                );
            }
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

        // Check fork status before mutable borrow
        let is_fork = self.tracks.get(&id).is_some_and(|t| t.is_fork);
        if is_fork {
            return;
        }

        // Check if any fork references this source — if so, don't pause the
        // source even if the player is far from it. The fork needs the source's
        // `<audio>` element to keep playing to produce signal.
        let has_active_fork = self.tracks.values().any(|t| t.source_id == Some(id));

        let Some(track) = self.tracks.get_mut(&id) else {
            return;
        };

        if distance < PREFETCH_DISTANCE || has_active_fork {
            // Within range: resume immediately if paused for distance
            track.silent_secs = 0.0;
            if track.paused_for_distance {
                track.paused_for_distance = false;
                // Restore the src that was cleared when pausing, then play.
                // NOTE: play() here runs outside a user gesture. Browsers
                // generally permit this on an already-activated element after
                // a script-initiated pause(), but this is not guaranteed by
                // the autoplay spec. If a platform rejects it, the error is
                // logged and the track stays silent until the next gesture.
                track.audio_el.set_src(&track.url);
                track.audio_el.set_preload("auto");
                Self::play_with_rejection_handler(&track.audio_el, id);
            }
        } else {
            // Beyond range: accumulate silence time, pause after debounce
            track.silent_secs += dt;
            if !track.paused_for_distance && track.silent_secs >= PAUSE_DEBOUNCE_SECS {
                track.paused_for_distance = true;
                track.silent_secs = 0.0;
                // pause() alone doesn't stop the browser from downloading.
                // Clear the src to force the browser to drop the connection.
                track.audio_el.pause().ok();
                track.audio_el.set_preload("none");
                track.audio_el.set_src("");
                // Force the browser to act on the cleared src
                track.audio_el.load();
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

    /// Fork a source track's audio graph for a pipe speaker.
    ///
    /// Creates a new gain + panner chain connected to the source's
    /// `MediaElementAudioSourceNode` (via re-connecting from the source's
    /// gain node input). The fork entry has its own gain/pan controls but
    /// does NOT own the `<audio>` element.
    pub fn fork_track(&mut self, source_id: usize, new_id: usize) -> Result<(), JsValue> {
        if !self.activated {
            self.pending_forks.push(PendingFork { source_id, new_id });
            return Ok(());
        }

        // If the source is distance-paused, resume it — the fork needs signal.
        if let Some(source) = self.tracks.get_mut(&source_id)
            && source.paused_for_distance
        {
            source.paused_for_distance = false;
            source.silent_secs = 0.0;
            source.audio_el.set_src(&source.url);
            source.audio_el.set_preload("auto");
            Self::play_with_rejection_handler(&source.audio_el, source_id);
        }

        let source = self
            .tracks
            .get(&source_id)
            .ok_or("source track not found")?;
        let source_node = source
            .source_node
            .as_ref()
            .ok_or("source track has no MediaElementAudioSourceNode")?;

        // Create independent gain + panner for the fork
        let gain_node = self.ctx.create_gain()?;
        gain_node.gain().set_value(0.0);

        let panner = if let Ok(panner_node) = web_sys::StereoPannerNode::new(&self.ctx) {
            panner_node.pan().set_value(0.0);
            // Connect: source's MediaElementAudioSourceNode → fork gain → fork panner → dest
            // This taps the raw audio signal directly, so the fork's volume is
            // independent of the source track's gain node.
            source_node.connect_with_audio_node(&gain_node)?;
            gain_node.connect_with_audio_node(&panner_node)?;
            panner_node.connect_with_audio_node(&self.ctx.destination())?;
            Panner::Stereo(panner_node)
        } else {
            source_node.connect_with_audio_node(&gain_node)?;
            gain_node.connect_with_audio_node(&self.ctx.destination())?;
            Panner::None
        };

        // Fork shares the source's audio element but must never control it
        let audio_el = source.audio_el.clone();
        let url = source.url.clone();

        self.tracks.insert(
            new_id,
            WebTrack {
                audio_el,
                source_node: None,
                gain_node,
                panner,
                url,
                paused_for_distance: false,
                silent_secs: 0.0,
                is_fork: true,
                source_id: Some(source_id),
            },
        );

        web_sys::console::log_1(&format!("Forked track {source_id} → speaker {new_id}").into());
        Ok(())
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
        self.tracks
            .values()
            .any(|t| !t.is_fork && t.audio_el.paused() && !t.paused_for_distance)
    }

    /// Resume a suspended `AudioContext` and restart any paused `<audio>`
    /// elements. Must be called from a user gesture. Handles bfcache restore
    /// on Android where both the context and elements may have stopped.
    pub fn resume(&self) {
        // Collect tracks that need resuming (browser-paused, not distance-paused).
        // We collect IDs + element clones so we can replay them after ctx.resume().
        let to_resume: Vec<(usize, HtmlAudioElement)> = self
            .tracks
            .iter()
            .filter(|(_, t)| !t.is_fork && t.audio_el.paused() && !t.paused_for_distance)
            .map(|(&id, t)| (id, t.audio_el.clone()))
            .collect();

        // Resume AudioContext first, then play() elements in the async continuation.
        // iOS Safari ignores play() on elements connected to a suspended AudioContext.
        match self.ctx.resume() {
            Ok(promise) => {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
                        web_sys::console::error_1(
                            &format!("AudioContext resume rejected: {e:?}").into(),
                        );
                        return;
                    }
                    for (id, audio_el) in to_resume {
                        Self::play_with_rejection_handler(&audio_el, id);
                    }
                });
            }
            Err(e) => {
                web_sys::console::error_1(&format!("audio resume error: {e:?}").into());
            }
        }
    }
}

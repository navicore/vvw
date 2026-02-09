//! Kira-based audio engine wrapper
//!
//! Provides [`GameAudioManager`] for managing the audio thread and
//! [`GameTrack`] handles for per-track volume and panning control.

use std::io::Cursor;

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};

/// Errors from the audio engine
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("failed to initialize audio manager: {0}")]
    Init(String),
    #[error("failed to load audio data: {0}")]
    LoadAudio(String),
    #[error("failed to play sound: {0}")]
    PlaySound(String),
}

/// Wraps kira's `AudioManager`. May be `!Send` on some platforms,
/// so store as a `NonSend` Bevy resource.
pub struct GameAudioManager {
    manager: AudioManager<DefaultBackend>,
}

impl GameAudioManager {
    /// Create a new audio manager (starts kira's dedicated audio thread).
    pub fn new() -> Result<Self, AudioError> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|_| AudioError::Init("failed to create audio manager".into()))?;
        tracing::info!("Kira audio manager initialized");
        Ok(Self { manager })
    }

    /// Load audio from raw bytes and play as a looping sound.
    /// Returns a [`GameTrack`] handle for controlling volume and panning.
    /// The track starts silent; the game's spatial system fades it in.
    pub fn add_track(&mut self, audio_bytes: Vec<u8>) -> Result<GameTrack, AudioError> {
        let cursor = Cursor::new(audio_bytes);
        let sound_data = StaticSoundData::from_cursor(cursor)
            .map_err(|e| AudioError::LoadAudio(e.to_string()))?
            .loop_region(..)
            .volume(Decibels(-60.0)); // Start near-silent

        let handle = self
            .manager
            .play(sound_data)
            .map_err(|e| AudioError::PlaySound(e.to_string()))?;

        Ok(GameTrack { handle })
    }
}

/// Handle for a single playing audio track.
/// `Send + Sync` — safe to store in Bevy resources and components.
pub struct GameTrack {
    handle: StaticSoundHandle,
}

impl GameTrack {
    /// Set volume from linear amplitude (0.0 = silence, 1.0 = unity gain).
    /// Converts to decibels internally for kira.
    pub fn set_volume(&mut self, amplitude: f32) {
        let db = if amplitude <= 0.001 {
            Decibels(-60.0)
        } else {
            Decibels(20.0 * amplitude.log10())
        };
        self.handle.set_volume(db, Tween::default());
    }

    /// Set stereo panning (-1.0 = left, 0.0 = center, 1.0 = right).
    /// Matches kira's panning convention directly.
    pub fn set_panning(&mut self, pan: f32) {
        self.handle.set_panning(pan, Tween::default());
    }

    /// Pause playback — frees audio thread resources while preserving state.
    pub fn pause(&mut self) {
        self.handle.pause(Tween::default());
    }

    /// Resume playback after a pause.
    pub fn resume(&mut self) {
        self.handle.resume(Tween::default());
    }

    /// Stop playback of this track.
    pub fn stop(&mut self) {
        self.handle.stop(Tween::default());
    }
}

impl vvw_core::audio::TrackHandle for GameTrack {
    fn set_volume(&mut self, amplitude: f32) {
        self.set_volume(amplitude);
    }

    fn set_panning(&mut self, pan: f32) {
        self.set_panning(pan);
    }

    fn pause(&mut self) {
        self.pause();
    }

    fn resume(&mut self) {
        self.resume();
    }

    fn stop(&mut self) {
        self.stop();
    }
}

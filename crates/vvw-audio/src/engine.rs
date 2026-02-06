use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::comms::{AudioChannels, AudioCommand, AudioEvent};
use crate::sampler::LoopingSampler;
use crate::types::SampleRate;

/// Audio engine configuration
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: SampleRate,
    pub block_size: usize,
}

/// A single audio track with a looping sampler and gain
pub struct Track {
    pub sampler: LoopingSampler,
    pub gain: f32,
}

/// Errors from the audio engine
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no output device found")]
    NoOutputDevice,
    #[error("no supported output config: {0}")]
    NoSupportedConfig(String),
    #[error("failed to build stream: {0}")]
    BuildStream(String),
    #[error("failed to start stream: {0}")]
    PlayStream(String),
}

/// The audio engine manages the cpal output stream
pub struct AudioEngine {
    pub config: AudioConfig,
    /// Kept alive to prevent the stream from being dropped
    _stream: Option<Stream>,
}

impl AudioEngine {
    /// Start the audio engine with the given tracks and communication channels.
    pub fn start(tracks: Vec<Track>, channels: AudioChannels) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| AudioError::NoSupportedConfig(e.to_string()))?;

        let sample_rate = supported_config.sample_rate().0;
        let channel_count = supported_config.channels() as usize;
        let sample_format = supported_config.sample_format();

        tracing::info!(
            "Audio device: {:?}, sample_rate={sample_rate}, channels={channel_count}, format={sample_format:?}",
            device.name().unwrap_or_default()
        );

        let config: cpal::StreamConfig = supported_config.into();
        let block_size = 512;

        // Send initialization event before moving channels into the closure
        let mut event_tx = channels.event_tx;
        let _ = event_tx.push(AudioEvent::EngineInitialized { sample_rate });

        let mut command_rx = channels.command_rx;
        let mut tracks = tracks;
        let mut is_running = false;

        // Pre-allocate scratch buffers
        let mut scratch_left = vec![0.0_f32; block_size];
        let mut scratch_right = vec![0.0_f32; block_size];

        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Process commands (non-blocking)
                    while let Ok(cmd) = command_rx.pop() {
                        match cmd {
                            AudioCommand::SetTrackGain { track_id, gain } => {
                                if let Some(track) = tracks.get_mut(track_id) {
                                    track.gain = gain;
                                }
                            }
                            AudioCommand::Start => {
                                is_running = true;
                                let _ = event_tx.push(AudioEvent::Started);
                            }
                            AudioCommand::Stop => {
                                is_running = false;
                                let _ = event_tx.push(AudioEvent::Stopped);
                            }
                        }
                    }

                    let num_frames = output.len() / channel_count;

                    if !is_running || tracks.is_empty() {
                        output.fill(0.0);
                        return;
                    }

                    // Ensure scratch buffers are large enough
                    if scratch_left.len() < num_frames {
                        scratch_left.resize(num_frames, 0.0);
                        scratch_right.resize(num_frames, 0.0);
                    }

                    // Clear output
                    output.fill(0.0);

                    // Mix all tracks
                    for track in &mut tracks {
                        track
                            .sampler
                            .generate(&mut scratch_left, &mut scratch_right, num_frames);

                        let gain = track.gain;
                        for i in 0..num_frames {
                            let base = i * channel_count;
                            // Left channel
                            if base < output.len() {
                                output[base] += scratch_left[i] * gain;
                            }
                            // Right channel
                            if base + 1 < output.len() {
                                output[base + 1] += scratch_right[i] * gain;
                            }
                        }
                    }
                },
                move |err| {
                    tracing::error!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| AudioError::BuildStream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::PlayStream(e.to_string()))?;

        Ok(Self {
            config: AudioConfig {
                sample_rate,
                block_size,
            },
            _stream: Some(stream),
        })
    }
}

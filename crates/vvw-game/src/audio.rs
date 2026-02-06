use std::f32::consts::TAU;
use std::sync::Mutex;

use bevy::prelude::*;
use vvw_audio::{
    create_channels, AudioCommand, AudioEngine, AudioEvent, LoopingSampler, Track,
};

use crate::maze::{Maze, TrackIcon};
use crate::player::{Player, PlayerMovement};
use crate::tiles::TilePos;

/// Resource wrapping the command sender (Mutex for Sync)
#[derive(Resource)]
pub struct AudioCommandSender(Mutex<rtrb::Producer<AudioCommand>>);

/// Resource wrapping the event receiver (Mutex for Sync)
#[derive(Resource)]
pub struct AudioEventReceiver(Mutex<rtrb::Consumer<AudioEvent>>);

/// Audio plugin that sets up audio engine integration with Bevy
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, setup_audio)
            .add_systems(Update, (update_track_gains, poll_audio_events));
    }
}

/// Generate a sine wave as interleaved stereo data at the given frequency
fn generate_sine(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let num_frames = (sample_rate as f32 * duration_secs) as usize;
    let mut data = Vec::with_capacity(num_frames * 2);
    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let sample = (TAU * frequency * t).sin() * 0.3; // 0.3 amplitude to avoid clipping
        data.push(sample); // left
        data.push(sample); // right
    }
    data
}

fn setup_audio(world: &mut World) {
    let track_positions = world.resource::<Maze>().find_track_icons();
    let num_tracks = track_positions.len();

    if num_tracks == 0 {
        tracing::warn!("No track icons found in maze, skipping audio setup");
        return;
    }

    // Use 44100 for sine generation; the engine will use the device's actual rate
    // but sine waves are simple enough that resampling artifacts are negligible
    let gen_sample_rate = 44100;
    let duration = 2.0; // 2 second loops

    // Frequencies for a C major chord: C4, E4, G4
    let frequencies = [261.63, 329.63, 392.00];

    let tracks: Vec<Track> = (0..num_tracks)
        .map(|i| {
            let freq = frequencies[i % frequencies.len()];
            let data = generate_sine(freq, gen_sample_rate, duration);
            tracing::info!(
                "Track {i}: {freq}Hz sine at position ({}, {})",
                track_positions[i].x,
                track_positions[i].y
            );
            Track {
                sampler: LoopingSampler::new(data),
                gain: 0.0, // Start silent
            }
        })
        .collect();

    let (ui_channels, audio_channels) = create_channels(256);

    // Send start command before handing off channels
    let mut command_tx = ui_channels.command_tx;
    let _ = command_tx.push(AudioCommand::Start);

    match AudioEngine::start(tracks, audio_channels) {
        Ok(engine) => {
            tracing::info!("Audio engine started successfully");
            world.insert_resource(AudioCommandSender(Mutex::new(command_tx)));
            world.insert_resource(AudioEventReceiver(Mutex::new(ui_channels.event_rx)));
            // Keep the engine alive for the lifetime of the app using insert_non_send_resource
            world.insert_non_send_resource(AudioEngineHolder(engine));
        }
        Err(e) => {
            tracing::error!("Failed to start audio engine: {e}");
        }
    }
}

/// Non-send resource to keep the audio engine (and its cpal stream) alive.
/// `cpal::Stream` is not `Send`+`Sync` so we must use `NonSend` via `World`.
struct AudioEngineHolder(#[allow(dead_code)] AudioEngine);

#[allow(clippy::needless_pass_by_value)]
fn update_track_gains(
    sender: Option<Res<AudioCommandSender>>,
    player_query: Query<&PlayerMovement, With<Player>>,
    track_query: Query<(&TrackIcon, &TilePos)>,
) {
    let Some(sender) = sender else { return };
    let Ok(movement) = player_query.single() else {
        return;
    };

    let Ok(mut tx) = sender.0.lock() else { return };

    for (track_icon, tile_pos) in &track_query {
        let distance = movement.tile_pos.distance(*tile_pos);
        let gain = (1.0 - distance / 10.0).max(0.0);

        let _ = tx.push(AudioCommand::SetTrackGain {
            track_id: track_icon.track_id,
            gain,
        });
    }
}

#[allow(clippy::needless_pass_by_value)]
fn poll_audio_events(receiver: Option<Res<AudioEventReceiver>>) {
    let Some(receiver) = receiver else { return };
    let Ok(mut rx) = receiver.0.lock() else { return };

    while let Ok(event) = rx.pop() {
        match &event {
            AudioEvent::Started => tracing::info!("Audio: playback started"),
            AudioEvent::Stopped => tracing::info!("Audio: playback stopped"),
            AudioEvent::EngineInitialized { sample_rate } => {
                tracing::info!("Audio: engine initialized at {sample_rate}Hz");
            }
            AudioEvent::Error(msg) => tracing::error!("Audio error: {msg}"),
        }
    }
}

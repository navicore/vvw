use crate::types::SampleRate;

/// Commands sent from the game thread to the audio thread
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Set the gain for a specific track
    SetTrackGain { track_id: usize, gain: f32 },
    /// Start audio playback
    Start,
    /// Stop audio playback
    Stop,
}

/// Events sent from the audio thread back to the game thread
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Audio playback has started
    Started,
    /// Audio playback has stopped
    Stopped,
    /// Engine initialized with the negotiated sample rate
    EngineInitialized { sample_rate: SampleRate },
    /// An error occurred in the audio engine
    Error(String),
}

/// Channel endpoints held by the UI/game thread
pub struct UiChannels {
    /// Send commands to the audio thread
    pub command_tx: rtrb::Producer<AudioCommand>,
    /// Receive events from the audio thread
    pub event_rx: rtrb::Consumer<AudioEvent>,
}

/// Channel endpoints held by the audio thread
pub struct AudioChannels {
    /// Receive commands from the game thread
    pub command_rx: rtrb::Consumer<AudioCommand>,
    /// Send events back to the game thread
    pub event_tx: rtrb::Producer<AudioEvent>,
}

/// Create a pair of channel bundles for game<->audio communication
pub fn create_channels(capacity: usize) -> (UiChannels, AudioChannels) {
    let (command_tx, command_rx) = rtrb::RingBuffer::new(capacity);
    let (event_tx, event_rx) = rtrb::RingBuffer::new(capacity);

    (
        UiChannels {
            command_tx,
            event_rx,
        },
        AudioChannels {
            command_rx,
            event_tx,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_command() {
        let (mut ui, mut audio) = create_channels(16);
        ui.command_tx
            .push(AudioCommand::SetTrackGain {
                track_id: 0,
                gain: 0.5,
            })
            .unwrap();
        let cmd = audio.command_rx.pop().unwrap();
        match cmd {
            AudioCommand::SetTrackGain { track_id, gain } => {
                assert_eq!(track_id, 0);
                assert!((gain - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn roundtrip_event() {
        let (mut ui, mut audio) = create_channels(16);
        audio
            .event_tx
            .push(AudioEvent::EngineInitialized { sample_rate: 44100 })
            .unwrap();
        let evt = ui.event_rx.pop().unwrap();
        match evt {
            AudioEvent::EngineInitialized { sample_rate } => assert_eq!(sample_rate, 44100),
            _ => panic!("unexpected event"),
        }
    }
}

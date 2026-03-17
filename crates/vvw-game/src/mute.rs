//! Mute mode — silences all audio without affecting spatial audio computation
//!
//! Registers a "Mute" mode via the interaction modes framework. The game layer
//! has no systems — the platform layer reads `ActiveMode` and zeros gains when
//! the mute mode is active.

use bevy::prelude::*;

use vvw_core::modes::{ModeDescriptor, ModeId};

use crate::modes::ModeRegistry;

pub const MUTE_MODE_ID: &str = "mute";

pub struct MutePlugin;

impl Plugin for MutePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mute_mode);
    }
}

fn register_mute_mode(mut registry: ResMut<ModeRegistry>) {
    registry.register(ModeDescriptor {
        id: ModeId(MUTE_MODE_ID.into()),
        label: "Mute".into(),
        suppresses_movement: false,
        order: 0,
    });
}

//! VVW - Visual Virtual World
//!
//! An audio exploration game where you navigate mazes to discover music.

use bevy::prelude::*;
use clap::Parser;
use vvw_game::VvwGamePlugin;

/// VVW - Visual Virtual World audio exploration game
#[derive(Parser, Debug)]
#[command(name = "vvw")]
#[command(about = "Navigate mazes to discover and experience spatial audio")]
struct Args {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    let args = Args::parse();

    // Initialize logging
    let log_filter = if args.debug {
        "debug,wgpu=warn,naga=warn,bevy_render=info"
    } else {
        "info,wgpu=warn,naga=warn"
    };

    tracing_subscriber::fmt().with_env_filter(log_filter).init();

    tracing::info!("Starting VVW - Visual Virtual World");

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "VVW - Visual Virtual World".into(),
                        resolution: (800, 600).into(),
                        ..default()
                    }),
                    ..default()
                })
                .disable::<bevy::log::LogPlugin>() // We initialize tracing manually
                .disable::<bevy::audio::AudioPlugin>(), // We use kira directly
        )
        .add_plugins(VvwGamePlugin)
        .run();
}

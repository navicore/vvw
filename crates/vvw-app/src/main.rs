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
                .disable::<bevy::log::LogPlugin>(), // We initialize tracing manually
        )
        .add_plugins(VvwGamePlugin)
        .add_systems(Startup, setup_camera)
        .run();
}

fn setup_camera(mut commands: Commands) {
    // Set a dark background color
    commands.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.1)));

    // 2D camera centered on the maze
    // The maze is 15x11 tiles at 32px each = 480x352
    // Tile positions start at (16, 16) for tile (0,0) so center is at (240+16, 176+16) = (256, 192)
    let maze_center_x = 15.0 * 32.0 / 2.0 + 16.0;
    let maze_center_y = 11.0 * 32.0 / 2.0 + 16.0;

    commands.spawn((
        Camera2d,
        Transform::from_xyz(maze_center_x, maze_center_y, 0.0),
    ));
}

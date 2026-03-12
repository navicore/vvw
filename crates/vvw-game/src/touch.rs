//! Touch controls — D-pad overlay for mobile browsers

use avian2d::prelude::*;
use bevy::input::touch::Touches;
use bevy::prelude::*;

use vvw_core::lighting::{LightMode, LightingConfig};

use crate::player::{
    Player, PlayerHeading, PlayerMovement, ROTATION_SPEED, handle_player_input, sync_player_light,
};

/// Plugin for touch-based movement controls
pub struct TouchControlsPlugin;

impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchState>()
            .add_systems(Startup, spawn_dpad_overlay)
            .add_systems(
                Update,
                (
                    detect_touch_device,
                    handle_touch_input
                        .after(handle_player_input)
                        .before(sync_player_light),
                    update_dpad_visuals,
                )
                    .chain(),
            );
    }
}

/// Tracks whether a touch device has been detected
#[derive(Resource, Default)]
struct TouchState {
    touch_detected: bool,
}

/// Marker for the D-pad overlay container
#[derive(Component)]
struct DPadOverlay;

/// D-pad button with its movement direction
#[derive(Component)]
struct DPadButton {
    direction: Vec2,
}

const DPAD_SIZE: f32 = 150.0;
const BUTTON_SIZE: f32 = 48.0;
/// Extra bottom margin to clear phone browser navigation/gesture bars
const DPAD_MARGIN_BOTTOM: f32 = 80.0;
const DPAD_ALPHA: f32 = 0.06;
const DPAD_PRESSED_ALPHA: f32 = 0.15;

fn spawn_dpad_overlay(mut commands: Commands) {
    let container = commands
        .spawn((
            DPadOverlay,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-DPAD_SIZE / 2.0)),
                bottom: Val::Px(DPAD_MARGIN_BOTTOM),
                width: Val::Px(DPAD_SIZE),
                height: Val::Px(DPAD_SIZE),
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();

    // Up — top center
    spawn_dpad_button(
        &mut commands,
        container,
        Vec2::Y,
        dpad_node(
            Val::Px((DPAD_SIZE - BUTTON_SIZE) / 2.0),
            Val::Auto,
            Val::Auto,
            Val::Px(0.0),
        ),
    );
    // Down — bottom center
    spawn_dpad_button(
        &mut commands,
        container,
        Vec2::NEG_Y,
        dpad_node(
            Val::Px((DPAD_SIZE - BUTTON_SIZE) / 2.0),
            Val::Px(0.0),
            Val::Auto,
            Val::Auto,
        ),
    );
    // Left — middle left
    spawn_dpad_button(
        &mut commands,
        container,
        Vec2::NEG_X,
        dpad_node(
            Val::Px(0.0),
            Val::Px((DPAD_SIZE - BUTTON_SIZE) / 2.0),
            Val::Auto,
            Val::Auto,
        ),
    );
    // Right — middle right
    spawn_dpad_button(
        &mut commands,
        container,
        Vec2::X,
        dpad_node(
            Val::Auto,
            Val::Px((DPAD_SIZE - BUTTON_SIZE) / 2.0),
            Val::Px(0.0),
            Val::Auto,
        ),
    );
}

fn dpad_node(left: Val, bottom: Val, right: Val, top: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Px(BUTTON_SIZE),
        height: Val::Px(BUTTON_SIZE),
        left,
        bottom,
        right,
        top,
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

fn spawn_dpad_button(commands: &mut Commands, parent: Entity, direction: Vec2, node: Node) {
    let child = commands
        .spawn((
            DPadButton { direction },
            node,
            Interaction::None,
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, DPAD_ALPHA)),
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, DPAD_ALPHA * 0.5)),
        ))
        .id();
    commands.entity(parent).add_child(child);
}

/// Show the D-pad overlay once any touch event is detected
#[allow(clippy::needless_pass_by_value)]
fn detect_touch_device(
    touches: Res<Touches>,
    mut state: ResMut<TouchState>,
    mut query: Query<&mut Visibility, With<DPadOverlay>>,
) {
    if state.touch_detected {
        return;
    }
    if touches.iter().next().is_some() || touches.iter_just_released().next().is_some() {
        state.touch_detected = true;
        for mut vis in &mut query {
            *vis = Visibility::Inherited;
        }
    }
}

/// Read D-pad button interactions and apply velocity to the player.
/// Gated on `touch_detected` so mouse clicks on D-pad buttons don't
/// double-apply velocity alongside keyboard input on desktop.
#[allow(clippy::needless_pass_by_value)]
fn handle_touch_input(
    time: Res<Time>,
    state: Res<TouchState>,
    lighting: Res<LightingConfig>,
    button_query: Query<(&Interaction, &DPadButton)>,
    mut player_query: Query<
        (&PlayerMovement, &mut LinearVelocity, &mut PlayerHeading),
        With<Player>,
    >,
) {
    if !state.touch_detected {
        return;
    }

    let mut pressed = Vec2::ZERO;
    for (interaction, button) in &button_query {
        if *interaction == Interaction::Pressed {
            pressed += button.direction;
        }
    }

    let dt = time.delta_secs();
    let is_flashlight = lighting.player_light_mode == LightMode::Flashlight;

    for (movement, mut velocity, mut heading) in &mut player_query {
        if is_flashlight {
            // Left/Right rotate heading, Up/Down move along heading
            if pressed.x < 0.0 {
                let angle = ROTATION_SPEED * dt;
                heading.0 = Vec2::from_angle(angle).rotate(heading.0);
            }
            if pressed.x > 0.0 {
                let angle = -ROTATION_SPEED * dt;
                heading.0 = Vec2::from_angle(angle).rotate(heading.0);
            }
            // Prevent float drift from accumulating over long sessions
            heading.0 = heading.0.normalize_or_zero();

            let mut forward = 0.0;
            if pressed.y > 0.0 {
                forward += 1.0;
            }
            if pressed.y < 0.0 {
                forward -= 1.0;
            }

            velocity.0 += heading.0 * forward * movement.speed * dt;
        } else {
            // Lantern mode: cardinal direction movement
            let direction = if pressed == Vec2::ZERO {
                Vec2::ZERO
            } else {
                pressed.normalize()
            };
            velocity.0 += direction * movement.speed * dt;
        }
    }
}

/// Update button visuals based on interaction state
#[allow(clippy::needless_pass_by_value)]
fn update_dpad_visuals(
    mut query: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), With<DPadButton>>,
) {
    for (interaction, mut bg, mut border) in &mut query {
        let alpha = if *interaction == Interaction::Pressed {
            DPAD_PRESSED_ALPHA
        } else {
            DPAD_ALPHA
        };
        *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, alpha * 0.5));
        *border = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, alpha));
    }
}

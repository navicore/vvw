//! Interaction modes framework — shared mode registry, control surface, and gesture detection
//!
//! Features register modes at startup. The player toggles the control surface via
//! right-click (desktop) / two-finger tap (mobile) / Tab key. The control surface
//! lets the player cycle through modes and activate/deactivate them.
//!
//! Zero cost when no modes are registered — all systems are gated on a non-empty registry.

use bevy::input::touch::Touches;
use bevy::prelude::*;

pub use vvw_core::modes::{ModeDescriptor, ModeId};

// ── Resources ──────────────────────────────────────────────────────────────

/// Registry of available interaction modes. Populated at startup by feature plugins.
#[derive(Resource, Default)]
pub struct ModeRegistry {
    pub modes: Vec<ModeDescriptor>,
}

impl ModeRegistry {
    pub fn register(&mut self, desc: ModeDescriptor) {
        self.modes.push(desc);
    }

    pub fn is_empty(&self) -> bool {
        self.modes.is_empty()
    }

    /// Returns true if the given mode suppresses player movement.
    pub fn suppresses_movement(&self, id: &ModeId) -> bool {
        self.modes
            .iter()
            .find(|m| m.id == *id)
            .is_some_and(|m| m.suppresses_movement)
    }
}

/// Currently active mode. `None` = normal play.
#[derive(Resource, Default)]
pub struct ActiveMode(pub Option<ModeId>);

/// Whether the control surface UI is currently visible.
#[derive(Resource, Default)]
struct ControlSurfaceVisible(bool);

/// Which mode is selected (but not necessarily activated) in the cycle UI.
#[derive(Resource, Default)]
struct SelectedModeIndex(usize);

// ── Two-finger tap detection ───────────────────────────────────────────────

/// Tracks state for detecting two-finger taps (vs pinch/scroll gestures).
///
/// Simpler approach: record positions when exactly 2 fingers are down.
/// When all fingers lift, check if both stayed still and the duration was short.
#[derive(Resource, Default)]
struct TwoFingerTapState {
    /// Set when we first see exactly 2 fingers pressed
    tracking: bool,
    /// Set when gesture is rejected (e.g. 3+ fingers); prevents re-arming
    /// until all fingers lift.
    cancelled: bool,
    positions: [(u64, Vec2); 2],
    /// Last known position for each finger, updated when fingers lift across
    /// frames so movement checks work even after `get_pressed` returns None.
    last_positions: [Vec2; 2],
    start_time: f64,
    /// Cooldown after a successful tap to prevent re-triggering
    cooldown_until: f64,
}

/// Max finger movement (pixels) before we reject as a pinch/drag
const TAP_MOVE_THRESHOLD: f32 = 50.0;
/// Max duration (seconds) fingers can be held before we reject
const TAP_MAX_DURATION: f64 = 1.5;
/// Cooldown (seconds) after a successful tap before accepting new touches
const TAP_COOLDOWN: f64 = 0.5;

// ── UI markers ─────────────────────────────────────────────────────────────

#[derive(Component)]
struct ControlSurfaceOverlay;

#[derive(Component)]
struct CycleButton;

#[derive(Component)]
struct ActionButton;

#[derive(Component)]
struct ModeLabelText;

// ── Query type aliases ─────────────────────────────────────────────────────

type CycleButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BorderColor,
        &'static mut BackgroundColor,
    ),
    (With<CycleButton>, Without<ActionButton>),
>;

// ── Constants ──────────────────────────────────────────────────────────────

const SURFACE_MARGIN_BOTTOM: f32 = 80.0;
const SURFACE_MARGIN_RIGHT: f32 = 20.0;
const BUTTON_SIZE: f32 = 48.0;
const BUTTON_GAP: f32 = 8.0;
const SURFACE_ALPHA: f32 = 0.06;
const SURFACE_PRESSED_ALPHA: f32 = 0.15;
const INACTIVE_BORDER_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, SURFACE_ALPHA);

// ── Plugin ─────────────────────────────────────────────────────────────────

/// Framework plugin — always registered. Zero cost when no modes exist.
pub struct InteractionModesPlugin;

impl Plugin for InteractionModesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModeRegistry>()
            .init_resource::<ActiveMode>()
            .init_resource::<ControlSurfaceVisible>()
            .init_resource::<SelectedModeIndex>()
            .init_resource::<TwoFingerTapState>()
            .add_systems(PostStartup, spawn_control_surface)
            .add_systems(
                Update,
                (
                    detect_toggle_gesture,
                    detect_dismiss,
                    update_surface_visibility,
                    handle_cycle_click,
                    handle_action_click,
                    update_action_visual,
                )
                    .chain()
                    .run_if(registry_has_modes),
            );
    }
}

fn registry_has_modes(registry: Res<ModeRegistry>) -> bool {
    !registry.is_empty()
}

// ── Mock feature plugins ───────────────────────────────────────────────────

/// Mock feature 1 — registers a mode for testing the control surface.
/// Enable via `mock_feature1: true` in album config. Delete once a real
/// feature plugin exists.
pub struct MockFeature1Plugin;

impl Plugin for MockFeature1Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mock1).add_systems(
            Update,
            log_mode_changes.run_if(resource_changed::<ActiveMode>),
        );
    }
}

fn register_mock1(mut registry: ResMut<ModeRegistry>) {
    registry.register(ModeDescriptor {
        id: ModeId("mock_feature1".into()),
        label: "Mock 1".into(),
        suppresses_movement: false,
    });
}

/// Mock feature 2 — registers a second mode for testing mode cycling.
/// Enable via `mock_feature2: true` in album config.
pub struct MockFeature2Plugin;

impl Plugin for MockFeature2Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_mock2);
    }
}

fn register_mock2(mut registry: ResMut<ModeRegistry>) {
    registry.register(ModeDescriptor {
        id: ModeId("mock_feature2".into()),
        label: "Mock 2".into(),
        suppresses_movement: false,
    });
}

fn log_mode_changes(active: Res<ActiveMode>) {
    if let Some(id) = &active.0 {
        info!("Mode activated: {}", id.0);
    } else {
        info!("Mode deactivated");
    }
}

// ── Gesture detection ──────────────────────────────────────────────────────

/// Toggle control surface on Tab key or two-finger tap.
///
/// Takes `Option<ResMut<ActiveMode>>` via a helper to avoid borrowing
/// `ResMut` (and triggering change detection) on frames where we don't write.
fn detect_toggle_gesture(
    keyboard: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
    time: Res<Time>,
    mut tap_state: ResMut<TwoFingerTapState>,
    mut visible: ResMut<ControlSurfaceVisible>,
    active: Res<ActiveMode>,
    mut commands: Commands,
) {
    let toggled = keyboard.just_pressed(KeyCode::Tab)
        || detect_two_finger_tap(&touches, &time, &mut tap_state);

    if toggled {
        visible.0 = !visible.0;
        // Dismissing the surface clears any active mode
        if !visible.0 && active.0.is_some() {
            commands.insert_resource(ActiveMode(None));
        }
    }
}

/// Detect two-finger tap using a count-based approach.
///
/// Instead of tracking individual touch IDs through state transitions,
/// we simply observe: when exactly 2 fingers are down, record their
/// positions and start time. When all fingers lift, check if both
/// stayed still and the gesture was short. This naturally handles
/// both fingers arriving on the same frame.
fn detect_two_finger_tap(touches: &Touches, time: &Time, state: &mut TwoFingerTapState) -> bool {
    let now = time.elapsed_secs_f64();

    // Cooldown after a successful tap
    if now < state.cooldown_until {
        return false;
    }

    // Count currently pressed touches without allocating
    let count = touches.iter().count();

    // Clear cancelled state once all fingers lift
    if state.cancelled {
        if count == 0 {
            state.cancelled = false;
        }
        return false;
    }

    if count >= 2 && !state.tracking {
        // Two or more fingers just appeared — start tracking
        let mut iter = touches.iter();
        let first = iter.next().unwrap();
        let second = iter.next().unwrap();
        state.tracking = true;
        state.positions = [
            (first.id(), first.position()),
            (second.id(), second.position()),
        ];
        state.last_positions = [first.position(), second.position()];
        state.start_time = now;
        return false;
    }

    if state.tracking && count > 2 {
        // More fingers arrived — cancel and don't re-arm until all lift
        state.tracking = false;
        state.cancelled = true;
        return false;
    }

    if state.tracking && count == 2 {
        // Still holding — update last-known positions and check movement
        for (i, (id, start_pos)) in state.positions.iter().enumerate() {
            if let Some(touch) = touches.get_pressed(*id) {
                state.last_positions[i] = touch.position();
                if touch.position().distance(*start_pos) > TAP_MOVE_THRESHOLD {
                    state.tracking = false;
                    return false;
                }
            }
        }
        // Check timeout while fingers are still down
        if now - state.start_time > TAP_MAX_DURATION {
            state.tracking = false;
            return false;
        }
        return false;
    }

    if state.tracking && count <= 1 {
        // Update last_positions from just-released events (covers same-frame
        // lift where the count==2 hold branch never ran this frame)
        for (i, (id, _)) in state.positions.iter().enumerate() {
            for released in touches.iter_just_released() {
                if released.id() == *id {
                    state.last_positions[i] = released.position();
                }
            }
        }

        // Check final positions for both fingers against start positions
        for (i, (_, start_pos)) in state.positions.iter().enumerate() {
            if state.last_positions[i].distance(*start_pos) > TAP_MOVE_THRESHOLD {
                state.tracking = false;
                return false;
            }
        }

        // Accept if duration was short
        state.tracking = false;
        let duration = now - state.start_time;
        if duration <= TAP_MAX_DURATION {
            state.cooldown_until = now + TAP_COOLDOWN;
            return true;
        }
    }

    false
}

/// Escape key dismisses control surface and clears active mode.
fn detect_dismiss(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<ControlSurfaceVisible>,
    active: Res<ActiveMode>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(KeyCode::Escape) && visible.0 {
        visible.0 = false;
        if active.0.is_some() {
            commands.insert_resource(ActiveMode(None));
        }
    }
}

// ── Control surface UI ─────────────────────────────────────────────────────

fn spawn_control_surface(mut commands: Commands, registry: Res<ModeRegistry>) {
    if registry.is_empty() {
        return;
    }

    let initial_label = registry
        .modes
        .first()
        .map_or_else(String::new, |m| m.label.clone());

    let container = commands
        .spawn((
            ControlSurfaceOverlay,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(SURFACE_MARGIN_RIGHT),
                bottom: Val::Px(SURFACE_MARGIN_BOTTOM),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(BUTTON_GAP),
                ..default()
            },
            Visibility::Hidden,
            ZIndex(10),
        ))
        .id();

    // Mode label
    let label = commands
        .spawn((
            ModeLabelText,
            Text::new(initial_label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        ))
        .id();
    commands.entity(container).add_child(label);

    // Cycle button — only when 2+ modes registered
    if registry.modes.len() > 1 {
        let cycle = commands
            .spawn((
                CycleButton,
                mode_button_node(),
                Interaction::None,
                BorderColor::all(INACTIVE_BORDER_COLOR),
                BackgroundColor(Color::srgba(0.5, 0.5, 1.0, SURFACE_ALPHA * 0.5)),
            ))
            .id();
        commands.entity(container).add_child(cycle);
    }

    // Action button — green border when active, dim when not
    let action = commands
        .spawn((
            ActionButton,
            mode_button_node(),
            Interaction::None,
            BorderColor::all(INACTIVE_BORDER_COLOR),
            BackgroundColor(Color::NONE),
        ))
        .id();
    commands.entity(container).add_child(action);
}

fn mode_button_node() -> Node {
    Node {
        width: Val::Px(BUTTON_SIZE),
        height: Val::Px(BUTTON_SIZE),
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

/// Show/hide the control surface container when visibility changes.
fn update_surface_visibility(
    visible: Res<ControlSurfaceVisible>,
    mut query: Query<&mut Visibility, With<ControlSurfaceOverlay>>,
) {
    if !visible.is_changed() {
        return;
    }
    for mut vis in &mut query {
        *vis = if visible.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Cycle through registered modes on button press.
fn handle_cycle_click(
    registry: Res<ModeRegistry>,
    mut selected: ResMut<SelectedModeIndex>,
    cycle_query: Query<&Interaction, (Changed<Interaction>, With<CycleButton>)>,
    mut label_query: Query<&mut Text, With<ModeLabelText>>,
) {
    for interaction in &cycle_query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if registry.modes.is_empty() {
            return;
        }
        selected.0 = (selected.0 + 1) % registry.modes.len();
        let Some(mode) = registry.modes.get(selected.0) else {
            return;
        };
        for mut text in &mut label_query {
            (**text).clone_from(&mode.label);
        }
    }
}

/// Activate or deactivate the selected mode on action button press.
fn handle_action_click(
    registry: Res<ModeRegistry>,
    mut selected: ResMut<SelectedModeIndex>,
    mut active: ResMut<ActiveMode>,
    action_query: Query<&Interaction, (Changed<Interaction>, With<ActionButton>)>,
) {
    for interaction in &action_query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Clamp index if registry shrank
        if selected.0 >= registry.modes.len() {
            selected.0 = 0;
        }
        let Some(mode) = registry.modes.get(selected.0) else {
            return;
        };
        let target = &mode.id;
        if active.0.as_ref() == Some(target) {
            active.0 = None;
        } else {
            active.0 = Some(target.clone());
        }
    }
}

/// Update button visuals based on active state and interaction.
/// Only writes components when values actually change to avoid spurious
/// change-detection signals every frame.
fn update_action_visual(
    active: Res<ActiveMode>,
    registry: Res<ModeRegistry>,
    selected: Res<SelectedModeIndex>,
    mut action_query: Query<
        (&Interaction, &mut BorderColor, &mut BackgroundColor),
        With<ActionButton>,
    >,
    mut cycle_query: CycleButtonQuery,
) {
    // Update cycle button pressed alpha
    for (interaction, mut border, mut bg) in &mut cycle_query {
        let alpha = if *interaction == Interaction::Pressed {
            SURFACE_PRESSED_ALPHA
        } else {
            SURFACE_ALPHA
        };
        let new_bg = BackgroundColor(Color::srgba(0.5, 0.5, 1.0, alpha * 0.5));
        let new_border = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, alpha));
        if *bg != new_bg {
            *bg = new_bg;
        }
        if *border != new_border {
            *border = new_border;
        }
    }

    // Green border when the selected mode is active, dim when not
    let selected_is_active = registry
        .modes
        .get(selected.0)
        .is_some_and(|m| active.0.as_ref() == Some(&m.id));

    for (interaction, mut border, mut bg) in &mut action_query {
        let new_border = if selected_is_active {
            BorderColor::all(Color::srgba(0.2, 0.8, 0.3, 0.8))
        } else {
            let alpha = if *interaction == Interaction::Pressed {
                SURFACE_PRESSED_ALPHA
            } else {
                SURFACE_ALPHA
            };
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, alpha))
        };
        let new_bg = BackgroundColor(Color::NONE);
        if *border != new_border {
            *border = new_border;
        }
        if *bg != new_bg {
            *bg = new_bg;
        }
    }
}

//! 2D Lighting renderer
//!
//! Uses a dark overlay sprite with light sprites rendered on a separate layer.
//! Light sprites use radial gradients to simulate point lights.
//! The overlay dims the scene; lights brighten areas within their radius.

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::components::{AmbientLight2d, PointLight2d};

/// Cached radial gradient texture used by all light glow sprites
#[derive(Resource)]
struct LightGradientTexture(Handle<Image>);

/// Marker for the darkness overlay sprite
#[derive(Component)]
struct DarknessOverlay;

/// Marker for light glow sprites (children of `PointLight2d` entities)
#[derive(Component)]
struct LightGlow;

/// Z-layer for the darkness overlay (above scene, below UI)
const OVERLAY_Z: f32 = 90.0;
/// Z-layer for light sprites (above overlay)
const LIGHT_Z: f32 = 91.0;

/// Plugin that sets up the 2D lighting system
pub struct Lighting2dPlugin;

impl Plugin for Lighting2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AmbientLight2d>()
            .add_systems(PostStartup, setup_lighting)
            .add_systems(
                PostUpdate,
                (update_overlay, sync_light_sprites, spawn_missing_glow).chain(),
            );
    }
}

fn setup_lighting(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Spawn a large dark overlay — covers the entire visible area
    commands.spawn((
        Sprite {
            color: Color::srgba(0.0, 0.0, 0.0, 0.85),
            custom_size: Some(Vec2::splat(4000.0)), // Large enough to cover any view
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, OVERLAY_Z),
        DarknessOverlay,
    ));

    // Generate a 128×128 radial gradient image for light glow sprites
    let size: u32 = 128;
    let center = size as f32 / 2.0;
    let radius = center;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = dx.hypot(dy);
            let alpha = if dist < radius {
                (1.0 - dist / radius).powf(1.5)
            } else {
                0.0
            };
            let byte_alpha = (alpha * 255.0) as u8;
            data.extend_from_slice(&[255, 255, 255, byte_alpha]);
        }
    }
    let image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    let handle = images.add(image);
    commands.insert_resource(LightGradientTexture(handle));
}

/// Update the darkness overlay to follow the camera and match ambient brightness
#[allow(clippy::needless_pass_by_value, clippy::type_complexity)]
fn update_overlay(
    ambient: Res<AmbientLight2d>,
    camera_query: Query<&Transform, With<Camera2d>>,
    mut overlay_query: Query<
        (&mut Transform, &mut Sprite),
        (With<DarknessOverlay>, Without<Camera2d>),
    >,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok((mut overlay_transform, mut sprite)) = overlay_query.single_mut() else {
        return;
    };

    // Follow camera position
    overlay_transform.translation.x = camera_transform.translation.x;
    overlay_transform.translation.y = camera_transform.translation.y;
    overlay_transform.translation.z = OVERLAY_Z;

    // Alpha based on ambient brightness: lower brightness = more darkness
    let alpha = (1.0 - ambient.brightness).clamp(0.0, 1.0);
    let amb_color = ambient.color.to_srgba();
    sprite.color = Color::srgba(
        amb_color.red * 0.1,
        amb_color.green * 0.1,
        amb_color.blue * 0.1,
        alpha,
    );
}

/// Spawn glow sprites for any `PointLight2d` entities that don't have one yet.
///
/// The glow is added as a sibling (child of the `PointLight2d`'s parent) so that
/// it inherits the parent's world `Transform` directly — avoiding broken propagation
/// through the intermediate `PointLight2d` entity which may lack a `Transform`.
#[allow(clippy::needless_pass_by_value)]
fn spawn_missing_glow(
    mut commands: Commands,
    gradient: Res<LightGradientTexture>,
    light_query: Query<(Entity, &PointLight2d, &ChildOf), Without<LightGlow>>,
    glow_query: Query<(), With<LightGlow>>,
    children_query: Query<&Children>,
) {
    for (entity, light, child_of) in &light_query {
        let parent = child_of.parent();

        // Check if the parent already has a LightGlow child (sibling of this entity)
        let has_glow = children_query
            .get(parent)
            .is_ok_and(|children| children.iter().any(|c| glow_query.get(c).is_ok()));

        if has_glow {
            continue;
        }

        // Tag the PointLight2d entity so we can match glows to lights later
        commands.entity(entity).insert(LightGlow);

        let light_color = light.color.to_srgba();
        let glow_entity = commands
            .spawn((
                Sprite {
                    image: gradient.0.clone(),
                    color: Color::srgba(
                        light_color.red * light.intensity,
                        light_color.green * light.intensity,
                        light_color.blue * light.intensity,
                        0.35,
                    ),
                    custom_size: Some(Vec2::splat(light.radius * 2.0)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, LIGHT_Z),
                LightGlow,
            ))
            .id();
        commands.entity(parent).add_child(glow_entity);
    }
}

/// Keep light glow sprites in sync with their `PointLight2d` parameters.
///
/// Glows are siblings of the `PointLight2d` entity — both are children of the
/// same parent. We query the parent's children to find the glow sprite.
#[allow(clippy::needless_pass_by_value)]
fn sync_light_sprites(
    light_query: Query<(&PointLight2d, &ChildOf), Changed<PointLight2d>>,
    children_query: Query<&Children>,
    mut glow_query: Query<&mut Sprite, (With<LightGlow>, Without<PointLight2d>)>,
) {
    for (light, child_of) in &light_query {
        let Ok(siblings) = children_query.get(child_of.parent()) else {
            continue;
        };
        for sibling in siblings.iter() {
            if let Ok(mut sprite) = glow_query.get_mut(sibling) {
                let light_color = light.color.to_srgba();
                sprite.color = Color::srgba(
                    light_color.red * light.intensity,
                    light_color.green * light.intensity,
                    light_color.blue * light.intensity,
                    0.85,
                );
                sprite.custom_size = Some(Vec2::splat(light.radius * 2.0));
            }
        }
    }
}

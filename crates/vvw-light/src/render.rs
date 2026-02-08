//! 2D Lighting renderer — shadow-casting light map
//!
//! Renders a single overlay sprite whose texture is a per-tile light map.
//! Each texel's alpha controls darkness: lit tiles are transparent, dark tiles are opaque.
//! Brightness is computed via Bresenham raycasting against the `LightOccluderGrid`.
//! Bilinear texture filtering smooths the tile-resolution data on the GPU.

use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::components::{AmbientLight2d, LightOccluderGrid, PointLight2d};

/// Handle to the light map texture asset
#[derive(Resource)]
struct LightMapHandle(Handle<Image>);

/// Cached grid dimensions to detect resizes
#[derive(Resource, Default)]
struct LightMapDimensions {
    width: usize,
    height: usize,
}

/// Reusable per-tile brightness buffer to avoid per-frame allocation
#[derive(Resource, Default)]
struct BrightnessBuffer(Vec<f32>);

/// Marker for the single overlay sprite
#[derive(Component)]
struct LightMapOverlay;

/// Z-layer for the darkness overlay (above scene, below UI)
const OVERLAY_Z: f32 = 90.0;

/// Plugin that sets up the 2D lighting system
pub struct Lighting2dPlugin;

impl Plugin for Lighting2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AmbientLight2d>()
            .init_resource::<LightOccluderGrid>()
            .init_resource::<LightMapDimensions>()
            .init_resource::<BrightnessBuffer>()
            .add_systems(PostStartup, setup_lightmap)
            .add_systems(PostUpdate, (check_grid_resize, update_lightmap).chain());
    }
}

fn setup_lightmap(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Create a 1x1 placeholder image with bilinear filtering
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0, 0, 0, 255], // fully dark placeholder
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });

    let handle = images.add(image);
    commands.insert_resource(LightMapHandle(handle.clone()));

    commands.spawn((
        Sprite {
            image: handle,
            custom_size: Some(Vec2::splat(1.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, OVERLAY_Z),
        LightMapOverlay,
    ));
}

/// When the occluder grid dimensions change, recreate the image at the new resolution
/// and reposition the overlay sprite to cover the maze.
#[allow(clippy::needless_pass_by_value)]
fn check_grid_resize(
    grid: Res<LightOccluderGrid>,
    mut dims: ResMut<LightMapDimensions>,
    light_map: Res<LightMapHandle>,
    mut images: ResMut<Assets<Image>>,
    mut overlay_query: Query<(&mut Transform, &mut Sprite), With<LightMapOverlay>>,
) {
    if grid.width == dims.width && grid.height == dims.height {
        return;
    }

    let w = grid.width.max(1);
    let h = grid.height.max(1);
    dims.width = grid.width;
    dims.height = grid.height;

    // Recreate the image at grid resolution
    if let Some(image) = images.get_mut(&light_map.0) {
        let pixel_count = w * h;
        image.data = Some(vec![0u8; pixel_count * 4]);
        image.texture_descriptor.size = Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        };
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });
    }

    // Reposition and resize overlay to cover the maze
    let world_w = grid.width as f32 * grid.tile_size;
    let world_h = grid.height as f32 * grid.tile_size;
    if let Ok((mut transform, mut sprite)) = overlay_query.single_mut() {
        sprite.custom_size = Some(Vec2::new(world_w, world_h));
        transform.translation = Vec3::new(world_w / 2.0, world_h / 2.0, OVERLAY_Z);
    }
}

/// Compute per-tile brightness and write to the light map texture every frame.
#[allow(clippy::needless_pass_by_value, clippy::similar_names)]
fn update_lightmap(
    grid: Res<LightOccluderGrid>,
    ambient: Res<AmbientLight2d>,
    light_map: Res<LightMapHandle>,
    mut images: ResMut<Assets<Image>>,
    light_query: Query<(&PointLight2d, &ChildOf)>,
    transform_query: Query<&GlobalTransform>,
    mut buf: ResMut<BrightnessBuffer>,
) {
    let w = grid.width;
    let h = grid.height;
    if w == 0 || h == 0 {
        return;
    }

    let tile_size = grid.tile_size;
    if tile_size <= 0.0 {
        return;
    }

    // Reuse the brightness buffer; resize only when the grid changes
    let total = w * h;
    buf.0.resize(total, 0.0);
    buf.0.fill(ambient.brightness);
    let brightness = &mut buf.0;

    // Accumulate light contributions
    for (light, child_of) in &light_query {
        let parent = child_of.parent();
        let Ok(global_tf) = transform_query.get(parent) else {
            continue;
        };
        let world_pos = global_tf.translation();
        let light_tx = world_pos.x / tile_size;
        let light_ty = world_pos.y / tile_size;
        let radius_tiles = light.radius / tile_size;

        // Bounding box in tile coords (clamp as floats to avoid i32 overflow)
        let fmin_x = (light_tx - radius_tiles).floor().max(0.0);
        let fmax_x = (light_tx + radius_tiles).ceil().min((w - 1) as f32);
        let fmin_y = (light_ty - radius_tiles).floor().max(0.0);
        let fmax_y = (light_ty + radius_tiles).ceil().min((h - 1) as f32);

        if fmax_x < 0.0 || fmax_y < 0.0 || fmin_x >= w as f32 || fmin_y >= h as f32 {
            continue;
        }

        let min_x = fmin_x as usize;
        let max_x = fmax_x as usize;
        let min_y = fmin_y as usize;
        let max_y = fmax_y as usize;

        let light_ix = light_tx.round() as i32;
        let light_iy = light_ty.round() as i32;

        for ty in min_y..=max_y {
            for tx in min_x..=max_x {
                let dx = tx as f32 + 0.5 - light_tx;
                let dy = ty as f32 + 0.5 - light_ty;
                let dist = dx.hypot(dy);

                if dist > radius_tiles {
                    continue;
                }

                if !bresenham_visible(&grid, light_ix, light_iy, tx as i32, ty as i32) {
                    continue;
                }

                let falloff_factor = (1.0 - dist / radius_tiles).powf(light.falloff);
                let contribution = light.intensity * falloff_factor;
                let idx = ty * w + tx;
                if let Some(cell) = brightness.get_mut(idx) {
                    *cell = (*cell + contribution).min(1.0);
                }
            }
        }
    }

    // Write RGBA pixels to the image
    let Some(image) = images.get_mut(&light_map.0) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };

    // Image row 0 = top of texture, but tile row 0 = bottom of world → flip Y
    debug_assert_eq!(data.len(), w * h * 4, "lightmap data size mismatch");
    for ty in 0..h {
        let img_row = h - 1 - ty;
        for tx in 0..w {
            let idx = ty * w + tx;
            let pixel_idx = (img_row * w + tx) * 4;
            if pixel_idx + 3 < data.len() {
                let alpha = ((1.0 - brightness[idx].clamp(0.0, 1.0)) * 255.0) as u8;
                data[pixel_idx] = 0;
                data[pixel_idx + 1] = 0;
                data[pixel_idx + 2] = 0;
                data[pixel_idx + 3] = alpha;
            }
        }
    }
}

/// Bresenham line-of-sight check on the occluder grid.
/// Returns true if the line from (x0,y0) to (x1,y1) is unblocked.
/// Skips the start tile; checks all intermediate tiles.
fn bresenham_visible(grid: &LightOccluderGrid, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    if x0 == x1 && y0 == y1 {
        return true;
    }

    let mut cx = x0;
    let mut cy = y0;

    let dx = (x1 - cx).abs();
    let dy = -(y1 - cy).abs();
    let sx = if cx < x1 { 1 } else { -1 };
    let sy = if cy < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if cx == x1 && cy == y1 {
            return true;
        }

        // Check intermediate tiles (skip start)
        if (cx != x0 || cy != y0) && grid.is_occluder(cx, cy) {
            return false;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

pub mod capture;
pub mod share_ui;
pub mod streaming;
pub mod video_decoder;
pub mod video_encoder;
pub mod window_capture;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::game_state::AppState;
use crate::network::ReceivedScreenFrame;
use crate::world::setup::{
    BUTTON_OFFSET_X, BUTTON_SIZE, FRAME_THICKNESS, SCREEN_HEIGHT, SCREEN_WIDTH, SCREEN_Y,
};
use crate::world::{Screen, ScreenControlButton, ScreenControlEvent, ScreenFrame};
use capture::{
    cleanup_capture, handle_capture_events, process_display_capture, process_window_capture,
    start_capture, CaptureSource, ScreenTexture,
};
use share_ui::{
    cleanup_share_ui, handle_share_ui_interaction, setup_share_ui, update_source_list,
    ShareUIState,
};
use streaming::LatestCapturedFrame;

/// Resource tracking current screen dimensions for aspect ratio adjustment.
#[derive(Resource, Default)]
pub struct ScreenDimensions {
    /// Current width of the screen in world units.
    pub width: f32,
    /// Current height of the screen in world units.
    pub height: f32,
    /// Whether dimensions have been initialized.
    pub initialized: bool,
}

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShareUIState>()
            .init_resource::<ScreenTexture>()
            .init_resource::<LatestCapturedFrame>()
            .init_resource::<ScreenDimensions>()
            .add_event::<CaptureSource>()
            .add_systems(
                Update,
                (
                    open_share_ui.run_if(in_state(AppState::InGame)),
                    handle_share_ui_interaction.run_if(resource_exists::<share_ui::ShareUIRoot>),
                    update_source_list.run_if(resource_exists::<share_ui::ShareUIRoot>),
                    handle_capture_events,
                    (
                        handle_received_screen_frames,
                        update_screen_aspect_ratio.after(handle_received_screen_frames),
                    )
                        .run_if(in_state(AppState::InGame)),
                ),
            )
            // Exclusive systems for capture (need direct World access)
            .add_systems(
                Update,
                (start_capture, process_display_capture, process_window_capture),
            )
            .add_systems(OnExit(AppState::InGame), (cleanup_share_ui, cleanup_capture));
    }
}

fn open_share_ui(
    mut events: EventReader<ScreenControlEvent>,
    mut commands: Commands,
    ui_root: Option<Res<share_ui::ShareUIRoot>>,
    mut state: ResMut<ShareUIState>,
) {
    for _ in events.read() {
        if ui_root.is_none() {
            setup_share_ui(&mut commands);
            // Mark for refresh so the list repopulates
            state.needs_refresh = true;
            state.selected_source = None;
        }
    }
}

/// Counter for logging received frames
#[derive(Resource, Default)]
struct ReceivedFrameCounter(u32);

/// Handle received screen frames from the network and update the screen texture.
fn handle_received_screen_frames(
    mut events: EventReader<ReceivedScreenFrame>,
    mut screen_texture: ResMut<ScreenTexture>,
    mut screen_dims: ResMut<ScreenDimensions>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut screen_query: Query<&mut MeshMaterial3d<StandardMaterial>, With<Screen>>,
    mut counter: Local<ReceivedFrameCounter>,
) {
    // Process only the most recent frame to avoid lag
    let Some(frame) = events.read().last() else {
        return;
    };

    counter.0 += 1;

    let expected_size = (frame.width * frame.height * 4) as usize;
    if frame.rgba.len() != expected_size {
        error!(
            "Received frame size mismatch: got {}, expected {}",
            frame.rgba.len(),
            expected_size
        );
        return;
    }

    // Calculate new screen dimensions based on video aspect ratio
    let video_aspect = frame.width as f32 / frame.height as f32;
    let base_aspect = SCREEN_WIDTH / SCREEN_HEIGHT;

    let (new_width, new_height) = if video_aspect >= base_aspect {
        // Video is wider than or equal to base - constrain by width
        (SCREEN_WIDTH, SCREEN_WIDTH / video_aspect)
    } else {
        // Video is taller than base - constrain by height
        (SCREEN_HEIGHT * video_aspect, SCREEN_HEIGHT)
    };

    // Update screen dimensions if changed
    if !screen_dims.initialized
        || (screen_dims.width - new_width).abs() > 0.01
        || (screen_dims.height - new_height).abs() > 0.01
    {
        info!(
            "Adjusting screen for {}x{} video (aspect {:.3}): screen {:.2}x{:.2}",
            frame.width, frame.height, video_aspect, new_width, new_height
        );
        screen_dims.width = new_width;
        screen_dims.height = new_height;
        screen_dims.initialized = true;
    }

    let size = Extent3d {
        width: frame.width,
        height: frame.height,
        depth_or_array_layers: 1,
    };

    let mut new_image = Image::new(
        size,
        TextureDimension::D2,
        frame.rgba.clone(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    new_image.texture_descriptor.usage = bevy::render::render_resource::TextureUsages::COPY_DST
        | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING;

    let new_handle = images.add(new_image);

    // Remove old image if exists
    if let Some(old_handle) = screen_texture.handle.take() {
        images.remove(&old_handle);
    }

    // Get or create material
    let material_handle = if let Some(mat_handle) = screen_texture.material_handle.clone() {
        // Update existing material
        if let Some(material) = materials.get_mut(&mat_handle) {
            material.base_color_texture = Some(new_handle.clone());
        }
        mat_handle
    } else {
        // Create new material for first frame
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(new_handle.clone()),
            unlit: true,
            ..default()
        });
        screen_texture.material_handle = Some(material.clone());
        material
    };

    // Always apply material to screen (ensures it's set even after world respawn)
    for mut screen_mat in screen_query.iter_mut() {
        if screen_mat.0 != material_handle {
            screen_mat.0 = material_handle.clone();
        }
    }

    screen_texture.handle = Some(new_handle);
}

/// Update the screen mesh scale and frame positions based on current dimensions.
fn update_screen_aspect_ratio(
    screen_dims: Res<ScreenDimensions>,
    mut screen_query: Query<
        &mut Transform,
        (
            With<Screen>,
            Without<ScreenFrame>,
            Without<ScreenControlButton>,
        ),
    >,
    mut frame_query: Query<
        (&ScreenFrame, &mut Transform),
        (Without<Screen>, Without<ScreenControlButton>),
    >,
    mut button_query: Query<
        &mut Transform,
        (
            With<ScreenControlButton>,
            Without<Screen>,
            Without<ScreenFrame>,
        ),
    >,
) {
    if !screen_dims.initialized || !screen_dims.is_changed() {
        return;
    }

    let scale_x = screen_dims.width / SCREEN_WIDTH;
    let scale_y = screen_dims.height / SCREEN_HEIGHT;

    info!(
        "Applying screen scale: {:.3} x {:.3} (dims {:.2}x{:.2})",
        scale_x, scale_y, screen_dims.width, screen_dims.height
    );

    // Update screen scale
    for mut transform in screen_query.iter_mut() {
        transform.scale.x = scale_x;
        transform.scale.y = scale_y;
    }

    // Update frame positions
    let half_width = screen_dims.width / 2.0;
    let half_height = screen_dims.height / 2.0;

    for (frame_side, mut transform) in frame_query.iter_mut() {
        match frame_side {
            ScreenFrame::Top => {
                transform.translation.y = SCREEN_Y + half_height + FRAME_THICKNESS / 2.0;
                transform.scale.x = (screen_dims.width + FRAME_THICKNESS * 2.0)
                    / (SCREEN_WIDTH + FRAME_THICKNESS * 2.0);
            }
            ScreenFrame::Bottom => {
                transform.translation.y = SCREEN_Y - half_height - FRAME_THICKNESS / 2.0;
                transform.scale.x = (screen_dims.width + FRAME_THICKNESS * 2.0)
                    / (SCREEN_WIDTH + FRAME_THICKNESS * 2.0);
            }
            ScreenFrame::Left => {
                transform.translation.x = -half_width - FRAME_THICKNESS / 2.0;
                transform.scale.y = screen_dims.height / SCREEN_HEIGHT;
            }
            ScreenFrame::Right => {
                transform.translation.x = half_width + FRAME_THICKNESS / 2.0;
                transform.scale.y = screen_dims.height / SCREEN_HEIGHT;
            }
        }
    }

    // Update button position (bottom-right of screen)
    for mut transform in button_query.iter_mut() {
        transform.translation.x = half_width + FRAME_THICKNESS + BUTTON_OFFSET_X + BUTTON_SIZE / 2.0;
        transform.translation.y = SCREEN_Y - half_height + BUTTON_SIZE / 2.0;
    }
}

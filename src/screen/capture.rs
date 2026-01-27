use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use scrap::{Capturer, Display};
use std::io::ErrorKind;
use std::time::{Duration, Instant};

use crate::world::Screen;
use super::window_capture::capture_window;

/// Type of capture source
#[derive(Clone, Copy, Debug)]
pub enum CaptureSourceType {
    Display(usize),
    Window(isize), // HWND on Windows
}

/// Event to start capturing a source.
#[derive(Event)]
pub struct CaptureSource {
    pub source: CaptureSourceType,
}

/// Resource holding the screen texture handle.
#[derive(Resource, Default)]
pub struct ScreenTexture {
    pub handle: Option<Handle<Image>>,
    pub material_handle: Option<Handle<StandardMaterial>>,
}

/// NonSend resource for active display capture (must run on main thread).
pub struct ActiveDisplayCapture {
    pub capturer: Capturer,
    pub width: u32,
    pub height: u32,
    pub last_capture: Instant,
    pub capture_interval: Duration,
    pub frame_count: u32,
    pub would_block_count: u32,
}

/// Resource for active window capture
#[derive(Resource)]
pub struct ActiveWindowCapture {
    pub hwnd: isize,
    pub width: u32,
    pub height: u32,
    pub last_capture: Instant,
    pub capture_interval: Duration,
    pub frame_count: u32,
}

/// Resource to signal that capture should start.
#[derive(Resource)]
pub struct PendingCapture {
    pub source: CaptureSourceType,
}

/// Handle capture events and create pending capture.
pub fn handle_capture_events(
    mut events: EventReader<CaptureSource>,
    mut commands: Commands,
) {
    for event in events.read() {
        info!("Capture event received: {:?}", event.source);
        commands.insert_resource(PendingCapture {
            source: event.source,
        });
    }
}

/// Exclusive system to start capture (display or window).
pub fn start_capture(world: &mut World) {
    let pending = world.remove_resource::<PendingCapture>();
    let Some(pending) = pending else {
        return;
    };

    // Clean up any existing captures
    world.remove_non_send_resource::<ActiveDisplayCapture>();
    world.remove_resource::<ActiveWindowCapture>();

    match pending.source {
        CaptureSourceType::Display(screen_index) => {
            start_display_capture(world, screen_index);
        }
        CaptureSourceType::Window(hwnd) => {
            start_window_capture_impl(world, hwnd);
        }
    }

    // Apply material to screen
    apply_material_to_screen(world);
}

fn start_display_capture(world: &mut World, screen_index: usize) {
    info!("Starting display capture for screen {}", screen_index);

    let displays = match Display::all() {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to enumerate displays: {}", e);
            return;
        }
    };

    let display = match displays.into_iter().nth(screen_index) {
        Some(d) => d,
        None => {
            error!("Display {} not found", screen_index);
            return;
        }
    };

    let width = display.width() as u32;
    let height = display.height() as u32;

    let capturer = match Capturer::new(display) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create capturer: {}", e);
            return;
        }
    };

    create_capture_texture(world, width, height);

    world.insert_non_send_resource(ActiveDisplayCapture {
        capturer,
        width,
        height,
        last_capture: Instant::now() - Duration::from_millis(100),
        capture_interval: Duration::from_millis(33),
        frame_count: 0,
        would_block_count: 0,
    });

    info!("Display capture started: {}x{}", width, height);
}

fn start_window_capture_impl(world: &mut World, hwnd: isize) {
    info!("Starting window capture for hwnd {}", hwnd);

    match capture_window(hwnd) {
        Some((rgba, width, height)) => {
            info!("Window capture initialized: {}x{}", width, height);

            create_capture_texture(world, width, height);
            update_texture(world, rgba, width, height, true);

            world.insert_resource(ActiveWindowCapture {
                hwnd,
                width,
                height,
                last_capture: Instant::now(),
                capture_interval: Duration::from_millis(33),
                frame_count: 1,
            });
        }
        None => {
            error!("Failed to capture window with hwnd {}", hwnd);
        }
    }
}

fn create_capture_texture(world: &mut World, width: u32, height: u32) {
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[64, 64, 64, 255],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = bevy::render::render_resource::TextureUsages::COPY_DST
        | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING;

    let image_handle = world.resource_mut::<Assets<Image>>().add(image);

    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color_texture: Some(image_handle.clone()),
            unlit: true,
            ..default()
        });

    if let Some(mut screen_texture) = world.get_resource_mut::<ScreenTexture>() {
        screen_texture.handle = Some(image_handle);
        screen_texture.material_handle = Some(material);
    }

    info!("Created capture texture: {}x{}", width, height);
}

fn apply_material_to_screen(world: &mut World) {
    let screen_texture = world.resource::<ScreenTexture>();
    let Some(material_handle) = screen_texture.material_handle.clone() else {
        return;
    };

    let mut query = world.query_filtered::<&mut MeshMaterial3d<StandardMaterial>, With<Screen>>();
    for mut screen_mat in query.iter_mut(world) {
        info!("Applying capture material to screen");
        screen_mat.0 = material_handle.clone();
    }
}

fn update_texture(world: &mut World, rgba: Vec<u8>, width: u32, height: u32, log: bool) {
    let screen_texture = world.resource::<ScreenTexture>();
    let image_handle = screen_texture.handle.clone();
    let material_handle = screen_texture.material_handle.clone();

    let expected_size = (width * height * 4) as usize;
    if rgba.len() != expected_size {
        error!("RGBA size mismatch: got {}, expected {}", rgba.len(), expected_size);
        return;
    }

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut new_image = Image::new(
        size,
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    new_image.texture_descriptor.usage =
        bevy::render::render_resource::TextureUsages::COPY_DST
            | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING;

    let new_handle = world.resource_mut::<Assets<Image>>().add(new_image);

    // Remove old image
    if let Some(old_handle) = image_handle {
        world.resource_mut::<Assets<Image>>().remove(&old_handle);
    }

    // Update material
    if let Some(mat_handle) = material_handle {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        if let Some(material) = materials.get_mut(&mat_handle) {
            material.base_color_texture = Some(new_handle.clone());
        }
    }

    // Store new handle
    if let Some(mut screen_texture) = world.get_resource_mut::<ScreenTexture>() {
        screen_texture.handle = Some(new_handle);
    }

    if log {
        info!("Updated capture texture");
    }
}

/// Exclusive system to process display capture frames.
pub fn process_display_capture(world: &mut World) {
    let Some(capture) = world.get_non_send_resource::<ActiveDisplayCapture>() else {
        return;
    };

    let interval = capture.capture_interval;
    let last = capture.last_capture;
    let width = capture.width;
    let height = capture.height;
    let frame_count = capture.frame_count;

    // Rate limit (but not for first frames)
    if frame_count > 0 && last.elapsed() < interval {
        return;
    }

    let max_attempts = if frame_count == 0 { 10 } else { 3 };

    let frame_data = {
        let mut capture = world.get_non_send_resource_mut::<ActiveDisplayCapture>().unwrap();
        let mut result = None;

        for _attempt in 0..max_attempts {
            match capture.capturer.frame() {
                Ok(frame) => {
                    let stride = frame.len() / height as usize;
                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

                    // Read rows in reverse (DXGI is flipped)
                    for y in (0..height as usize).rev() {
                        for x in 0..width as usize {
                            let i = y * stride + x * 4;
                            if i + 3 < frame.len() {
                                rgba.push(frame[i + 2]); // R
                                rgba.push(frame[i + 1]); // G
                                rgba.push(frame[i]);     // B
                                rgba.push(255);          // A
                            }
                        }
                    }

                    capture.frame_count += 1;
                    capture.would_block_count = 0;
                    capture.last_capture = Instant::now();
                    result = Some(rgba);
                    break;
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        capture.would_block_count += 1;
                        if capture.frame_count == 0 && capture.would_block_count == 1 {
                            info!("Display capture waiting for first frame...");
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    } else {
                        error!("Display capture error: {}", e);
                        break;
                    }
                }
            }
        }
        result
    };

    if let Some(rgba) = frame_data {
        let log = frame_count < 5;
        update_texture(world, rgba, width, height, log);
    }
}

/// System to process window capture frames.
pub fn process_window_capture(world: &mut World) {
    // Check if we have an active window capture
    let capture_info = {
        let Some(capture) = world.get_resource::<ActiveWindowCapture>() else {
            return;
        };

        if capture.last_capture.elapsed() < capture.capture_interval {
            return;
        }

        (capture.hwnd, capture.frame_count)
    };

    let (hwnd, frame_count) = capture_info;

    // Capture the window
    if let Some((rgba, width, height)) = capture_window(hwnd) {
        let log = frame_count < 5;
        update_texture(world, rgba, width, height, log);

        // Update capture state
        if let Some(mut capture) = world.get_resource_mut::<ActiveWindowCapture>() {
            capture.last_capture = Instant::now();
            capture.frame_count += 1;
            capture.width = width;
            capture.height = height;
        }
    }
}

pub fn cleanup_capture(world: &mut World) {
    world.remove_non_send_resource::<ActiveDisplayCapture>();
    world.remove_resource::<ActiveWindowCapture>();
    world.remove_resource::<PendingCapture>();
}

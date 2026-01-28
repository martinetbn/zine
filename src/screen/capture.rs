use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use scrap::{Capturer, Display};
use std::io::ErrorKind;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::world::Screen;
use crate::world::setup::{SCREEN_HEIGHT, SCREEN_WIDTH};
use super::streaming::LatestCapturedFrame;
use super::window_capture::{start_wgc_capture, WgcCapturedFrame};
use super::ScreenDimensions;

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
    pub fps_counter: u32,
    pub fps_timer: Instant,
}

/// Resource for active window capture with background thread (using WGC)
#[derive(Resource)]
pub struct ActiveWindowCapture {
    pub hwnd: isize,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub frame_receiver: Mutex<Receiver<WgcCapturedFrame>>,
    pub stop_sender: Mutex<Sender<()>>,
    pub fps_counter: u32,
    pub fps_timer: Instant,
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
    // Stop background window capture thread if running
    if let Some(capture) = world.get_resource::<ActiveWindowCapture>() {
        if let Ok(sender) = capture.stop_sender.lock() {
            let _ = sender.send(());
        }
    }
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
        capture_interval: Duration::from_millis(16), // ~60fps
        frame_count: 0,
        would_block_count: 0,
        fps_counter: 0,
        fps_timer: Instant::now(),
    });

    info!("Display capture started: {}x{}", width, height);
}

fn start_window_capture_impl(world: &mut World, hwnd: isize) {
    info!("Starting window capture for hwnd {}", hwnd);

    // Start WGC capture - it handles the background thread internally
    match start_wgc_capture(hwnd) {
        Some((frame_rx, stop_tx)) => {
            info!("WGC window capture started for hwnd {}", hwnd);

            // Create initial texture with placeholder dimensions (will be updated on first frame)
            create_capture_texture(world, 1920, 1080);

            world.insert_resource(ActiveWindowCapture {
                hwnd,
                width: 0,
                height: 0,
                frame_count: 0,
                frame_receiver: Mutex::new(frame_rx),
                stop_sender: Mutex::new(stop_tx),
                fps_counter: 0,
                fps_timer: Instant::now(),
            });
        }
        None => {
            error!("Failed to start WGC capture for hwnd {}", hwnd);
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
    // Update the latest captured frame for streaming
    if let Some(mut latest_frame) = world.get_resource_mut::<LatestCapturedFrame>() {
        latest_frame.rgba = rgba.clone();
        latest_frame.width = width;
        latest_frame.height = height;
        latest_frame.frame_number += 1;
    }

    // Update screen dimensions for aspect ratio adjustment
    let video_aspect = width as f32 / height as f32;
    let base_aspect = SCREEN_WIDTH / SCREEN_HEIGHT;

    let (new_width, new_height) = if video_aspect >= base_aspect {
        (SCREEN_WIDTH, SCREEN_WIDTH / video_aspect)
    } else {
        (SCREEN_HEIGHT * video_aspect, SCREEN_HEIGHT)
    };

    if let Some(mut screen_dims) = world.get_resource_mut::<ScreenDimensions>() {
        if !screen_dims.initialized
            || (screen_dims.width - new_width).abs() > 0.01
            || (screen_dims.height - new_height).abs() > 0.01
        {
            info!(
                "Adjusting screen for {}x{} capture (aspect {:.3}): screen {:.2}x{:.2}",
                width, height, video_aspect, new_width, new_height
            );
            screen_dims.width = new_width;
            screen_dims.height = new_height;
            screen_dims.initialized = true;
        }
    }

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
                    let pixel_count = (width * height) as usize;
                    let mut rgba = vec![0u8; pixel_count * 4];

                    // Read rows in reverse (DXGI is flipped) with optimized conversion
                    for y in 0..height as usize {
                        let src_y = height as usize - 1 - y;
                        let src_row_start = src_y * stride;
                        let dst_row_start = y * width as usize * 4;

                        for x in 0..width as usize {
                            let src_i = src_row_start + x * 4;
                            let dst_i = dst_row_start + x * 4;
                            // BGRA -> RGBA
                            rgba[dst_i] = frame[src_i + 2];     // R
                            rgba[dst_i + 1] = frame[src_i + 1]; // G
                            rgba[dst_i + 2] = frame[src_i];     // B
                            rgba[dst_i + 3] = 255;              // A
                        }
                    }

                    capture.frame_count += 1;
                    capture.would_block_count = 0;
                    capture.last_capture = Instant::now();
                    capture.fps_counter += 1;

                    // Log FPS every second
                    if capture.fps_timer.elapsed() >= Duration::from_secs(1) {
                        info!("Display capture FPS: {}", capture.fps_counter);
                        capture.fps_counter = 0;
                        capture.fps_timer = Instant::now();
                    }

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

/// System to process window capture frames (receives from background thread).
pub fn process_window_capture(world: &mut World) {
    // Check if we have an active window capture and try to receive a frame
    let frame_data = {
        let Some(capture) = world.get_resource::<ActiveWindowCapture>() else {
            return;
        };

        // Non-blocking receive - get the latest frame if available
        let mut latest_frame = None;
        if let Ok(receiver) = capture.frame_receiver.lock() {
            while let Ok(frame) = receiver.try_recv() {
                latest_frame = Some(frame);
            }
        }

        latest_frame.map(|f| (f, capture.frame_count))
    };

    if let Some((frame, frame_count)) = frame_data {
        let log = frame_count < 5;
        update_texture(world, frame.rgba, frame.width, frame.height, log);

        // Update capture state
        if let Some(mut capture) = world.get_resource_mut::<ActiveWindowCapture>() {
            capture.frame_count += 1;
            capture.width = frame.width;
            capture.height = frame.height;
            capture.fps_counter += 1;

            // Log FPS every second
            if capture.fps_timer.elapsed() >= Duration::from_secs(1) {
                info!("Window capture FPS: {}", capture.fps_counter);
                capture.fps_counter = 0;
                capture.fps_timer = Instant::now();
            }
        }
    }
}

pub fn cleanup_capture(world: &mut World) {
    // Stop background window capture thread if running
    if let Some(capture) = world.get_resource::<ActiveWindowCapture>() {
        if let Ok(sender) = capture.stop_sender.lock() {
            let _ = sender.send(());
        }
    }

    world.remove_non_send_resource::<ActiveDisplayCapture>();
    world.remove_resource::<ActiveWindowCapture>();
    world.remove_resource::<PendingCapture>();
}

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use scrap::{Capturer, Display};
use std::io::ErrorKind;
use std::time::{Duration, Instant};

use crate::world::Screen;

/// Event to start capturing a screen.
#[derive(Event)]
pub struct CaptureSource {
    pub screen_index: usize,
}

/// Resource holding the screen texture handle.
#[derive(Resource, Default)]
pub struct ScreenTexture {
    pub handle: Option<Handle<Image>>,
    pub material_handle: Option<Handle<StandardMaterial>>,
}

/// NonSend resource for active screen capture (must run on main thread).
pub struct ActiveCapture {
    pub capturer: Capturer,
    pub width: u32,
    pub height: u32,
    pub screen_index: usize,
    pub last_capture: Instant,
    pub capture_interval: Duration,
    pub frame_count: u32,
    pub material_applied: bool,
    pub would_block_count: u32,
    pub created_at: Instant,
}

/// Resource to signal that capture should start.
#[derive(Resource)]
pub struct PendingCapture {
    pub screen_index: usize,
}

/// Exclusive system to start screen capture.
pub fn start_screen_capture(world: &mut World) {
    // Check if there's a pending capture
    let pending = world.remove_resource::<PendingCapture>();
    let Some(pending) = pending else {
        return;
    };

    info!("Starting capture for screen {}", pending.screen_index);

    // Get the display
    let displays = match Display::all() {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to enumerate displays: {}", e);
            return;
        }
    };

    let display = match displays.into_iter().nth(pending.screen_index) {
        Some(d) => d,
        None => {
            error!("Display {} not found", pending.screen_index);
            return;
        }
    };

    let width = display.width() as u32;
    let height = display.height() as u32;

    // Create the capturer
    let capturer = match Capturer::new(display) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create capturer: {}", e);
            return;
        }
    };

    // Create the texture
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[128, 0, 128, 255], // Start with purple to verify texture is applied
        TextureFormat::Rgba8Unorm, // Use linear, not sRGB, for screen capture
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = bevy::render::render_resource::TextureUsages::COPY_DST
        | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING;

    info!("Created texture: {}x{}, format={:?}", width, height, image.texture_descriptor.format);

    // Add image and get handle
    let image_handle = world.resource_mut::<Assets<Image>>().add(image);

    // Create material with the texture
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color_texture: Some(image_handle.clone()),
            unlit: true,
            ..default()
        });

    // Store the handles
    if let Some(mut screen_texture) = world.get_resource_mut::<ScreenTexture>() {
        screen_texture.handle = Some(image_handle.clone());
        screen_texture.material_handle = Some(material.clone());
        info!("Stored texture handle and material handle in ScreenTexture resource");
    } else {
        error!("ScreenTexture resource not found!");
    }

    // Store the active capture as NonSend
    world.insert_non_send_resource(ActiveCapture {
        capturer,
        width,
        height,
        screen_index: pending.screen_index,
        last_capture: Instant::now() - Duration::from_millis(100), // Start immediately
        capture_interval: Duration::from_millis(33), // ~30 FPS
        frame_count: 0,
        material_applied: false,
        would_block_count: 0,
        created_at: Instant::now(),
    });

    info!("Screen capture started: {}x{}", width, height);
}

/// Handle capture events and create pending capture.
pub fn handle_capture_events(
    mut events: EventReader<CaptureSource>,
    mut commands: Commands,
) {
    for event in events.read() {
        commands.insert_resource(PendingCapture {
            screen_index: event.screen_index,
        });
    }
}

/// Exclusive system to process capture frames on main thread.
pub fn process_capture_frames(world: &mut World) {
    // Get the capture resource if it exists
    let Some(capture) = world.get_non_send_resource_mut::<ActiveCapture>() else {
        return;
    };

    // Extract values we need
    let interval = capture.capture_interval;
    let last = capture.last_capture;
    let width = capture.width;
    let height = capture.height;
    let frame_count = capture.frame_count;
    let screen_index = capture.screen_index;
    let created_at = capture.created_at;
    let would_block_count = capture.would_block_count;

    // If we've been waiting too long without any frames, show a test pattern
    // to verify the texture pipeline works, then try recreating the capturer
    if frame_count == 0 && created_at.elapsed() > Duration::from_secs(2) && would_block_count > 50 {
        // Generate a test pattern to verify texture updates work
        info!("Capturer not receiving frames after 2s, showing test pattern...");

        let mut test_pattern = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                // Create a colorful gradient pattern
                let r = ((x * 255) / width) as u8;
                let g = ((y * 255) / height) as u8;
                let b = (((x + y) * 128) / (width + height)) as u8;
                test_pattern.push(r);
                test_pattern.push(g);
                test_pattern.push(b);
                test_pattern.push(255);
            }
        }

        // Update texture with test pattern
        let screen_texture = world.resource::<ScreenTexture>();
        if let Some(handle) = screen_texture.handle.clone() {
            let mut images = world.resource_mut::<Assets<Image>>();
            if let Some(image) = images.get_mut(&handle) {
                if image.data.len() == test_pattern.len() {
                    image.data.copy_from_slice(&test_pattern);
                    info!("Test pattern applied to texture ({} bytes)", test_pattern.len());
                } else {
                    image.data = test_pattern;
                    info!("Test pattern set on texture (replaced data)");
                }
            }
        }

        // Mark that we've shown the test pattern by incrementing frame_count
        if let Some(mut capture) = world.get_non_send_resource_mut::<ActiveCapture>() {
            capture.frame_count = 1; // Prevent showing test pattern repeatedly
            capture.would_block_count = 0;
            capture.created_at = Instant::now(); // Reset timer for potential retry
        }

        // Also apply material if not done yet
        let material_applied = world
            .get_non_send_resource::<ActiveCapture>()
            .map(|c| c.material_applied)
            .unwrap_or(false);

        if !material_applied {
            let screen_texture = world.resource::<ScreenTexture>();
            if let Some(material_handle) = screen_texture.material_handle.clone() {
                let mut query = world.query_filtered::<&mut MeshMaterial3d<StandardMaterial>, With<Screen>>();
                for mut screen_mat in query.iter_mut(world) {
                    info!("Applying material to screen for test pattern");
                    screen_mat.0 = material_handle.clone();
                }
                if let Some(mut capture) = world.get_non_send_resource_mut::<ActiveCapture>() {
                    capture.material_applied = true;
                }
            }
        }

        return;
    }

    // Rate limit captures (but not for the first few attempts)
    if frame_count > 0 && last.elapsed() < interval {
        return;
    }

    // Try multiple times per system call to increase chances of getting a frame
    // DXGI only provides frames when screen content changes
    let max_attempts = if frame_count == 0 { 10 } else { 3 };

    let frame_data = {
        let mut capture = world.get_non_send_resource_mut::<ActiveCapture>().unwrap();
        let mut result = None;

        for attempt in 0..max_attempts {
            match capture.capturer.frame() {
                Ok(frame) => {
                    // Convert BGRA to RGBA - copy data while we have the frame
                    // DXGI captures with Y flipped, so we read rows in reverse order
                    let stride = frame.len() / height as usize;
                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

                    // Log first few frames for debugging
                    if frame_count < 5 {
                        info!("Got frame #{} (attempt {}): {} bytes, stride={}, {}x{}",
                              frame_count, attempt, frame.len(), stride, width, height);
                    }

                    // Read rows from bottom to top to flip the image
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

                    // Log first few frames for debugging
                    if frame_count < 5 {
                        let non_black = rgba.chunks(4).take(100).filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0).count();
                        info!("Frame #{}: RGBA {} bytes, first 100 pixels non-black: {}", frame_count, rgba.len(), non_black);
                    }

                    capture.frame_count += 1;
                    capture.would_block_count = 0;
                    result = Some(rgba);
                    break;
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        capture.would_block_count += 1;
                        // Only log waiting message if we haven't captured any frames yet
                        if capture.frame_count == 0 {
                            if capture.would_block_count == 1 {
                                info!("Capture waiting for first frame... (move a window on the captured display to trigger)");
                            } else if capture.would_block_count % 300 == 0 {
                                info!("Still waiting for first frame... (WouldBlock #{})", capture.would_block_count);
                            }
                        }
                        // Small sleep to avoid tight loop
                        std::thread::sleep(Duration::from_millis(1));
                    } else {
                        error!("Capture error: {} (kind: {:?})", e, e.kind());
                        break;
                    }
                }
            }
        }
        result
    };

    // Update timestamp after the frame is processed
    if frame_data.is_some() {
        if let Some(mut capture) = world.get_non_send_resource_mut::<ActiveCapture>() {
            capture.last_capture = Instant::now();
        }
    }

    // Update texture if we got a frame
    if let Some(rgba) = frame_data {
        let screen_texture = world.resource::<ScreenTexture>();
        let image_handle = screen_texture.handle.clone();
        let material_handle = screen_texture.material_handle.clone();

        // Update the texture - create new image and update material reference
        let expected_size = (width * height * 4) as usize;
        if rgba.len() == expected_size {
            // Create a new image with the captured data
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

            // Add as new asset
            let new_handle = world.resource_mut::<Assets<Image>>().add(new_image);

            // Remove old image if we have one (do this before borrowing materials)
            if let Some(old_handle) = image_handle.clone() {
                world.resource_mut::<Assets<Image>>().remove(&old_handle);
            }

            // Update the material to use the new texture
            if let Some(mat_handle) = material_handle.clone() {
                let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
                if let Some(material) = materials.get_mut(&mat_handle) {
                    material.base_color_texture = Some(new_handle.clone());
                    if frame_count < 5 {
                        info!("Updated material with new texture handle");
                    }
                }
            }

            // Store the new handle
            if let Some(mut screen_texture) = world.get_resource_mut::<ScreenTexture>() {
                screen_texture.handle = Some(new_handle);
            }

            if frame_count < 5 {
                info!("Created new texture and updated material");
            }
        } else {
            error!("RGBA size mismatch: got {}, expected {}", rgba.len(), expected_size);
        }

        // Check if we already applied the material
        let material_applied = world
            .get_non_send_resource::<ActiveCapture>()
            .map(|c| c.material_applied)
            .unwrap_or(false);

        // Update the screen material (first time only)
        if let Some(material_handle) = material_handle {
            if !material_applied {
                let mut query = world.query_filtered::<&mut MeshMaterial3d<StandardMaterial>, With<Screen>>();
                let mut found_screen = false;
                for mut screen_mat in query.iter_mut(world) {
                    found_screen = true;
                    info!("Applying capture material to screen (was: {:?})", screen_mat.0);
                    screen_mat.0 = material_handle.clone();
                }
                if found_screen {
                    if let Some(mut capture) = world.get_non_send_resource_mut::<ActiveCapture>() {
                        capture.material_applied = true;
                    }
                } else {
                    error!("No Screen entity found to apply material to!");
                }
            }
        } else {
            error!("No material handle in ScreenTexture");
        }
    }
}

pub fn cleanup_capture(world: &mut World) {
    world.remove_non_send_resource::<ActiveCapture>();
    world.remove_resource::<PendingCapture>();
}

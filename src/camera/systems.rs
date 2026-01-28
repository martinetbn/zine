use bevy::{input::mouse::MouseMotion, prelude::*, window::CursorGrabMode};

use crate::player::{CameraController, Player, MOUSE_SENSITIVITY, PITCH_LIMIT};

/// Tracks whether the Alt key is currently holding the cursor unlocked
#[derive(Resource, Default)]
pub struct AltCursorUnlock {
    /// True if Alt key caused the cursor to be unlocked
    pub active: bool,
}

pub fn grab_cursor(mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();
    window.cursor_options.grab_mode = CursorGrabMode::Confined;
    window.cursor_options.visible = false;
}

pub fn toggle_cursor_grab(keyboard_input: Res<ButtonInput<KeyCode>>, mut windows: Query<&mut Window>) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        let mut window = windows.single_mut();
        match window.cursor_options.grab_mode {
            CursorGrabMode::None => {
                window.cursor_options.grab_mode = CursorGrabMode::Confined;
                window.cursor_options.visible = false;
            }
            _ => {
                window.cursor_options.grab_mode = CursorGrabMode::None;
                window.cursor_options.visible = true;
            }
        }
    }
}

pub fn mouse_look(
    mut mouse_motion: EventReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut CameraController), With<Player>>,
    windows: Query<&Window>,
) {
    let window = windows.single();

    // Only process mouse look when cursor is grabbed
    if window.cursor_options.grab_mode == CursorGrabMode::None {
        mouse_motion.clear();
        return;
    }

    let (mut transform, mut controller) = query.single_mut();

    for event in mouse_motion.read() {
        controller.yaw -= event.delta.x * MOUSE_SENSITIVITY;
        controller.pitch -= event.delta.y * MOUSE_SENSITIVITY;

        // Clamp pitch to prevent flipping
        controller.pitch = controller.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    // Apply rotation
    transform.rotation = Quat::from_euler(EulerRot::YXZ, controller.yaw, controller.pitch, 0.0);
}

pub fn center_cursor(mut windows: Query<&mut Window>, alt_unlock: Res<AltCursorUnlock>) {
    // Don't center cursor while Alt is held
    if alt_unlock.active {
        return;
    }

    let mut window = windows.single_mut();

    // Only center cursor when it's grabbed and window is focused
    if window.cursor_options.grab_mode != CursorGrabMode::None && window.focused {
        let center = Vec2::new(window.width() / 2.0, window.height() / 2.0);
        window.set_cursor_position(Some(center));
    }
}

pub fn handle_alt_cursor_unlock(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window>,
    mut alt_unlock: ResMut<AltCursorUnlock>,
) {
    let mut window = windows.single_mut();

    if keyboard_input.just_pressed(KeyCode::AltLeft) || keyboard_input.just_pressed(KeyCode::AltRight) {
        // Only unlock if cursor is currently grabbed
        if window.cursor_options.grab_mode != CursorGrabMode::None {
            alt_unlock.active = true;
            window.cursor_options.grab_mode = CursorGrabMode::None;
            window.cursor_options.visible = true;
        }
    }

    if keyboard_input.just_released(KeyCode::AltLeft) || keyboard_input.just_released(KeyCode::AltRight) {
        // Only re-lock if we were the ones who unlocked it
        if alt_unlock.active {
            alt_unlock.active = false;
            window.cursor_options.grab_mode = CursorGrabMode::Confined;
            window.cursor_options.visible = false;
        }
    }
}

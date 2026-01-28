use bevy::{input::mouse::MouseMotion, prelude::*, window::CursorGrabMode};

use crate::player::{CameraController, Player, MOUSE_SENSITIVITY, PITCH_LIMIT};

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

pub fn center_cursor(mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();

    // Only center cursor when it's grabbed and window is focused
    if window.cursor_options.grab_mode != CursorGrabMode::None && window.focused {
        let center = Vec2::new(window.width() / 2.0, window.height() / 2.0);
        window.set_cursor_position(Some(center));
    }
}

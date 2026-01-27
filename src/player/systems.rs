use bevy::prelude::*;

use super::components::{
    Player, Velocity, GRAVITY, GROUND_LEVEL, JUMP_VELOCITY, PLAYER_HEIGHT, PLAYER_SPEED,
};
use crate::world::ROOM_HALF_DEPTH;
use crate::world::ROOM_HALF_WIDTH;

pub fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Transform, &mut Velocity), With<Player>>,
) {
    let (transform, mut velocity) = query.single_mut();

    // Get movement direction from WASD
    let mut direction = Vec3::ZERO;

    if keyboard_input.pressed(KeyCode::KeyW) {
        direction.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        direction.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }

    // Normalize diagonal movement
    if direction.length() > 0.0 {
        direction = direction.normalize();
    }

    // Apply movement relative to camera facing direction (only yaw)
    let forward = transform.forward();
    let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_flat = Vec3::new(-forward.z, 0.0, forward.x).normalize_or_zero();

    let move_direction = forward_flat * -direction.z + right_flat * direction.x;

    // Set horizontal velocity
    velocity.0.x = move_direction.x * PLAYER_SPEED;
    velocity.0.z = move_direction.z * PLAYER_SPEED;

    // Jump (only when grounded)
    let is_grounded = transform.translation.y <= GROUND_LEVEL + PLAYER_HEIGHT + 0.01;
    if keyboard_input.just_pressed(KeyCode::Space) && is_grounded {
        velocity.0.y = JUMP_VELOCITY;
    }
}

pub fn apply_gravity(time: Res<Time>, mut query: Query<(&Transform, &mut Velocity), With<Player>>) {
    let (transform, mut velocity) = query.single_mut();

    let is_grounded = transform.translation.y <= GROUND_LEVEL + PLAYER_HEIGHT + 0.01;

    if !is_grounded {
        velocity.0.y -= GRAVITY * time.delta_secs();
    }
}

pub fn apply_velocity(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    let (mut transform, mut velocity) = query.single_mut();

    // Apply velocity to position
    transform.translation += velocity.0 * time.delta_secs();

    // Ground collision
    if transform.translation.y < GROUND_LEVEL + PLAYER_HEIGHT {
        transform.translation.y = GROUND_LEVEL + PLAYER_HEIGHT;
        velocity.0.y = 0.0;
    }

    // Wall collisions (keep player inside room)
    transform.translation.x = transform.translation.x.clamp(-ROOM_HALF_WIDTH, ROOM_HALF_WIDTH);
    transform.translation.z = transform.translation.z.clamp(-ROOM_HALF_DEPTH, ROOM_HALF_DEPTH);
}

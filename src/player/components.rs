use bevy::prelude::*;

/// Marker component for the player entity.
#[derive(Component)]
pub struct Player;

/// Velocity component for physics-based movement.
#[derive(Component, Default)]
pub struct Velocity(pub Vec3);

/// Camera controller for first-person mouse look.
#[derive(Component)]
pub struct CameraController {
    pub pitch: f32,
    pub yaw: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: std::f32::consts::PI, // Start facing -Z direction
        }
    }
}

// Player physics constants
pub const PLAYER_SPEED: f32 = 5.0;
pub const JUMP_VELOCITY: f32 = 8.0;
pub const GRAVITY: f32 = 20.0;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const GROUND_LEVEL: f32 = 0.0;

// Mouse look constants
pub const MOUSE_SENSITIVITY: f32 = 0.003;
pub const PITCH_LIMIT: f32 = 1.5; // ~86 degrees, just under 90

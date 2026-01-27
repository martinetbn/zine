pub mod components;
pub mod systems;

use bevy::prelude::*;

pub use components::{
    CameraController, Player, Velocity, MOUSE_SENSITIVITY, PITCH_LIMIT, PLAYER_HEIGHT,
};

use crate::game_state::AppState;
use systems::{apply_gravity, apply_velocity, player_movement};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (player_movement, apply_gravity, apply_velocity).run_if(in_state(AppState::InGame)),
        );
    }
}

pub mod setup;

use bevy::prelude::*;

use crate::game_state::AppState;
use setup::setup_world;

// Room dimensions
pub const ROOM_WIDTH: f32 = 10.0;
pub const ROOM_DEPTH: f32 = 10.0;
pub const ROOM_HEIGHT: f32 = 4.0;
pub const WALL_THICKNESS: f32 = 0.2;

// Room bounds for collision (slightly less than actual size to account for walls)
pub const ROOM_HALF_WIDTH: f32 = 4.8;
pub const ROOM_HALF_DEPTH: f32 = 4.8;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_world);
    }
}

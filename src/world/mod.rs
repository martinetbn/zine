pub mod components;
pub mod crosshair;
pub mod interaction;
pub mod setup;

use bevy::prelude::*;

pub use components::{Interactable, Screen, ScreenControlButton};
pub use interaction::ScreenControlEvent;

use crate::game_state::AppState;
use crosshair::{cleanup_crosshair, setup_crosshair};
use interaction::{
    handle_screen_control_interaction, highlight_interactables, on_screen_control_event,
    update_looking_at, LookingAt,
};
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
        app.init_resource::<LookingAt>()
            .add_event::<ScreenControlEvent>()
            .add_systems(OnEnter(AppState::InGame), (setup_world, setup_crosshair))
            .add_systems(OnExit(AppState::InGame), cleanup_crosshair)
            .add_systems(
                Update,
                (
                    update_looking_at,
                    highlight_interactables,
                    handle_screen_control_interaction,
                    on_screen_control_event,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

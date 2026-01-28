pub mod systems;

use bevy::prelude::*;

use crate::game_state::AppState;
use systems::{center_cursor, grab_cursor, handle_alt_cursor_unlock, mouse_look, toggle_cursor_grab, AltCursorUnlock};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AltCursorUnlock>()
            .add_systems(OnEnter(AppState::InGame), grab_cursor)
            .add_systems(
                Update,
                (mouse_look, center_cursor, toggle_cursor_grab, handle_alt_cursor_unlock).run_if(in_state(AppState::InGame)),
            );
    }
}

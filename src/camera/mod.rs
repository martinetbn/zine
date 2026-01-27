pub mod systems;

use bevy::prelude::*;

use systems::{center_cursor, grab_cursor, mouse_look, toggle_cursor_grab};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, grab_cursor)
            .add_systems(Update, (mouse_look, center_cursor, toggle_cursor_grab));
    }
}

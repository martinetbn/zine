mod camera;
mod character;
mod game_state;
mod menu;
mod network;
mod player;
mod screen;
mod world;

use bevy::{prelude::*, window::PresentMode};

use camera::CameraPlugin;
use character::CharacterPlugin;
use game_state::AppState;
use menu::MenuPlugin;
use network::NetworkPlugin;
use player::PlayerPlugin;
use screen::ScreenPlugin;
use world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Zine".to_string(),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
        )
        .init_state::<AppState>()
        .add_plugins((
            MenuPlugin,
            NetworkPlugin,
            WorldPlugin,
            PlayerPlugin,
            CameraPlugin,
            ScreenPlugin,
            CharacterPlugin,
        ))
        .run();
}

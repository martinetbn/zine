mod camera;
mod game_state;
mod menu;
mod network;
mod player;
mod world;

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};

use camera::CameraPlugin;
use game_state::AppState;
use menu::MenuPlugin;
use network::NetworkPlugin;
use player::PlayerPlugin;
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
        .add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin::default(),
        ))
        .init_state::<AppState>()
        .add_plugins((
            MenuPlugin,
            NetworkPlugin,
            WorldPlugin,
            PlayerPlugin,
            CameraPlugin,
        ))
        .run();
}

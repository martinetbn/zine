pub mod components;
pub mod styles;
pub mod systems;

use bevy::prelude::*;

use crate::game_state::AppState;
use systems::*;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app
            // Main menu
            .add_systems(OnEnter(AppState::MainMenu), (setup_main_menu, release_cursor))
            .add_systems(OnExit(AppState::MainMenu), cleanup_main_menu)
            .add_systems(
                Update,
                (button_interaction, handle_host_click, handle_join_click)
                    .run_if(in_state(AppState::MainMenu)),
            )
            // Browser
            .add_systems(OnEnter(AppState::Browsing), setup_browser)
            .add_systems(OnExit(AppState::Browsing), cleanup_browser)
            .add_systems(
                Update,
                (
                    button_interaction,
                    handle_back_click,
                    update_session_list,
                    handle_session_click,
                )
                    .run_if(in_state(AppState::Browsing)),
            )
            // Cleanup menu camera when entering game
            .add_systems(OnEnter(AppState::InGame), cleanup_menu_camera);
    }
}

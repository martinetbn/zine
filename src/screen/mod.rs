pub mod capture;
pub mod share_ui;

use bevy::prelude::*;

use crate::game_state::AppState;
use crate::world::ScreenControlEvent;
use capture::{
    cleanup_capture, handle_capture_events, process_capture_frames, start_screen_capture,
    CaptureSource, ScreenTexture,
};
use share_ui::{
    cleanup_share_ui, handle_share_ui_interaction, setup_share_ui, update_source_list,
    ShareUIState,
};

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShareUIState>()
            .init_resource::<ScreenTexture>()
            .add_event::<CaptureSource>()
            .add_systems(
                Update,
                (
                    open_share_ui.run_if(in_state(AppState::InGame)),
                    handle_share_ui_interaction.run_if(resource_exists::<share_ui::ShareUIRoot>),
                    update_source_list.run_if(resource_exists::<share_ui::ShareUIRoot>),
                    handle_capture_events,
                ),
            )
            // Exclusive systems for capture (need direct World access)
            .add_systems(Update, (start_screen_capture, process_capture_frames))
            .add_systems(OnExit(AppState::InGame), (cleanup_share_ui, cleanup_capture));
    }
}

fn open_share_ui(
    mut events: EventReader<ScreenControlEvent>,
    mut commands: Commands,
    ui_root: Option<Res<share_ui::ShareUIRoot>>,
    mut state: ResMut<ShareUIState>,
) {
    for _ in events.read() {
        if ui_root.is_none() {
            setup_share_ui(&mut commands);
            // Mark for refresh so the list repopulates
            state.needs_refresh = true;
            state.selected_source = None;
        }
    }
}

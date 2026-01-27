use bevy::prelude::*;
use std::net::UdpSocket;

use super::discovery::SelectedSession;
use crate::game_state::AppState;

/// Resource indicating this instance is a client.
#[derive(Resource)]
pub struct GameClient {
    pub socket: UdpSocket,
}

pub fn client_plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Connecting), setup_client)
        .add_systems(OnExit(AppState::InGame), cleanup_client)
        .add_systems(
            Update,
            client_connect_check.run_if(in_state(AppState::Connecting)),
        );
}

fn setup_client(mut commands: Commands, selected: Option<Res<SelectedSession>>) {
    let Some(selected) = selected else {
        error!("No session selected");
        return;
    };

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to bind client socket: {}", e);
            return;
        }
    };

    if let Err(e) = socket.set_nonblocking(true) {
        error!("Failed to set non-blocking: {}", e);
        return;
    }

    // Connect to server (for UDP this just sets the default destination)
    if let Err(e) = socket.connect(selected.0.address) {
        error!("Failed to connect: {}", e);
        return;
    }

    commands.insert_resource(GameClient { socket });

    info!("Connecting to server at {}", selected.0.address);
}

fn cleanup_client(mut commands: Commands) {
    commands.remove_resource::<GameClient>();
    commands.remove_resource::<SelectedSession>();
}

fn client_connect_check(
    mut next_state: ResMut<NextState<AppState>>,
    client: Option<Res<GameClient>>,
) {
    // For now, just transition to InGame once client socket is ready
    // In a real implementation, we'd do a handshake with the server
    if client.is_some() {
        next_state.set(AppState::InGame);
    }
}

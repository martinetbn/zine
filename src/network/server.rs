use bevy::prelude::*;
use std::net::UdpSocket;

use super::discovery::GAME_PORT;
use crate::game_state::AppState;

/// Resource indicating this instance is the server/host.
#[derive(Resource)]
pub struct GameServer {
    pub socket: UdpSocket,
    pub player_count: u32,
}

pub fn server_plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Hosting), setup_server)
        .add_systems(OnExit(AppState::InGame), cleanup_server)
        .add_systems(
            Update,
            server_ready_check.run_if(in_state(AppState::Hosting)),
        );
}

fn setup_server(mut commands: Commands) {
    let server_addr = format!("0.0.0.0:{}", GAME_PORT);

    let socket = match UdpSocket::bind(&server_addr) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to bind server socket: {}", e);
            return;
        }
    };

    if let Err(e) = socket.set_nonblocking(true) {
        error!("Failed to set non-blocking: {}", e);
        return;
    }

    commands.insert_resource(GameServer {
        socket,
        player_count: 1,
    });

    info!("Server started on port {}", GAME_PORT);
}

fn cleanup_server(mut commands: Commands) {
    commands.remove_resource::<GameServer>();
}

fn server_ready_check(
    mut next_state: ResMut<NextState<AppState>>,
    server: Option<Res<GameServer>>,
) {
    // Transition to InGame once server is set up
    if server.is_some() {
        next_state.set(AppState::InGame);
    }
}

use bevy::prelude::*;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use super::discovery::GAME_PORT;
use super::protocol::{ClientMessage, LocalPlayerId, PlayerId, PlayerState, ServerMessage};
use crate::game_state::AppState;
use crate::player::Player;

/// Resource indicating this instance is the server/host.
#[derive(Resource)]
pub struct GameServer {
    pub socket: UdpSocket,
    pub clients: HashMap<SocketAddr, PlayerId>,
    pub player_states: HashMap<PlayerId, PlayerState>,
    pub next_player_id: PlayerId,
}

/// Timer for sending state updates.
#[derive(Resource)]
pub struct ServerSyncTimer(pub Timer);

pub fn server_plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Hosting), setup_server)
        .add_systems(OnExit(AppState::InGame), cleanup_server)
        .add_systems(
            Update,
            server_ready_check.run_if(in_state(AppState::Hosting)),
        )
        .add_systems(
            Update,
            (receive_client_messages, broadcast_game_state, update_host_player_state)
                .run_if(in_state(AppState::InGame).and(resource_exists::<GameServer>)),
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

    // Host is player 0
    let host_id: PlayerId = 0;
    let mut player_states = HashMap::new();
    player_states.insert(
        host_id,
        PlayerState {
            id: host_id,
            position: [0.0, 1.8, 4.0],
            yaw: std::f32::consts::PI,
        },
    );

    commands.insert_resource(GameServer {
        socket,
        clients: HashMap::new(),
        player_states,
        next_player_id: 1,
    });

    commands.insert_resource(LocalPlayerId(host_id));
    commands.insert_resource(ServerSyncTimer(Timer::new(
        Duration::from_millis(50), // 20 updates per second
        TimerMode::Repeating,
    )));

    info!("Server started on port {}", GAME_PORT);
}

fn cleanup_server(mut commands: Commands) {
    commands.remove_resource::<GameServer>();
    commands.remove_resource::<ServerSyncTimer>();
    commands.remove_resource::<LocalPlayerId>();
}

fn server_ready_check(
    mut next_state: ResMut<NextState<AppState>>,
    server: Option<Res<GameServer>>,
) {
    if server.is_some() {
        next_state.set(AppState::InGame);
    }
}

fn receive_client_messages(mut server: ResMut<GameServer>) {
    let mut buf = [0u8; 1024];

    loop {
        match server.socket.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                if let Ok(msg) = serde_json::from_slice::<ClientMessage>(&buf[..len]) {
                    match msg {
                        ClientMessage::Join => {
                            // New client joining
                            if !server.clients.contains_key(&src_addr) {
                                let player_id = server.next_player_id;
                                server.next_player_id += 1;
                                server.clients.insert(src_addr, player_id);
                                server.player_states.insert(
                                    player_id,
                                    PlayerState {
                                        id: player_id,
                                        position: [0.0, 1.8, 4.0],
                                        yaw: std::f32::consts::PI,
                                    },
                                );

                                info!("Player {} joined from {}", player_id, src_addr);

                                // Send welcome message
                                let welcome = ServerMessage::Welcome { your_id: player_id };
                                if let Ok(data) = serde_json::to_vec(&welcome) {
                                    let _ = server.socket.send_to(&data, src_addr);
                                }
                            }
                        }
                        ClientMessage::PlayerUpdate { position, yaw } => {
                            // Update player state
                            if let Some(&player_id) = server.clients.get(&src_addr) {
                                if let Some(state) = server.player_states.get_mut(&player_id) {
                                    state.position = position;
                                    state.yaw = yaw;
                                }
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                error!("Server receive error: {}", e);
                break;
            }
        }
    }
}

fn update_host_player_state(
    mut server: ResMut<GameServer>,
    player_query: Query<&Transform, With<Player>>,
    local_id: Res<LocalPlayerId>,
) {
    if let Ok(transform) = player_query.get_single() {
        if let Some(state) = server.player_states.get_mut(&local_id.0) {
            state.position = transform.translation.into();
            // Extract yaw from rotation
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            state.yaw = yaw;
        }
    }
}

fn broadcast_game_state(
    time: Res<Time>,
    mut timer: ResMut<ServerSyncTimer>,
    server: Res<GameServer>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let players: Vec<PlayerState> = server.player_states.values().cloned().collect();
    let msg = ServerMessage::GameState { players };

    if let Ok(data) = serde_json::to_vec(&msg) {
        for &client_addr in server.clients.keys() {
            let _ = server.socket.send_to(&data, client_addr);
        }
    }
}

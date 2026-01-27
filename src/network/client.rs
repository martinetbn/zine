use bevy::prelude::*;
use std::net::UdpSocket;
use std::time::Duration;

use super::discovery::SelectedSession;
use super::protocol::{
    ClientMessage, LocalPlayerId, PlayerState, RemotePlayer, RemotePlayers, ServerMessage,
};
use crate::game_state::AppState;
use crate::player::Player;

/// Resource indicating this instance is a client.
#[derive(Resource)]
pub struct GameClient {
    pub socket: UdpSocket,
}

/// Timer for sending updates to server.
#[derive(Resource)]
pub struct ClientSyncTimer(pub Timer);

pub fn client_plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Connecting), setup_client)
        .add_systems(OnExit(AppState::InGame), cleanup_client)
        .add_systems(
            Update,
            (client_receive, client_connect_check).run_if(in_state(AppState::Connecting)),
        )
        .add_systems(
            Update,
            (client_receive, send_player_update)
                .run_if(in_state(AppState::InGame).and(resource_exists::<GameClient>)),
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

    if let Err(e) = socket.connect(selected.0.address) {
        error!("Failed to connect: {}", e);
        return;
    }

    // Send join request
    let join_msg = ClientMessage::Join;
    if let Ok(data) = serde_json::to_vec(&join_msg) {
        let _ = socket.send(&data);
    }

    commands.insert_resource(GameClient { socket });
    commands.insert_resource(RemotePlayers::default());
    commands.insert_resource(ClientSyncTimer(Timer::new(
        Duration::from_millis(50),
        TimerMode::Repeating,
    )));

    info!("Connecting to server at {}", selected.0.address);
}

fn cleanup_client(mut commands: Commands) {
    commands.remove_resource::<GameClient>();
    commands.remove_resource::<SelectedSession>();
    commands.remove_resource::<LocalPlayerId>();
    commands.remove_resource::<RemotePlayers>();
    commands.remove_resource::<ClientSyncTimer>();
}

fn client_receive(
    client: Option<Res<GameClient>>,
    mut commands: Commands,
    mut remote_players: Option<ResMut<RemotePlayers>>,
    local_id: Option<Res<LocalPlayerId>>,
) {
    let Some(client) = client else { return };

    let mut buf = [0u8; 4096];

    loop {
        match client.socket.recv(&mut buf) {
            Ok(len) => {
                if let Ok(msg) = serde_json::from_slice::<ServerMessage>(&buf[..len]) {
                    match msg {
                        ServerMessage::Welcome { your_id } => {
                            info!("Received welcome, assigned ID: {}", your_id);
                            commands.insert_resource(LocalPlayerId(your_id));
                        }
                        ServerMessage::GameState { players } => {
                            if let Some(ref mut remote) = remote_players {
                                // Filter out local player
                                let my_id = local_id.as_ref().map(|id| id.0);
                                remote.players = players
                                    .into_iter()
                                    .filter(|p| Some(p.id) != my_id)
                                    .collect();
                            }
                        }
                        ServerMessage::PlayerLeft { id } => {
                            info!("Player {} left", id);
                            if let Some(ref mut remote) = remote_players {
                                remote.players.retain(|p| p.id != id);
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                error!("Client receive error: {}", e);
                break;
            }
        }
    }
}

fn client_connect_check(
    mut next_state: ResMut<NextState<AppState>>,
    local_id: Option<Res<LocalPlayerId>>,
) {
    // Transition to InGame once we have our player ID
    if local_id.is_some() {
        info!("Connected to server!");
        next_state.set(AppState::InGame);
    }
}

fn send_player_update(
    time: Res<Time>,
    mut timer: ResMut<ClientSyncTimer>,
    client: Res<GameClient>,
    player_query: Query<&Transform, With<Player>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    if let Ok(transform) = player_query.get_single() {
        let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let msg = ClientMessage::PlayerUpdate {
            position: transform.translation.into(),
            yaw,
        };

        if let Ok(data) = serde_json::to_vec(&msg) {
            let _ = client.socket.send(&data);
        }
    }
}

/// System to spawn/update remote player visuals.
pub fn update_remote_player_visuals(
    mut commands: Commands,
    remote_players: Option<Res<RemotePlayers>>,
    mut remote_query: Query<(Entity, &RemotePlayer, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(remote_players) = remote_players else {
        return;
    };

    if !remote_players.is_changed() {
        return;
    }

    let existing_ids: Vec<u64> = remote_query.iter().map(|(_, rp, _)| rp.id).collect();

    for player_state in &remote_players.players {
        if let Some((_, _, mut transform)) = remote_query
            .iter_mut()
            .find(|(_, rp, _)| rp.id == player_state.id)
        {
            // Update existing remote player
            transform.translation = Vec3::from_array(player_state.position);
            transform.translation.y -= 0.9; // Offset to ground level for the mesh
            transform.rotation = Quat::from_rotation_y(player_state.yaw);
        } else {
            // Spawn new remote player
            info!("Spawning remote player {}", player_state.id);
            commands.spawn((
                RemotePlayer { id: player_state.id },
                Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.6, 0.8),
                    ..default()
                })),
                Transform::from_translation(Vec3::from_array(player_state.position))
                    .with_rotation(Quat::from_rotation_y(player_state.yaw)),
            ));
        }
    }

    // Remove players that left
    let current_ids: Vec<u64> = remote_players.players.iter().map(|p| p.id).collect();
    for (entity, rp, _) in remote_query.iter() {
        if !current_ids.contains(&rp.id) {
            commands.entity(entity).despawn_recursive();
        }
    }
}

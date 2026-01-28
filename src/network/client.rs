use bevy::prelude::*;
use std::net::UdpSocket;
use std::time::Duration;

use super::discovery::SelectedSession;
use super::protocol::{
    ClientMessage, LocalPlayerId, NetworkTransform, RemotePlayer, RemotePlayers, ServerMessage,
};
use crate::game_state::AppState;
use crate::player::Player;

use crate::screen::video_decoder::{VideoDecoder, VideoJitterBuffer};

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
        );

    app.add_systems(
        Update,
        (client_receive, send_player_update, process_video_decoder)
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

    // Increase receive buffer for screen streaming fragments
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        unsafe {
            let buf_size: i32 = 1024 * 1024; // 1MB receive buffer
            let raw = socket.as_raw_socket();
            winapi::um::winsock2::setsockopt(
                raw as usize,
                winapi::um::winsock2::SOL_SOCKET as i32,
                winapi::um::winsock2::SO_RCVBUF as i32,
                &buf_size as *const i32 as *const i8,
                std::mem::size_of::<i32>() as i32,
            );
        }
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

    if let Some(decoder) = VideoDecoder::new() {
        commands.insert_resource(decoder);
        commands.insert_resource(VideoJitterBuffer::default());
        info!("Video decoder initialized (OpenH264)");
    } else {
        error!("Failed to initialize video decoder");
    }

    info!("Connecting to server at {}", selected.0.address);
}

fn cleanup_client(mut commands: Commands) {
    commands.remove_resource::<GameClient>();
    commands.remove_resource::<SelectedSession>();
    commands.remove_resource::<LocalPlayerId>();
    commands.remove_resource::<RemotePlayers>();
    commands.remove_resource::<ClientSyncTimer>();
    commands.remove_resource::<VideoDecoder>();
    commands.remove_resource::<VideoJitterBuffer>();
}

/// Event to update the screen texture with received frame data.
#[derive(Event)]
pub struct ReceivedScreenFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn client_receive(
    client: Option<Res<GameClient>>,
    mut commands: Commands,
    mut remote_players: Option<ResMut<RemotePlayers>>,
    local_id: Option<Res<LocalPlayerId>>,
    mut video_decoder: Option<ResMut<VideoDecoder>>,
) {
    let Some(client) = client else { return };

    // Large buffer to handle video frame chunks
    let mut buf = [0u8; 32768];

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
                        ServerMessage::VideoFrame(chunk) => {
                            static CHUNK_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                            let count = CHUNK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if count % 100 == 0 {
                                info!("Received video chunk {} (frame {}, chunk {}/{})", count, chunk.frame_id, chunk.chunk_idx, chunk.total_chunks);
                            }
                            if let Some(ref mut decoder) = video_decoder {
                                decoder.add_chunk(chunk);
                            }
                        }
                        ServerMessage::VideoCodec(info) => {
                            if let Some(ref mut decoder) = video_decoder {
                                decoder.set_codec_info(info);
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

/// Process decoded video frames
fn process_video_decoder(
    mut decoder: Option<ResMut<VideoDecoder>>,
    mut jitter: Option<ResMut<VideoJitterBuffer>>,
    mut screen_frame_events: EventWriter<ReceivedScreenFrame>,
) {
    // Get decoded frames from decoder and add to jitter buffer
    if let Some(ref mut decoder) = decoder {
        while let Some(frame) = decoder.get_decoded() {
            static RECEIVED_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let count = RECEIVED_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 30 == 0 {
                info!("process_video_decoder: received frame {} ({}x{})", count, frame.width, frame.height);
            }
            if let Some(ref mut jitter) = jitter {
                jitter.push(frame);
            }
        }
    }

    // Pop frames from jitter buffer
    if let Some(ref mut jitter) = jitter {
        if let Some(frame) = jitter.pop() {
            screen_frame_events.send(ReceivedScreenFrame {
                rgba: frame.rgba,
                width: frame.width,
                height: frame.height,
            });
        }
    }
}

/// Interpolation speed - higher = faster catch-up, lower = smoother but more latency.
const INTERPOLATION_SPEED: f32 = 15.0;

/// System to spawn/update remote player visuals (sets network targets).
pub fn update_remote_player_visuals(
    mut commands: Commands,
    remote_players: Option<Res<RemotePlayers>>,
    mut remote_query: Query<(Entity, &RemotePlayer, &mut NetworkTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(remote_players) = remote_players else {
        return;
    };

    if !remote_players.is_changed() {
        return;
    }

    for player_state in &remote_players.players {
        let mut target_pos = Vec3::from_array(player_state.position);
        target_pos.y -= 0.9; // Offset to ground level for the mesh

        if let Some((_, _, mut net_transform)) = remote_query
            .iter_mut()
            .find(|(_, rp, _)| rp.id == player_state.id)
        {
            // Update target for existing remote player
            net_transform.target_position = target_pos;
            net_transform.target_yaw = player_state.yaw;
        } else {
            // Spawn new remote player with NetworkTransform
            info!("Spawning remote player {}", player_state.id);
            commands.spawn((
                RemotePlayer { id: player_state.id },
                NetworkTransform {
                    target_position: target_pos,
                    target_yaw: player_state.yaw,
                },
                Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.6, 0.8),
                    ..default()
                })),
                Transform::from_translation(target_pos)
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

/// System to smoothly interpolate remote players towards their target transforms.
pub fn interpolate_remote_players(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &NetworkTransform), With<RemotePlayer>>,
) {
    let dt = time.delta_secs();
    let t = (INTERPOLATION_SPEED * dt).min(1.0);

    for (mut transform, net_transform) in query.iter_mut() {
        // Lerp position
        transform.translation = transform
            .translation
            .lerp(net_transform.target_position, t);

        // Slerp rotation (smoothly interpolate yaw)
        let target_rotation = Quat::from_rotation_y(net_transform.target_yaw);
        transform.rotation = transform.rotation.slerp(target_rotation, t);
    }
}

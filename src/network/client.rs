use bevy::prelude::*;
use std::net::UdpSocket;
use std::time::Duration;

use super::discovery::SelectedSession;
use super::protocol::{
    ClientMessage, LocalPlayerId, NetworkTransform, RemotePlayer, RemotePlayers, ServerMessage,
};
use crate::character::{CharacterAssets, CharacterAnimationState, NeedsAnimationSetup};
use crate::game_state::AppState;
use crate::player::Player;

use crate::screen::audio_decoder::AudioDecoder;
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
        .add_systems(OnExit(AppState::Connecting), cleanup_on_connect_fail)
        .add_systems(
            Update,
            (client_receive, client_connect_check, handle_host_disconnected)
                .run_if(in_state(AppState::Connecting)),
        );

    app.add_systems(
        Update,
        (client_receive, send_player_update, process_video_decoder, handle_host_disconnected)
            .run_if(in_state(AppState::InGame).and(resource_exists::<GameClient>)),
    );
}

/// Handle host disconnection by returning to main menu.
fn handle_host_disconnected(
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    disconnected: Option<Res<HostDisconnected>>,
) {
    if disconnected.is_some() {
        commands.remove_resource::<HostDisconnected>();
        next_state.set(AppState::MainMenu);
    }
}

/// Cleanup client resources if connecting fails.
fn cleanup_on_connect_fail(
    mut commands: Commands,
    client: Option<Res<GameClient>>,
    local_id: Option<Res<LocalPlayerId>>,
) {
    // Only cleanup if we're transitioning back to menu (no local ID means connection failed)
    if client.is_some() && local_id.is_none() {
        commands.remove_resource::<GameClient>();
        commands.remove_resource::<RemotePlayers>();
        commands.remove_resource::<ClientSyncTimer>();
        commands.remove_resource::<VideoDecoder>();
        commands.remove_resource::<VideoJitterBuffer>();
        commands.remove_resource::<AudioDecoder>();
        commands.remove_resource::<HostDisconnected>();
        commands.remove_resource::<SelectedSession>();
    }
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

    // Initialize audio decoder and playback
    if let Some(audio_decoder) = AudioDecoder::new() {
        commands.insert_resource(audio_decoder);
        info!("Audio decoder initialized (Opus)");
    } else {
        warn!("Failed to initialize audio decoder - audio playback disabled");
    }

    info!("Connecting to server at {}", selected.0.address);
}

fn cleanup_client(mut commands: Commands, client: Option<Res<GameClient>>) {
    // Send leave message to server if we have a connection
    if let Some(client) = client {
        let leave_msg = ClientMessage::Leave;
        if let Ok(data) = serde_json::to_vec(&leave_msg) {
            let _ = client.socket.send(&data);
        }
    }

    commands.remove_resource::<GameClient>();
    commands.remove_resource::<SelectedSession>();
    commands.remove_resource::<LocalPlayerId>();
    commands.remove_resource::<RemotePlayers>();
    commands.remove_resource::<ClientSyncTimer>();
    commands.remove_resource::<VideoDecoder>();
    commands.remove_resource::<VideoJitterBuffer>();
    commands.remove_resource::<AudioDecoder>();
    commands.remove_resource::<HostDisconnected>();
}

/// Event to update the screen texture with received frame data.
#[derive(Event)]
pub struct ReceivedScreenFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Resource to signal that the host has disconnected.
#[derive(Resource)]
pub struct HostDisconnected;

fn client_receive(
    client: Option<Res<GameClient>>,
    mut commands: Commands,
    mut remote_players: Option<ResMut<RemotePlayers>>,
    local_id: Option<Res<LocalPlayerId>>,
    mut video_decoder: Option<ResMut<VideoDecoder>>,
    audio_decoder: Option<Res<AudioDecoder>>,
    disconnected: Option<Res<HostDisconnected>>,
) {
    // Skip receiving if already marked as disconnected
    if disconnected.is_some() {
        return;
    }

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
                        ServerMessage::AudioFrame(chunk) => {
                            if let Some(ref decoder) = audio_decoder {
                                decoder.add_chunk(chunk);
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                // Host disconnected - this is expected when host closes
                info!("Host disconnected");
                commands.insert_resource(HostDisconnected);
                break;
            }
            Err(_) => {
                // Other errors - treat as disconnection
                commands.insert_resource(HostDisconnected);
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
    player_query: Query<(&Transform, &crate::player::CameraController), With<Player>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    if let Ok((transform, camera_controller)) = player_query.get_single() {
        let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let msg = ClientMessage::PlayerUpdate {
            position: transform.translation.into(),
            yaw,
            pitch: camera_controller.pitch,
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
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;
    use std::sync::Mutex;

    // FPS tracking for decoded frames
    static DECODED_FPS_COUNTER: AtomicU32 = AtomicU32::new(0);
    static DISPLAYED_FPS_COUNTER: AtomicU32 = AtomicU32::new(0);
    static LAST_FPS_LOG: Mutex<Option<Instant>> = Mutex::new(None);

    // Get decoded frames from decoder and add to jitter buffer
    if let Some(ref mut decoder) = decoder {
        while let Some(frame) = decoder.get_decoded() {
            DECODED_FPS_COUNTER.fetch_add(1, Ordering::Relaxed);
            if let Some(ref mut jitter) = jitter {
                jitter.push(frame);
            }
        }
    }

    // Pop frames from jitter buffer - drain any backlog and use the latest
    if let Some(ref mut jitter) = jitter {
        let mut latest_frame = None;
        while let Some(frame) = jitter.pop() {
            latest_frame = Some(frame);
        }
        if let Some(frame) = latest_frame {
            DISPLAYED_FPS_COUNTER.fetch_add(1, Ordering::Relaxed);
            screen_frame_events.send(ReceivedScreenFrame {
                rgba: frame.rgba,
                width: frame.width,
                height: frame.height,
            });
        }
    }

    // Log FPS every second
    let should_log = {
        let mut last = LAST_FPS_LOG.lock().unwrap();
        match *last {
            None => {
                *last = Some(Instant::now());
                false
            }
            Some(t) if t.elapsed().as_secs() >= 1 => {
                *last = Some(Instant::now());
                true
            }
            _ => false,
        }
    };

    if should_log {
        DECODED_FPS_COUNTER.swap(0, Ordering::Relaxed);
        DISPLAYED_FPS_COUNTER.swap(0, Ordering::Relaxed);
    }
}

/// Interpolation speed - higher = faster catch-up, lower = smoother but more latency.
const INTERPOLATION_SPEED: f32 = 15.0;

/// System to spawn/update remote player visuals (sets network targets).
pub fn update_remote_player_visuals(
    mut commands: Commands,
    remote_players: Option<Res<RemotePlayers>>,
    mut remote_query: Query<(Entity, &RemotePlayer, &mut NetworkTransform, &mut CharacterAnimationState)>,
    character_assets: Option<Res<CharacterAssets>>,
) {
    let Some(remote_players) = remote_players else {
        return;
    };

    if !remote_players.is_changed() {
        return;
    }

    // Player height constant (eye level above feet)
    const PLAYER_HEIGHT: f32 = 2.0;
    // Model pivot offset (adjust if character floats or clips)
    const MODEL_OFFSET: f32 = -0.15;

    for player_state in &remote_players.players {
        // Convert from eye position to character feet position
        let target_pos = Vec3::new(
            player_state.position[0],
            player_state.position[1] - PLAYER_HEIGHT + MODEL_OFFSET,
            player_state.position[2],
        );

        // Add PI to yaw to flip the character to face the correct direction
        let corrected_yaw = player_state.yaw + std::f32::consts::PI;

        if let Some((_, _, mut net_transform, mut anim_state)) = remote_query
            .iter_mut()
            .find(|(_, rp, _, _)| rp.id == player_state.id)
        {
            // Check if player is moving (for animation state)
            let distance = net_transform.target_position.distance(target_pos);

            // If significant movement detected, mark as walking and update timestamp
            if distance > 0.05 {
                anim_state.is_walking = true;
                anim_state.last_walk_time = 0.0; // Will be updated by decay system
            }

            // Update target for existing remote player
            net_transform.target_position = target_pos;
            net_transform.target_yaw = corrected_yaw;
            net_transform.target_pitch = player_state.pitch;
        } else {
            // Spawn new remote player with character model
            info!("Spawning remote player {}", player_state.id);

            // Add character scene if assets are loaded
            if let Some(ref assets) = character_assets {
                commands.spawn((
                    RemotePlayer { id: player_state.id },
                    NetworkTransform {
                        target_position: target_pos,
                        target_yaw: corrected_yaw,
                        target_pitch: player_state.pitch,
                    },
                    CharacterAnimationState::default(),
                    Transform::from_translation(target_pos)
                        .with_rotation(Quat::from_rotation_y(corrected_yaw))
                        .with_scale(Vec3::splat(1.0)), // Character scale
                    SceneRoot(assets.scene.clone()),
                    NeedsAnimationSetup,
                ));
            } else {
                // Fallback: spawn without character model (will be added later)
                warn!("Character assets not loaded yet, spawning player without model");
                commands.spawn((
                    RemotePlayer { id: player_state.id },
                    NetworkTransform {
                        target_position: target_pos,
                        target_yaw: corrected_yaw,
                        target_pitch: player_state.pitch,
                    },
                    CharacterAnimationState::default(),
                    Transform::from_translation(target_pos)
                        .with_rotation(Quat::from_rotation_y(corrected_yaw)),
                ));
            }
        }
    }

    // Remove players that left
    let current_ids: Vec<u64> = remote_players.players.iter().map(|p| p.id).collect();
    for (entity, rp, _, _) in remote_query.iter() {
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

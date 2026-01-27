use bevy::prelude::*;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use super::discovery::GAME_PORT;
use super::protocol::{ClientMessage, LocalPlayerId, PlayerId, PlayerState, ServerMessage};
use crate::game_state::AppState;
use crate::player::Player;
use crate::screen::streaming::{
    fragment_frame, BackgroundEncoder, BackgroundSender, LatestCapturedFrame, ScreenStreamState,
};

use crate::screen::video_encoder::{VideoEncoder, VideoSender};

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
        );

    app.add_systems(
        Update,
        (
            receive_client_messages,
            broadcast_game_state,
            update_host_player_state,
            broadcast_video_frames,
            broadcast_screen_frames, // Fallback for clients without hardware decoding
        )
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

    // Increase send buffer for screen streaming
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        unsafe {
            let buf_size: i32 = 1024 * 1024; // 1MB send buffer
            let raw = socket.as_raw_socket();
            winapi::um::winsock2::setsockopt(
                raw as usize,
                winapi::um::winsock2::SOL_SOCKET as i32,
                winapi::um::winsock2::SO_SNDBUF as i32,
                &buf_size as *const i32 as *const i8,
                std::mem::size_of::<i32>() as i32,
            );
        }
    }

    // Clone sockets for background streaming before moving into GameServer
    let stream_socket = socket.try_clone().ok();
    let video_socket = socket.try_clone().ok();

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

    // Create background sender with cloned socket (sends from same port as server)
    if let Some(stream_socket) = stream_socket {
        commands.insert_resource(BackgroundSender::new(stream_socket));
    }

    commands.insert_resource(LocalPlayerId(host_id));
    commands.insert_resource(ServerSyncTimer(Timer::new(
        Duration::from_millis(50), // 20 updates per second
        TimerMode::Repeating,
    )));
    commands.insert_resource(ScreenStreamState::default());
    commands.insert_resource(LastStreamedFrame::default());

    // Initialize encoding - prefer hardware encoding if available
    // Try to create hardware encoder (1920x1080 @ 30fps as default)
    if let Some(video_encoder) = VideoEncoder::new(1920, 1080, 30) {
        info!("Using hardware video encoder");
        commands.insert_resource(video_encoder);

        // Create video sender with cloned socket
        if let Some(vs) = video_socket {
            commands.insert_resource(VideoSender::new(vs));
        }
    } else {
        info!("Hardware encoding not available, using JPEG fallback");
        commands.insert_resource(BackgroundEncoder::new());
    }

    info!("Server started on port {}", GAME_PORT);
}

fn cleanup_server(mut commands: Commands) {
    commands.remove_resource::<GameServer>();
    commands.remove_resource::<ServerSyncTimer>();
    commands.remove_resource::<LocalPlayerId>();
    commands.remove_resource::<ScreenStreamState>();
    commands.remove_resource::<LastStreamedFrame>();
    commands.remove_resource::<BackgroundEncoder>();
    commands.remove_resource::<BackgroundSender>();
    commands.remove_resource::<VideoEncoder>();
    commands.remove_resource::<VideoSender>();
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

/// Tracks the last frame number we submitted for encoding.
#[derive(Resource, Default)]
pub struct LastStreamedFrame(pub u64);

/// Debug counter for logging
#[derive(Resource, Default)]
pub struct StreamDebugCounter(pub u32);

fn broadcast_screen_frames(
    server: Res<GameServer>,
    latest_frame: Option<Res<LatestCapturedFrame>>,
    mut stream_state: ResMut<ScreenStreamState>,
    mut last_streamed: ResMut<LastStreamedFrame>,
    encoder: Option<Res<BackgroundEncoder>>,
    sender: Option<Res<BackgroundSender>>,
    mut debug_counter: Local<StreamDebugCounter>,
) {
    let Some(encoder) = encoder else {
        debug_counter.0 += 1;
        if debug_counter.0 % 100 == 1 {
            warn!("No BackgroundEncoder resource!");
        }
        return;
    };

    let Some(sender) = sender else {
        return;
    };

    // Submit new frames for encoding (non-blocking)
    if let Some(ref latest_frame) = latest_frame {
        let has_data = !latest_frame.rgba.is_empty();
        let is_new = latest_frame.frame_number != last_streamed.0;
        let interval_ok = stream_state.last_stream_time.elapsed() >= stream_state.stream_interval;

        if has_data && is_new && interval_ok {
            // Submit frame to background encoder (non-blocking)
            encoder.submit_frame(
                latest_frame.rgba.clone(),
                latest_frame.width,
                latest_frame.height,
            );
            last_streamed.0 = latest_frame.frame_number;
            stream_state.last_stream_time = std::time::Instant::now();
        }
    } else {
        debug_counter.0 += 1;
        if debug_counter.0 % 100 == 1 {
            warn!("No LatestCapturedFrame resource!");
        }
    }

    // Check for encoded frames and send them (non-blocking)
    if server.clients.is_empty() {
        return;
    }

    if let Some(encoded) = encoder.get_encoded() {
        // Fragment the frame
        let fragments = fragment_frame(
            encoded.jpeg_data,
            stream_state.frame_id,
            encoded.width,
            encoded.height,
        );

        // Submit to background sender (non-blocking)
        let clients: Vec<SocketAddr> = server.clients.keys().cloned().collect();
        sender.submit_fragments(fragments, clients);

        // Update frame id
        stream_state.frame_id = stream_state.frame_id.wrapping_add(1);
    }
}

/// Broadcast video frames using hardware encoding (H.264)
fn broadcast_video_frames(
    server: Res<GameServer>,
    latest_frame: Option<Res<LatestCapturedFrame>>,
    mut stream_state: ResMut<ScreenStreamState>,
    mut last_streamed: ResMut<LastStreamedFrame>,
    encoder: Option<Res<VideoEncoder>>,
    sender: Option<Res<VideoSender>>,
) {
    let Some(encoder) = encoder else {
        return;
    };

    let Some(sender) = sender else {
        return;
    };

    // Submit new frames for encoding
    if let Some(ref latest_frame) = latest_frame {
        let has_data = !latest_frame.rgba.is_empty();
        let is_new = latest_frame.frame_number != last_streamed.0;
        let interval_ok = stream_state.last_stream_time.elapsed() >= stream_state.stream_interval;

        if has_data && is_new && interval_ok {
            encoder.submit_frame(
                latest_frame.rgba.clone(),
                latest_frame.width,
                latest_frame.height,
            );
            last_streamed.0 = latest_frame.frame_number;
            stream_state.last_stream_time = std::time::Instant::now();
        }
    }

    // Check for encoded video and send
    if server.clients.is_empty() {
        return;
    }

    if let Some(encoded) = encoder.get_encoded() {
        let clients: Vec<SocketAddr> = server.clients.keys().cloned().collect();
        sender.submit_chunks(encoded.chunks, clients);
        stream_state.frame_id = stream_state.frame_id.wrapping_add(1);
    }
}

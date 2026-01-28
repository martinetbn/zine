use bevy::prelude::*;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use super::discovery::GAME_PORT;
use super::protocol::{ClientMessage, LocalPlayerId, PlayerId, PlayerState, ServerMessage};
use crate::game_state::AppState;
use crate::menu::NotificationEvent;
use crate::player::Player;
use crate::screen::streaming::{LatestCapturedFrame, ScreenStreamState};

use crate::network::protocol::AudioChunk;
use crate::screen::audio_capture::AudioCapture;
use crate::screen::audio_encoder::{AudioEncoder, AudioSender};
use crate::screen::video_encoder::{VideoEncoder, VideoSender};

/// Client timeout duration in seconds.
const CLIENT_TIMEOUT_SECS: u64 = 5;

/// Resource indicating this instance is the server/host.
#[derive(Resource)]
pub struct GameServer {
    pub socket: UdpSocket,
    pub clients: HashMap<SocketAddr, PlayerId>,
    pub client_last_activity: HashMap<SocketAddr, Instant>,
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
            check_client_timeouts,
            broadcast_game_state,
            update_host_player_state,
            broadcast_video_frames,
            broadcast_audio_frames,
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

    // Clone sockets for streaming before moving into GameServer
    let video_socket = socket.try_clone().ok();
    let audio_socket = socket.try_clone().ok();

    // Host is player 0
    let host_id: PlayerId = 0;
    let mut player_states = HashMap::new();
    player_states.insert(
        host_id,
        PlayerState {
            id: host_id,
            position: [0.0, 1.8, 4.0],
            yaw: std::f32::consts::PI,
            pitch: 0.0,
        },
    );

    commands.insert_resource(GameServer {
        socket,
        clients: HashMap::new(),
        client_last_activity: HashMap::new(),
        player_states,
        next_player_id: 1,
    });

    commands.insert_resource(LocalPlayerId(host_id));
    commands.insert_resource(ServerSyncTimer(Timer::new(
        Duration::from_millis(50), // 20 updates per second
        TimerMode::Repeating,
    )));
    commands.insert_resource(ScreenStreamState::default());
    commands.insert_resource(LastStreamedFrame::default());

    // Initialize H.264 video encoder (1920x1080 @ 30fps as default)
    if let Some(video_encoder) = VideoEncoder::new(1920, 1080, 30) {
        info!("Video encoder initialized (OpenH264)");
        commands.insert_resource(video_encoder);

        // Create video sender with cloned socket
        if let Some(vs) = video_socket {
            commands.insert_resource(VideoSender::new(vs));
        }
    } else {
        error!("Failed to initialize video encoder");
    }

    // Initialize audio capture (system loopback)
    if let Some(audio_capture) = AudioCapture::new() {
        let sample_rate = audio_capture.sample_rate;
        let channels = audio_capture.channels;
        commands.insert_resource(audio_capture);

        // Initialize Opus audio encoder
        if let Some(audio_encoder) = AudioEncoder::new(sample_rate, channels) {
            info!("Audio encoder initialized (Opus)");
            commands.insert_resource(audio_encoder);

            // Create audio sender with cloned socket
            if let Some(as_socket) = audio_socket {
                commands.insert_resource(AudioSender::new(as_socket));
            }
        } else {
            error!("Failed to initialize audio encoder");
        }
    } else {
        warn!("Failed to initialize audio capture - audio streaming disabled");
    }

    info!("Server started on port {}", GAME_PORT);
}

fn cleanup_server(mut commands: Commands) {
    commands.remove_resource::<GameServer>();
    commands.remove_resource::<ServerSyncTimer>();
    commands.remove_resource::<LocalPlayerId>();
    commands.remove_resource::<ScreenStreamState>();
    commands.remove_resource::<LastStreamedFrame>();
    commands.remove_resource::<VideoEncoder>();
    commands.remove_resource::<VideoSender>();
    commands.remove_resource::<AudioCapture>();
    commands.remove_resource::<AudioEncoder>();
    commands.remove_resource::<AudioSender>();
}

fn server_ready_check(
    mut next_state: ResMut<NextState<AppState>>,
    server: Option<Res<GameServer>>,
) {
    if server.is_some() {
        next_state.set(AppState::InGame);
    }
}

fn receive_client_messages(
    mut server: ResMut<GameServer>,
    mut notifications: EventWriter<NotificationEvent>,
) {
    let mut buf = [0u8; 1024];
    let mut players_to_remove: Vec<SocketAddr> = Vec::new();

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
                                server.client_last_activity.insert(src_addr, Instant::now());
                                server.player_states.insert(
                                    player_id,
                                    PlayerState {
                                        id: player_id,
                                        position: [0.0, 1.8, 4.0],
                                        yaw: std::f32::consts::PI,
                                        pitch: 0.0,
                                    },
                                );

                                info!("Player {} joined from {}", player_id, src_addr);
                                notifications.send(NotificationEvent("A user has joined".to_string()));

                                // Send welcome message
                                let welcome = ServerMessage::Welcome { your_id: player_id };
                                if let Ok(data) = serde_json::to_vec(&welcome) {
                                    let _ = server.socket.send_to(&data, src_addr);
                                }
                            }
                        }
                        ClientMessage::PlayerUpdate { position, yaw, pitch } => {
                            // Update player state and activity timestamp
                            if let Some(&player_id) = server.clients.get(&src_addr) {
                                server.client_last_activity.insert(src_addr, Instant::now());
                                if let Some(state) = server.player_states.get_mut(&player_id) {
                                    state.position = position;
                                    state.yaw = yaw;
                                    state.pitch = pitch;
                                }
                            }
                        }
                        ClientMessage::Leave => {
                            // Client leaving gracefully
                            if server.clients.contains_key(&src_addr) {
                                players_to_remove.push(src_addr);
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                // Client forcibly disconnected - this is expected on Windows
                // The timeout system will clean up the client
                continue;
            }
            Err(e) => {
                error!("Server receive error: {}", e);
                break;
            }
        }
    }

    // Remove players who sent Leave messages
    for addr in players_to_remove {
        remove_client(&mut server, addr, &mut notifications);
    }
}

/// Helper to remove a client and notify others.
fn remove_client(
    server: &mut GameServer,
    addr: SocketAddr,
    notifications: &mut EventWriter<NotificationEvent>,
) {
    if let Some(player_id) = server.clients.remove(&addr) {
        server.client_last_activity.remove(&addr);
        server.player_states.remove(&player_id);

        info!("Player {} left", player_id);
        notifications.send(NotificationEvent("A user has left".to_string()));

        // Notify remaining clients
        let msg = ServerMessage::PlayerLeft { id: player_id };
        if let Ok(data) = serde_json::to_vec(&msg) {
            for &client_addr in server.clients.keys() {
                let _ = server.socket.send_to(&data, client_addr);
            }
        }
    }
}

/// Check for clients that haven't sent updates and remove them.
fn check_client_timeouts(
    mut server: ResMut<GameServer>,
    mut notifications: EventWriter<NotificationEvent>,
) {
    let timeout = Duration::from_secs(CLIENT_TIMEOUT_SECS);
    let now = Instant::now();

    let timed_out: Vec<SocketAddr> = server
        .client_last_activity
        .iter()
        .filter(|(_, last_activity)| now.duration_since(**last_activity) > timeout)
        .map(|(addr, _)| *addr)
        .collect();

    for addr in timed_out {
        remove_client(&mut server, addr, &mut notifications);
    }
}

fn update_host_player_state(
    mut server: ResMut<GameServer>,
    player_query: Query<(&Transform, &crate::player::CameraController), With<Player>>,
    local_id: Res<LocalPlayerId>,
) {
    if let Ok((transform, camera_controller)) = player_query.get_single() {
        if let Some(state) = server.player_states.get_mut(&local_id.0) {
            state.position = transform.translation.into();
            // Extract yaw from rotation
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            state.yaw = yaw;
            state.pitch = camera_controller.pitch;
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

/// Broadcast video frames using H.264 encoding
fn broadcast_video_frames(
    server: Res<GameServer>,
    latest_frame: Option<Res<LatestCapturedFrame>>,
    mut stream_state: ResMut<ScreenStreamState>,
    mut last_streamed: ResMut<LastStreamedFrame>,
    encoder: Option<Res<VideoEncoder>>,
    sender: Option<Res<VideoSender>>,
) {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;
    use std::sync::Mutex;

    static SUBMITTED_FPS: AtomicU32 = AtomicU32::new(0);
    static SENT_FPS: AtomicU32 = AtomicU32::new(0);
    static LAST_LOG: Mutex<Option<Instant>> = Mutex::new(None);

    let Some(encoder) = encoder else {
        return;
    };

    let Some(sender) = sender else {
        return;
    };

    // Submit new frames for encoding - no interval gating, let encoder handle it
    if let Some(ref latest_frame) = latest_frame {
        let has_data = !latest_frame.rgba.is_empty();
        let is_new = latest_frame.frame_number != last_streamed.0;

        if has_data && is_new {
            encoder.submit_frame(
                latest_frame.rgba.clone(),
                latest_frame.width,
                latest_frame.height,
            );
            last_streamed.0 = latest_frame.frame_number;
            SUBMITTED_FPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Check for encoded video and send
    if server.clients.is_empty() {
        return;
    }

    if let Some(encoded) = encoder.get_encoded() {
        let clients: Vec<SocketAddr> = server.clients.keys().cloned().collect();
        SENT_FPS.fetch_add(1, Ordering::Relaxed);
        sender.submit_chunks(encoded.chunks, clients);
        stream_state.frame_id = stream_state.frame_id.wrapping_add(1);
    }

    // Log FPS every second
    let should_log = {
        let mut last = LAST_LOG.lock().unwrap();
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
        SUBMITTED_FPS.swap(0, Ordering::Relaxed);
        SENT_FPS.swap(0, Ordering::Relaxed);
    }
}

/// Broadcast audio frames to all connected clients.
fn broadcast_audio_frames(
    server: Res<GameServer>,
    audio_capture: Option<Res<AudioCapture>>,
    audio_encoder: Option<Res<AudioEncoder>>,
    audio_sender: Option<Res<AudioSender>>,
) {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;
    use std::sync::Mutex;

    static CAPTURED_COUNT: AtomicU32 = AtomicU32::new(0);
    static SENT_COUNT: AtomicU32 = AtomicU32::new(0);
    static LAST_LOG: Mutex<Option<Instant>> = Mutex::new(None);

    let Some(capture) = audio_capture else {
        return;
    };

    let Some(encoder) = audio_encoder else {
        return;
    };

    let Some(sender) = audio_sender else {
        return;
    };

    // Capture audio samples and submit for encoding
    while let Some(samples) = capture.try_recv() {
        CAPTURED_COUNT.fetch_add(1, Ordering::Relaxed);
        encoder.submit_samples(samples, capture.sample_rate, capture.channels);
    }

    // Don't send if no clients
    if server.clients.is_empty() {
        return;
    }

    // Get encoded audio and send
    while let Some(encoded) = encoder.get_encoded() {
        let chunk = AudioChunk::new(
            encoded.sequence,
            encoded.sample_rate,
            encoded.channels,
            encoded.data,
        );
        let clients: Vec<SocketAddr> = server.clients.keys().cloned().collect();
        sender.send(chunk, clients);
        SENT_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // Log audio stats every second
    let should_log = {
        let mut last = LAST_LOG.lock().unwrap();
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
        let captured = CAPTURED_COUNT.swap(0, Ordering::Relaxed);
        let sent = SENT_COUNT.swap(0, Ordering::Relaxed);
        if captured > 0 || sent > 0 {
            info!("Server audio - captured: {}/s, sent: {}/s", captured, sent);
        }
    }
}

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// Port used for LAN discovery broadcasts.
pub const DISCOVERY_PORT: u16 = 7777;

/// Port used for game connections.
pub const GAME_PORT: u16 = 5000;

/// Information about a LAN session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LanSession {
    pub name: String,
    pub address: SocketAddr,
    pub player_count: u32,
}

/// Resource holding the currently selected session to connect to.
#[derive(Resource, Clone)]
pub struct SelectedSession(pub LanSession);

/// Resource holding all discovered sessions.
#[derive(Resource, Default)]
pub struct DiscoveredSessions(pub Vec<LanSession>);

/// Resource for the discovery broadcast socket (host).
#[derive(Resource)]
pub struct BroadcastSocket(pub UdpSocket);

/// Resource for the discovery listener socket (client).
#[derive(Resource)]
pub struct ListenerSocket(pub UdpSocket);

/// Timer for broadcast intervals.
#[derive(Resource)]
pub struct BroadcastTimer(pub Timer);

/// Announcement packet sent by hosts.
#[derive(Serialize, Deserialize)]
struct SessionAnnouncement {
    name: String,
    port: u16,
    player_count: u32,
}

pub fn setup_broadcast(mut commands: Commands) {
    // Create broadcast socket
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to create broadcast socket: {}", e);
            return;
        }
    };

    if let Err(e) = socket.set_broadcast(true) {
        error!("Failed to enable broadcast: {}", e);
        return;
    }

    if let Err(e) = socket.set_nonblocking(true) {
        error!("Failed to set non-blocking: {}", e);
        return;
    }

    commands.insert_resource(BroadcastSocket(socket));
    commands.insert_resource(BroadcastTimer(Timer::new(
        Duration::from_secs(1),
        TimerMode::Repeating,
    )));

    info!("Broadcasting session on LAN");
}

pub fn cleanup_broadcast(mut commands: Commands) {
    commands.remove_resource::<BroadcastSocket>();
    commands.remove_resource::<BroadcastTimer>();
}

pub fn broadcast_session(
    time: Res<Time>,
    timer: Option<ResMut<BroadcastTimer>>,
    socket: Option<Res<BroadcastSocket>>,
) {
    let (Some(socket), Some(mut timer)) = (socket, timer) else {
        return;
    };

    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let announcement = SessionAnnouncement {
        name: "Local Game".to_string(),
        port: GAME_PORT,
        player_count: 1,
    };

    let data = match serde_json::to_vec(&announcement) {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to serialize announcement: {}", e);
            return;
        }
    };

    // Broadcast to LAN
    let broadcast_addr = format!("255.255.255.255:{}", DISCOVERY_PORT);
    if let Err(e) = socket.0.send_to(&data, &broadcast_addr) {
        // WouldBlock is expected for non-blocking sockets
        if e.kind() != std::io::ErrorKind::WouldBlock {
            error!("Failed to broadcast: {}", e);
        }
    }
}

pub fn setup_listener(mut commands: Commands) {
    // Create listener socket
    let socket = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to create listener socket: {}", e);
            return;
        }
    };

    if let Err(e) = socket.set_nonblocking(true) {
        error!("Failed to set non-blocking: {}", e);
        return;
    }

    commands.insert_resource(ListenerSocket(socket));
    commands.insert_resource(DiscoveredSessions::default());

    info!("Listening for LAN sessions");
}

pub fn cleanup_listener(mut commands: Commands) {
    commands.remove_resource::<ListenerSocket>();
}

pub fn listen_for_sessions(
    socket: Option<Res<ListenerSocket>>,
    mut sessions: ResMut<DiscoveredSessions>,
) {
    let Some(socket) = socket else { return };

    let mut buf = [0u8; 1024];

    // Try to receive announcements
    loop {
        match socket.0.recv_from(&mut buf) {
            Ok((len, src_addr)) => {
                if let Ok(announcement) = serde_json::from_slice::<SessionAnnouncement>(&buf[..len])
                {
                    let session = LanSession {
                        name: announcement.name,
                        address: SocketAddr::new(src_addr.ip(), announcement.port),
                        player_count: announcement.player_count,
                    };

                    // Update or add session
                    if let Some(existing) = sessions
                        .0
                        .iter_mut()
                        .find(|s| s.address.ip() == session.address.ip())
                    {
                        existing.name = session.name;
                        existing.player_count = session.player_count;
                    } else {
                        info!("Discovered session: {} at {}", session.name, session.address);
                        sessions.0.push(session);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No more data available
                break;
            }
            Err(e) => {
                error!("Error receiving: {}", e);
                break;
            }
        }
    }
}

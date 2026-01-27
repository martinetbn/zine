use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Unique identifier for a player in the session.
pub type PlayerId = u64;

/// Messages sent from client to server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    /// Client sending their current position and rotation.
    PlayerUpdate { position: [f32; 3], yaw: f32 },
    /// Client requesting to join.
    Join,
}

/// Messages sent from server to clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    /// Welcome message with assigned player ID.
    Welcome { your_id: PlayerId },
    /// Update containing all player states.
    GameState { players: Vec<PlayerState> },
    /// A player has disconnected.
    PlayerLeft { id: PlayerId },
}

/// State of a single player, broadcast by the server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerState {
    pub id: PlayerId,
    pub position: [f32; 3],
    pub yaw: f32,
}

/// Resource storing the local player's network ID.
#[derive(Resource)]
pub struct LocalPlayerId(pub PlayerId);

/// Component marking a remote player entity.
#[derive(Component)]
pub struct RemotePlayer {
    pub id: PlayerId,
}

/// Resource tracking all known remote players for the client.
#[derive(Resource, Default)]
pub struct RemotePlayers {
    pub players: Vec<PlayerState>,
}

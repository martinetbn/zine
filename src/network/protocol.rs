use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
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
    /// Screen frame fragment for streaming (JPEG fallback).
    ScreenFrame(ScreenFragment),
    /// H.264 video frame chunk for hardware-accelerated streaming.
    VideoFrame(VideoChunk),
    /// Video codec information for client initialization.
    VideoCodec(VideoCodecInfo),
}

/// A fragment of a screen frame for network transmission.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenFragment {
    /// Frame sequence number.
    pub frame_id: u32,
    /// Fragment index within this frame.
    pub fragment_idx: u16,
    /// Total number of fragments for this frame.
    pub total_fragments: u16,
    /// Frame width (sent in first fragment).
    pub width: u32,
    /// Frame height (sent in first fragment).
    pub height: u32,
    /// JPEG data chunk (base64 encoded for efficient JSON serialization).
    pub data: String,
}

impl ScreenFragment {
    /// Create a new fragment with base64-encoded data.
    pub fn new(
        frame_id: u32,
        fragment_idx: u16,
        total_fragments: u16,
        width: u32,
        height: u32,
        raw_data: &[u8],
    ) -> Self {
        Self {
            frame_id,
            fragment_idx,
            total_fragments,
            width,
            height,
            data: BASE64.encode(raw_data),
        }
    }

    /// Decode the base64 data back to bytes.
    pub fn decode_data(&self) -> Option<Vec<u8>> {
        BASE64.decode(&self.data).ok()
    }
}

/// H.264 video chunk for hardware-accelerated streaming.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoChunk {
    /// Frame sequence number.
    pub frame_id: u32,
    /// Chunk index within this frame.
    pub chunk_idx: u16,
    /// Total chunks for this frame.
    pub total_chunks: u16,
    /// Whether this is a keyframe (I-frame).
    pub is_keyframe: bool,
    /// H.264 NAL unit data (base64 encoded).
    data_b64: String,
}

impl VideoChunk {
    pub fn new(frame_id: u32, chunk_idx: u16, total_chunks: u16, is_keyframe: bool, data: Vec<u8>) -> Self {
        Self {
            frame_id,
            chunk_idx,
            total_chunks,
            is_keyframe,
            data_b64: BASE64.encode(&data),
        }
    }

    pub fn decode_data(&self) -> Option<Vec<u8>> {
        BASE64.decode(&self.data_b64).ok()
    }

    // For internal use - direct data access
    pub fn data(&self) -> Vec<u8> {
        self.decode_data().unwrap_or_default()
    }
}

/// Video codec information sent to clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoCodecInfo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Codec extradata (SPS/PPS for H.264).
    pub extradata: Vec<u8>,
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

/// Component storing the target transform for interpolation.
#[derive(Component)]
pub struct NetworkTransform {
    pub target_position: Vec3,
    pub target_yaw: f32,
}

/// Resource tracking all known remote players for the client.
#[derive(Resource, Default)]
pub struct RemotePlayers {
    pub players: Vec<PlayerState>,
}

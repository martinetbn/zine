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
    /// H.264 video frame chunk for streaming.
    VideoFrame(VideoChunk),
    /// Video codec information for client initialization.
    VideoCodec(VideoCodecInfo),
    /// Opus audio chunk for streaming.
    AudioFrame(AudioChunk),
}

/// H.264 video chunk for streaming.
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

/// Opus audio chunk for streaming.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioChunk {
    /// Sequence number for ordering and loss detection.
    pub sequence: u32,
    /// Sample rate of the audio (e.g., 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u8,
    /// Opus-encoded audio data (base64 encoded).
    data_b64: String,
}

impl AudioChunk {
    pub fn new(sequence: u32, sample_rate: u32, channels: u8, data: Vec<u8>) -> Self {
        Self {
            sequence,
            sample_rate,
            channels,
            data_b64: BASE64.encode(&data),
        }
    }

    pub fn decode_data(&self) -> Option<Vec<u8>> {
        BASE64.decode(&self.data_b64).ok()
    }

    pub fn data(&self) -> Vec<u8> {
        self.decode_data().unwrap_or_default()
    }
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

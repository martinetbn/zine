use bevy::prelude::*;
use std::time::{Duration, Instant};

/// Resource holding the latest captured frame for streaming.
#[derive(Resource, Default)]
pub struct LatestCapturedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_number: u64,
}

/// Resource tracking screen streaming state.
#[derive(Resource)]
pub struct ScreenStreamState {
    pub frame_id: u32,
    pub last_stream_time: Instant,
    pub stream_interval: Duration,
}

impl Default for ScreenStreamState {
    fn default() -> Self {
        Self {
            frame_id: 0,
            last_stream_time: Instant::now() - Duration::from_secs(1),
            stream_interval: Duration::from_millis(16), // ~60fps target
        }
    }
}

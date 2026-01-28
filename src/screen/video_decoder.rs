//! H.264 video decoder using OpenH264.
//!
//! Uses Cisco's OpenH264 library for software H.264 decoding.
//! No external dependencies required - the library is downloaded automatically at build time.

use bevy::prelude::*;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use crate::network::protocol::{VideoChunk, VideoCodecInfo};

/// Decoded frame ready for display
pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_id: u32,
}

/// Timeout for incomplete frame assembly (ms)
const FRAME_ASSEMBLY_TIMEOUT_MS: u64 = 200;

/// Resource for video frame assembly and decoding
#[derive(Resource)]
pub struct VideoDecoder {
    /// Send assembled NAL units for decoding
    send_data: Mutex<Sender<(Vec<u8>, u32)>>,
    /// Receive decoded RGBA frames
    recv_decoded: Mutex<Receiver<DecodedFrame>>,
    /// Current frame being assembled
    current_frame_id: u32,
    /// Chunks for current frame
    chunks: Vec<Option<Vec<u8>>>,
    /// Total chunks expected
    total_chunks: u16,
    /// Chunks received count
    received_count: u16,
    /// When we started assembling current frame
    frame_start_time: Option<Instant>,
}

impl VideoDecoder {
    pub fn new() -> Option<Self> {
        let (data_tx, data_rx) = mpsc::channel::<(Vec<u8>, u32)>();
        let (decoded_tx, decoded_rx) = mpsc::channel::<DecodedFrame>();

        // Spawn decoder thread
        thread::spawn(move || {
            run_decoder_thread(data_rx, decoded_tx);
        });

        Some(Self {
            send_data: Mutex::new(data_tx),
            recv_decoded: Mutex::new(decoded_rx),
            current_frame_id: 0,
            chunks: Vec::new(),
            total_chunks: 0,
            received_count: 0,
            frame_start_time: None,
        })
    }

    /// Set codec information from server (not needed for OpenH264 but kept for API compatibility)
    pub fn set_codec_info(&mut self, _info: VideoCodecInfo) {
        // OpenH264 decoder auto-detects dimensions from the stream
    }

    /// Add a received video chunk
    pub fn add_chunk(&mut self, chunk: VideoChunk) {
        let is_newer = chunk.frame_id > self.current_frame_id
            || (chunk.frame_id == 0 && self.current_frame_id > 1000);
        let is_first = self.total_chunks == 0;

        // Check if current frame assembly has timed out
        let timed_out = self.frame_start_time
            .map(|t| t.elapsed().as_millis() > FRAME_ASSEMBLY_TIMEOUT_MS as u128)
            .unwrap_or(false);

        // Reset for new frame or timeout
        if is_first || is_newer || timed_out {
            if timed_out && self.received_count > 0 {
                warn!(
                    "Frame {} timed out ({}/{} chunks), skipping to frame {}",
                    self.current_frame_id, self.received_count, self.total_chunks, chunk.frame_id
                );
            }
            self.current_frame_id = chunk.frame_id;
            self.total_chunks = chunk.total_chunks;
            self.chunks = vec![None; chunk.total_chunks as usize];
            self.received_count = 0;
            self.frame_start_time = Some(Instant::now());
        }

        // Ignore old frames
        if chunk.frame_id != self.current_frame_id {
            return;
        }

        // Store chunk
        let idx = chunk.chunk_idx as usize;
        if idx < self.chunks.len() && self.chunks[idx].is_none() {
            self.chunks[idx] = Some(chunk.decode_data().unwrap_or_default());
            self.received_count += 1;

            // Check if complete
            if self.received_count == self.total_chunks {
                // Assemble NAL data
                let mut data = Vec::new();
                for c in &self.chunks {
                    if let Some(d) = c {
                        data.extend_from_slice(d);
                    }
                }

                static ASSEMBLED_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let count = ASSEMBLED_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 30 == 0 {
                    info!("Assembled complete frame {} ({} bytes)", count, data.len());
                }

                // Send for decoding
                if let Ok(sender) = self.send_data.lock() {
                    let _ = sender.send((data, self.current_frame_id));
                }

                // Reset
                self.chunks.clear();
                self.received_count = 0;
                self.total_chunks = 0;
                self.frame_start_time = None;
            }
        }
    }

    /// Get decoded frame if available
    pub fn get_decoded(&self) -> Option<DecodedFrame> {
        if let Ok(receiver) = self.recv_decoded.lock() {
            let mut latest = None;
            while let Ok(frame) = receiver.try_recv() {
                latest = Some(frame);
            }
            latest
        } else {
            None
        }
    }

    /// Reset frame assembly state (call when connection issues detected)
    pub fn reset_assembly(&mut self) {
        self.chunks.clear();
        self.received_count = 0;
        self.total_chunks = 0;
        self.frame_start_time = None;
        info!("Video decoder assembly state reset");
    }
}

/// Convert YUV420 planar to RGBA
fn yuv420_to_rgba(y_plane: &[u8], u_plane: &[u8], v_plane: &[u8], width: usize, height: usize, y_stride: usize, uv_stride: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; width * height * 4];

    for row in 0..height {
        for col in 0..width {
            let y_idx = row * y_stride + col;
            let uv_idx = (row / 2) * uv_stride + (col / 2);

            let y = y_plane[y_idx] as i32;
            let u = u_plane[uv_idx] as i32;
            let v = v_plane[uv_idx] as i32;

            // YUV to RGB conversion (BT.601)
            let c = y - 16;
            let d = u - 128;
            let e = v - 128;

            let r = ((298 * c + 409 * e + 128) >> 8).clamp(0, 255) as u8;
            let g = ((298 * c - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
            let b = ((298 * c + 516 * d + 128) >> 8).clamp(0, 255) as u8;

            let rgba_idx = (row * width + col) * 4;
            rgba[rgba_idx] = r;
            rgba[rgba_idx + 1] = g;
            rgba[rgba_idx + 2] = b;
            rgba[rgba_idx + 3] = 255;
        }
    }

    rgba
}

/// Run the decoder thread using OpenH264
fn run_decoder_thread(data_rx: Receiver<(Vec<u8>, u32)>, decoded_tx: Sender<DecodedFrame>) {
    let mut decoder = match Decoder::new() {
        Ok(dec) => dec,
        Err(e) => {
            error!("Failed to create OpenH264 decoder: {:?}", e);
            return;
        }
    };

    info!("Video decoder started using OpenH264");

    let mut consecutive_errors: u32 = 0;
    let mut last_successful_decode = Instant::now();

    while let Ok((mut data, mut frame_id)) = data_rx.recv() {
        // Skip to latest data
        while let Ok((newer_data, newer_id)) = data_rx.try_recv() {
            data = newer_data;
            frame_id = newer_id;
        }

        // Reset decoder if too many consecutive errors or long time since success
        if consecutive_errors > 30 || last_successful_decode.elapsed().as_secs() > 5 {
            info!("Resetting decoder after {} errors or {}s without success",
                consecutive_errors, last_successful_decode.elapsed().as_secs());
            decoder = match Decoder::new() {
                Ok(dec) => dec,
                Err(e) => {
                    error!("Failed to recreate decoder: {:?}", e);
                    continue;
                }
            };
            consecutive_errors = 0;
        }

        // Decode the H.264 data
        match decoder.decode(&data) {
            Ok(Some(yuv)) => {
                consecutive_errors = 0;
                last_successful_decode = Instant::now();

                let (width, height) = yuv.dimensions();

                // Get YUV planes
                let y_plane = yuv.y();
                let u_plane = yuv.u();
                let v_plane = yuv.v();

                // Get strides (y_stride, u_stride, v_stride)
                let (y_stride, u_stride, _v_stride) = yuv.strides();

                // Convert to RGBA
                let rgba = yuv420_to_rgba(y_plane, u_plane, v_plane, width, height, y_stride, u_stride);

                static DECODED_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let count = DECODED_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 30 == 0 {
                    info!("Decoded frame {} ({}x{})", count, width, height);
                }

                let _ = decoded_tx.send(DecodedFrame {
                    rgba,
                    width: width as u32,
                    height: height as u32,
                    frame_id,
                });
            }
            Ok(None) => {
                // No frame produced (might need more data or waiting for keyframe)
                consecutive_errors += 1;
            }
            Err(e) => {
                consecutive_errors += 1;
                // Only log occasionally to avoid spam
                static LAST_ERROR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let last = LAST_ERROR.load(std::sync::atomic::Ordering::Relaxed);
                if now > last + 5 {
                    LAST_ERROR.store(now, std::sync::atomic::Ordering::Relaxed);
                    warn!("Decode error (may need keyframe): {:?}", e);
                }
            }
        }
    }
}

/// Jitter buffer for video frames
#[derive(Resource)]
pub struct VideoJitterBuffer {
    frames: VecDeque<DecodedFrame>,
    target_size: usize,
    min_delay: std::time::Duration,
    last_released_id: u32,
    frame_times: VecDeque<Instant>,
}

impl Default for VideoJitterBuffer {
    fn default() -> Self {
        Self {
            frames: VecDeque::with_capacity(8),
            target_size: 2,
            min_delay: std::time::Duration::from_millis(20), // ~1.2 frames at 60fps
            last_released_id: 0,
            frame_times: VecDeque::with_capacity(8),
        }
    }
}

impl VideoJitterBuffer {
    pub fn push(&mut self, frame: DecodedFrame) {
        if frame.frame_id <= self.last_released_id && self.last_released_id > 0 {
            return;
        }

        // Insert sorted by frame_id
        let pos = self.frames.iter().position(|f| f.frame_id > frame.frame_id);
        let now = Instant::now();
        match pos {
            Some(i) => {
                self.frames.insert(i, frame);
                self.frame_times.insert(i, now);
            }
            None => {
                self.frames.push_back(frame);
                self.frame_times.push_back(now);
            }
        }

        // Limit buffer size
        while self.frames.len() > self.target_size * 2 {
            self.frames.pop_front();
            self.frame_times.pop_front();
        }
    }

    pub fn pop(&mut self) -> Option<DecodedFrame> {
        let should_release = if let Some(oldest_time) = self.frame_times.front() {
            self.frames.len() >= self.target_size || oldest_time.elapsed() >= self.min_delay
        } else {
            false
        };

        if should_release {
            self.frame_times.pop_front();
            if let Some(frame) = self.frames.pop_front() {
                self.last_released_id = frame.frame_id;
                return Some(frame);
            }
        }

        None
    }
}

//! H.264 video encoder using OpenH264.
//!
//! Uses Cisco's OpenH264 library for software H.264 encoding.
//! No external dependencies required - the library is downloaded automatically at build time.

use bevy::prelude::*;
use openh264::encoder::{Encoder, EncoderConfig};
use openh264::OpenH264API;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::network::protocol::{ServerMessage, VideoChunk};

/// Frame to be encoded
struct FrameToEncode {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Encoded video data ready to send
pub struct EncodedVideoData {
    pub chunks: Vec<VideoChunk>,
}

/// Maximum chunk size for network transmission
const MAX_CHUNK_SIZE: usize = 4000;

/// Resource for background H.264 encoding with dynamic resolution support
#[derive(Resource)]
pub struct VideoEncoder {
    send_frame: Mutex<Sender<FrameToEncode>>,
    recv_encoded: Mutex<Receiver<EncodedVideoData>>,
}

impl VideoEncoder {
    /// Create a new video encoder with dynamic resolution support.
    pub fn new(_width: u32, _height: u32, _fps: u32) -> Option<Self> {
        let (frame_tx, frame_rx) = mpsc::channel::<FrameToEncode>();
        let (encoded_tx, encoded_rx) = mpsc::channel::<EncodedVideoData>();

        // Spawn encoding thread - will adapt to incoming frame dimensions
        thread::spawn(move || {
            run_encoder_thread(frame_rx, encoded_tx);
        });

        Some(Self {
            send_frame: Mutex::new(frame_tx),
            recv_encoded: Mutex::new(encoded_rx),
        })
    }

    /// Submit a frame for encoding (non-blocking)
    pub fn submit_frame(&self, rgba: Vec<u8>, width: u32, height: u32) {
        if let Ok(sender) = self.send_frame.lock() {
            let _ = sender.send(FrameToEncode { rgba, width, height });
        }
    }

    /// Get encoded video data if available (non-blocking)
    pub fn get_encoded(&self) -> Option<EncodedVideoData> {
        if let Ok(receiver) = self.recv_encoded.lock() {
            let mut latest = None;
            while let Ok(data) = receiver.try_recv() {
                latest = Some(data);
            }
            latest
        } else {
            None
        }
    }
}

/// Convert RGBA to YUV420 frame for OpenH264
fn rgba_to_yuv_frame(rgba: &[u8], width: u32, height: u32) -> YuvFrame {
    let w = width as usize;
    let h = height as usize;

    // YUV420: Y plane (w*h) + U plane (w/2 * h/2) + V plane (w/2 * h/2)
    let y_size = w * h;
    let uv_size = (w / 2) * (h / 2);

    let mut y_plane = vec![0u8; y_size];
    let mut u_plane = vec![0u8; uv_size];
    let mut v_plane = vec![0u8; uv_size];

    // Convert each pixel
    for y in 0..h {
        for x in 0..w {
            let rgba_idx = (y * w + x) * 4;
            let r = rgba[rgba_idx] as i32;
            let g = rgba[rgba_idx + 1] as i32;
            let b = rgba[rgba_idx + 2] as i32;

            // RGB to YUV conversion (BT.601)
            let y_val = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
            y_plane[y * w + x] = y_val.clamp(0, 255) as u8;

            // Subsample U and V (2x2 blocks)
            if y % 2 == 0 && x % 2 == 0 {
                let uv_idx = (y / 2) * (w / 2) + (x / 2);
                let u_val = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
                let v_val = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
                u_plane[uv_idx] = u_val.clamp(0, 255) as u8;
                v_plane[uv_idx] = v_val.clamp(0, 255) as u8;
            }
        }
    }

    YuvFrame {
        y: y_plane,
        u: u_plane,
        v: v_plane,
        width: w,
        height: h,
    }
}

/// YUV420 buffer that implements YUVSource for OpenH264
struct YuvFrame {
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    width: usize,
    height: usize,
}

impl openh264::formats::YUVSource for YuvFrame {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }

    fn y(&self) -> &[u8] {
        &self.y
    }

    fn u(&self) -> &[u8] {
        &self.u
    }

    fn v(&self) -> &[u8] {
        &self.v
    }
}

/// Run the encoder thread using OpenH264 with dynamic resolution support
fn run_encoder_thread(
    frame_rx: Receiver<FrameToEncode>,
    encoded_tx: Sender<EncodedVideoData>,
) {
    let mut encoder: Option<Encoder> = None;
    let mut current_width: u32 = 0;
    let mut current_height: u32 = 0;
    let mut frame_count: u32 = 0;

    info!("Video encoder thread started (dynamic resolution)");

    while let Ok(mut frame) = frame_rx.recv() {
        // Skip to latest frame
        while let Ok(newer) = frame_rx.try_recv() {
            frame = newer;
        }

        // Validate frame size
        let expected_size = (frame.width * frame.height * 4) as usize;
        if frame.rgba.len() != expected_size {
            continue;
        }

        // Check if we need to create encoder (first frame or resolution change)
        if encoder.is_none() || frame.width != current_width || frame.height != current_height {
            info!(
                "Creating encoder for {}x{} (was {}x{})",
                frame.width, frame.height, current_width, current_height
            );
            // Configure encoder for 60fps with good quality (10 Mbps)
            let config = EncoderConfig::new()
                .set_bitrate_bps(10_000_000)
                .max_frame_rate(60.0)
                .enable_skip_frame(false);
            let api = OpenH264API::from_source();
            encoder = match Encoder::with_api_config(api, config) {
                Ok(enc) => {
                    current_width = frame.width;
                    current_height = frame.height;
                    Some(enc)
                }
                Err(e) => {
                    error!("Failed to create OpenH264 encoder: {:?}", e);
                    continue;
                }
            };
        }

        // Force keyframe every 60 frames for late-joining clients
        if let Some(ref mut enc) = encoder {
            if frame_count > 0 && frame_count % 60 == 0 {
                info!("Forcing keyframe at frame {}", frame_count);
                enc.force_intra_frame();
            }
        }

        let Some(ref mut enc) = encoder else {
            continue;
        };

        // Convert RGBA to YUV420 planes
        let yuv_frame = rgba_to_yuv_frame(&frame.rgba, frame.width, frame.height);

        if frame_count == 0 {
            info!("YUV frame: y={} u={} v={} dims={}x{}",
                yuv_frame.y.len(), yuv_frame.u.len(), yuv_frame.v.len(),
                yuv_frame.width, yuv_frame.height);
        }

        // Encode the frame
        match enc.encode(&yuv_frame) {
            Ok(bitstream) => {
                // Get encoded data as bytes
                let encoded_data = bitstream.to_vec();

                if frame_count % 30 == 0 {
                    info!("Encoded frame {}: {} bytes", frame_count, encoded_data.len());
                }

                if !encoded_data.is_empty() {
                    // Check if this is a keyframe (IDR NAL unit type = 5)
                    let is_keyframe = encoded_data.len() > 4 && (encoded_data[4] & 0x1F) == 5;

                    // Fragment into chunks for network transmission
                    let total_chunks =
                        ((encoded_data.len() + MAX_CHUNK_SIZE - 1) / MAX_CHUNK_SIZE) as u16;
                    let chunks: Vec<VideoChunk> = encoded_data
                        .chunks(MAX_CHUNK_SIZE)
                        .enumerate()
                        .map(|(idx, chunk)| {
                            VideoChunk::new(
                                frame_count,
                                idx as u16,
                                total_chunks,
                                is_keyframe && idx == 0,
                                chunk.to_vec(),
                            )
                        })
                        .collect();

                    let _ = encoded_tx.send(EncodedVideoData { chunks });
                    frame_count = frame_count.wrapping_add(1);
                }
            }
            Err(e) => {
                error!("Encoding error: {:?}", e);
            }
        }
    }
}

/// Resource for background video chunk sending
#[derive(Resource)]
pub struct VideoSender {
    send_chunks: Mutex<Sender<(Vec<VideoChunk>, Vec<SocketAddr>)>>,
}

impl VideoSender {
    pub fn new(socket: std::net::UdpSocket) -> Self {
        let (chunks_tx, chunks_rx) = mpsc::channel::<(Vec<VideoChunk>, Vec<SocketAddr>)>();

        thread::spawn(move || {
            while let Ok((chunks, clients)) = chunks_rx.recv() {
                // Skip to latest
                let (mut chunks, mut clients) = (chunks, clients);
                while let Ok((newer_chunks, newer_clients)) = chunks_rx.try_recv() {
                    chunks = newer_chunks;
                    clients = newer_clients;
                }

                for (i, chunk) in chunks.into_iter().enumerate() {
                    let msg = ServerMessage::VideoFrame(chunk);
                    if let Ok(data) = serde_json::to_vec(&msg) {
                        for client in &clients {
                            let _ = socket.send_to(&data, client);
                        }
                    }

                    // Minimal pacing
                    if i > 0 && i % 5 == 0 {
                        thread::sleep(Duration::from_micros(100));
                    }
                }
            }
        });

        Self {
            send_chunks: Mutex::new(chunks_tx),
        }
    }

    pub fn submit_chunks(&self, chunks: Vec<VideoChunk>, clients: Vec<SocketAddr>) {
        if clients.is_empty() || chunks.is_empty() {
            return;
        }
        if let Ok(sender) = self.send_chunks.lock() {
            let _ = sender.send((chunks, clients));
        }
    }
}

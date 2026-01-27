use bevy::prelude::*;
use image::ImageReader;
use jpeg_encoder::{ColorType, Encoder};
use std::collections::VecDeque;
use std::io::Cursor;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::network::protocol::ScreenFragment;

/// Maximum size of raw data in a single fragment.
const FRAGMENT_SIZE: usize = 4000;

/// Minimum JPEG quality.
const MIN_QUALITY: u8 = 40;
/// Maximum JPEG quality.
const MAX_QUALITY: u8 = 85;
/// Starting JPEG quality.
const INITIAL_QUALITY: u8 = 70;

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
            stream_interval: Duration::from_millis(33), // ~30fps target
        }
    }
}

/// Encoded frame ready to be sent
pub struct EncodedFrame {
    pub jpeg_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Frame to be encoded
struct FrameToEncode {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Adaptive quality controller - shared between encoder and main thread
struct AdaptiveQuality {
    current_quality: AtomicU8,
    pending_frames: AtomicU32,
}

impl AdaptiveQuality {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            current_quality: AtomicU8::new(INITIAL_QUALITY),
            pending_frames: AtomicU32::new(0),
        })
    }

    fn get_quality(&self) -> u8 {
        self.current_quality.load(Ordering::Relaxed)
    }

    fn increment_pending(&self) {
        self.pending_frames.fetch_add(1, Ordering::Relaxed);
    }

    fn decrement_pending(&self) {
        self.pending_frames.fetch_sub(1, Ordering::Relaxed);
    }

    /// Adjust quality based on encoding backlog
    fn adjust(&self) {
        let pending = self.pending_frames.load(Ordering::Relaxed);
        let current = self.current_quality.load(Ordering::Relaxed);

        let new_quality = if pending > 3 {
            // Falling behind - reduce quality significantly
            current.saturating_sub(10).max(MIN_QUALITY)
        } else if pending > 1 {
            // Slight backlog - reduce quality a bit
            current.saturating_sub(3).max(MIN_QUALITY)
        } else if pending == 0 {
            // Keeping up - can increase quality
            (current + 2).min(MAX_QUALITY)
        } else {
            current
        };

        if new_quality != current {
            self.current_quality.store(new_quality, Ordering::Relaxed);
        }
    }
}

/// Resource for background JPEG encoding with adaptive quality
#[derive(Resource)]
pub struct BackgroundEncoder {
    send_frame: Mutex<Sender<FrameToEncode>>,
    recv_encoded: Mutex<Receiver<EncodedFrame>>,
    quality: Arc<AdaptiveQuality>,
}

impl BackgroundEncoder {
    pub fn new() -> Self {
        let (frame_tx, frame_rx) = mpsc::channel::<FrameToEncode>();
        let (encoded_tx, encoded_rx) = mpsc::channel::<EncodedFrame>();
        let quality = AdaptiveQuality::new();
        let quality_clone = Arc::clone(&quality);

        // Spawn encoding thread
        thread::spawn(move || {
            while let Ok(mut frame) = frame_rx.recv() {
                // Skip to the latest frame - discard any queued older frames
                let mut skipped = 0;
                while let Ok(newer_frame) = frame_rx.try_recv() {
                    frame = newer_frame;
                    skipped += 1;
                }

                // Update pending count (we're processing 1, skipped the rest)
                for _ in 0..skipped {
                    quality_clone.decrement_pending();
                }

                // Adjust quality based on backlog
                quality_clone.adjust();
                let q = quality_clone.get_quality();

                // Encode with adaptive quality using fast jpeg-encoder
                if let Some(jpeg_data) = encode_jpeg_fast(&frame.rgba, frame.width, frame.height, q) {
                    let _ = encoded_tx.send(EncodedFrame {
                        jpeg_data,
                        width: frame.width,
                        height: frame.height,
                    });
                }

                quality_clone.decrement_pending();
            }
        });

        Self {
            send_frame: Mutex::new(frame_tx),
            recv_encoded: Mutex::new(encoded_rx),
            quality,
        }
    }

    /// Submit a frame for encoding (non-blocking)
    pub fn submit_frame(&self, rgba: Vec<u8>, width: u32, height: u32) {
        if let Ok(sender) = self.send_frame.lock() {
            self.quality.increment_pending();
            let _ = sender.send(FrameToEncode { rgba, width, height });
        }
    }

    /// Get encoded frame if available (non-blocking)
    pub fn get_encoded(&self) -> Option<EncodedFrame> {
        if let Ok(receiver) = self.recv_encoded.lock() {
            let mut latest = None;
            while let Ok(frame) = receiver.try_recv() {
                latest = Some(frame);
            }
            latest
        } else {
            None
        }
    }

    /// Get current adaptive quality level
    pub fn current_quality(&self) -> u8 {
        self.quality.get_quality()
    }
}

/// Fast JPEG encoding using jpeg-encoder crate (much faster than image crate)
fn encode_jpeg_fast(rgba: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    let expected_size = (width * height * 4) as usize;
    if rgba.len() != expected_size {
        return None;
    }

    // Convert RGBA to RGB (JPEG doesn't support alpha)
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for pixel in rgba.chunks_exact(4) {
        rgb.push(pixel[0]); // R
        rgb.push(pixel[1]); // G
        rgb.push(pixel[2]); // B
    }

    // Encode using fast jpeg-encoder
    let mut output = Vec::new();
    let encoder = Encoder::new(&mut output, quality);

    if encoder.encode(&rgb, width as u16, height as u16, ColorType::Rgb).is_err() {
        return None;
    }

    Some(output)
}

/// Data to send to clients
struct FragmentsToSend {
    fragments: Vec<ScreenFragment>,
    clients: Vec<SocketAddr>,
}

/// Resource for background fragment sending (avoids blocking main thread)
#[derive(Resource)]
pub struct BackgroundSender {
    send_fragments: Mutex<Sender<FragmentsToSend>>,
}

impl BackgroundSender {
    pub fn new(socket: UdpSocket) -> Self {
        let (frag_tx, frag_rx) = mpsc::channel::<FragmentsToSend>();

        // Spawn sending thread (owns the socket)
        thread::spawn(move || {
            while let Ok(mut to_send) = frag_rx.recv() {
                // Skip to the latest frame's fragments
                while let Ok(newer) = frag_rx.try_recv() {
                    to_send = newer;
                }

                let fragment_count = to_send.fragments.len();
                for (i, fragment) in to_send.fragments.into_iter().enumerate() {
                    if let Ok(data) = serde_json::to_vec(&crate::network::protocol::ServerMessage::ScreenFrame(fragment)) {
                        for client_addr in &to_send.clients {
                            let _ = socket.send_to(&data, client_addr);
                        }
                    }
                    // Minimal pacing delay
                    if i < fragment_count - 1 {
                        thread::sleep(Duration::from_micros(50));
                    }
                }
            }
        });

        Self {
            send_fragments: Mutex::new(frag_tx),
        }
    }

    /// Submit fragments for sending (non-blocking)
    pub fn submit_fragments(&self, fragments: Vec<ScreenFragment>, clients: Vec<SocketAddr>) {
        if clients.is_empty() {
            return;
        }
        if let Ok(sender) = self.send_fragments.lock() {
            let _ = sender.send(FragmentsToSend { fragments, clients });
        }
    }
}

/// Fragment JPEG data into network-safe chunks.
pub fn fragment_frame(
    jpeg_data: Vec<u8>,
    frame_id: u32,
    width: u32,
    height: u32,
) -> Vec<ScreenFragment> {
    let total_fragments = ((jpeg_data.len() + FRAGMENT_SIZE - 1) / FRAGMENT_SIZE) as u16;
    let mut fragments = Vec::with_capacity(total_fragments as usize);

    for (idx, chunk) in jpeg_data.chunks(FRAGMENT_SIZE).enumerate() {
        fragments.push(ScreenFragment::new(
            frame_id,
            idx as u16,
            total_fragments,
            width,
            height,
            chunk,
        ));
    }

    fragments
}

/// Decode JPEG data back to RGBA.
pub fn decode_jpeg_to_rgba(jpeg_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let cursor = Cursor::new(jpeg_data);
    let reader = ImageReader::with_format(cursor, image::ImageFormat::Jpeg);

    match reader.decode() {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let width = rgba.width();
            let height = rgba.height();
            Some((rgba.into_raw(), width, height))
        }
        Err(_) => None,
    }
}

// ============================================================================
// Client-side Jitter Buffer
// ============================================================================

/// A frame waiting in the jitter buffer
struct BufferedFrame {
    jpeg_data: Vec<u8>,
    width: u32,
    height: u32,
    frame_id: u32,
    received_at: Instant,
}

/// Jitter buffer for smooth playback on client
#[derive(Resource)]
pub struct JitterBuffer {
    frames: VecDeque<BufferedFrame>,
    /// Target buffer size in frames (latency vs smoothness tradeoff)
    target_size: usize,
    /// Minimum time to hold frames before releasing
    min_delay: Duration,
    last_released_id: u32,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self {
            frames: VecDeque::with_capacity(8),
            target_size: 2, // Buffer 2 frames (~66ms at 30fps)
            min_delay: Duration::from_millis(30),
            last_released_id: 0,
        }
    }
}

impl JitterBuffer {
    /// Add a complete frame to the buffer
    pub fn push_frame(&mut self, jpeg_data: Vec<u8>, width: u32, height: u32, frame_id: u32) {
        // Don't add frames older than what we've already played
        if frame_id <= self.last_released_id && self.last_released_id > 0 {
            return;
        }

        // Insert in order by frame_id
        let frame = BufferedFrame {
            jpeg_data,
            width,
            height,
            frame_id,
            received_at: Instant::now(),
        };

        // Find insertion point to keep sorted by frame_id
        let pos = self.frames.iter().position(|f| f.frame_id > frame_id);
        match pos {
            Some(i) => self.frames.insert(i, frame),
            None => self.frames.push_back(frame),
        }

        // Limit buffer size - drop oldest if too many
        while self.frames.len() > self.target_size * 2 {
            self.frames.pop_front();
        }
    }

    /// Get the next frame to display, if ready
    pub fn pop_frame(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        // Need at least target_size frames buffered, or oldest frame has waited long enough
        let should_release = if let Some(oldest) = self.frames.front() {
            self.frames.len() >= self.target_size || oldest.received_at.elapsed() >= self.min_delay
        } else {
            false
        };

        if should_release {
            if let Some(frame) = self.frames.pop_front() {
                self.last_released_id = frame.frame_id;
                return Some((frame.jpeg_data, frame.width, frame.height));
            }
        }

        None
    }

    /// Check if buffer is ready (has enough frames)
    pub fn is_ready(&self) -> bool {
        self.frames.len() >= self.target_size
    }

    /// Get current buffer depth
    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

/// Resource for assembling received frame fragments on clients.
#[derive(Resource)]
pub struct FrameAssembler {
    pub current_frame_id: u32,
    pub fragments: Vec<Option<Vec<u8>>>,
    pub total_fragments: u16,
    pub received_count: u16,
    pub width: u32,
    pub height: u32,
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self {
            current_frame_id: 0,
            fragments: Vec::new(),
            total_fragments: 0,
            received_count: 0,
            width: 0,
            height: 0,
        }
    }
}

impl FrameAssembler {
    /// Process a received fragment. Returns assembled JPEG data if frame is complete.
    pub fn add_fragment(&mut self, fragment: ScreenFragment) -> Option<(Vec<u8>, u32, u32, u32)> {
        let is_first_ever = self.total_fragments == 0;
        let is_newer_frame = fragment.frame_id > self.current_frame_id
            || (fragment.frame_id == 0 && self.current_frame_id > fragment.frame_id.wrapping_add(1000));

        if is_first_ever || is_newer_frame {
            self.current_frame_id = fragment.frame_id;
            self.total_fragments = fragment.total_fragments;
            self.fragments = vec![None; fragment.total_fragments as usize];
            self.received_count = 0;
            self.width = fragment.width;
            self.height = fragment.height;
        }

        if fragment.frame_id != self.current_frame_id {
            return None;
        }

        let idx = fragment.fragment_idx as usize;
        if idx < self.fragments.len() && self.fragments[idx].is_none() {
            if let Some(decoded) = fragment.decode_data() {
                self.fragments[idx] = Some(decoded);
                self.received_count += 1;
            }
        }

        if self.received_count == self.total_fragments {
            let mut jpeg_data = Vec::new();
            for frag in &self.fragments {
                if let Some(data) = frag {
                    jpeg_data.extend_from_slice(data);
                }
            }

            let width = self.width;
            let height = self.height;
            let frame_id = self.current_frame_id;

            self.fragments.clear();
            self.received_count = 0;

            return Some((jpeg_data, width, height, frame_id));
        }

        None
    }
}

//! H.264 video decoder using FFmpeg CLI.

use bevy::prelude::*;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
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

/// Resource for video frame assembly and decoding
#[derive(Resource)]
pub struct VideoDecoder {
    /// Send assembled NAL units for decoding
    send_data: Mutex<Sender<(Vec<u8>, u32)>>,
    /// Receive decoded RGBA frames
    recv_decoded: Mutex<Receiver<DecodedFrame>>,
    /// Codec info from server
    codec_info: Option<VideoCodecInfo>,
    /// Current frame being assembled
    current_frame_id: u32,
    /// Chunks for current frame
    chunks: Vec<Option<Vec<u8>>>,
    /// Total chunks expected
    total_chunks: u16,
    /// Chunks received count
    received_count: u16,
    /// Decoder dimensions (set from first frame)
    width: u32,
    height: u32,
}

impl VideoDecoder {
    pub fn new() -> Option<Self> {
        // Check if FFmpeg is available
        if std::process::Command::new("ffmpeg")
            .args(["-version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            warn!("FFmpeg not found in PATH, video decoding disabled");
            return None;
        }

        let (data_tx, data_rx) = mpsc::channel::<(Vec<u8>, u32)>();
        let (decoded_tx, decoded_rx) = mpsc::channel::<DecodedFrame>();

        // Spawn decoder thread
        thread::spawn(move || {
            run_decoder_thread(data_rx, decoded_tx);
        });

        Some(Self {
            send_data: Mutex::new(data_tx),
            recv_decoded: Mutex::new(decoded_rx),
            codec_info: None,
            current_frame_id: 0,
            chunks: Vec::new(),
            total_chunks: 0,
            received_count: 0,
            width: 1920,
            height: 1080,
        })
    }

    /// Set codec information from server
    pub fn set_codec_info(&mut self, info: VideoCodecInfo) {
        self.width = info.width;
        self.height = info.height;
        self.codec_info = Some(info);
    }

    /// Add a received video chunk
    pub fn add_chunk(&mut self, chunk: VideoChunk) {
        let is_newer = chunk.frame_id > self.current_frame_id
            || (chunk.frame_id == 0 && self.current_frame_id > 1000);
        let is_first = self.total_chunks == 0;

        // Reset for new frame
        if is_first || is_newer {
            self.current_frame_id = chunk.frame_id;
            self.total_chunks = chunk.total_chunks;
            self.chunks = vec![None; chunk.total_chunks as usize];
            self.received_count = 0;
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

                // Send for decoding
                if let Ok(sender) = self.send_data.lock() {
                    let _ = sender.send((data, self.current_frame_id));
                }

                // Reset
                self.chunks.clear();
                self.received_count = 0;
                self.total_chunks = 0;
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
}

impl Default for VideoDecoder {
    fn default() -> Self {
        Self::new().expect("Failed to initialize video decoder")
    }
}

/// Run the decoder thread using FFmpeg subprocess
fn run_decoder_thread(data_rx: Receiver<(Vec<u8>, u32)>, decoded_tx: Sender<DecodedFrame>) {
    // We'll spawn FFmpeg on first data received to know dimensions
    // Default to 1920x1080, can be adjusted
    let width = 1920u32;
    let height = 1080u32;

    // Spawn FFmpeg decoder process
    // Input: H.264 Annex B from stdin
    // Output: Raw RGBA to stdout
    let size_str = format!("{}x{}", width, height);

    let mut ffmpeg = match std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            // Input format
            "-f",
            "h264",
            "-i",
            "pipe:0",
            // Output format
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size_str,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            error!("Failed to spawn FFmpeg decoder: {}", e);
            return;
        }
    };

    let mut stdin = ffmpeg.stdin.take().expect("Failed to get stdin");
    let mut stdout = ffmpeg.stdout.take().expect("Failed to get stdout");

    // Frame size in bytes (RGBA)
    let frame_size = (width * height * 4) as usize;

    // Spawn thread to read decoded output
    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = vec![0u8; frame_size];
        loop {
            match stdout.read_exact(&mut buf) {
                Ok(()) => {
                    let _ = output_tx.send(buf.clone());
                }
                Err(_) => break,
            }
        }
    });

    info!("Video decoder started: {}x{}", width, height);

    while let Ok((mut data, mut frame_id)) = data_rx.recv() {
        // Skip to latest data
        while let Ok((newer_data, newer_id)) = data_rx.try_recv() {
            data = newer_data;
            frame_id = newer_id;
        }

        // Write H.264 data to FFmpeg stdin
        if stdin.write_all(&data).is_err() {
            error!("Failed to write to FFmpeg decoder");
            break;
        }

        // Collect decoded frames
        while let Ok(rgba) = output_rx.try_recv() {
            let _ = decoded_tx.send(DecodedFrame {
                rgba,
                width,
                height,
                frame_id,
            });
        }
    }

    drop(stdin);
    let _ = ffmpeg.wait();
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
            min_delay: std::time::Duration::from_millis(30),
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

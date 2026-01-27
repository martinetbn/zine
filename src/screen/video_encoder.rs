//! Hardware-accelerated H.264 video encoder using FFmpeg CLI (via ffmpeg-sidecar).
//!
//! Tries hardware encoders in order: NVENC (NVIDIA), AMF (AMD), QSV (Intel),
//! then falls back to software encoding (libx264).

use bevy::prelude::*;
use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::network::protocol::{ServerMessage, VideoChunk, VideoCodecInfo};

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

/// Hardware encoder type detected
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardwareEncoder {
    Nvenc,        // NVIDIA
    Amf,          // AMD
    Qsv,          // Intel QuickSync
    VideoToolbox, // macOS
    Software,     // libx264 fallback
}

impl std::fmt::Display for HardwareEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HardwareEncoder::Nvenc => write!(f, "NVENC (NVIDIA)"),
            HardwareEncoder::Amf => write!(f, "AMF (AMD)"),
            HardwareEncoder::Qsv => write!(f, "QuickSync (Intel)"),
            HardwareEncoder::VideoToolbox => write!(f, "VideoToolbox (macOS)"),
            HardwareEncoder::Software => write!(f, "libx264 (Software)"),
        }
    }
}

/// Resource for background H.264 encoding
#[derive(Resource)]
pub struct VideoEncoder {
    send_frame: Mutex<Sender<FrameToEncode>>,
    recv_encoded: Mutex<Receiver<EncodedVideoData>>,
    codec_info: VideoCodecInfo,
}

impl VideoEncoder {
    /// Create a new video encoder. Returns None if FFmpeg is not available.
    pub fn new(width: u32, height: u32, fps: u32) -> Option<Self> {
        // Check if FFmpeg is available
        if std::process::Command::new("ffmpeg")
            .args(["-version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            warn!("FFmpeg not found in PATH, video encoding disabled");
            return None;
        }

        let (frame_tx, frame_rx) = mpsc::channel::<FrameToEncode>();
        let (encoded_tx, encoded_rx) = mpsc::channel::<EncodedVideoData>();

        // Detect best encoder
        let (encoder_name, hw_type) = find_best_encoder();
        info!("Using video encoder: {} ({})", encoder_name, hw_type);

        let codec_info = VideoCodecInfo {
            codec: "h264".to_string(),
            width,
            height,
            fps,
            extradata: Vec::new(),
        };

        // Spawn encoding thread
        let enc_name = encoder_name.clone();
        thread::spawn(move || {
            run_encoder_thread(frame_rx, encoded_tx, &enc_name, width, height, fps);
        });

        Some(Self {
            send_frame: Mutex::new(frame_tx),
            recv_encoded: Mutex::new(encoded_rx),
            codec_info,
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

    /// Get codec information for clients
    pub fn codec_info(&self) -> &VideoCodecInfo {
        &self.codec_info
    }
}

/// Find the best available encoder by testing FFmpeg
fn find_best_encoder() -> (String, HardwareEncoder) {
    // Get list of available encoders from FFmpeg
    let output = match std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
        Err(_) => String::new(),
    };

    // Try hardware encoders in order of preference
    let encoders = [
        ("h264_nvenc", HardwareEncoder::Nvenc),
        ("h264_amf", HardwareEncoder::Amf),
        ("h264_qsv", HardwareEncoder::Qsv),
        ("h264_videotoolbox", HardwareEncoder::VideoToolbox),
        ("libx264", HardwareEncoder::Software),
    ];

    for (name, hw_type) in encoders {
        if output.contains(name) {
            return (name.to_string(), hw_type);
        }
    }

    // Ultimate fallback
    ("libx264".to_string(), HardwareEncoder::Software)
}

/// Maximum chunk size for network transmission
const MAX_CHUNK_SIZE: usize = 4000;

/// Run the encoder thread using FFmpeg subprocess
fn run_encoder_thread(
    frame_rx: Receiver<FrameToEncode>,
    encoded_tx: Sender<EncodedVideoData>,
    encoder_name: &str,
    width: u32,
    height: u32,
    fps: u32,
) {
    // Build encoder-specific options
    let mut encoder_opts = vec!["-c:v", encoder_name];

    if encoder_name.contains("nvenc") {
        encoder_opts.extend(["-preset", "p1", "-tune", "ull", "-rc", "cbr", "-b:v", "4M"]);
    } else if encoder_name.contains("amf") {
        encoder_opts.extend(["-usage", "ultralowlatency", "-rc", "cbr", "-b:v", "4M"]);
    } else if encoder_name.contains("qsv") {
        encoder_opts.extend(["-preset", "veryfast"]);
    } else if encoder_name == "libx264" {
        encoder_opts.extend(["-preset", "ultrafast", "-tune", "zerolatency", "-crf", "23"]);
    }

    let size_str = format!("{}x{}", width, height);
    let fps_str = fps.to_string();

    // Spawn FFmpeg process
    // Input: raw RGBA frames from stdin
    // Output: H.264 stream to stdout
    let mut ffmpeg = match std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel", "error",
            // Input format
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-s", &size_str,
            "-r", &fps_str,
            "-i", "pipe:0",
        ])
        .args(&encoder_opts)
        .args([
            // Output format - raw H.264 Annex B
            "-f", "h264",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            error!("Failed to spawn FFmpeg encoder: {}", e);
            return;
        }
    };

    let mut stdin = ffmpeg.stdin.take().expect("Failed to get stdin");
    let mut stdout = ffmpeg.stdout.take().expect("Failed to get stdout");

    // Spawn thread to read encoded output
    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        use std::io::Read;
        let mut buf = [0u8; 65536];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let _ = output_tx.send(buf[..n].to_vec());
                }
                Err(_) => break,
            }
        }
    });

    let mut frame_count: u32 = 0;

    info!("Video encoder started: {}x{} @ {}fps using {}", width, height, fps, encoder_name);

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

        // Write raw frame to FFmpeg stdin
        if stdin.write_all(&frame.rgba).is_err() {
            error!("Failed to write frame to FFmpeg");
            break;
        }

        // Collect any encoded output
        let mut encoded_data = Vec::new();
        while let Ok(data) = output_rx.try_recv() {
            encoded_data.extend(data);
        }

        if !encoded_data.is_empty() {
            // Fragment into chunks for network transmission
            let chunks: Vec<VideoChunk> = encoded_data
                .chunks(MAX_CHUNK_SIZE)
                .enumerate()
                .map(|(idx, chunk)| {
                    VideoChunk::new(
                        frame_count,
                        idx as u16,
                        ((encoded_data.len() + MAX_CHUNK_SIZE - 1) / MAX_CHUNK_SIZE) as u16,
                        idx == 0, // First chunk might be keyframe
                        chunk.to_vec(),
                    )
                })
                .collect();

            let _ = encoded_tx.send(EncodedVideoData { chunks });
            frame_count = frame_count.wrapping_add(1);
        }
    }

    // Close stdin to signal EOF to FFmpeg
    drop(stdin);
    let _ = ffmpeg.wait();
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

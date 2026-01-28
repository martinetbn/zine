use bevy::prelude::*;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::network::protocol::{AudioChunk, ServerMessage};

/// Audio encoder resource for streaming.
/// Currently uses raw PCM with simple compression for simplicity.
#[derive(Resource)]
pub struct AudioEncoder {
    /// Sender for raw audio samples to encode.
    tx: Sender<AudioFrame>,
    /// Receiver for encoded audio chunks.
    rx: Arc<Mutex<Receiver<EncodedAudio>>>,
    /// Encoder thread handle.
    _thread: JoinHandle<()>,
}

struct AudioFrame {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

pub struct EncodedAudio {
    pub data: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u8,
    pub sequence: u32,
}

impl AudioEncoder {
    /// Create a new audio encoder.
    pub fn new(sample_rate: u32, channels: u16) -> Option<Self> {
        let (input_tx, input_rx) = mpsc::channel::<AudioFrame>();
        let (output_tx, output_rx) = mpsc::channel::<EncodedAudio>();
        let output_rx = Arc::new(Mutex::new(output_rx));

        let thread = thread::spawn(move || {
            let mut sequence: u32 = 0;

            info!("Audio encoder started: {} Hz, {} channels (PCM)", sample_rate, channels);

            while let Ok(frame) = input_rx.recv() {
                // Convert f32 samples to i16 for transmission (reduces size by half)
                let samples_i16: Vec<i16> = frame
                    .samples
                    .iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect();

                // Convert i16 samples to bytes (little-endian)
                let mut data = Vec::with_capacity(samples_i16.len() * 2);
                for sample in samples_i16 {
                    data.extend_from_slice(&sample.to_le_bytes());
                }

                // Split into chunks to fit in UDP packets (max ~1200 bytes to be safe)
                const MAX_CHUNK_SIZE: usize = 1200;
                for chunk_data in data.chunks(MAX_CHUNK_SIZE) {
                    let encoded = EncodedAudio {
                        data: chunk_data.to_vec(),
                        sample_rate: frame.sample_rate,
                        channels: frame.channels as u8,
                        sequence,
                    };
                    sequence = sequence.wrapping_add(1);
                    if output_tx.send(encoded).is_err() {
                        return; // Receiver dropped
                    }
                }
            }
        });

        Some(Self {
            tx: input_tx,
            rx: output_rx,
            _thread: thread,
        })
    }

    /// Submit audio samples for encoding.
    pub fn submit_samples(&self, samples: Vec<f32>, sample_rate: u32, channels: u16) {
        let _ = self.tx.send(AudioFrame {
            samples,
            sample_rate,
            channels,
        });
    }

    /// Get the next encoded audio chunk if available.
    pub fn get_encoded(&self) -> Option<EncodedAudio> {
        self.rx.lock().ok()?.try_recv().ok()
    }
}

/// Resource for sending audio over the network.
#[derive(Resource)]
pub struct AudioSender {
    tx: Sender<(AudioChunk, Vec<SocketAddr>)>,
    _thread: JoinHandle<()>,
}

impl AudioSender {
    pub fn new(socket: UdpSocket) -> Self {
        let (tx, rx) = mpsc::channel::<(AudioChunk, Vec<SocketAddr>)>();

        let thread = thread::spawn(move || {
            while let Ok((chunk, clients)) = rx.recv() {
                let msg = ServerMessage::AudioFrame(chunk);
                if let Ok(data) = serde_json::to_vec(&msg) {
                    for client in clients {
                        let _ = socket.send_to(&data, client);
                    }
                }
            }
        });

        Self { tx, _thread: thread }
    }

    pub fn send(&self, chunk: AudioChunk, clients: Vec<SocketAddr>) {
        let _ = self.tx.send((chunk, clients));
    }
}

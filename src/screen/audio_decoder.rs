use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use ringbuf::{traits::*, HeapRb};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use crate::network::protocol::AudioChunk;

/// Audio decoder and playback resource for the client.
/// The actual playback stream runs in a background thread to avoid Send/Sync issues.
#[derive(Resource)]
pub struct AudioDecoder {
    /// Sender for received audio chunks.
    chunk_tx: Sender<AudioChunk>,
}

impl AudioDecoder {
    /// Create a new audio decoder with playback.
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = Self::get_playback_config(&device)?;

        let sample_rate = config.sample_rate.0;
        let channels = config.channels;

        info!(
            "Audio playback: {} Hz, {} channels",
            sample_rate, channels
        );

        // Create ring buffer for audio samples (500ms buffer)
        let buffer_size = (sample_rate as usize) * (channels as usize) / 2;
        let ring = HeapRb::<f32>::new(buffer_size);
        let (producer, consumer) = ring.split();
        let producer = Arc::new(Mutex::new(producer));
        let consumer = Arc::new(Mutex::new(consumer));

        let (chunk_tx, chunk_rx) = mpsc::channel::<AudioChunk>();

        // Decoder thread - decodes PCM and pushes to ring buffer
        let producer_clone = producer.clone();
        std::thread::spawn(move || {
            let mut last_sequence: Option<u32> = None;
            let mut resample_buffer = Vec::with_capacity(4096);

            info!("Audio decoder thread started");

            while let Ok(chunk) = chunk_rx.recv() {
                // Check for packet loss (just log it for now)
                if let Some(last_seq) = last_sequence {
                    let expected = last_seq.wrapping_add(1);
                    if chunk.sequence != expected {
                        let lost = chunk.sequence.wrapping_sub(expected);
                        if lost > 0 && lost < 100 {
                            trace!("Audio packet loss: {} packets", lost);
                        }
                    }
                }
                last_sequence = Some(chunk.sequence);

                // Decode PCM data (i16 little-endian to f32)
                let pcm_data = chunk.data();
                if pcm_data.is_empty() || pcm_data.len() % 2 != 0 {
                    continue;
                }

                let samples: Vec<f32> = pcm_data
                    .chunks_exact(2)
                    .map(|bytes| {
                        let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                        sample as f32 / 32768.0
                    })
                    .collect();

                // Resample if needed and push to ring buffer
                if let Ok(mut prod) = producer_clone.lock() {
                    resample_and_push(
                        &samples,
                        chunk.sample_rate,
                        sample_rate,
                        chunk.channels as u16,
                        channels,
                        &mut resample_buffer,
                        &mut prod,
                    );
                }
            }
        });

        // Playback thread - runs the audio stream
        // Stream must be created in the same thread that runs it
        let consumer_clone = consumer.clone();
        std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_output_device() {
                Some(d) => d,
                None => {
                    error!("No audio output device found");
                    return;
                }
            };

            let config = match Self::get_playback_config(&device) {
                Some(c) => c,
                None => {
                    error!("Could not get audio output config");
                    return;
                }
            };

            let err_fn = |err| error!("Audio playback error: {}", err);

            let stream = match device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if let Ok(mut cons) = consumer_clone.lock() {
                        // Use the Observer trait method
                        use ringbuf::traits::Observer;
                        let available = cons.occupied_len();
                        let to_read = data.len().min(available);

                        // Read from ring buffer
                        cons.pop_slice(&mut data[..to_read]);

                        // Fill rest with silence
                        for sample in &mut data[to_read..] {
                            *sample = 0.0;
                        }
                    } else {
                        // Fill with silence if we can't get the lock
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                    }
                },
                err_fn,
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to create audio output stream: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                error!("Failed to start audio playback: {}", e);
                return;
            }

            info!("Audio playback started");

            // Keep stream alive
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        });

        Some(Self { chunk_tx })
    }

    fn get_playback_config(device: &cpal::Device) -> Option<StreamConfig> {
        let supported_configs = device.supported_output_configs().ok()?;

        // Prefer 48kHz stereo
        for config in supported_configs {
            if config.channels() >= 2 && config.sample_format() == SampleFormat::F32 {
                let min_rate = config.min_sample_rate().0;
                let max_rate = config.max_sample_rate().0;

                let target_rate = if min_rate <= 48000 && max_rate >= 48000 {
                    48000
                } else if min_rate <= 44100 && max_rate >= 44100 {
                    44100
                } else {
                    max_rate.min(48000)
                };

                return Some(StreamConfig {
                    channels: 2,
                    sample_rate: cpal::SampleRate(target_rate),
                    buffer_size: cpal::BufferSize::Default,
                });
            }
        }

        device.default_output_config().ok().map(|c| c.into())
    }

    /// Add a received audio chunk for decoding and playback.
    pub fn add_chunk(&self, chunk: AudioChunk) {
        let _ = self.chunk_tx.send(chunk);
    }
}

/// Simple linear resampling and channel conversion.
fn resample_and_push(
    samples: &[f32],
    src_rate: u32,
    dst_rate: u32,
    src_channels: u16,
    dst_channels: u16,
    buffer: &mut Vec<f32>,
    producer: &mut ringbuf::HeapProd<f32>,
) {
    buffer.clear();

    // First, handle channel conversion - convert to mono for processing
    let mono_samples: Vec<f32> = if src_channels >= 2 {
        samples
            .chunks(src_channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect()
    } else {
        samples.to_vec()
    };

    // Resample if needed
    let resampled: Vec<f32> = if src_rate != dst_rate {
        let ratio = dst_rate as f64 / src_rate as f64;
        let out_len = (mono_samples.len() as f64 * ratio) as usize;
        let mut out = Vec::with_capacity(out_len);

        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f64;

            let sample = if src_idx + 1 < mono_samples.len() {
                mono_samples[src_idx] * (1.0 - frac as f32)
                    + mono_samples[src_idx + 1] * frac as f32
            } else if src_idx < mono_samples.len() {
                mono_samples[src_idx]
            } else {
                0.0
            };
            out.push(sample);
        }
        out
    } else {
        mono_samples
    };

    // Expand to output channels
    for sample in resampled {
        for _ in 0..dst_channels {
            buffer.push(sample);
        }
    }

    // Push to ring buffer (non-blocking, drops oldest if full)
    let _ = producer.push_slice(buffer);
}

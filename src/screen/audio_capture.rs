use bevy::prelude::*;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

/// Resource managing system audio capture via WASAPI loopback.
#[derive(Resource)]
pub struct AudioCapture {
    /// Receiver for captured audio samples (f32, interleaved stereo).
    rx: Arc<Mutex<Receiver<Vec<f32>>>>,
    /// Sample rate of the capture.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
}

#[cfg(windows)]
mod wasapi_loopback {
    use super::*;
    use std::ptr::null_mut;
    use std::sync::mpsc::Sender;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    pub struct CaptureInfo {
        pub sample_rate: u32,
        pub channels: u16,
    }

    pub fn start_loopback_capture(
        tx: Sender<Vec<f32>>,
        info_tx: Sender<CaptureInfo>,
    ) -> Option<()> {
        std::thread::spawn(move || {
            if let Err(e) = capture_thread(tx, info_tx) {
                error!("WASAPI loopback capture error: {:?}", e);
            }
        });

        Some(())
    }

    fn capture_thread(
        tx: Sender<Vec<f32>>,
        info_tx: Sender<CaptureInfo>,
    ) -> windows::core::Result<()> {
        unsafe {
            // Initialize COM
            CoInitializeEx(Some(null_mut()), COINIT_MULTITHREADED).ok()?;

            // Get default audio endpoint (render device for loopback)
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

            // Activate audio client
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            // Get mix format
            let mix_format_ptr = audio_client.GetMixFormat()?;
            let mix_format = &*mix_format_ptr;

            let sample_rate = mix_format.nSamplesPerSec;
            let channels = mix_format.nChannels;
            let bits_per_sample = mix_format.wBitsPerSample;

            info!(
                "WASAPI loopback: {} Hz, {} channels, {} bits",
                sample_rate, channels, bits_per_sample
            );

            // Send actual capture info back
            let _ = info_tx.send(CaptureInfo {
                sample_rate,
                channels,
            });

            // Initialize audio client in loopback mode
            // Use 100ms buffer for loopback
            let buffer_duration = 1_000_000i64; // 100ms in 100-nanosecond units
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                buffer_duration,
                0,
                mix_format_ptr,
                None,
            )?;

            // Get capture client
            let capture_client: IAudioCaptureClient = audio_client.GetService()?;

            // Start capturing
            audio_client.Start()?;
            info!("WASAPI loopback capture started");

            // Polling loop - check for data periodically
            loop {
                // Sleep for ~10ms to avoid busy waiting
                std::thread::sleep(std::time::Duration::from_millis(10));

                // Get next packet size
                let packet_size = capture_client.GetNextPacketSize()?;
                if packet_size == 0 {
                    continue;
                }

                // Process all available packets
                loop {
                    let mut buffer_ptr: *mut u8 = null_mut();
                    let mut num_frames: u32 = 0;
                    let mut flags: u32 = 0;

                    let result = capture_client.GetBuffer(
                        &mut buffer_ptr,
                        &mut num_frames,
                        &mut flags,
                        None,
                        None,
                    );

                    if result.is_err() || num_frames == 0 {
                        break;
                    }

                    // Check if buffer contains silence
                    let is_silent = (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0;

                    if !is_silent && num_frames > 0 && !buffer_ptr.is_null() {
                        let samples = convert_buffer_to_f32(
                            buffer_ptr,
                            num_frames as usize,
                            channels as usize,
                            bits_per_sample,
                        );

                        if tx.send(samples).is_err() {
                            // Receiver dropped, stop capturing
                            audio_client.Stop()?;
                            CoUninitialize();
                            return Ok(());
                        }
                    }

                    capture_client.ReleaseBuffer(num_frames)?;

                    // Check if there's more data
                    let next_size = capture_client.GetNextPacketSize()?;
                    if next_size == 0 {
                        break;
                    }
                }
            }
        }
    }

    unsafe fn convert_buffer_to_f32(
        buffer_ptr: *mut u8,
        num_frames: usize,
        channels: usize,
        bits_per_sample: u16,
    ) -> Vec<f32> {
        let total_samples = num_frames * channels;

        match bits_per_sample {
            32 => {
                // Assume float32
                let float_ptr = buffer_ptr as *const f32;
                let slice = std::slice::from_raw_parts(float_ptr, total_samples);
                slice.to_vec()
            }
            16 => {
                // 16-bit signed integer
                let int_ptr = buffer_ptr as *const i16;
                let slice = std::slice::from_raw_parts(int_ptr, total_samples);
                slice.iter().map(|&s| s as f32 / 32768.0).collect()
            }
            24 => {
                // 24-bit signed integer (packed in 3 bytes)
                let mut samples = Vec::with_capacity(total_samples);
                for i in 0..total_samples {
                    let byte_offset = i * 3;
                    let b0 = *buffer_ptr.add(byte_offset) as i32;
                    let b1 = *buffer_ptr.add(byte_offset + 1) as i32;
                    let b2 = *buffer_ptr.add(byte_offset + 2) as i32;
                    // Sign extend
                    let value = (b0 | (b1 << 8) | (b2 << 16)) << 8 >> 8;
                    samples.push(value as f32 / 8388608.0);
                }
                samples
            }
            _ => {
                warn!("Unsupported bit depth: {}", bits_per_sample);
                vec![0.0; total_samples]
            }
        }
    }
}

impl AudioCapture {
    /// Start capturing system audio (loopback).
    pub fn new() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let rx = Arc::new(Mutex::new(rx));

        #[cfg(windows)]
        {
            use wasapi_loopback::CaptureInfo;

            let (info_tx, info_rx) = mpsc::channel::<CaptureInfo>();

            wasapi_loopback::start_loopback_capture(tx, info_tx)?;

            // Wait for actual capture info from thread (with timeout)
            let info = info_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap_or_else(|_| {
                    warn!("Timeout waiting for audio capture info, using defaults");
                    CaptureInfo {
                        sample_rate: 48000,
                        channels: 2,
                    }
                });

            info!(
                "Audio capture initialized: {} Hz, {} channels (WASAPI loopback)",
                info.sample_rate, info.channels
            );

            Some(Self {
                rx,
                sample_rate: info.sample_rate,
                channels: info.channels,
            })
        }

        #[cfg(not(windows))]
        {
            drop(tx);
            warn!("Audio loopback capture not supported on this platform");
            None
        }
    }

    /// Try to receive captured audio samples.
    pub fn try_recv(&self) -> Option<Vec<f32>> {
        self.rx.lock().ok()?.try_recv().ok()
    }
}

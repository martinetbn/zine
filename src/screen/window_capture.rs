//! Window enumeration and capture for Windows platform using Windows Graphics Capture API

use bevy::prelude::*;

/// Information about a capturable window
#[derive(Clone, Debug, Default)]
pub struct WindowInfo {
    pub title: String,
    pub hwnd: isize,
}

/// Enumerate all visible windows that can be captured
#[cfg(windows)]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    use windows_capture::window::Window;

    match Window::enumerate() {
        Ok(windows) => {
            let mut result = Vec::new();
            for window in windows {
                if let Ok(title) = window.title() {
                    // Filter out system windows and empty titles
                    if !title.is_empty()
                        && !title.starts_with("MSCTFIME")
                        && !title.starts_with("Default IME")
                        && title != "Program Manager"
                        && title != "Settings"
                    {
                        // Get HWND as isize
                        let hwnd = window.as_raw_hwnd() as isize;
                        result.push(WindowInfo { title, hwnd });
                    }
                }
            }
            info!("Found {} capturable windows", result.len());
            result
        }
        Err(e) => {
            error!("Failed to enumerate windows: {:?}", e);
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    Vec::new()
}

/// Captured frame data from the capture thread
pub struct WgcCapturedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Start continuous window capture using Windows Graphics Capture API.
/// Returns a receiver for frames and a stop sender.
#[cfg(windows)]
pub fn start_wgc_capture(
    hwnd: isize,
) -> Option<(
    std::sync::mpsc::Receiver<WgcCapturedFrame>,
    std::sync::mpsc::Sender<()>,
)> {
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use windows_capture::{
        capture::{Context, GraphicsCaptureApiHandler},
        frame::Frame,
        graphics_capture_api::InternalCaptureControl,
        settings::{
            ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
            MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
        },
        window::Window,
    };

    let (frame_tx, frame_rx) = mpsc::channel::<WgcCapturedFrame>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // Find the window by HWND
    let windows = match Window::enumerate() {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to enumerate windows: {:?}", e);
            return None;
        }
    };

    let target_window = windows
        .into_iter()
        .find(|w| w.as_raw_hwnd() as isize == hwnd);

    let target_window = match target_window {
        Some(w) => w,
        None => {
            error!("Window with HWND {} not found", hwnd);
            return None;
        }
    };

    let title = target_window.title().unwrap_or_default();
    info!("Starting WGC capture for window: {} (hwnd: {})", title, hwnd);

    // Capture handler that sends frames through channel
    struct CaptureHandler {
        frame_tx: mpsc::Sender<WgcCapturedFrame>,
        stop_rx: Arc<Mutex<mpsc::Receiver<()>>>,
    }

    impl GraphicsCaptureApiHandler for CaptureHandler {
        type Flags = (mpsc::Sender<WgcCapturedFrame>, Arc<Mutex<mpsc::Receiver<()>>>);
        type Error = Box<dyn std::error::Error + Send + Sync>;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (frame_tx, stop_rx) = ctx.flags;
            Ok(Self { frame_tx, stop_rx })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            // Check for stop signal
            if let Ok(rx) = self.stop_rx.lock() {
                if rx.try_recv().is_ok() {
                    capture_control.stop();
                    return Ok(());
                }
            }

            // Get frame buffer - WGC provides BGRA format
            let width = frame.width();
            let height = frame.height();

            if width == 0 || height == 0 {
                return Ok(());
            }

            // Get the raw buffer
            let mut buffer = match frame.buffer() {
                Ok(b) => b,
                Err(_) => return Ok(()),
            };

            let raw_data = buffer.as_raw_buffer();
            let pixel_count = (width * height) as usize;
            let mut rgba = vec![0u8; pixel_count * 4];

            // Convert BGRA to RGBA and flip vertically
            // WGC provides top-down, but pipeline expects bottom-up
            let stride = width as usize * 4;
            for y in 0..height as usize {
                let src_y = y;
                let dst_y = height as usize - 1 - y;
                let src_row = src_y * stride;
                let dst_row = dst_y * stride;

                for x in 0..width as usize {
                    let src_i = src_row + x * 4;
                    let dst_i = dst_row + x * 4;
                    if src_i + 3 < raw_data.len() {
                        rgba[dst_i] = raw_data[src_i + 2];     // R (from B)
                        rgba[dst_i + 1] = raw_data[src_i + 1]; // G
                        rgba[dst_i + 2] = raw_data[src_i];     // B (from R)
                        rgba[dst_i + 3] = raw_data[src_i + 3]; // A
                    }
                }
            }

            // Send frame (non-blocking, drop if receiver is gone)
            let _ = self.frame_tx.send(WgcCapturedFrame { rgba, width, height });

            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            info!("WGC capture closed");
            Ok(())
        }
    }

    let stop_rx_arc = Arc::new(Mutex::new(stop_rx));

    // Start capture in background thread
    thread::spawn(move || {
        let settings = Settings::new(
            target_window,
            CursorCaptureSettings::Default,
            DrawBorderSettings::Default,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (frame_tx, stop_rx_arc),
        );

        if let Err(e) = CaptureHandler::start(settings) {
            error!("WGC capture error: {:?}", e);
        }
    });

    Some((frame_rx, stop_tx))
}

#[cfg(not(windows))]
pub fn start_wgc_capture(
    _hwnd: isize,
) -> Option<(
    std::sync::mpsc::Receiver<WgcCapturedFrame>,
    std::sync::mpsc::Sender<()>,
)> {
    None
}

// Legacy function for compatibility - not used with WGC but kept for non-Windows
#[cfg(not(windows))]
pub fn capture_window(_hwnd: isize) -> Option<(Vec<u8>, u32, u32)> {
    None
}

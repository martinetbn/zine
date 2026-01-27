//! Window enumeration and capture for Windows platform

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
    use std::process::Command;

    // Use PowerShell to enumerate windows - completely isolates from Bevy's process
    let script = r#"
Get-Process | Where-Object { $_.MainWindowTitle -ne '' } | ForEach-Object {
    "$($_.MainWindowHandle)|$($_.MainWindowTitle)"
}
"#;

    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        Ok(output) => {
            if !output.status.success() {
                error!("PowerShell failed: {}", String::from_utf8_lossy(&output.stderr));
                return Vec::new();
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut windows = Vec::new();

            for line in stdout.lines() {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() == 2 {
                    if let Ok(hwnd) = parts[0].parse::<isize>() {
                        let title = parts[1].to_string();
                        // Filter out system windows
                        if !title.is_empty()
                            && !title.starts_with("MSCTFIME")
                            && !title.starts_with("Default IME")
                            && title != "Program Manager"
                            && title != "Settings"
                        {
                            windows.push(WindowInfo { title, hwnd });
                        }
                    }
                }
            }

            info!("Found {} capturable windows via PowerShell", windows.len());
            windows
        }
        Err(e) => {
            error!("Failed to run PowerShell: {}", e);
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    Vec::new()
}

/// Capture a window by its HWND and return RGBA pixel data
#[cfg(windows)]
pub fn capture_window(hwnd: isize) -> Option<(Vec<u8>, u32, u32)> {
    use std::panic::catch_unwind;

    match catch_unwind(|| capture_window_impl(hwnd)) {
        Ok(result) => result,
        Err(e) => {
            error!("Window capture panicked: {:?}", e);
            None
        }
    }
}

#[cfg(windows)]
fn capture_window_impl(hwnd: isize) -> Option<(Vec<u8>, u32, u32)> {
    use win_screenshot::prelude::*;

    // Try PrintWindow first
    match capture_window_ex(hwnd, Using::PrintWindow, Area::Full, None, None) {
        Ok(buf) => {
            let width = buf.width;
            let height = buf.height;

            if width == 0 || height == 0 {
                warn!("Window has zero size");
                return None;
            }

            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for y in (0..height).rev() {
                for x in 0..width {
                    let i = ((y * width + x) * 4) as usize;
                    if i + 3 < buf.pixels.len() {
                        rgba.push(buf.pixels[i + 2]); // R
                        rgba.push(buf.pixels[i + 1]); // G
                        rgba.push(buf.pixels[i]);     // B
                        rgba.push(buf.pixels[i + 3]); // A
                    }
                }
            }

            Some((rgba, width, height))
        }
        Err(e) => {
            warn!("PrintWindow failed: {:?}, trying BitBlt", e);

            // Try BitBlt fallback
            match capture_window_ex(hwnd, Using::BitBlt, Area::Full, None, None) {
                Ok(buf) => {
                    let width = buf.width;
                    let height = buf.height;

                    if width == 0 || height == 0 {
                        return None;
                    }

                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                    for y in (0..height).rev() {
                        for x in 0..width {
                            let i = ((y * width + x) * 4) as usize;
                            if i + 3 < buf.pixels.len() {
                                rgba.push(buf.pixels[i + 2]); // R
                                rgba.push(buf.pixels[i + 1]); // G
                                rgba.push(buf.pixels[i]);     // B
                                rgba.push(buf.pixels[i + 3]); // A
                            }
                        }
                    }

                    Some((rgba, width, height))
                }
                Err(e) => {
                    error!("BitBlt also failed: {:?}", e);
                    None
                }
            }
        }
    }
}

// Stub implementation for non-Windows platforms
#[cfg(not(windows))]
pub fn capture_window(_hwnd: isize) -> Option<(Vec<u8>, u32, u32)> {
    None
}

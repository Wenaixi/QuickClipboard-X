#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{capture_monitor, ensure_com_initialized, get_monitor_handle, CaptureError, CaptureRect, CapturedFrame};

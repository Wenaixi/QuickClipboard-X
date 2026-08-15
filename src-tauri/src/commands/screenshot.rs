use tauri::AppHandle;

#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
use crate::windows::screenshot_window;

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSelection {
    pub left: u32,
    pub top: u32,
    #[allow(dead_code)]
    pub right: u32,
    #[allow(dead_code)]
    pub bottom: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "windows")]
fn start_screenshot_by_mode(app: &AppHandle, action: Option<&str>) -> Result<(), String> {
    screenshot_window::start_screenshot(app, action)
}

#[tauri::command]
pub fn set_mouse_position(x: i32, y: i32) -> Result<(), String> {
    crate::utils::mouse::set_cursor_position(x, y)
}

#[tauri::command]
pub fn get_mouse_position() -> (i32, i32) {
    crate::utils::mouse::get_cursor_position()
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_screenshot(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, None)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_screenshot_quick_save(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("save"))
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_screenshot_quick_pin(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("pin"))
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_screenshot_quick_ocr(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("ai"))
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn screenshot_window_ready(app: AppHandle) -> Result<(), String> {
    screenshot_window::screenshot_window_ready(&app)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn cancel_screenshot(app: AppHandle, session_id: String) -> Result<(), String> {
    screenshot_window::cancel_screenshot(&app, &session_id)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn complete_screenshot(
    app: AppHandle,
    session_id: String,
    selection: ScreenshotSelection,
    action: String,
) -> Result<(), String> {
    screenshot_window::complete_screenshot(&app, &session_id, selection, &action).await
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn close_screenshot_window(app: AppHandle) -> Result<(), String> {
    screenshot_window::close_screenshot_window(&app)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_normal_screenshot(app: AppHandle) -> Result<(), String> {
    start_screenshot(app)
}

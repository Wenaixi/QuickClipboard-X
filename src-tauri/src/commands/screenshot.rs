use serde::Deserialize;
use tauri::AppHandle;

use crate::windows::screenshot_window;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSelection {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub width: u32,
    pub height: u32,
}

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

#[tauri::command]
pub fn start_screenshot(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, None)
}

#[tauri::command]
pub fn start_screenshot_quick_save(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("save"))
}

#[tauri::command]
pub fn start_screenshot_quick_pin(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("pin"))
}

#[tauri::command]
pub fn start_screenshot_quick_ocr(app: AppHandle) -> Result<(), String> {
    start_screenshot_by_mode(&app, Some("ai"))
}

#[tauri::command]
pub fn screenshot_window_ready(app: AppHandle) -> Result<(), String> {
    screenshot_window::screenshot_window_ready(&app)
}

#[tauri::command]
pub fn cancel_screenshot(app: AppHandle, session_id: String) -> Result<(), String> {
    screenshot_window::cancel_screenshot(&app, &session_id)
}

#[tauri::command]
pub async fn complete_screenshot(
    app: AppHandle,
    session_id: String,
    selection: ScreenshotSelection,
    action: String,
) -> Result<(), String> {
    screenshot_window::complete_screenshot(&app, &session_id, selection, &action).await
}

#[tauri::command]
pub fn close_screenshot_window(app: AppHandle) -> Result<(), String> {
    screenshot_window::close_screenshot_window(&app)
}

// 保留普通截屏命令名，兼容旧版前端调用。
#[tauri::command]
pub fn start_normal_screenshot(app: AppHandle) -> Result<(), String> {
    start_screenshot(app)
}

use tauri::AppHandle;

#[cfg(target_os = "windows")]
use tauri::WebviewWindow;
#[cfg(target_os = "windows")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use crate::windows::screenshot_window;

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize, Serialize)]
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

#[cfg(target_os = "windows")]
fn require_screenshot_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == crate::windows::screenshot_window::SCREENSHOT_WINDOW_LABEL {
        Ok(())
    } else {
        Err("该截图命令只能由截图浮窗调用".to_string())
    }
}

#[tauri::command]
pub fn set_mouse_position(x: i32, y: i32) -> Result<(), String> {
    crate::utils::mouse::set_cursor_position(x, y)
}

#[tauri::command]
pub fn get_mouse_position() -> (i32, i32) {
    crate::utils::mouse::get_cursor_position()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/commands/screenshot.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取截图命令源码失败")
    }

    #[test]
    fn ai_configuration_status_is_independent_of_screenshot_action_toggle() {
        assert!(screenshot_ai_config_is_valid(
            "test-key",
            "https://api.example.com/v1",
            "Qwen/Qwen2.5-VL-7B-Instruct",
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn screenshot_session_commands_require_the_invoking_screenshot_window() {
        let source = source();
        for command in [
            "pub fn screenshot_window_ready",
            "pub fn cancel_screenshot",
            "pub fn find_screenshot_window_at_point",
            "pub async fn complete_screenshot",
            "pub fn close_screenshot_window",
        ] {
            let start = source.find(command).expect("缺少截图会话命令");
            let body = &source[start..];
            let guard = body.find("require_screenshot_window(&window)?;")
                .expect("截图会话命令缺少调用窗口校验");
            let next_command = body.find("\n#[cfg(target_os = \"windows\")]").unwrap_or(body.len());
            assert!(guard < next_command, "{command} 必须在自身函数体内校验调用窗口");
        }
    }
}

fn screenshot_ai_config_is_valid(api_key: &str, base_url: &str, model: &str) -> bool {
    crate::services::screenshot::validate_configuration(api_key, base_url, model).is_ok()
}

#[tauri::command]
pub fn get_screenshot_ai_config_status() -> bool {
    let settings = crate::services::settings::get_settings();
    screenshot_ai_config_is_valid(
        &settings.ai_api_key,
        &settings.ai_base_url,
        &settings.ai_model,
    )
}

#[tauri::command]
pub async fn test_screenshot_ai_config() -> Result<(), String> {
    let settings = crate::services::settings::get_settings();
    crate::services::screenshot::test_configuration(
        &settings.ai_api_key,
        &settings.ai_base_url,
        &settings.ai_model,
    )
    .await
    .map_err(|error| error.to_string())
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
pub fn screenshot_window_ready(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require_screenshot_window(&window)?;
    screenshot_window::screenshot_window_ready(&app)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn cancel_screenshot(app: AppHandle, window: WebviewWindow, session_id: String) -> Result<(), String> {
    require_screenshot_window(&window)?;
    screenshot_window::cancel_screenshot(&app, &session_id)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn find_screenshot_window_at_point(
    window: WebviewWindow,
    session_id: String,
    x: i32,
    y: i32,
) -> Result<Option<ScreenshotSelection>, String> {
    require_screenshot_window(&window)?;
    screenshot_window::find_window_selection(&session_id, x, y)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn complete_screenshot(
    app: AppHandle,
    window: WebviewWindow,
    session_id: String,
    selection: ScreenshotSelection,
    action: String,
) -> Result<(), String> {
    require_screenshot_window(&window)?;
    screenshot_window::complete_screenshot(&app, &session_id, selection, &action).await
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn close_screenshot_window(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require_screenshot_window(&window)?;
    screenshot_window::close_screenshot_window(&app)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn start_normal_screenshot(app: AppHandle) -> Result<(), String> {
    start_screenshot(app)
}

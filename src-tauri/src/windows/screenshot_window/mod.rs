use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::services::screenshot::capture::{capture_monitor, ensure_com_initialized, get_monitor_handle, CaptureRect};
use crate::services::screenshot::{choose_and_save_screenshot, copy_screenshot, copy_screenshot_text, emit_screenshot_history_update, encode_and_store_png, prepare_pin_path, recognize_image, MainWindowVisibilityRevision, MonitorRect, SessionPhase, ScreenshotSessionManager, StartSessionResult};
use crate::services::settings::get_settings;
use crate::windows::main_window::{get_main_window, hide_main_window, is_main_window_visible, show_main_window};

pub const SCREENSHOT_WINDOW_LABEL: &str = "screenshot";
const SCREENSHOT_CONFIGURE_EVENT: &str = "screenshot:configure";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotMonitorInfo {
    pub left: i32,
    pub top: i32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotBootstrap {
    pub session_id: String,
    pub monitor: ScreenshotMonitorInfo,
    pub device_pixel_ratio: f64,
    pub quick_action: bool,
    pub initial_action: Option<String>,
    pub screenshot_ai_enabled: bool,
}

#[derive(Default)]
struct ScreenshotWindowState {
    sessions: ScreenshotSessionManager,
    visibility_revision: u64,
    pending_bootstrap: Option<ScreenshotBootstrap>,
    window_ready: bool,
}

static STATE: Lazy<Mutex<ScreenshotWindowState>> = Lazy::new(|| Mutex::new(ScreenshotWindowState::default()));

fn monitor_for_cursor(app: &AppHandle) -> Result<ScreenshotMonitorInfo, String> {
    let cursor = app.cursor_position().map_err(|error| format!("读取鼠标位置失败: {error}"))?;
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|error| format!("获取鼠标所在显示器失败: {error}"))?
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "没有可用的显示器".to_string())?;
    let position = monitor.position();
    let size = monitor.size();
    let scale_factor = monitor.scale_factor().max(1.0);
    Ok(ScreenshotMonitorInfo {
        left: position.x,
        top: position.y,
        physical_width: size.width,
        physical_height: size.height,
        logical_width: ((size.width as f64) / scale_factor).round().max(1.0) as u32,
        logical_height: ((size.height as f64) / scale_factor).round().max(1.0) as u32,
        scale_factor,
    })
}

fn bootstrap_for_session(
    session_id: String,
    monitor: ScreenshotMonitorInfo,
    initial_action: Option<&str>,
    screenshot_ai_enabled: bool,
) -> ScreenshotBootstrap {
    ScreenshotBootstrap {
        device_pixel_ratio: monitor.scale_factor,
        session_id,
        monitor,
        quick_action: initial_action.is_some(),
        initial_action: initial_action.map(str::to_string),
        screenshot_ai_enabled,
    }
}

fn configure_window(window: &WebviewWindow, monitor: &ScreenshotMonitorInfo) -> Result<(), String> {
    window.set_position(PhysicalPosition::new(monitor.left, monitor.top)).map_err(|error| format!("设置截图窗口位置失败: {error}"))?;
    window.set_size(PhysicalSize::new(monitor.physical_width, monitor.physical_height)).map_err(|error| format!("设置截图窗口大小失败: {error}"))?;
    window.set_always_on_top(true).map_err(|error| format!("设置截图窗口置顶失败: {error}"))?;
    window.set_ignore_cursor_events(false).map_err(|error| format!("设置截图窗口交互失败: {error}"))?;
    window.show().map_err(|error| format!("显示截图窗口失败: {error}"))?;
    window.set_focus().map_err(|error| format!("聚焦截图窗口失败: {error}"))?;
    Ok(())
}

fn session_monitor_rect(monitor: &ScreenshotMonitorInfo) -> Result<MonitorRect, String> {
    MonitorRect::new(monitor.left, monitor.top, monitor.physical_width, monitor.physical_height).map_err(|error| format!("截图显示器区域无效: {error:?}"))
}

fn validate_screenshot_action(action: &str) -> Option<String> {
    match action {
        "copy" | "save" | "pin" | "ai" => None,
        other => Some(format!("不支持的截图动作: {other}")),
    }
}

#[cfg(test)]
mod source_guards {
    fn source_file(relative: &str) -> String {
        std::fs::read_to_string(format!("{}/src/{relative}", env!("CARGO_MANIFEST_DIR")))
            .expect("读取截图源码失败")
    }

    #[test]
    fn screenshot_lifecycle_uses_unified_failure_cleanup() {
        let source = source_file("windows/screenshot_window/mod.rs");
        assert!(source.contains("fn finish_failed_screenshot"));
        assert!(source.contains("state.sessions.cancel(session_id, revision)"));
        assert!(source.contains("if plan.session_id == session_id"));
        assert!(source.contains("state.window_ready = false;"));
    }

    #[test]
    fn cancelled_ai_session_cannot_write_recognized_text_to_clipboard() {
        let source = source_file("windows/screenshot_window/mod.rs");
        let ai_start = source.find("\"ai\" => {").expect("缺少 AI 截图动作");
        let ai_body = &source[ai_start..];
        let ai_body = &ai_body[..ai_body.find("        other =>").expect("缺少动作兜底")];
        let result_guard = ai_body
            .rfind("if !is_current_processing_session(session_id) {")
            .expect("AI 动作缺少结果返回后的取消守卫");
        let copy_text = ai_body
            .find("copy_screenshot_text(&result.text)")
            .expect("AI 动作缺少文本复制");
        assert!(result_guard < copy_text, "取消检查必须先于 AI 文本写入剪贴板");
    }

    #[test]
    fn screenshot_capability_has_no_private_or_global_filesystem_scope() {
        let source = source_file("../capabilities/screenshot.json");
        assert!(!source.contains("screenshot-suite:default"));
        assert!(!source.contains("fs:default"));
        assert!(!source.contains("\"path\": \"**\""));
    }
}

fn cleanup_plan(plan: crate::services::screenshot::CleanupPlan, app: &AppHandle) -> Result<(), String> {
    if plan.hide_overlay {
        if let Some(window) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
            window.hide().map_err(|error| format!("隐藏截图窗口失败: {error}"))?;
        }
    }
    for path in plan.temp_files {
        let _ = std::fs::remove_file(path);
    }
    if plan.restore_main_window {
        if let Some(window) = get_main_window(app) {
            show_main_window(&window);
        }
    }
    Ok(())
}

fn is_current_processing_session(session_id: &str) -> bool {
    let state = STATE.lock();
    state
        .sessions
        .is_current_phase(session_id, SessionPhase::Processing)
}

fn finish_failed_screenshot(app: &AppHandle, session_id: &str) {
    let plan = {
        let mut state = STATE.lock();
        let revision = MainWindowVisibilityRevision(state.visibility_revision);
        match state.sessions.cancel(session_id, revision) {
            Ok(plan) => {
                if plan.session_id == session_id {
                    state.pending_bootstrap = None;
                }
                Some(plan)
            }
            Err(_) => None,
        }
    };
    if let Some(plan) = plan {
        let _ = cleanup_plan(plan, app);
    }
}

pub fn start_screenshot(app: &AppHandle, initial_action: Option<&str>) -> Result<(), String> {
    let monitor = monitor_for_cursor(app)?;
    let rect = session_monitor_rect(&monitor)?;
    let settings = get_settings();
    let (session_id, existing) = {
        let mut state = STATE.lock();
        match state.sessions.start(rect, initial_action.is_some()) {
            StartSessionResult::Started(session) => {
                let session_id = session.session_id().to_string();
                let should_hide_main_window = is_main_window_visible(app);
                if should_hide_main_window {
                    let window = match get_main_window(app) {
                        Some(window) => window,
                        None => {
                            let revision = MainWindowVisibilityRevision(state.visibility_revision);
                            let _ = state.sessions.cancel(&session_id, revision);
                            return Err("主窗口不存在，无法启动截图".to_string());
                        }
                    };
                    hide_main_window(&window);
                    state.visibility_revision += 1;
                    let revision = MainWindowVisibilityRevision(state.visibility_revision);
                    if let Err(error) = state.sessions.mark_main_window_hidden(&session_id, revision) {
                        let _ = state.sessions.cancel(&session_id, revision);
                        show_main_window(&window);
                        return Err(format!("记录主窗口隐藏失败: {error:?}"));
                    }
                }
                (session_id, false)
            }
            StartSessionResult::Existing { session_id, .. } => (session_id, true),
        }
    };
    let window = if let Some(window) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
        window
    } else {
        {
            let mut state = STATE.lock();
            state.window_ready = false;
            state.pending_bootstrap = None;
        }
        match WebviewWindowBuilder::new(app, SCREENSHOT_WINDOW_LABEL, WebviewUrl::App("windows/screenshot/index.html".into()))
            .title("截图")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .focused(true)
            .build()
        {
            Ok(window) => window,
            Err(error) => {
                finish_failed_screenshot(app, &session_id);
                return Err(format!("创建截图窗口失败: {error}"));
            }
        }
    };
    let bootstrap = bootstrap_for_session(
        session_id,
        monitor.clone(),
        if existing { None } else { initial_action },
        settings.screenshot_ai_enabled,
    );
    if let Err(error) = configure_window(&window, &monitor) {
        finish_failed_screenshot(app, &bootstrap.session_id);
        return Err(error);
    }
    let ready = {
        let mut state = STATE.lock();
        let ready = state.window_ready;
        if !ready {
            state.pending_bootstrap = Some(bootstrap.clone());
        }
        ready
    };
    if ready {
        if let Err(error) = window.emit(SCREENSHOT_CONFIGURE_EVENT, &bootstrap) {
            finish_failed_screenshot(app, &bootstrap.session_id);
            return Err(format!("发送截图会话配置失败: {error}"));
        }
    }
    if existing {
        if let Err(error) = window.set_focus() {
            finish_failed_screenshot(app, &bootstrap.session_id);
            return Err(format!("聚焦截图窗口失败: {error}"));
        }
    }
    Ok(())
}

pub fn screenshot_window_ready(app: &AppHandle) -> Result<(), String> {
    let bootstrap = {
        let mut state = STATE.lock();
        state.window_ready = true;
        state.pending_bootstrap.take()
    };
    if let Some(bootstrap) = bootstrap {
        let window = match app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
            Some(window) => window,
            None => {
                finish_failed_screenshot(app, &bootstrap.session_id);
                return Err("截图窗口尚未创建".to_string());
            }
        };
        if let Err(error) = window.emit(SCREENSHOT_CONFIGURE_EVENT, &bootstrap) {
            finish_failed_screenshot(app, &bootstrap.session_id);
            return Err(format!("发送截图会话配置失败: {error}"));
        }
    }
    Ok(())
}

pub fn cancel_screenshot(app: &AppHandle, session_id: &str) -> Result<(), String> {
    let plan = {
        let mut state = STATE.lock();
        let revision = MainWindowVisibilityRevision(state.visibility_revision);
        let plan = state
            .sessions
            .cancel(session_id, revision)
            .map_err(|error| format!("取消截图会话失败: {error:?}"))?;
        if plan.session_id == session_id {
            state.pending_bootstrap = None;
        }
        plan
    };
    cleanup_plan(plan, app)
}

pub async fn complete_screenshot(app: &AppHandle, session_id: &str, selection: crate::commands::screenshot::ScreenshotSelection, action: &str) -> Result<(), String> {
    if selection.width == 0 || selection.height == 0 {
        finish_failed_screenshot(app, session_id);
        return Err("截图选区不能为空".to_string());
    }

    // 获取当前会话的显示器物理区域并进入 Processing 状态
    let monitor_rect = {
        let mut state = STATE.lock();
        if let Err(error) = state.sessions.begin_processing(session_id) {
            drop(state);
            finish_failed_screenshot(app, session_id);
            return Err(format!("截图会话状态无效: {error:?}"));
        }
        match state.sessions.current() {
            Some(session) => session.monitor(),
            None => {
                drop(state);
                finish_failed_screenshot(app, session_id);
                return Err("会话已结束".to_string());
            }
        }
    };

    // 从显示器物理区域中心获取 Win32 显示器句柄
    let center_x = monitor_rect.left.saturating_add((monitor_rect.width / 2) as i32);
    let center_y = monitor_rect.top.saturating_add((monitor_rect.height / 2) as i32);
    let hmonitor = get_monitor_handle(center_x, center_y);
    if hmonitor.0.is_null() {
        finish_failed_screenshot(app, session_id);
        return Err("无法取得截图显示器句柄".to_string());
    }
    let hmonitor_raw = hmonitor.0 as isize;

    // 构建裁切选区（前端已转换为物理像素，参考 MonitorRect 原点偏移）
    let capture_rect = CaptureRect {
        left: selection.left,
        top: selection.top,
        width: selection.width,
        height: selection.height,
    };

    // 后台线程执行 WGC 捕获（需要 COM MTA 初始化）
    let captured_result = match tokio::task::spawn_blocking(move || -> Result<_, String> {
        let hmonitor = windows::Win32::Graphics::Gdi::HMONITOR(hmonitor_raw as *mut std::ffi::c_void);
        ensure_com_initialized().map_err(|e| e.to_string())?;
        capture_monitor(hmonitor, capture_rect).map_err(|e| e.to_string())
    })
    .await
    {
        Ok(result) => result,
        Err(error) => {
            finish_failed_screenshot(app, session_id);
            return Err(format!("捕获线程失败: {error}"));
        }
    };

    let captured = match captured_result {
        Ok(frame) => frame,
        Err(error) => {
            finish_failed_screenshot(app, session_id);
            return Err(error);
        }
    };

    if !is_current_processing_session(session_id) {
        finish_failed_screenshot(app, session_id);
        return Err("截图会话已取消".to_string());
    }

    if let Some(error) = validate_screenshot_action(action) {
        finish_failed_screenshot(app, session_id);
        return Err(error);
    }

    if !is_current_processing_session(session_id) {
        finish_failed_screenshot(app, session_id);
        return Err("截图会话已取消".to_string());
    }

    // PNG 编码与文件写入放在线程池，避免占用异步运行时线程。
    let captured_width = captured.width;
    let captured_height = captured.height;
    let captured_rgba = captured.rgba;
    let stored = match tokio::task::spawn_blocking(move || {
        encode_and_store_png(captured_width, captured_height, &captured_rgba)
    })
    .await
    {
        Ok(Ok(stored)) => stored,
        Ok(Err(error)) => {
            finish_failed_screenshot(app, session_id);
            return Err(format!("存储截图失败: {error}"));
        }
        Err(error) => {
            finish_failed_screenshot(app, session_id);
            return Err(format!("截图存储线程失败: {error}"));
        }
    };

    if !is_current_processing_session(session_id) {
        finish_failed_screenshot(app, session_id);
        return Err("截图会话已取消".to_string());
    }

    // 根据动作执行
    let action_result: Result<(), String> = match action {
        "copy" => {
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            let copy_result = copy_screenshot(&stored).map_err(|e| e.to_string());
            match copy_result {
                Ok(clipboard_id) => emit_screenshot_history_update(app, clipboard_id).map_err(|e| e.to_string()),
                Err(error) => Err(error),
            }
        }
        "save" => {
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            match choose_and_save_screenshot(&stored, app) {
                Ok(_) => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        }
        "pin" => {
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            match prepare_pin_path(&stored) {
                Ok(pin_path) => crate::windows::pin_image_window::pin_image_from_file(
                    app.clone(),
                    pin_path.to_string_lossy().to_string(),
                    None, None, None, None, None, None, None, None, None, None, None,
                )
                .await
                .map_err(|e| format!("贴图失败: {e}")),
                Err(error) => Err(error.to_string()),
            }
        },
        "ai" => {
            let settings = get_settings();
            if !settings.screenshot_ai_enabled {
                Err("截图 AI 识别已关闭".to_string())
            } else {
                if !is_current_processing_session(session_id) {
                    return Err("截图会话已取消".to_string());
                }
                let result = recognize_image(
                    &stored.absolute_path,
                    &settings.ai_api_key,
                    &settings.ai_base_url,
                    &settings.ai_model,
                    Some(&settings.screenshot_ai_prompt),
                )
                .await
                .map_err(|e| e.to_string());
                if !is_current_processing_session(session_id) {
                    return Err("截图会话已取消".to_string());
                }
                match result {
                    Ok(result) if !result.text.trim().is_empty() => {
                        match copy_screenshot_text(&result.text) {
                            Ok(clipboard_id) => emit_screenshot_history_update(app, clipboard_id).map_err(|e| e.to_string()),
                            Err(error) => Err(error.to_string()),
                        }
                    }
                    Ok(_) => Err("AI 未识别出文本".to_string()),
                    Err(error) => Err(error),
                }
            }
        }
        other => Err(format!("不支持的截图动作: {other}")),
    };

    if let Err(error) = action_result {
        finish_failed_screenshot(app, session_id);
        return Err(error);
    }

    if !is_current_processing_session(session_id) {
        finish_failed_screenshot(app, session_id);
        return Err("截图会话已取消".to_string());
    }

    // 正常完成：清理会话并恢复主窗口
    let plan = {
        let mut state = STATE.lock();
        let revision = MainWindowVisibilityRevision(state.visibility_revision);
        state
            .sessions
            .finish(session_id, revision)
            .map_err(|e| format!("完成截图会话失败: {e:?}"))?
    };
    cleanup_plan(plan, app)
}

pub fn close_screenshot_window(app: &AppHandle) -> Result<(), String> {
    let session_id = {
        let state = STATE.lock();
        state.sessions.current().map(|session| session.session_id().to_string())
    };

    if let Some(session_id) = session_id {
        cancel_screenshot(app, &session_id)?;
        return Ok(());
    }

    if let Some(window) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
        window
            .hide()
            .map_err(|error| format!("隐藏截图窗口失败: {error}"))?;
    }
    Ok(())
}

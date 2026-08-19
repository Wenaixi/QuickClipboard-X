use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::services::screenshot::capture::{capture_monitor, ensure_com_initialized, get_monitor_handle, CaptureRect};
use crate::services::screenshot::{choose_screenshot_save_destination, copy_screenshot, copy_screenshot_text, emit_screenshot_history_update, encode_and_store_png, prepare_pin_path, recognize_image, save_screenshot, validate_configuration, MainWindowVisibilityRevision, MonitorRect, SessionPhase, ScreenshotSessionManager, StartSessionResult};
use crate::services::settings::{get_settings, update_with};
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
    pub screenshot_ai_configured: bool,
    pub screenshot_magnifier_enabled: bool,
    pub magnifier_background: Option<String>,
    // 截图窗口生命周期模式：quick（隐藏复用）/ dispose（销毁）/ auto（超时释放），
    // 前端据此决定成功路径是否关闭窗口，实现设置项的真实后端消费（此前零消费）。
    pub lifecycle_mode: String,
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
    screenshot_ai_configured: bool,
    screenshot_magnifier_enabled: bool,
    magnifier_background: Option<String>,
    lifecycle_mode: String,
) -> ScreenshotBootstrap {
    ScreenshotBootstrap {
        device_pixel_ratio: monitor.scale_factor,
        session_id,
        monitor,
        quick_action: initial_action.is_some(),
        initial_action: initial_action.map(str::to_string),
        screenshot_ai_enabled,
        screenshot_ai_configured,
        screenshot_magnifier_enabled,
        magnifier_background,
        lifecycle_mode,
    }
}

// 放大镜背景快照：在会话开始时捕获一次当前显示器全屏并编码为 data URL。
// 快照只作为放大镜采样源，捕获失败时优雅降级为 None，不阻塞截图主流程。
fn capture_magnifier_background(monitor: &ScreenshotMonitorInfo) -> Option<String> {
    // 以监视器中心取句柄与采样原点，避免捕获期间光标移动到其它显示器导致快照来源错位。
    let hmonitor = get_monitor_handle(
        monitor.left.saturating_add((monitor.physical_width / 2) as i32),
        monitor.top.saturating_add((monitor.physical_height / 2) as i32),
    );
    let selection = CaptureRect {
        left: 0,
        top: 0,
        width: monitor.physical_width,
        height: monitor.physical_height,
    };
    let frame = capture_monitor(hmonitor, selection).ok()?;
    let png_bytes = crate::services::screenshot::encode_snapshot_png(
        frame.width,
        frame.height,
        &frame.rgba,
    )
    .ok()?;
    Some(format!("data:image/png;base64,{}", BASE64.encode(png_bytes)))
}

fn screenshot_ai_is_configured(settings: &crate::services::settings::AppSettings) -> bool {
    settings.screenshot_ai_enabled
        && validate_configuration(&settings.ai_api_key, &settings.ai_base_url, &settings.ai_model).is_ok()
}

fn confirm_screenshot_ai_cloud_access(app: &AppHandle) -> Result<bool, String> {
    let settings = get_settings();
    if settings.screenshot_ai_cloud_confirmed {
        return Ok(true);
    }

    let message = if settings.language.starts_with("zh") {
        "AI 识别会将当前截图选区发送至你配置的 AI 服务进行处理。图片不会使用本地 OCR 静默替代。\n\n是否继续？"
    } else {
        "AI recognition sends the current screenshot selection to your configured AI service. It will not silently fall back to local OCR.\n\nContinue?"
    };
    // tauri-plugin-dialog 的 MessageDialogBuilder 只有回调式 show() 与阻塞式 blocking_show()，
    // 没有 async 形态；调用方（complete_screenshot）把整个确认放进 spawn_blocking 执行，
    // 避免阻塞 tokio worker 线程。
    if !app
        .dialog()
        .message(message)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
    {
        return Ok(false);
    }

    update_with(|settings| settings.screenshot_ai_cloud_confirmed = true)
        .map_err(|error| format!("保存截图 AI 隐私确认失败: {error}"))?;
    Ok(true)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalWindowRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

fn window_selection_from_rect(
    monitor: MonitorRect,
    rect: PhysicalWindowRect,
) -> Option<crate::commands::screenshot::ScreenshotSelection> {
    let monitor_right = monitor.left.checked_add(i32::try_from(monitor.width).ok()?)?;
    let monitor_bottom = monitor.top.checked_add(i32::try_from(monitor.height).ok()?)?;
    let left = rect.left.max(monitor.left);
    let top = rect.top.max(monitor.top);
    let right = rect.right.min(monitor_right);
    let bottom = rect.bottom.min(monitor_bottom);
    if right <= left || bottom <= top {
        return None;
    }
    let left = u32::try_from(left - monitor.left).ok()?;
    let top = u32::try_from(top - monitor.top).ok()?;
    let right = u32::try_from(right - monitor.left).ok()?;
    let bottom = u32::try_from(bottom - monitor.top).ok()?;
    Some(crate::commands::screenshot::ScreenshotSelection {
        left,
        top,
        right,
        bottom,
        width: right - left,
        height: bottom - top,
    })
}

fn validate_screenshot_action(action: &str) -> Option<String> {
    match action {
        "copy" | "save" | "pin" | "ai" => None,
        other => Some(format!("不支持的截图动作: {other}")),
    }
}

fn validate_ai_screenshot_action(
    settings: &crate::services::settings::AppSettings,
) -> Result<(), String> {
    if !settings.screenshot_ai_enabled {
        return Err("截图 AI 识别已关闭".to_string());
    }
    if !screenshot_ai_is_configured(settings) {
        return Err("截图 AI 识别尚未完成配置".to_string());
    }
    Ok(())
}

fn validate_initial_screenshot_action(
    initial_action: Option<&str>,
    settings: &crate::services::settings::AppSettings,
) -> Result<(), String> {
    let Some(action) = initial_action else {
        return Ok(());
    };

    if let Some(error) = validate_screenshot_action(action) {
        return Err(error);
    }
    if action == "ai" {
        validate_ai_screenshot_action(settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod source_guards {
    use super::{validate_ai_screenshot_action, validate_initial_screenshot_action};

    fn source_file(relative: &str) -> String {
        std::fs::read_to_string(format!("{}/src/{relative}", env!("CARGO_MANIFEST_DIR")))
            .expect("读取截图源码失败")
    }

    #[test]
    fn quick_ai_action_requires_enabled_and_configured_vision_settings() {
        let mut settings = crate::services::settings::AppSettings::default();
        assert!(validate_initial_screenshot_action(Some("copy"), &settings).is_ok());
        assert_eq!(
            validate_initial_screenshot_action(Some("ai"), &settings),
            Err("截图 AI 识别尚未完成配置".to_string()),
        );

        settings.screenshot_ai_enabled = false;
        settings.ai_api_key = "test-key".to_string();
        settings.ai_base_url = "https://api.example.com/v1".to_string();
        settings.ai_model = "Qwen/Qwen2.5-VL-7B-Instruct".to_string();
        assert_eq!(
            validate_initial_screenshot_action(Some("ai"), &settings),
            Err("截图 AI 识别已关闭".to_string()),
        );

        settings.screenshot_ai_enabled = true;
        assert!(validate_initial_screenshot_action(Some("ai"), &settings).is_ok());

        settings.screenshot_ai_enabled = false;
        assert_eq!(
            validate_ai_screenshot_action(&settings),
            Err("截图 AI 识别已关闭".to_string()),
        );
        settings.screenshot_ai_enabled = true;
        assert!(validate_ai_screenshot_action(&settings).is_ok());
    }

    #[test]
    fn bootstrap_carries_screenshot_window_lifecycle_mode() {
        let source = source_file("windows/screenshot_window/mod.rs");
        assert!(source.contains("pub lifecycle_mode: String"), "bootstrap 必须携带生命周期模式");
        // 用 \n+缩进行首锚定生产调用点，避免命中测试自身断言字面量（§10.4 自指陷阱）。
        assert!(
            source.contains("\n        settings.screenshot_window_lifecycle_mode.clone(),"),
            "bootstrap 必须从设置读取生命周期模式"
        );
    }

    #[test]
    fn configure_window_orders_position_size_topmost_interaction_show_focus() {
        let source = source_file("windows/screenshot_window/mod.rs");
        // 行首锚定生产声明，避免测试模块内 configure_window 字面自指命中（§10.4）。
        let start = source
            .find("\nfn configure_window")
            .expect("缺少截图窗口配置函数");
        let body = &source[start..];
        let pos = body.find("window.set_position(").expect("缺少位置配置");
        let size = body.find("window.set_size(").expect("缺少尺寸配置");
        let topmost = body.find("window.set_always_on_top(true)").expect("缺少置顶配置");
        let ignore = body.find("window.set_ignore_cursor_events(false)").expect("缺少交互配置");
        let show = body.find("window.show()").expect("缺少显示调用");
        let focus = body.find("window.set_focus()").expect("缺少聚焦调用");
        // 先定位再显示避免目标位置闪烁，置顶先于显示避免非置顶闪现，聚焦放最后。
        assert!(pos < size && size < topmost && topmost < ignore && ignore < show && show < focus,
            "配置顺序必须为 位置→尺寸→置顶→交互→显示→聚焦: {pos} {size} {topmost} {ignore} {show} {focus}");
    }

    #[test]
    fn window_selection_fully_outside_monitor_returns_none() {
        // 窗口完全在显示器左侧之外（right < monitor.left），无可截图区域。
        let monitor = crate::services::screenshot::MonitorRect::new(-1920, 0, 1920, 1080).unwrap();
        let selection = super::window_selection_from_rect(
            monitor,
            super::PhysicalWindowRect { left: -4000, top: 0, right: -3000, bottom: 1080 },
        );
        assert!(selection.is_none(), "窗口完全在显示器外时不得产生选区");

        // 窗口完全在显示器下方之外。
        let below = super::window_selection_from_rect(
            monitor,
            super::PhysicalWindowRect { left: 0, top: 2000, right: 100, bottom: 3000 },
        );
        assert!(below.is_none(), "窗口完全在显示器下方时不得产生选区");
    }

    #[test]
    fn window_selection_inside_monitor_keeps_exact_rect() {
        let monitor = crate::services::screenshot::MonitorRect::new(-1920, 0, 1920, 1080).unwrap();
        // 负坐标显示器上的完整内部窗口：坐标是显示器内相对值。
        let selection = super::window_selection_from_rect(
            monitor,
            super::PhysicalWindowRect { left: -1800, top: 100, right: -800, bottom: 600 },
        )
        .expect("内部窗口应原样保留");
        assert_eq!((selection.left, selection.top, selection.right, selection.bottom), (120, 100, 1120, 600));
        assert_eq!((selection.width, selection.height), (1000, 500));
    }

    #[test]
    fn window_selection_partially_overflowing_each_edge_is_clamped() {
        let monitor = crate::services::screenshot::MonitorRect::new(-1920, 0, 1920, 1080).unwrap();
        // 左上越界 + 右下越界：四侧同时夹紧。返回值是显示器内相对坐标：
        // left -3000 夹到 0，right 100 在显示器内是 100-(-1920)=2020 超过宽度夹到 1920。
        let selection = super::window_selection_from_rect(
            monitor,
            super::PhysicalWindowRect { left: -3000, top: -100, right: 100, bottom: 2000 },
        )
        .expect("部分越界窗口应保留显示器内区域");
        assert_eq!((selection.left, selection.top, selection.right, selection.bottom), (0, 0, 1920, 1080));
        assert_eq!((selection.width, selection.height), (1920, 1080));
    }

    #[test]
    fn window_selection_is_clamped_to_the_active_monitor() {
        let monitor = crate::services::screenshot::MonitorRect::new(-1920, 0, 1920, 1080).unwrap();
        let selection = super::window_selection_from_rect(
            monitor,
            super::PhysicalWindowRect { left: -3000, top: -40, right: -100, bottom: 1200 },
        )
        .expect("跨边界窗口应保留当前显示器内的可截图区域");

        assert_eq!((selection.left, selection.top, selection.right, selection.bottom), (0, 0, 1820, 1080));
        assert_eq!((selection.width, selection.height), (1820, 1080));
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
    fn ai_action_requires_valid_configuration_and_cloud_confirmation_before_request() {
        let source = source_file("windows/screenshot_window/mod.rs");
        let ai_start = source.find("\"ai\" => {").expect("缺少 AI 截图动作");
        let ai_body = &source[ai_start..];
        let ai_body = &ai_body[..ai_body.find("        other =>").expect("缺少动作兜底")];
        let config_check = ai_body
            .find("validate_ai_screenshot_action(&settings)")
            .expect("AI 动作缺少可用性校验");
        let confirmation = ai_body
            .find("spawn_blocking({")
            .expect("AI 动作缺少云端发送确认");
        let request = ai_body
            .find("recognize_image(")
            .expect("AI 动作缺少识别请求");
        assert!(config_check < confirmation && confirmation < request, "必须先校验配置、确认云端发送，再发起 AI 请求");
    }

    #[test]
    fn ai_failure_paths_must_cleanup_via_unified_failure_handler() {
        let source = source_file("windows/screenshot_window/mod.rs");
        let ai_start = source.find("\"ai\" => {").expect("缺少 AI 截图动作");
        let other_idx = source[ai_start..]
            .find("        other =>")
            .map(|offset| ai_start + offset)
            .expect("缺少动作兜底");
        let ai_body = &source[ai_start..other_idx];
        // AI 识别失败（Err）与未识别出文本（Ok 空）两条失败路径必须统一走失败清理。
        assert!(ai_body.contains("Err(error) => Err(error)"), "识别错误必须原样向上传播");
        assert!(ai_body.contains("Ok(_) => Err(\"AI 未识别出文本\".to_string())"), "空识别结果必须报错");
        // 生产收口在 other 兜底之后；ai_body 内 rfind 会命中测试模块自身的断言字面量
        // （§10.4 自指陷阱），必须从兜底之后正向找。
        let cleanup_start = source[other_idx..]
            .find("if let Err(error) = action_result {")
            .map(|offset| other_idx + offset)
            .expect("缺少动作结果统一收口");
        // 收口之后是正常完成路径；必须截到函数末尾，否则后缀含测试模块自身的
        // finish_failed_screenshot 字面量会让 contains 永远为真（§10.4 自指陷阱）。
        let function_end = source[cleanup_start..]
            .find("// 正常完成")
            .map(|offset| cleanup_start + offset)
            .expect("缺少正常完成注释锚点");
        let cleanup_tail = &source[cleanup_start..function_end];
        assert!(cleanup_tail.contains("finish_failed_screenshot(app, session_id);"), "AI 失败必须走统一失败清理");
        assert!(cleanup_tail.contains("return Err(error);"), "统一收口必须传播错误");
    }

    #[test]
    fn save_and_pin_actions_keep_dialogs_on_ui_path_and_file_work_off_runtime() {
        let source = source_file("windows/screenshot_window/mod.rs");
        let save_start = source.find("\"save\" => {").expect("缺少保存截图动作");
        let pin_start = source.find("\"pin\" => {").expect("缺少贴图截图动作");
        let ai_start = source.find("\"ai\" => {").expect("缺少 AI 截图动作");
        let save_body = &source[save_start..pin_start];
        let pin_body = &source[pin_start..ai_start];

        assert!(save_body.contains("choose_screenshot_save_destination(&stored, app)"));
        assert!(save_body.contains("spawn_blocking(move || save_screenshot(&stored, &destination))"));
        assert!(save_body.rfind("if !is_current_processing_session(session_id) {").is_some());
        assert!(pin_body.contains("spawn_blocking(move || prepare_pin_path(&stored_for_pin))"));
        assert!(pin_body.rfind("if !is_current_processing_session(session_id) {").is_some());
    }

    #[test]
    fn save_dialog_cancel_runs_failure_cleanup_and_ends_session() {
        let source = source_file("windows/screenshot_window/mod.rs");
        let save_start = source.find("\"save\" => {").expect("缺少保存截图动作");
        let pin_start = source.find("\"pin\" => {").expect("缺少贴图截图动作");
        let save_body = &source[save_start..pin_start];
        let cancel = save_body.find("Ok(None) => {").expect("保存对话框取消分支缺失");
        let after_cancel = &save_body[cancel..];
        let cleanup = after_cancel
            .find("finish_failed_screenshot(app, session_id);")
            .expect("保存取消必须走统一失败清理");
        let cancel_message = after_cancel.find("\"已取消保存截图\"").expect("缺少取消保存提示");
        assert!(cleanup < cancel_message, "取消分支必须先清理再返回错误");
    }

    #[test]
    fn existing_session_skips_reconfigure_to_keep_monitor_consistent() {
        let source = source_file("windows/screenshot_window/mod.rs");
        // 测试模块自身含 "pub fn start_screenshot" 字面量（§10.4 自指陷阱），
        // 用 rfind 取文件中最后出现的真实函数签名，天然排除测试模块内的同名字面。
        let start = source.rfind("pub fn start_screenshot").expect("缺少截图启动函数");
        let body = &source[start..];
        let existing = body
            .find("if existing {\n        // 已有会话：窗口已按原显示器配置，仅重新显示与聚焦")
            .expect("已有会话提前返回分支缺失");
        assert!(
            body.find("register_screenshot_temp_file(session_id, stored.absolute_path.clone()) {\n        let _ = std::fs::remove_file(&stored.absolute_path);\n        finish_failed_screenshot(app, session_id);\n        return Err(error);")
                .is_some(),
            "register 失败必须与其它失败路径对称清理"
        );
        let busy_guard = body
            .find("if !matches!(STATE.lock().sessions.phase(), Some(SessionPhase::Selecting)) {")
            .expect("已有会话分支缺少处理中拒绝重入");
        let show = body.find("window.show()").expect("已有会话分支缺少重新显示");
        assert!(busy_guard < show, "处理中守卫必须先于窗口显示");
        let ok = body.find("return Ok(());").expect("已有会话分支缺少提前返回");
        assert!(existing < show && show < ok, "已有会话必须重新显示并提前返回: existing={existing} show={show} ok={ok}");
        assert!(body.contains("capture_magnifier_background(&monitor)"), "放大镜背景必须用已解析监视器采样");
    }

    #[test]
    fn screenshot_bootstrap_carries_magnifier_flag_and_background_snapshot() {
        let source = source_file("windows/screenshot_window/mod.rs");
        assert!(source.contains("pub screenshot_magnifier_enabled: bool"));
        assert!(source.contains("pub magnifier_background: Option<String>"));
        assert!(source.contains("fn capture_magnifier_background"));
        assert!(source.contains("encode_snapshot_png"));
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

fn begin_screenshot_commit(session_id: &str) -> Result<(), String> {
    let mut state = STATE.lock();
    state
        .sessions
        .begin_commit(session_id)
        .map_err(|error| format!("截图会话无法提交结果: {error:?}"))
}

fn register_screenshot_temp_file(session_id: &str, path: std::path::PathBuf) -> Result<(), String> {
    let mut state = STATE.lock();
    state
        .sessions
        .register_temp_file(session_id, path)
        .map_err(|error| format!("登记截图临时文件失败: {error:?}"))
}

fn finish_failed_screenshot(app: &AppHandle, session_id: &str) {
    let plan = {
        let mut state = STATE.lock();
        let revision = MainWindowVisibilityRevision(state.visibility_revision);
        let plan = state
            .sessions
            .cancel(session_id, revision)
            .or_else(|error| match error {
                crate::services::screenshot::SessionError::CommitInProgress { .. } => {
                    state.sessions.finish(session_id, revision)
                }
                other => Err(other),
            });
        match plan {
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

/// 截图会话是否活跃。供主窗口贴边监控查询：截图期间必须抑制贴边悬浮弹出，
/// 否则用户鼠标移到屏幕边缘框选时，被截图隐藏的主窗口会被 edge_monitor 弹回遮挡选区。
pub fn is_screenshot_active() -> bool {
    STATE.lock().sessions.phase().is_some()
}

pub fn start_screenshot(app: &AppHandle, initial_action: Option<&str>) -> Result<(), String> {
    let settings = get_settings();
    validate_initial_screenshot_action(initial_action, &settings)?;
    let monitor = monitor_for_cursor(app)?;
    let rect = session_monitor_rect(&monitor)?;
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
    if existing {
        // 已有会话：窗口已按原显示器配置，仅重新显示与聚焦，
        // 避免用新光标显示器重配导致视图层与捕获层显示器信息错位。
        // 前一次截图仍在处理中时拒绝重入：避免全局快捷键闪现处理中的窗口。
        if !matches!(STATE.lock().sessions.phase(), Some(SessionPhase::Selecting)) {
            return Err("截图正在处理中，请稍候".to_string());
        }
        if let Err(error) = window.show() {
            finish_failed_screenshot(app, &session_id);
            return Err(format!("显示截图窗口失败: {error}"));
        }
        if let Err(error) = window.set_focus() {
            finish_failed_screenshot(app, &session_id);
            return Err(format!("聚焦截图窗口失败: {error}"));
        }
        return Ok(());
    }
    let magnifier_background = if settings.screenshot_magnifier_enabled {
        capture_magnifier_background(&monitor)
    } else {
        None
    };
    let bootstrap = bootstrap_for_session(
        session_id,
        monitor.clone(),
        if existing { None } else { initial_action },
        settings.screenshot_ai_enabled,
        screenshot_ai_is_configured(&settings),
        settings.screenshot_magnifier_enabled,
        magnifier_background,
        settings.screenshot_window_lifecycle_mode.clone(),
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

pub fn find_window_selection(
    session_id: &str,
    x: i32,
    y: i32,
) -> Result<Option<crate::commands::screenshot::ScreenshotSelection>, String> {
    let monitor = {
        let state = STATE.lock();
        let session = state
            .sessions
            .current()
            .ok_or_else(|| "截图会话已结束".to_string())?;
        if session.session_id() != session_id || !state.sessions.is_current_phase(session_id, SessionPhase::Selecting) {
            return Err("截图会话状态无效".to_string());
        }
        session.monitor()
    };

    Ok(crate::services::screenshot::capture::find_window_rect_at_point(x, y)
        .and_then(|rect| window_selection_from_rect(monitor, PhysicalWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })))
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
        let _com_initialization = ensure_com_initialized().map_err(|e| e.to_string())?;
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
        let _ = std::fs::remove_file(&stored.absolute_path);
        finish_failed_screenshot(app, session_id);
        return Err("截图会话已取消".to_string());
    }
    if let Err(error) = register_screenshot_temp_file(session_id, stored.absolute_path.clone()) {
        let _ = std::fs::remove_file(&stored.absolute_path);
        finish_failed_screenshot(app, session_id);
        return Err(error);
    }

    // 根据动作执行
    let action_result: Result<(), String> = match action {
        "copy" => {
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            begin_screenshot_commit(session_id)?;
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
            let destination = match choose_screenshot_save_destination(&stored, app) {
                Ok(Some(destination)) => destination,
                Ok(None) => {
                    // 用户在保存对话框点了取消：等同保存失败，统一走失败清理避免会话卡在处理中。
                    finish_failed_screenshot(app, session_id);
                    return Err("已取消保存截图".to_string());
                }
                Err(error) => {
                    finish_failed_screenshot(app, session_id);
                    return Err(error.to_string());
                }
            };
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            begin_screenshot_commit(session_id)?;
            let stored = stored.clone();
            match tokio::task::spawn_blocking(move || save_screenshot(&stored, &destination)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(format!("保存截图线程失败: {error}")),
            }
        }
        "pin" => {
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            let stored_for_pin = stored.clone();
            let pin_path = match tokio::task::spawn_blocking(move || prepare_pin_path(&stored_for_pin)).await {
                Ok(Ok(pin_path)) => pin_path,
                Ok(Err(error)) => {
                    finish_failed_screenshot(app, session_id);
                    return Err(error.to_string());
                }
                Err(error) => {
                    finish_failed_screenshot(app, session_id);
                    return Err(format!("贴图文件准备线程失败: {error}"));
                }
            };
            if !is_current_processing_session(session_id) {
                finish_failed_screenshot(app, session_id);
                return Err("截图会话已取消".to_string());
            }
            begin_screenshot_commit(session_id)?;
            crate::windows::pin_image_window::pin_image_from_file(
                app.clone(),
                pin_path.to_string_lossy().to_string(),
                None, None, None, None, None, None, None, None, None, None, None,
            )
            .await
            .map_err(|e| format!("贴图失败: {e}"))
        },
        "ai" => {
            let settings = get_settings();
            if let Err(error) = validate_ai_screenshot_action(&settings) {
                Err(error)
            } else if !tokio::task::spawn_blocking({
                let app_clone = app.clone();
                move || confirm_screenshot_ai_cloud_access(&app_clone)
            })
            .await
            .map_err(|error| format!("AI 确认对话框线程失败: {error}"))??
            {
                Err("已取消云端 AI 识别".to_string())
            } else {
                if !is_current_processing_session(session_id) {
                    finish_failed_screenshot(app, session_id);
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
                    finish_failed_screenshot(app, session_id);
                    return Err("截图会话已取消".to_string());
                }
                match result {
                    Ok(result) if !result.text.trim().is_empty() => {
                        begin_screenshot_commit(session_id)?;
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

    // 正常完成：清理会话并恢复主窗口。动作在最后一刻已切换到 Committing。
    let plan = {
        let mut state = STATE.lock();
        let revision = MainWindowVisibilityRevision(state.visibility_revision);
        match state.sessions.finish_and_retain_file(session_id, revision, &stored.absolute_path) {
            Ok(plan) => plan,
            Err(error) => {
                // 与其它失败路径保持对称：完成失败也走统一失败清理，避免会话残留。
                finish_failed_screenshot(app, session_id);
                return Err(format!("完成截图会话失败: {error:?}"));
            }
        }
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

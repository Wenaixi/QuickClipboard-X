use once_cell::sync::Lazy;
use serde::Serialize;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

// 标签常量提为 pub(crate)——focus.rs refresh_excluded_hwnds 的排除
// 列表要按真实标签取窗口,此前硬编码 "preview" 恒 None,预览窗 hwnd 从未进
// 排除列表,聚焦事件污染 LAST_FOCUS_HWND。
pub(crate) const PREVIEW_WINDOW_LABEL: &str = "preview-window";
const PREVIEW_REUSE_TTL_MS: u64 = 60_000;
const PREVIEW_ALWAYS_ON_TOP_REFRESH_DELAY_MS: u64 = 10;
const PREVIEW_HIDE_WATCHDOG_DURATION_MS: u64 = 5_000;
const PREVIEW_HIDE_WATCHDOG_INTERVAL_MS: u64 = 100;

static PREVIEW_REQUEST_VERSION: AtomicU64 = AtomicU64::new(0);
static PREVIEW_DESTROY_TIMER_VERSION: AtomicU64 = AtomicU64::new(0);
static PREVIEW_HIDE_WATCHDOG_VERSION: AtomicU64 = AtomicU64::new(0);
static PREVIEW_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static PREVIEW_DATA: Lazy<Mutex<Option<PreviewWindowData>>> = Lazy::new(|| Mutex::new(None));

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAnchorRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewWindowData {
    pub mode: String,
    pub source: String,
    pub item_id: String,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub scale_factor: f64,
    pub work_area_x: i32,
    pub work_area_y: i32,
    pub work_area_width: u32,
    pub work_area_height: u32,
    pub main_window_x: i32,
    pub main_window_y: i32,
    pub main_window_width: u32,
    pub main_window_height: u32,
    pub item_rect: Option<PreviewAnchorRect>,
    pub request_id: u64,
}

fn destroy_preview_window_internal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        let _ = window.hide();
        let _ = window.close();
    }
    if let Ok(mut guard) = PREVIEW_DATA.lock() {
        *guard = None;
    }
}

fn hide_preview_window_internal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn refresh_preview_window_always_on_top(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_always_on_top(false)
        .map_err(|e| format!("取消预览窗口置顶失败: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(
        PREVIEW_ALWAYS_ON_TOP_REFRESH_DELAY_MS,
    ));
    window
        .set_always_on_top(true)
        .map_err(|e| format!("恢复预览窗口置顶失败: {}", e))?;
    Ok(())
}

fn apply_preview_window_bounds(
    window: &WebviewWindow,
    work_area_x: i32,
    work_area_y: i32,
    work_area_width: u32,
    work_area_height: u32,
) -> Result<(), String> {
    window
        .set_position(PhysicalPosition::new(work_area_x, work_area_y))
        .map_err(|e| format!("设置预览窗口位置失败: {}", e))?;
    window
        .set_size(PhysicalSize::new(work_area_width, work_area_height))
        .map_err(|e| format!("设置预览窗口大小失败: {}", e))?;
    Ok(())
}

fn upsert_preview_data(data: PreviewWindowData) {
    if let Ok(mut guard) = PREVIEW_DATA.lock() {
        *guard = Some(data);
    }
}

fn schedule_preview_window_destroy(app: AppHandle, timer_version: u64, request_id: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(PREVIEW_REUSE_TTL_MS)).await;

        if PREVIEW_DESTROY_TIMER_VERSION.load(Ordering::SeqCst) != timer_version {
            return;
        }

        let current_request_id = PREVIEW_DATA
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|data| data.request_id))
            .unwrap_or_default();

        if current_request_id != request_id {
            return;
        }

        destroy_preview_window_internal(&app);
    });
}

fn schedule_preview_hide_watchdog(app: AppHandle, watchdog_version: u64) {
    tauri::async_runtime::spawn(async move {
        let started_at = Instant::now();

        loop {
            if PREVIEW_HIDE_WATCHDOG_VERSION.load(Ordering::SeqCst) != watchdog_version {
                return;
            }

            if !PREVIEW_SUPPRESSED.load(Ordering::SeqCst) {
                return;
            }

            if started_at.elapsed() >= Duration::from_millis(PREVIEW_HIDE_WATCHDOG_DURATION_MS) {
                return;
            }

            if app.get_webview_window(PREVIEW_WINDOW_LABEL).is_some() {
                destroy_preview_window_internal(&app);
            }

            tokio::time::sleep(Duration::from_millis(
                PREVIEW_HIDE_WATCHDOG_INTERVAL_MS,
            ))
            .await;
        }
    });
}

// 关闭预览窗口的前端回调兜底:emit "preview-window-will-hide" 后,隐藏
// 依赖前端调 finalize_hide_preview_window;preview JS 未加载/被阻断时
// 回调永不返回,窗口滞留。此看门狗按可见性判定 finalize 是否真的完成
// (finalize 只 hide + 调度延迟销毁,不推进 PREVIEW_DATA 的 request_id,
// 不能用请求代判断)——窗口已隐藏则正常路径不打扰 60s 复用 TTL,
// 超时仍可见才强制销毁兜底。
fn schedule_preview_close_watchdog(app: AppHandle, watchdog_version: u64) {
    tauri::async_runtime::spawn(async move {
        let started_at = Instant::now();

        loop {
            if PREVIEW_HIDE_WATCHDOG_VERSION.load(Ordering::SeqCst) != watchdog_version {
                return;
            }

            // finalize 已隐藏或窗口已销毁:正常路径,交由 destroy TTL 复用
            let visible = app
                .get_webview_window(PREVIEW_WINDOW_LABEL)
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false);
            if !visible {
                return;
            }

            if started_at.elapsed() >= Duration::from_millis(PREVIEW_HIDE_WATCHDOG_DURATION_MS) {
                destroy_preview_window_internal(&app);
                return;
            }

            tokio::time::sleep(Duration::from_millis(
                PREVIEW_HIDE_WATCHDOG_INTERVAL_MS,
            ))
            .await;
        }
    });
}

fn create_preview_window(
    app: &AppHandle,
    work_area_x: i32,
    work_area_y: i32,
    work_area_width: u32,
    work_area_height: u32,
    scale_factor: f64,
) -> Result<WebviewWindow, String> {
    let logical_scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let logical_x = work_area_x as f64 / logical_scale;
    let logical_y = work_area_y as f64 / logical_scale;
    let logical_width = (work_area_width as f64 / logical_scale).max(1.0);
    let logical_height = (work_area_height as f64 / logical_scale).max(1.0);

    let window = WebviewWindowBuilder::new(
        app,
        PREVIEW_WINDOW_LABEL,
        WebviewUrl::App("windows/preview/index.html".into()),
    )
    .title("预览窗口")
    .inner_size(logical_width, logical_height)
    .position(logical_x, logical_y)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .drag_and_drop(false)
    .build()
    .map_err(|e| format!("创建预览窗口失败: {}", e))?;

    apply_preview_window_bounds(
        &window,
        work_area_x,
        work_area_y,
        work_area_width,
        work_area_height,
    )?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|e| format!("设置预览窗口忽略鼠标事件失败: {}", e))?;

    Ok(window)
}

#[tauri::command]
pub async fn show_preview_window(
    app: AppHandle,
    mode: String,
    source: String,
    item_id: String,
    item_rect: Option<PreviewAnchorRect>,
) -> Result<(), String> {
    if PREVIEW_SUPPRESSED.load(Ordering::SeqCst) {
        return Ok(());
    }

    if crate::is_context_menu_visible() {
        return Ok(());
    }

    let window_state = crate::get_window_state();
    if window_state.state != crate::WindowState::Visible {
        eprintln!(
            "主窗口当前未处于可见状态，忽略预览窗口创建请求（state: {:?}, snapped: {}, hidden: {}）",
            window_state.state,
            window_state.is_snapped,
            window_state.is_hidden,
        );
        return Ok(());
    }

    let request_id = PREVIEW_REQUEST_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
    PREVIEW_DESTROY_TIMER_VERSION.fetch_add(1, Ordering::SeqCst);

    let monitor = crate::utils::screen::ScreenUtils::get_monitor_at_cursor(&app)?;
    let work_area = monitor.work_area();
    let scale_factor = monitor.scale_factor();
    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let work_area_x = work_area.position.x;
    let work_area_y = work_area.position.y;
    let work_area_width = work_area.size.width;
    let work_area_height = work_area.size.height;
    let (main_window_x, main_window_y, main_window_width, main_window_height) =
        app.get_webview_window("main")
            .and_then(|window| crate::get_window_bounds(&window).ok())
            .unwrap_or((0, 0, 0, 0));

    let preview_data = PreviewWindowData {
        mode,
        source,
        item_id,
        cursor_x,
        cursor_y,
        scale_factor,
        work_area_x,
        work_area_y,
        work_area_width,
        work_area_height,
        main_window_x,
        main_window_y,
        main_window_width,
        main_window_height,
        item_rect,
        request_id,
    };

    upsert_preview_data(preview_data.clone());

    if let Some(existing) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        apply_preview_window_bounds(
            &existing,
            work_area_x,
            work_area_y,
            work_area_width,
            work_area_height,
        )?;
        refresh_preview_window_always_on_top(&existing)?;
        existing
            .emit("preview-window-data-updated", &preview_data)
            .map_err(|e| format!("推送预览窗口数据失败: {}", e))?;
        return Ok(());
    }

    let app_for_create = app.clone();
    let preview_data_for_create = preview_data.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_error = None;
        for _ in 0..8 {
            if PREVIEW_REQUEST_VERSION.load(Ordering::SeqCst) != request_id {
                return;
            }

            let window = match create_preview_window(
                &app_for_create,
                work_area_x,
                work_area_y,
                work_area_width,
                work_area_height,
                scale_factor,
            ) {
                Ok(window) => window,
                Err(error) => {
                    if error.contains("already exists") {
                        last_error = Some(error);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        continue;
                    }

                    eprintln!("创建预览窗口失败: {}", error);
                    return;
                }
            };

            if PREVIEW_REQUEST_VERSION.load(Ordering::SeqCst) != request_id {
                let _ = window.close();
                return;
            }

            if let Err(error) = refresh_preview_window_always_on_top(&window) {
                eprintln!("刷新预览窗口置顶失败: {}", error);
            }
            let _ = window.emit("preview-window-data-updated", &preview_data_for_create);
            return;
        }

        if let Some(error) = last_error {
            eprintln!("创建预览窗口失败: {}", error);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn close_preview_window(app: AppHandle) -> Result<(), String> {
    PREVIEW_REQUEST_VERSION.fetch_add(1, Ordering::SeqCst);
    let request_id = PREVIEW_DATA
        .lock()
        .map_err(|_| "获取预览窗口状态失败".to_string())?
        .as_ref()
        .map(|data| data.request_id)
        .unwrap_or_default();

    if request_id == 0 {
        hide_preview_window_internal(&app);
        return Ok(());
    }

    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        window
            .emit("preview-window-will-hide", request_id)
            .map_err(|e| format!("发送预览窗口隐藏事件失败: {}", e))?;

        // 前端回调兜底看门狗:preview 的 JS 未加载/被阻断时,finalize_hide
        // 永不回来,预览窗口滞留到 destroy TTL(60s)。此处按需启动短超时
        // 兜底——窗口超时仍可见即强制销毁。
        let watchdog_version = PREVIEW_HIDE_WATCHDOG_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
        schedule_preview_close_watchdog(app, watchdog_version);
    } else {
        hide_preview_window_internal(&app);
    }

    Ok(())
}

pub fn suppress_preview_for_main_window_hide(app: &AppHandle) {
    PREVIEW_SUPPRESSED.store(true, Ordering::SeqCst);
    PREVIEW_REQUEST_VERSION.fetch_add(1, Ordering::SeqCst);
    PREVIEW_DESTROY_TIMER_VERSION.fetch_add(1, Ordering::SeqCst);
    destroy_preview_window_internal(app);

    let watchdog_version = PREVIEW_HIDE_WATCHDOG_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
    schedule_preview_hide_watchdog(app.clone(), watchdog_version);
}

pub fn resume_preview_after_main_window_show() {
    PREVIEW_SUPPRESSED.store(false, Ordering::SeqCst);
    PREVIEW_HIDE_WATCHDOG_VERSION.fetch_add(1, Ordering::SeqCst);
}

pub fn force_close_preview_window(app: &AppHandle) {
    PREVIEW_REQUEST_VERSION.fetch_add(1, Ordering::SeqCst);
    PREVIEW_DESTROY_TIMER_VERSION.fetch_add(1, Ordering::SeqCst);
    destroy_preview_window_internal(app);
}

#[tauri::command]
pub fn reveal_preview_window(app: AppHandle, request_id: u64) -> Result<(), String> {
    let current_request_id = PREVIEW_DATA
        .lock()
        .map_err(|_| "获取预览窗口状态失败".to_string())?
        .as_ref()
        .map(|data| data.request_id)
        .unwrap_or_default();

    if current_request_id != request_id {
        return Ok(());
    }

    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        refresh_preview_window_always_on_top(&window)?;
        window
            .show()
            .map_err(|e| format!("显示预览窗口失败: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn finalize_hide_preview_window(app: AppHandle, request_id: u64) -> Result<(), String> {
    let current_request_id = PREVIEW_DATA
        .lock()
        .map_err(|_| "获取预览窗口状态失败".to_string())?
        .as_ref()
        .map(|data| data.request_id)
        .unwrap_or_default();

    if current_request_id != request_id {
        return Ok(());
    }

    hide_preview_window_internal(&app);
    let timer_version = PREVIEW_DESTROY_TIMER_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
    schedule_preview_window_destroy(app, timer_version, request_id);
    Ok(())
}

#[tauri::command]
pub fn get_preview_window_data() -> Result<PreviewWindowData, String> {
    PREVIEW_DATA
        .lock()
        .map_err(|_| "获取预览窗口数据失败".to_string())?
        .clone()
        .ok_or_else(|| "预览窗口数据不存在".to_string())
}

#[cfg(test)]
mod tests {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    fn preview_source() -> String {
        strip_line_comments(&source_file("src/windows/preview_window/mod.rs"))
    }

    // 关闭预览窗口不能只 emit 就返回——preview JS 未加载/被阻断时,
    // finalize_hide 前端回调永不回来,窗口滞留到 destroy TTL(60s)。
    // close 路径必须启动兜底看门狗,超时窗口仍可见即强制销毁;且要与
    // 主窗口隐藏的 suppress 路径共用同一版本原子,互相取消对方已失效
    // 的看门狗(新代覆盖旧代)。
    #[test]
    fn close_preview_window_schedules_watchdog_fallback() {
        let src = preview_source();
        let close_body = fn_body(&src, "close_preview_window");
        let emit_pos = close_body
            .find("preview-window-will-hide")
            .expect("close_preview_window 必须 emit 隐藏事件");
        let schedule_pos = close_body
            .find("schedule_preview_close_watchdog(")
            .expect("close_preview_window 必须在 emit 后启动兜底看门狗");
        assert!(
            emit_pos < schedule_pos,
            "看门狗必须启动于 emit 之后(emit 是正常路径,看门狗是超时兜底)"
        );
        assert!(
            close_body.contains("PREVIEW_HIDE_WATCHDOG_VERSION.fetch_add(1,"),
            "看门狗必须推进共用版本原子,与 suppress 路径互相取消失效看门狗"
        );

        let watchdog_body = fn_body(&src, "schedule_preview_close_watchdog");
        assert!(
            watchdog_body.contains("PREVIEW_HIDE_WATCHDOG_DURATION_MS"),
            "看门狗必须有超时阈值"
        );
        assert!(
            watchdog_body.contains("destroy_preview_window_internal(&app)"),
            "看门狗超时后必须强制销毁预览窗口"
        );
        let visible_pos = watchdog_body
            .find("is_visible().ok()")
            .expect("看门狗必须按可见性判定 finalize 是否真的完成");
        let destroy_pos = watchdog_body
            .find("destroy_preview_window_internal(&app)")
            .expect("看门狗必须销毁窗口");
        assert!(
            visible_pos < destroy_pos,
            "必须确认窗口仍可见(超时未隐藏)才销毁"
        );
    }
}

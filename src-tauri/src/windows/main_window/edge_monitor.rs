use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, WebviewWindow};

use super::state::{observe_mouse_edge, MainWindowShowSource, MouseEdgeTransition};

static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);
static MONITORING_ACTIVE: AtomicBool = AtomicBool::new(false);
static MONITORING_GENERATION: AtomicU64 = AtomicU64::new(0);
static RESIZE_SUPPRESS_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

const RESIZE_SUPPRESS_DURATION_MS: u64 = 400;
const EDGE_HIDE_DELAY_MS: u64 = 200;
const AUTO_POPUP_TIMEOUT_MS: u64 = 2_000;

pub fn init_edge_monitor(window: WebviewWindow) {
    let window_for_event = window.clone();
    window_for_event.on_window_event(|event| {
        if matches!(
            event,
            tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::ScaleFactorChanged { .. }
        ) {
            suppress_edge_actions_after_resize();
        }
    });

    *MAIN_WINDOW.lock() = Some(window);
}

pub fn start_edge_monitoring() {
    let was_active = MONITORING_ACTIVE.swap(true, Ordering::Relaxed);
    if was_active {
        return;
    }

    let generation = MONITORING_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));

        let mut last_near_state = None;
        let mut last_hidden_state = false;
        let mut not_near_since_ms = None;
        let mouse_state_version = Arc::new(AtomicU64::new(0));

        loop {
            if !MONITORING_ACTIVE.load(Ordering::Relaxed)
                || MONITORING_GENERATION.load(Ordering::SeqCst) != generation
            {
                return;
            }

            let window = match MAIN_WINDOW.lock().clone() {
                Some(window) => window,
                None => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let state = crate::get_window_state();

            if is_resize_suppressed() {
                mouse_state_version.fetch_add(1, Ordering::SeqCst);
                not_near_since_ms = None;
                last_near_state = None;
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            if crate::windows::screenshot_window::is_screenshot_active() {
                mouse_state_version.fetch_add(1, Ordering::SeqCst);
                not_near_since_ms = None;
                last_near_state = None;
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            if !state.is_snapped || state.is_dragging {
                mouse_state_version.fetch_add(1, Ordering::SeqCst);
                not_near_since_ms = None;
                last_near_state = None;
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            let popup_state = super::state::mouse_auto_popup_state();
            if popup_state.is_expired(current_time_millis()) && !state.is_hidden {
                let session_id = popup_state.session_id;
                let window_for_task = window.clone();
                let mouse_state_version_for_task = mouse_state_version.clone();
                let expected_mouse_state_version = mouse_state_version.load(Ordering::SeqCst);
                let _ = window.app_handle().run_on_main_thread(move || {
                    if mouse_state_version_for_task.load(Ordering::SeqCst)
                        != expected_mouse_state_version
                    {
                        return;
                    }
                    let current_state = crate::get_window_state();
                    let current_popup = super::state::mouse_auto_popup_state();
                    if current_state.is_hidden
                        || current_state.is_dragging
                        || current_state.is_pinned
                        || !current_popup.is_current(session_id)
                    {
                        return;
                    }

                    if crate::hide_snapped_window(&window_for_task).is_ok() {
                        let _ = super::state::clear_mouse_auto_popup_for_session(session_id);
                    }
                });
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }

            if last_hidden_state != state.is_hidden {
                last_hidden_state = state.is_hidden;
                not_near_since_ms = None;
                mouse_state_version.fetch_add(1, Ordering::SeqCst);
                last_near_state = None;
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }

            if !crate::services::settings::is_edge_hover_popup_enabled() {
                last_near_state = None;
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }

            let is_near = match check_mouse_near_edge(&window, &state) {
                Ok(near) => near,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let (next_near_state, transition) = observe_mouse_edge(last_near_state, is_near);
            let state_changed = !matches!(transition, MouseEdgeTransition::Stable);
            if state_changed {
                mouse_state_version.fetch_add(1, Ordering::SeqCst);
            }
            last_near_state = next_near_state;

            if is_near || state.is_hidden || state.is_pinned {
                not_near_since_ms = None;
            } else if matches!(transition, MouseEdgeTransition::Leave)
                && not_near_since_ms.is_none()
            {
                not_near_since_ms = Some(current_time_millis());
            }

            if !state_changed && (is_near || state.is_hidden || state.is_pinned) {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }

            if matches!(transition, MouseEdgeTransition::Enter)
                && is_near
                && state.is_hidden
            {
                let window_for_task = window.clone();
                let _ = window.app_handle().run_on_main_thread(move || {
                    if crate::get_window_state().is_hidden {
                        if crate::show_snapped_window(
                            &window_for_task,
                            MainWindowShowSource::MouseAuto,
                        )
                        .is_ok()
                        {
                            let deadline = current_time_millis()
                                .saturating_add(AUTO_POPUP_TIMEOUT_MS);
                            let _ = super::state::start_mouse_auto_popup(deadline);
                        }
                    }
                });
            } else if !is_near
                && !state.is_hidden
                && !state.is_pinned
                && not_near_since_ms
                    .map(|since| {
                        current_time_millis().saturating_sub(since) >= EDGE_HIDE_DELAY_MS
                    })
                    .unwrap_or(false)
            {
                not_near_since_ms = None;
                let window_for_task = window.clone();
                let expected_mouse_state_version = mouse_state_version.load(Ordering::SeqCst);
                let mouse_state_version_for_task = mouse_state_version.clone();
                let _ = window.app_handle().run_on_main_thread(move || {
                    if mouse_state_version_for_task.load(Ordering::SeqCst)
                        != expected_mouse_state_version
                    {
                        return;
                    }

                    let current_state = crate::get_window_state();
                    let mouse_still_outside = check_mouse_near_edge(&window_for_task, &current_state)
                        .map(|near| !near)
                        .unwrap_or(false);
                    if mouse_still_outside && !current_state.is_hidden && !current_state.is_pinned {
                        let _ = crate::hide_snapped_window(&window_for_task);
                    }
                });
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

pub fn stop_edge_monitoring() {
    MONITORING_ACTIVE.store(false, Ordering::Relaxed);
}

fn suppress_edge_actions_after_resize() {
    let now_ms = current_time_millis();
    RESIZE_SUPPRESS_UNTIL_MS.store(
        now_ms.saturating_add(RESIZE_SUPPRESS_DURATION_MS),
        Ordering::SeqCst,
    );
}

fn is_resize_suppressed() -> bool {
    current_time_millis() < RESIZE_SUPPRESS_UNTIL_MS.load(Ordering::SeqCst)
}

fn current_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

const MOUSE_LEAVE_TOLERANCE: i32 = 8;

fn check_mouse_near_edge(
    window: &WebviewWindow,
    state: &super::state::MainWindowState,
) -> Result<bool, String> {
    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let (win_x, win_y, win_width, win_height) = crate::get_window_bounds(window)?;
    let (edge_snap_ratio, edge_snap_monitor_id, edge_hide_offset) =
        crate::services::settings::get_edge_monitor_settings();
    let ratio = state.snap_ratio.or(edge_snap_ratio).unwrap_or(
        super::snap::compute_snap_ratio(
            window.app_handle(),
            state.snap_edge,
            win_x,
            win_y,
            win_width as i32,
            win_height as i32,
        )?,
    );
    let resolved = super::snap::resolve_snapped_position(
        window.app_handle(),
        state.snap_edge,
        state
            .snap_monitor_id
            .as_deref()
            .or(edge_snap_monitor_id.as_deref()),
        ratio,
        win_width as i32,
        win_height as i32,
    )?;

    let base_trigger = if edge_hide_offset >= 10 {
        edge_hide_offset
    } else {
        10
    };
    let mouse_in_window = cursor_x >= win_x - MOUSE_LEAVE_TOLERANCE
        && cursor_x <= win_x + win_width as i32 + MOUSE_LEAVE_TOLERANCE
        && cursor_y >= win_y - MOUSE_LEAVE_TOLERANCE
        && cursor_y <= win_y + win_height as i32 + MOUSE_LEAVE_TOLERANCE;
    if mouse_in_window && !state.is_hidden {
        return Ok(false);
    }

    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let trigger = (base_trigger as f64 * scale).round() as i32;
    Ok(match resolved.edge {
        super::state::SnapEdge::Left => cursor_x <= resolved.x + trigger,
        super::state::SnapEdge::Right => {
            cursor_x >= resolved.x + win_width as i32 - trigger
        }
        super::state::SnapEdge::Top => cursor_y <= resolved.y + trigger,
        super::state::SnapEdge::Bottom => {
            cursor_y >= resolved.y + win_height as i32 - trigger
        }
        super::state::SnapEdge::None => false,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    fn source_file(path: &str) -> String {
        fs::read_to_string(format!("{}/{}", env!("CARGO_MANIFEST_DIR"), path))
            .expect("找不到源文件")
    }

    fn function_body(source: &str, name: &str) -> String {
        let start = source.find(name).expect("找不到函数");
        let tail = &source[start..];
        let end = tail.find("
fn ").unwrap_or(tail.len());
        tail[..end].to_string()
    }

    fn strip_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("
")
    }

    #[test]
    fn mouse_edge_show_declares_mouse_auto_source() {
        let source = source_file("src/windows/main_window/edge_monitor.rs");
        let body = strip_line_comments(&function_body(&source, "start_edge_monitoring"));
        assert!(body.contains("MainWindowShowSource::MouseAuto"));
    }

    #[test]
    fn mouse_edge_show_creates_timeout_session_after_success() {
        let source = source_file("src/windows/main_window/edge_monitor.rs");
        let body = strip_line_comments(&function_body(&source, "start_edge_monitoring"));
        let show = body.find("show_snapped_window").expect("缺少自动显示");
        let success = body[show..]
            .find(".is_ok()")
            .map(|offset| show + offset)
            .expect("自动显示必须检查成功");
        let session = body[success..]
            .find("start_mouse_auto_popup")
            .map(|offset| success + offset)
            .expect("自动显示成功后必须创建会话");
        assert!(success < session);
    }

    #[test]
    fn timeout_hide_keeps_session_until_hide_succeeds() {
        let source = source_file("src/windows/main_window/edge_monitor.rs");
        let body = strip_line_comments(&function_body(&source, "start_edge_monitoring"));
        let hide = body.find("hide_snapped_window").expect("缺少超时隐藏");
        let clear = body
            .find("clear_mouse_auto_popup_for_session")
            .expect("缺少会话清理");
        assert!(hide < clear);
        assert!(body.contains("current_popup.is_current(session_id)"));
    }

    #[test]
    fn delayed_hide_rechecks_on_stable_far_samples() {
        let source = source_file("src/windows/main_window/edge_monitor.rs");
        let body = strip_line_comments(&function_body(&source, "start_edge_monitoring"));
        let stable_guard = body
            .find("if !state_changed && (is_near || state.is_hidden || state.is_pinned)")
            .expect("缺少稳定采样快速路径");
        let hide = body[stable_guard..]
            .find("hide_snapped_window")
            .map(|offset| stable_guard + offset)
            .expect("稳定离开后必须仍有延迟隐藏路径");
        assert!(stable_guard < hide);
        assert!(!body[hide..].contains("MouseEdgeTransition::Leave"));
    }

    #[test]
    fn stale_timeout_task_is_invalidated_when_window_leaves_snap() {
        let source = source_file("src/windows/main_window/edge_monitor.rs");
        let body = strip_line_comments(&function_body(&source, "start_edge_monitoring"));
        let state_guard = body
            .find("if !state.is_snapped || state.is_dragging")
            .expect("缺少非贴边状态守卫");
        let version_bump = body[state_guard..]
            .find("mouse_state_version.fetch_add")
            .map(|offset| state_guard + offset)
            .expect("离开贴边状态必须失效旧任务");
        let timeout_task = body
            .find("mouse_state_version_for_task")
            .expect("超时任务必须携带状态版本");
        assert!(version_bump < timeout_task);
        assert!(body[state_guard..].contains("not_near_since_ms = None"));
        assert!(body[state_guard..].contains("last_near_state = None"));
        assert!(
            !body[state_guard..].contains("super::state::invalidate_mouse_auto_popup()"),
            "拖拽会话失效必须由 set_dragging 统一负责，监控线程不得重复推进会话代际"
        );
        let resize_guard = body
            .find("if is_resize_suppressed()")
            .expect("缺少调整大小状态守卫");
        let screenshot_guard = body
            .find("if crate::windows::screenshot_window::is_screenshot_active()")
            .expect("缺少截图状态守卫");
        assert!(body[resize_guard..].contains("mouse_state_version.fetch_add"));
        assert!(body[screenshot_guard..].contains("mouse_state_version.fetch_add"));
    }
}

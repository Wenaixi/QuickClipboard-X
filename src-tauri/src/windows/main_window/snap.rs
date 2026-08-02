use tauri::{WebviewWindow, Manager, Emitter};
use super::state::{SnapEdge, set_snap_edge, set_hidden_and_window_state, clear_snap, is_snapped};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SNAP_THRESHOLD: i32 = 30;
const FRONTEND_CONTENT_INSET_LOGICAL: f64 = 5.0;
// edge_monitor 50ms 轮询每次 resolve 都走 build_monitor_contexts;
// 显示器拓扑几乎不变,500ms TTL 把 20Hz 系统调用压到 ~2Hz。
const MONITOR_CONTEXT_CACHE_TTL: Duration = Duration::from_millis(500);

static ANIMATION_VERSION: AtomicU64 = AtomicU64::new(0);
static MONITOR_CONTEXT_CACHE: Lazy<Mutex<Option<(Instant, Vec<MonitorEdgeContext>)>>> =
    Lazy::new(|| Mutex::new(None));

// 决定 hide 路径是否走滑出动画。抽成纯函数便于 #[cfg(test)] 直接断言,
// 防止未来有人在条件里塞入 `edge_hover_popup_enabled` 之类导致 hover 关闭时
// 静默跳过滑出动画(4352887c 报告:动画条件仅由 clipboard_animation_enabled 决定)。
fn should_play_hide_animation(settings: &crate::services::AppSettings) -> bool {
    settings.clipboard_animation_enabled
}

// bump 动画版本号:让正在等待的 post-animation 任务醒来后自行退出,
// 避免用过期设置改写形态(发现 toggle 反转、半屏滑入点穿等)
fn cancel_pending_animation() {
    ANIMATION_VERSION.fetch_add(1, Ordering::SeqCst);
}

// 等待动画结束后调 refresh 重新写入形态;等待期间任何同步形态决策
// (refresh_hidden_snapped_window / hide_snapped_window / restore_edge_snap_on_startup)
// 都会再次 bump,本 task 醒来即自杀
fn schedule_post_animation_refresh(
    window: WebviewWindow,
    app: tauri::AppHandle,
    delay_ms: u64,
) -> Result<(), String> {
    let version = share_animation_version();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        if ANIMATION_VERSION.load(Ordering::SeqCst) != version {
            return;
        }
        let state = super::state::get_window_state();
        if !state.is_snapped || !state.is_hidden {
            return;
        }
        let _ = app.run_on_main_thread(move || {
            let _ = refresh_hidden_snapped_window(&window);
        });
    });
    Ok(())
}

// 动画结束后写回 ignore=false(打开交互)。
// show_snapped_window 在动画开启时必须保持穿透,等动画把窗口滑到屏上后再解锁,
// 避免动画中段在屏外/中间坐标短暂可点。
// 共享在飞动画版本,任何同步形态决策(cancel_pending_animation)会 bump 使本任务自杀。
fn schedule_post_animation_set_interactive(
    window: WebviewWindow,
    app: tauri::AppHandle,
    delay_ms: u64,
) {
    let version = share_animation_version();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        if ANIMATION_VERSION.load(Ordering::SeqCst) != version {
            return;
        }
        let _ = app.run_on_main_thread(move || {
            let _ = window.set_ignore_cursor_events(false);
        });
    });
}

#[derive(Clone, Debug)]
struct MonitorEdgeContext {
    id: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale_factor: f64,
    left_edge: bool,
    right_edge: bool,
    top_edge: bool,
    bottom_edge: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedSnapPosition {
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub edge: SnapEdge,
    pub monitor_id: String,
}

fn get_content_inset(scale_factor: f64) -> i32 {
    (FRONTEND_CONTENT_INSET_LOGICAL * scale_factor) as i32
}

fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.5
    }
}

fn build_monitor_identifier(monitor: &tauri::Monitor) -> String {
    if let Some(name) = monitor.name() {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let pos = monitor.position();
    let size = monitor.size();
    format!(
        "monitor:{}:{}:{}:{}:{:.3}",
        pos.x,
        pos.y,
        size.width,
        size.height,
        monitor.scale_factor()
    )
}

fn edge_to_setting_value(edge: SnapEdge) -> Option<String> {
    match edge {
        SnapEdge::Left => Some("left".to_string()),
        SnapEdge::Right => Some("right".to_string()),
        SnapEdge::Top => Some("top".to_string()),
        SnapEdge::Bottom => Some("bottom".to_string()),
        SnapEdge::None => None,
    }
}

fn edge_from_setting_value(value: &str) -> Option<SnapEdge> {
    match value {
        "left" => Some(SnapEdge::Left),
        "right" => Some(SnapEdge::Right),
        "top" => Some(SnapEdge::Top),
        "bottom" => Some(SnapEdge::Bottom),
        _ => None,
    }
}

// 纯函数:缓存条目是否仍新鲜。抽出来便于单测,不依赖 AppHandle。
fn monitor_context_cache_is_fresh(cached_at: Instant, now: Instant, ttl: Duration) -> bool {
    now.duration_since(cached_at) < ttl
}

fn build_monitor_contexts(app: &tauri::AppHandle) -> Result<Vec<MonitorEdgeContext>, String> {
    {
        let cache = MONITOR_CONTEXT_CACHE.lock();
        if let Some((cached_at, contexts)) = cache.as_ref() {
            if monitor_context_cache_is_fresh(*cached_at, Instant::now(), MONITOR_CONTEXT_CACHE_TTL)
            {
                return Ok(contexts.clone());
            }
        }
    }
    let contexts = build_monitor_contexts_uncached(app)?;
    *MONITOR_CONTEXT_CACHE.lock() = Some((Instant::now(), contexts.clone()));
    Ok(contexts)
}

fn build_monitor_contexts_uncached(
    app: &tauri::AppHandle,
) -> Result<Vec<MonitorEdgeContext>, String> {
    let monitors = app
        .available_monitors()
        .map_err(|e| format!("获取显示器列表失败: {}", e))?;
    let monitor_edges = crate::utils::screen::ScreenUtils::get_all_monitors_with_edges(app)?;
    let contexts = monitors
        .into_iter()
        .filter_map(|monitor| {
            let pos = monitor.position();
            let size = monitor.size();
            let x = pos.x;
            let y = pos.y;
            let width = size.width as i32;
            let height = size.height as i32;
            let scale_factor = monitor.scale_factor();
            monitor_edges
                .iter()
                .find(|(mx, my, mw, mh, _, _, _, _)| {
                    *mx == x && *my == y && *mw == width && *mh == height
                })
                .map(|(_, _, _, _, left_edge, right_edge, top_edge, bottom_edge)| {
                    MonitorEdgeContext {
                        id: build_monitor_identifier(&monitor),
                        x,
                        y,
                        width,
                        height,
                        scale_factor,
                        left_edge: *left_edge,
                        right_edge: *right_edge,
                        top_edge: *top_edge,
                        bottom_edge: *bottom_edge,
                    }
                })
        })
        .collect();
    Ok(contexts)
}

fn monitor_supports_edge(monitor: &MonitorEdgeContext, edge: SnapEdge) -> bool {
    match edge {
        SnapEdge::Left => monitor.left_edge,
        SnapEdge::Right => monitor.right_edge,
        SnapEdge::Top => monitor.top_edge,
        SnapEdge::Bottom => monitor.bottom_edge,
        SnapEdge::None => false,
    }
}

fn find_monitor_for_window(
    app: &tauri::AppHandle,
    edge: SnapEdge,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<MonitorEdgeContext, String> {
    let contexts = build_monitor_contexts(app)?;
    let anchor_x = x + width / 2;
    let anchor_y = y + height / 2;

    contexts
        .iter()
        .cloned()
        .filter(|monitor| monitor_supports_edge(monitor, edge))
        .filter(|monitor| {
            anchor_x >= monitor.x
                && anchor_x < monitor.x + monitor.width
                && anchor_y >= monitor.y
                && anchor_y < monitor.y + monitor.height
        })
        .next()
        .or_else(|| {
            contexts
                .iter()
                .cloned()
                .filter(|monitor| monitor_supports_edge(monitor, edge))
                .min_by_key(|monitor| {
                    let center_x = monitor.x + monitor.width / 2;
                    let center_y = monitor.y + monitor.height / 2;
                    let dx = center_x - anchor_x;
                    let dy = center_y - anchor_y;
                    dx * dx + dy * dy
                })
        })
        .ok_or_else(|| "未找到可用的贴边显示器".to_string())
}

fn resolve_usable_edge_on_monitor(
    monitor: &MonitorEdgeContext,
    preferred_edge: SnapEdge,
) -> Option<SnapEdge> {
    let ordered_edges = match preferred_edge {
        SnapEdge::Top => [SnapEdge::Top, SnapEdge::Left, SnapEdge::Right, SnapEdge::Bottom],
        SnapEdge::Bottom => [SnapEdge::Bottom, SnapEdge::Left, SnapEdge::Right, SnapEdge::Top],
        SnapEdge::Left => [SnapEdge::Left, SnapEdge::Top, SnapEdge::Bottom, SnapEdge::Right],
        SnapEdge::Right => [SnapEdge::Right, SnapEdge::Top, SnapEdge::Bottom, SnapEdge::Left],
        SnapEdge::None => [SnapEdge::None, SnapEdge::Top, SnapEdge::Bottom, SnapEdge::Left],
    };

    ordered_edges
        .into_iter()
        .find(|edge| monitor_supports_edge(monitor, *edge))
}

fn compute_local_ratio_in_monitor(
    monitor: &MonitorEdgeContext,
    edge: SnapEdge,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> f64 {
    match edge {
        SnapEdge::Top | SnapEdge::Bottom => {
            let span = (monitor.width - width).max(0) as f64;
            if span <= 0.0 {
                0.5
            } else {
                clamp_ratio((x - monitor.x) as f64 / span)
            }
        }
        SnapEdge::Left | SnapEdge::Right => {
            let span = (monitor.height - height).max(0) as f64;
            if span <= 0.0 {
                0.5
            } else {
                clamp_ratio((y - monitor.y) as f64 / span)
            }
        }
        SnapEdge::None => 0.5,
    }
}

fn resolve_position_in_monitor(
    monitor: &MonitorEdgeContext,
    edge: SnapEdge,
    ratio: f64,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let ratio = clamp_ratio(ratio);
    match edge {
        SnapEdge::Top | SnapEdge::Bottom => {
            let span = (monitor.width - width).max(0);
            let x = if span > 0 {
                monitor.x + (span as f64 * ratio).round() as i32
            } else {
                monitor.x
            };
            (x, monitor.y)
        }
        SnapEdge::Left | SnapEdge::Right => {
            let span = (monitor.height - height).max(0);
            let y = if span > 0 {
                monitor.y + (span as f64 * ratio).round() as i32
            } else {
                monitor.y
            };
            (monitor.x, y)
        }
        SnapEdge::None => (monitor.x, monitor.y),
    }
}

fn resolve_saved_monitor_and_edge(
    app: &tauri::AppHandle,
    preferred_edge: SnapEdge,
    monitor_id: Option<&str>,
) -> Result<(MonitorEdgeContext, SnapEdge), String> {
    let contexts = build_monitor_contexts(app)?;

    if let Some(monitor_id) = monitor_id {
        if let Some(monitor) = contexts.iter().find(|monitor| monitor.id == monitor_id) {
            if let Some(edge) = resolve_usable_edge_on_monitor(monitor, preferred_edge) {
                return Ok((monitor.clone(), edge));
            }
        }
    }

    if let Some(monitor) = contexts
        .iter()
        .find(|monitor| monitor_supports_edge(monitor, preferred_edge))
    {
        return Ok((monitor.clone(), preferred_edge));
    }

    contexts
        .iter()
        .find_map(|monitor| {
            resolve_usable_edge_on_monitor(monitor, preferred_edge)
                .map(|edge| (monitor.clone(), edge))
        })
        .ok_or_else(|| "未找到可用的贴边显示器".to_string())
}

fn resolve_startup_restore_window_size(
    window: &WebviewWindow,
    edge: SnapEdge,
    monitor_id: Option<&str>,
    settings: &crate::services::AppSettings,
) -> Result<(i32, i32), String> {
    let (monitor, _) = resolve_saved_monitor_and_edge(window.app_handle(), edge, monitor_id)?;

    if settings.remember_window_size {
        if let Some((saved_width, saved_height)) = settings.saved_window_size {
            let (logical_width, logical_height) =
                crate::utils::sizing::normalize_saved_window_size(saved_width, saved_height);
            return Ok((
                (logical_width * monitor.scale_factor).round().max(1.0) as i32,
                (logical_height * monitor.scale_factor).round().max(1.0) as i32,
            ));
        }
    }

    let size = window.outer_size().map_err(|e| e.to_string())?;
    Ok((size.width as i32, size.height as i32))
}

pub(crate) fn compute_snap_layout(
    app: &tauri::AppHandle,
    edge: SnapEdge,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(String, f64), String> {
    let monitor = find_monitor_for_window(app, edge, x, y, width, height)?;
    let ratio = compute_local_ratio_in_monitor(&monitor, edge, x, y, width, height);
    Ok((monitor.id.clone(), ratio))
}

pub(crate) fn compute_snap_ratio(
    app: &tauri::AppHandle,
    edge: SnapEdge,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<f64, String> {
    Ok(compute_snap_layout(app, edge, x, y, width, height)?.1)
}

pub(crate) fn resolve_snapped_position(
    app: &tauri::AppHandle,
    edge: SnapEdge,
    monitor_id: Option<&str>,
    ratio: f64,
    width: i32,
    height: i32,
) -> Result<ResolvedSnapPosition, String> {
    let (monitor, actual_edge) = resolve_saved_monitor_and_edge(app, edge, monitor_id)?;
    let content_inset = get_content_inset(monitor.scale_factor);
    let (base_x, base_y) =
        resolve_position_in_monitor(&monitor, actual_edge, ratio, width, height);

    let position = match actual_edge {
        SnapEdge::Left => (monitor.x - content_inset, base_y),
        SnapEdge::Right => (monitor.x + monitor.width - width + content_inset, base_y),
        SnapEdge::Top => (base_x, monitor.y - content_inset),
        SnapEdge::Bottom => (base_x, monitor.y + monitor.height - height + content_inset),
        SnapEdge::None => (base_x, base_y),
    };
    Ok(ResolvedSnapPosition {
        x: position.0,
        y: position.1,
        scale_factor: monitor.scale_factor,
        edge: actual_edge,
        monitor_id: monitor.id.clone(),
    })
}

fn resolve_hidden_position(
    app: &tauri::AppHandle,
    edge: SnapEdge,
    monitor_id: Option<&str>,
    ratio: f64,
    width: i32,
    height: i32,
    edge_hide_offset: i32,
) -> Result<ResolvedSnapPosition, String> {
    let (monitor, actual_edge) = resolve_saved_monitor_and_edge(app, edge, monitor_id)?;
    let content_inset = get_content_inset(monitor.scale_factor);
    let hide_offset = if edge_hide_offset == 0 {
        0
    } else {
        content_inset + edge_hide_offset
    };
    let (base_x, base_y) =
        resolve_position_in_monitor(&monitor, actual_edge, ratio, width, height);

    let position = match actual_edge {
        SnapEdge::Left => (monitor.x - width + hide_offset, base_y),
        SnapEdge::Right => (monitor.x + monitor.width - hide_offset, base_y),
        SnapEdge::Top => (base_x, monitor.y - height + hide_offset),
        SnapEdge::Bottom => (base_x, monitor.y + monitor.height - hide_offset),
        SnapEdge::None => (base_x, base_y),
    };
    Ok(ResolvedSnapPosition {
        x: position.0,
        y: position.1,
        scale_factor: monitor.scale_factor,
        edge: actual_edge,
        monitor_id: monitor.id.clone(),
    })
}

fn save_snap_layout(edge: SnapEdge, ratio: f64, monitor_id: Option<String>) {
    let edge_value = edge_to_setting_value(edge);
    let normalized_ratio = clamp_ratio(ratio);
    let _ = crate::services::settings::update_with(|settings| {
        settings.edge_snap_edge = edge_value.clone();
        settings.edge_snap_ratio = Some(normalized_ratio);
        settings.edge_snap_monitor_id = monitor_id.clone();
    });
}

fn clear_saved_snap_layout() {
    let _ = crate::services::settings::update_with(|settings| {
        settings.edge_snap_position = None;
        settings.edge_snap_edge = None;
        settings.edge_snap_ratio = None;
        settings.edge_snap_monitor_id = None;
    });
}

pub fn check_snap(window: &WebviewWindow) -> Result<(), String> {
    let settings = crate::get_settings();
    if !settings.edge_hide_enabled {
        return Ok(());
    }
    
    let (x, y, w, h) = crate::utils::positioning::get_window_bounds(window)?;
    
    let app = window.app_handle();
    let (monitor_x, monitor_y, monitor_w, monitor_h) = 
        crate::utils::screen::ScreenUtils::get_monitor_at_point(app, x, y)?;
    let monitor_right = monitor_x + monitor_w;
    let monitor_bottom = monitor_y + monitor_h;
    
    let (left_is_edge, right_is_edge, top_is_edge, bottom_is_edge) = 
        crate::utils::screen::ScreenUtils::get_real_edges_at_point(app, x, y)?;
    
    let edge = if left_is_edge && (x - monitor_x).abs() <= SNAP_THRESHOLD {
        Some(SnapEdge::Left)
    } else if right_is_edge && (monitor_right - (x + w as i32)).abs() <= SNAP_THRESHOLD {
        Some(SnapEdge::Right)
    } else if top_is_edge && (y - monitor_y).abs() <= SNAP_THRESHOLD {
        Some(SnapEdge::Top)
    } else if bottom_is_edge && (monitor_bottom - (y + h as i32)).abs() <= SNAP_THRESHOLD {
        Some(SnapEdge::Bottom)
    } else {
        None
    };
    
    if let Some(edge) = edge {
        let (monitor_id, ratio) = compute_snap_layout(app, edge, x, y, w as i32, h as i32)?;
        set_snap_edge(edge, Some((x, y)), Some(monitor_id.clone()), Some(ratio));
        save_snap_layout(edge, ratio, Some(monitor_id));
        snap_to_edge(window, edge)?;
        super::edge_monitor::start_edge_monitoring();
    } else {
        clear_snap();
        clear_saved_snap_layout();
        super::edge_monitor::stop_edge_monitoring();
    }
    
    Ok(())
}

pub fn snap_to_edge(window: &WebviewWindow, edge: SnapEdge) -> Result<(), String> {
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y, _, _) = crate::utils::positioning::get_window_bounds(window)?;
    let settings = crate::get_settings();
    let ratio = compute_snap_ratio(
        window.app_handle(),
        edge,
        x,
        y,
        size.width as i32,
        size.height as i32,
    )?;
    
    let resolved = resolve_snapped_position(
        window.app_handle(),
        edge,
        settings.edge_snap_monitor_id.as_deref(),
        ratio,
        size.width as i32,
        size.height as i32,
    )?;
    
    window.set_position(tauri::PhysicalPosition::new(resolved.x, resolved.y))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

pub fn hide_snapped_window(window: &WebviewWindow) -> Result<(), String> {
    use tauri::Manager;

    let state = super::state::get_window_state();

    if !state.is_snapped || state.is_hidden {
        return Ok(());
    }

    if crate::is_context_menu_visible() {
        return Ok(());
    }

    crate::windows::preview_window::suppress_preview_for_main_window_hide(&window.app_handle());
    let _ = crate::windows::pin_image_window::close_image_preview(window.app_handle().clone());
    #[cfg(feature = "gpu-image-viewer")]
    let _ = crate::windows::native_pin_window::close_native_image_preview();
    let _ = crate::windows::preview_window::close_preview_window(window.app_handle().clone());
    let _ = window.emit("edge-snap-hide", ());

    let settings = crate::get_settings();

    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y, _, _) = crate::utils::positioning::get_window_bounds(window)?;
    let ratio = compute_snap_ratio(
        window.app_handle(),
        state.snap_edge,
        x,
        y,
        size.width as i32,
        size.height as i32,
    )
    .or_else(|error| state.snap_ratio.or(settings.edge_snap_ratio).ok_or(error))?;
    let resolved = resolve_hidden_position(
        window.app_handle(),
        state.snap_edge,
        state
            .snap_monitor_id
            .as_deref()
            .or(settings.edge_snap_monitor_id.as_deref()),
        ratio,
        size.width as i32,
        size.height as i32,
        settings.edge_hide_offset,
    )?;

    // 先统一记账为屏外 resolved,形态(穿透/置顶/显隐)只交给 refresh
    set_snap_edge(
        resolved.edge,
        Some((resolved.x, resolved.y)),
        Some(resolved.monitor_id.clone()),
        Some(ratio),
    );
    set_hidden_and_window_state(true, super::state::WindowState::Hidden);
    save_snap_layout(resolved.edge, ratio, Some(resolved.monitor_id));

    // 形态决策(穿透/置顶/显隐)全部交给 refresh_hidden_snapped_window 唯一决策点
    // 动画分支:先滑到屏外坐标,延迟 200ms 再 refresh,避免半屏状态被穿透导致触发条闪一下
    // 动画条件由 should_play_hide_animation 决定,hover 关闭不跳过滑出动画
    if should_play_hide_animation(&settings) {
        animate_window_position(window, x, y, resolved.x, resolved.y, 200)?;
        schedule_post_animation_refresh(window.clone(), window.app_handle().clone(), 200)?;
    } else {
        refresh_hidden_snapped_window(window)?;
    }

    crate::services::memory::schedule_cleanup_after_main_window_hide();
    crate::input_monitor::disable_mouse_monitoring();
    crate::input_monitor::disable_navigation_keys();

    Ok(())
}

pub fn refresh_hidden_snapped_window(window: &WebviewWindow) -> Result<(), String> {
    let state = super::state::get_window_state();

    if !state.is_snapped || !state.is_hidden {
        return Ok(());
    }

    // 取消任何在飞的 post-animation 延迟任务,避免它用过期的设置改写形态
    cancel_pending_animation();

    // 尾部写入必须尊重并发 show:若中途被抢先 set_hidden_and_window_state(false, Visible),
    // 跳过本次反手覆盖(否则 toggle 会反复走 hide 路径)。
    // 入口早返已保证 state.is_hidden=true,只需 re-check 当前状态。

    let settings = crate::get_settings();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y, _, _) = crate::utils::positioning::get_window_bounds(window)?;
    let ratio = state
        .snap_ratio
        .or(settings.edge_snap_ratio)
        .unwrap_or(compute_snap_ratio(
            window.app_handle(),
            state.snap_edge,
            x,
            y,
            size.width as i32,
            size.height as i32,
        )?);
    let resolved = resolve_hidden_position(
        window.app_handle(),
        state.snap_edge,
        state
            .snap_monitor_id
            .as_deref()
            .or(settings.edge_snap_monitor_id.as_deref()),
        ratio,
        size.width as i32,
        size.height as i32,
        settings.edge_hide_offset,
    )?;

    // 形态唯一决策点:先落位再统一显隐/置顶/穿透
    // set_position 必须先做,失败直接 return —— 避免出现停在屏外却已 show + 可点 的窗口
    window
        .set_position(tauri::PhysicalPosition::new(resolved.x, resolved.y))
        .map_err(|e| e.to_string())?;

    if settings.edge_hover_popup_enabled {
        // 悬浮开启:触发条停留屏外但需保留可弹出,show 后立刻穿透,避免拦截边缘点击。
        // 仅当窗口不可见时才 show,避免重复 show 激活/抢焦点打断用户操作
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_always_on_top(true);
        let _ = window.set_ignore_cursor_events(true);
    } else {
        // 悬浮关闭:真正 hide,清掉 WS_EX_TRANSPARENT 残影;
        // 不动 set_always_on_top —— 用户置顶偏好由 show_snapped_window 的
        // refresh_always_on_top 按 settings.is_pinned 重写,这里吃掉也白吃
        let _ = window.hide();
        let _ = window.set_ignore_cursor_events(false);
    }
    set_snap_edge(
        resolved.edge,
        Some((resolved.x, resolved.y)),
        Some(resolved.monitor_id.clone()),
        Some(ratio),
    );
    // 原子写 is_hidden + WindowState,避免与并发 toggle 交错时出现
    // is_hidden=true 但 state=Visible 的撕裂态(会误判 should_show)。
    // 写入前 re-check:若中途被并发 show 抢先 set_hidden_and_window_state(false, Visible),
    // 尊重对方的写入,跳过本路径的反手覆盖。
    if super::state::get_window_state().is_hidden {
        set_hidden_and_window_state(true, super::state::WindowState::Hidden);
    }
    save_snap_layout(resolved.edge, ratio, Some(resolved.monitor_id));

    Ok(())
}

pub fn needs_hidden_snap_refresh(window: &WebviewWindow) -> Result<bool, String> {
    let state = super::state::get_window_state();

    if !state.is_snapped || !state.is_hidden {
        return Ok(false);
    }

    let settings = crate::get_settings();
    // 即便 hover 关,显示器变更时也要让 snap_monitor_id 与 state 重同步,
    // 避免拔屏后 reopen hover 时触发条漂到虚拟坐标
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y, _, _) = crate::utils::positioning::get_window_bounds(window)?;
    let ratio = state
        .snap_ratio
        .or(settings.edge_snap_ratio)
        .unwrap_or(compute_snap_ratio(
            window.app_handle(),
            state.snap_edge,
            x,
            y,
            size.width as i32,
            size.height as i32,
        )?);
    let resolved = resolve_hidden_position(
        window.app_handle(),
        state.snap_edge,
        state
            .snap_monitor_id
            .as_deref()
            .or(settings.edge_snap_monitor_id.as_deref()),
        ratio,
        size.width as i32,
        size.height as i32,
        settings.edge_hide_offset,
    )?;

    const POSITION_TOLERANCE: i32 = 2;
    Ok(
        (x - resolved.x).abs() > POSITION_TOLERANCE
            || (y - resolved.y).abs() > POSITION_TOLERANCE,
    )
}

pub fn show_snapped_window(window: &WebviewWindow) -> Result<(), String> {
    crate::windows::preview_window::resume_preview_after_main_window_show();

    let state = super::state::get_window_state();

    if !state.is_snapped || !state.is_hidden {
        return Ok(());
    }

    // 取消任何在飞的 hide 动画/post-refresh 任务:
    // hide 动画先记账 is_hidden=true 再异步滑出,若用户立刻 toggle,
    // 在飞 hide 帧/延迟 refresh 会继续写屏外坐标并覆盖本次 show 的形态,
    // 导致窗口停在屏外且点穿。bump 版本让 hide 线程下一帧自杀。
    cancel_pending_animation();

    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (x, y, _, _) = crate::utils::positioning::get_window_bounds(window)?;
    let settings = crate::get_settings();

    let ratio = state
        .snap_ratio
        .or(settings.edge_snap_ratio)
        .unwrap_or(compute_snap_ratio(
            window.app_handle(),
            state.snap_edge,
            x,
            y,
            size.width as i32,
            size.height as i32,
        )?);
    let resolved = resolve_snapped_position(
        window.app_handle(),
        state.snap_edge,
        state
            .snap_monitor_id
            .as_deref()
            .or(settings.edge_snap_monitor_id.as_deref()),
        ratio,
        size.width as i32,
        size.height as i32,
    )?;

    // 顺序与 refresh 唯一决策点对齐:先落位(set_position)再打开交互(ignore=false),
    // 最后 show。避免 set_ignore_cursor_events(false) 早于 set_position 导致
    // 窗口短暂在屏外坐标变成可点击但不显示,被 raw_input / 无障碍 API 命中触发幽灵 click。
    if settings.clipboard_animation_enabled {
        // 动画分支:animate 自带 set_position 逐步到位,期间必须保持穿透,
        // 否则动画中段在屏外/中间坐标可点,被 raw_input 命中再次 hide。
        // 显式 set_ignore_cursor_events(true) 在 show 之后立即写,防止 show 抢焦点
        // 后窗口在屏外坐标被点击截获;动画结束后由 schedule_post_animation_set_interactive 写回 false。
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.emit("edge-snap-show", ());
        let _ = crate::commands::window::emit_main_window_refresh_needed_event(&window.app_handle());
        animate_window_position(window, x, y, resolved.x, resolved.y, 200)?;
        schedule_post_animation_set_interactive(window.clone(), window.app_handle().clone(), 200);
    } else {
        // 非动画分支:先 set_position 落到屏上,再打开交互,最后 show。
        // 全程同步无中段,不会暴露屏外可点状态。
        window
            .set_position(tauri::PhysicalPosition::new(resolved.x, resolved.y))
            .map_err(|e| e.to_string())?;
        let _ = window.set_ignore_cursor_events(false);
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.emit("edge-snap-show", ());
        let _ = crate::commands::window::emit_main_window_refresh_needed_event(&window.app_handle());
    }
    set_snap_edge(
        resolved.edge,
        Some((resolved.x, resolved.y)),
        Some(resolved.monitor_id.clone()),
        Some(ratio),
    );
    set_hidden_and_window_state(false, super::state::WindowState::Visible);
    save_snap_layout(resolved.edge, ratio, Some(resolved.monitor_id));
    crate::services::webdav_sync::notify_main_window_shown(window.app_handle().clone());
    let _ = super::refresh_always_on_top(window);

    crate::input_monitor::enable_mouse_monitoring();
    crate::input_monitor::enable_navigation_keys();
    
    Ok(())
}

// 动画开始:独占一次 bump,取走当前版本。同一批动画后续的 post-refresh 不得再 bump。
fn begin_animation() -> u64 {
    ANIMATION_VERSION.fetch_add(1, Ordering::SeqCst) + 1
}

// post-animation 刷新共享在飞动画的版本,不 bump。
// begin_animation 独占初始 bump(同批动画首次 spawn 时 fetch_add 取走当前版本),
// cancel_pending_animation(同步形态决策)可 bump 让在飞动画与 post-refresh 一起失效;
// 本函数仅 load 共享,不 bump。
fn share_animation_version() -> u64 {
    ANIMATION_VERSION.load(Ordering::SeqCst)
}

fn animate_window_position(
    window: &WebviewWindow,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    duration_ms: u64,
) -> Result<(), String> {
    let version = begin_animation();
    let window_clone = window.clone();
    let app = window.app_handle().clone();
    
    std::thread::spawn(move || {
        let frame_duration = Duration::from_millis(16);
        let total_frames = duration_ms / 16;
        
        if total_frames == 0 {
            let window_for_task = window_clone.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = window_for_task.set_position(tauri::PhysicalPosition::new(end_x, end_y));
            });
            return;
        }
        
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        
        for frame in 0..=total_frames {
            if ANIMATION_VERSION.load(Ordering::SeqCst) != version {
                return;
            }
            
            let progress = frame as f32 / total_frames as f32;
            let eased_progress = 1.0 - (1.0 - progress).powi(2);
            
            let current_x = start_x + (dx as f32 * eased_progress) as i32;
            let current_y = start_y + (dy as f32 * eased_progress) as i32;
            
            let window_for_task = window_clone.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = window_for_task.set_position(tauri::PhysicalPosition::new(current_x, current_y));
            });
            
            if frame < total_frames {
                std::thread::sleep(frame_duration);
            }
        }
        
        if ANIMATION_VERSION.load(Ordering::SeqCst) == version {
            let window_for_task = window_clone.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = window_for_task.set_position(tauri::PhysicalPosition::new(end_x, end_y));
            });
        }
    });
    
    Ok(())
}

pub fn restore_from_snap(window: &WebviewWindow) -> Result<(), String> {
    let state = super::state::get_window_state();
    
    if let Some(pos) = state.snap_position {
        window.set_position(tauri::PhysicalPosition::new(pos.0, pos.1))
            .map_err(|e| e.to_string())?;
    }
    
    clear_snap();
    Ok(())
}

pub fn is_window_snapped() -> bool {
    is_snapped()
}

// 启动时恢复贴边隐藏状态
pub fn restore_edge_snap_on_startup(window: &WebviewWindow) -> Result<(), String> {
    let settings = crate::get_settings();

    if !settings.edge_hide_enabled {
        return Ok(());
    }

    let snapped_edge = settings
        .edge_snap_edge
        .as_deref()
        .and_then(edge_from_setting_value);
    let snapped_ratio = settings.edge_snap_ratio.map(clamp_ratio);
    let (snapped_edge, snapped_ratio) = match (snapped_edge, snapped_ratio) {
        (Some(edge), Some(ratio)) => (edge, ratio),
        _ => return Ok(()),
    };

    let (restore_width, restore_height) = resolve_startup_restore_window_size(
        window,
        snapped_edge,
        settings.edge_snap_monitor_id.as_deref(),
        &settings,
    )?;
    let resolved = resolve_hidden_position(
        window.app_handle(),
        snapped_edge,
        settings.edge_snap_monitor_id.as_deref(),
        snapped_ratio,
        restore_width,
        restore_height,
        settings.edge_hide_offset,
    )?;

    set_snap_edge(
        resolved.edge,
        Some((resolved.x, resolved.y)),
        Some(resolved.monitor_id.clone()),
        Some(snapped_ratio),
    );
    set_hidden_and_window_state(true, super::state::WindowState::Hidden);
    save_snap_layout(resolved.edge, snapped_ratio, Some(resolved.monitor_id));

    // 形态决策(穿透/置顶/显隐)交给 refresh,与其他隐藏路径保持一致
    refresh_hidden_snapped_window(window)?;

    crate::services::memory::schedule_cleanup_after_main_window_hide();
    crate::input_monitor::disable_mouse_monitoring();
    crate::input_monitor::disable_navigation_keys();

    super::edge_monitor::start_edge_monitoring();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Finding D2: build_monitor_contexts 500ms TTL 新鲜度纯函数。
    // edge_monitor 50ms 轮询 + snap_ratio 双 None 时每 tick 都走
    // available_monitors + get_all_monitors_with_edges;TTL 把 20Hz 压到 ~2Hz。
    #[test]
    fn monitor_context_cache_freshness_respects_ttl() {
        let cached_at = Instant::now();
        let ttl = Duration::from_millis(500);
        assert!(
            monitor_context_cache_is_fresh(cached_at, cached_at + Duration::from_millis(100), ttl),
            "TTL 内必须命中缓存"
        );
        assert!(
            !monitor_context_cache_is_fresh(cached_at, cached_at + Duration::from_millis(500), ttl),
            "恰好 TTL 边界必须 miss(严格 <)"
        );
        assert!(
            !monitor_context_cache_is_fresh(cached_at, cached_at + Duration::from_millis(600), ttl),
            "过期后必须 miss"
        );
    }

    // 源码护栏:build_monitor_contexts 必须走 TTL 缓存包装,真正的系统调用
    // 只在 build_monitor_contexts_uncached 内。
    #[test]
    fn build_monitor_contexts_uses_ttl_cache() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 snap.rs 源文件");
        // 剥行注释
        let body: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.contains("MONITOR_CONTEXT_CACHE"),
            "必须有 MONITOR_CONTEXT_CACHE 静态缓存"
        );
        assert!(
            body.contains("fn build_monitor_contexts_uncached"),
            "系统调用必须隔离到 build_monitor_contexts_uncached"
        );
        assert!(
            body.contains("MONITOR_CONTEXT_CACHE_TTL"),
            "必须定义 TTL 常量"
        );
        // 包装函数内不得直接 available_monitors(只 uncached 可调)
        let start = body
            .find("fn build_monitor_contexts(")
            .expect("找不到 build_monitor_contexts 定义");
        let after = &body[start..];
        // 到下一个 fn build_monitor_contexts_uncached 为止
        let end_rel = after
            .find("fn build_monitor_contexts_uncached")
            .unwrap_or(after.len());
        let wrapper = &after[..end_rel];
        assert!(
            !wrapper.contains("available_monitors()"),
            "TTL 包装层禁止直接 available_monitors,必须走 uncached"
        );
        assert!(
            wrapper.contains("build_monitor_contexts_uncached"),
            "包装层 miss 时必须调 uncached"
        );
    }

    // 回归测试:隐藏动画与 post-animation 刷新必须共享同一版本号。
    // 原实现两者各自 fetch_add,动画线程第一帧因版本不匹配即自杀,滑出动画一帧不播。
    #[test]
    fn post_animation_refresh_shares_version_with_in_flight_animation() {
        let animation_version = begin_animation();
        let post_refresh_version = share_animation_version();
        assert_eq!(
            animation_version,
            post_refresh_version,
            "post-refresh 必须共享在飞动画的版本,不得再次 bump"
        );
        assert_eq!(
            animation_version,
            ANIMATION_VERSION.load(Ordering::SeqCst),
            "动画版本必须等于全局版本,动画第一帧才不会被取消"
        );
    }

    // cancel(同步形态决策)必须 bump 版本,让在飞动画与 post-refresh 都失效
    #[test]
    fn cancel_pending_animation_invalidates_in_flight_animation() {
        let animation_version = begin_animation();
        cancel_pending_animation();
        assert_ne!(
            animation_version,
            ANIMATION_VERSION.load(Ordering::SeqCst),
            "cancel 后动画必须失效"
        );
    }

    // #1 show 取消在飞 hide 动画:show_snapped_window 必须 cancel_pending_animation
    // 使 hide 动画/延迟 refresh 失效,避免过期任务用过期设置改写形态
    #[test]
    fn show_cancels_in_flight_hide_animation_session() {
        // 行为:版本号协调
        let hide_animation_version = begin_animation();
        cancel_pending_animation();
        assert_ne!(
            hide_animation_version,
            share_animation_version(),
            "show 必须使在飞 hide 动画失效"
        );
        let show_animation_version = begin_animation();
        assert_eq!(
            show_animation_version,
            share_animation_version(),
            "show 动画取到的新版本必须与全局一致"
        );

        // 源码护栏:show_snapped_window 体内必须显式 cancel,
        // 且必须在原子写回 is_hidden=false 之前完成
        // 运行时读源(include_str! 自指会编译期递归,不可用)
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 src/windows/main_window/snap.rs 源文件");
        let show_start = source
            .find("pub fn show_snapped_window")
            .expect("找不到 show_snapped_window 定义");
        let after = &source[show_start..];
        let show_end_rel = after
            .find("\npub fn ")
            .or_else(|| after.find("\nfn "))
            .unwrap_or(after.len());
        let show_body = &after[..show_end_rel];
        let cancel_pos = show_body
            .find("cancel_pending_animation()")
            .expect("show_snapped_window 必须显式调用 cancel_pending_animation 取消在飞 hide 动画");
        let set_hidden_pos = show_body
            .find("set_hidden_and_window_state(false")
            .expect("show_snapped_window 缺少原子写回 false");
        assert!(
            cancel_pos < set_hidden_pos,
            "cancel 必须在 set_hidden_and_window_state(false, ...) 之前,\
             否则 post-refresh 仍能用旧设置覆盖 show 形态(当前位置:cancel={}, set_hidden={})",
            cancel_pos,
            set_hidden_pos
        );
    }

    // #2 show 动画 begin 自身 bump 兜底:即使 show 未显式 cancel,
    // begin_animation 也会 bump 版本,使 hide 动画线程下一帧自杀
    #[test]
    fn show_animation_bump_invalidates_in_flight_hide() {
        let hide_version = begin_animation();
        let show_version = begin_animation();
        assert_ne!(hide_version, show_version, "show 动画必须使 hide 动画失效");
        assert_eq!(
            show_version,
            share_animation_version(),
            "show 动画自己必须存活"
        );
    }

    // #3 hide 动画条件仅由 clipboard_animation_enabled 决定,
    // hover 关闭不得跳过滑出动画(4352887c 修复)。
    // 用纯函数直接断言,防止条件被人重新塞入 `edge_hover_popup_enabled` 之类。
    #[test]
    fn hide_animation_condition_depends_only_on_clipboard_animation() {
        use crate::services::settings::AppSettings;
        let mut s = AppSettings::default();
        s.clipboard_animation_enabled = true;
        s.edge_hover_popup_enabled = true;
        assert!(
            should_play_hide_animation(&s),
            "clipboard 开 + hover 开 ⇒ 应滑出动画"
        );
        s.edge_hover_popup_enabled = false;
        assert!(
            should_play_hide_animation(&s),
            "hover 关闭不得影响 hide 动画(4352887c 行为)"
        );
        s.clipboard_animation_enabled = false;
        assert!(
            !should_play_hide_animation(&s),
            "clipboard 动画关闭 ⇒ 不滑出"
        );
        // 反向断言:其它无关字段(start_hidden 等)不影响决定
        s.clipboard_animation_enabled = true;
        s.edge_hover_popup_enabled = true;
        s.start_hidden = true;
        assert!(
            should_play_hide_animation(&s),
            "start_hidden 等无关字段不得影响 hide 动画"
        );
    }

    // #4 show 路径顺序:set_position 必须先于 set_ignore_cursor_events(false)
    // 与 refresh 唯一决策点对齐,避免中间窗口在屏外坐标短暂可点击
    // 运行时读源(include_str! 自指会编译期递归,不可用)
    #[test]
    fn show_path_set_position_precedes_ignore_cursor_false() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 src/windows/main_window/snap.rs 源文件");
        let show_start = source
            .find("pub fn show_snapped_window")
            .expect("找不到 show_snapped_window 定义");
        let after = &source[show_start..];
        let show_end_rel = after
            .find("\npub fn ")
            .or_else(|| after.find("\nfn "))
            .unwrap_or(after.len());
        let show_body = &after[..show_end_rel];
        let pos_ignore_off = show_body
            .find("set_ignore_cursor_events(false)")
            .expect("show 路径未发现 set_ignore_cursor_events(false) 调用");
        let pos_set_position = show_body
            .find("set_position")
            .expect("show 路径未发现 set_position 调用");
        assert!(
            pos_set_position < pos_ignore_off,
            "show 路径必须先 set_position 再 set_ignore_cursor_events(false)。\
             当前 set_position 位于第 {} 字符,set_ignore_cursor_events(false) 位于第 {} 字符",
            pos_set_position, pos_ignore_off
        );
    }

    // #5 show 动画分支必须显式 set_ignore_cursor_events(true),且必须位于
    // show() 之后(因 show 抢焦点会清掉 WS_EX_TRANSPARENT),与 refresh 顺序对齐。
    // 复现条件:hover-on → 关闭态 → 快捷键/托盘唤出,hide 路径把 ignore 置 false 后
    // show 动画不重新打开穿透,半滑入时屏外坐标仍可被 raw_input 命中。
    // 运行时读源(include_str! 自指会编译期递归,不可用)
    #[test]
    fn show_animation_branch_sets_ignore_true_before_show() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 src/windows/main_window/snap.rs 源文件");
        let show_start = source
            .find("pub fn show_snapped_window")
            .expect("找不到 show_snapped_window 定义");
        let after = &source[show_start..];
        let show_end_rel = after
            .find("\npub fn ")
            .or_else(|| after.find("\nfn "))
            .unwrap_or(after.len());
        let show_body = &after[..show_end_rel];

        // 截取动画分支体
        let anim_branch_start = show_body
            .find("clipboard_animation_enabled")
            .expect("show 路径缺少动画分支判断");
        let anim_branch = &show_body[anim_branch_start..];
        // 动画分支以 } else { 或函数末尾结束
        let anim_branch_end = anim_branch
            .find("} else {")
            .unwrap_or_else(|| anim_branch.find("\n    }").unwrap_or(anim_branch.len()));
        let anim_branch_body = &anim_branch[..anim_branch_end];

        let pos_ignore_true = anim_branch_body
            .find("let _ = window.set_ignore_cursor_events(true);")
            .unwrap_or_else(|| {
                panic!(
                    "show 动画分支必须显式 set_ignore_cursor_events(true),\
                     否则 hover-on→关闭态→唤出 时半滑入坐标被点击截获。\
                     当前动画分支体:\n{}",
                    anim_branch_body
                )
            });
        let pos_show = anim_branch_body
            .find("window.show()")
            .expect("show 动画分支缺少 window.show() 调用");
        assert!(
            pos_show < pos_ignore_true,
            "show 动画分支必须先 show() 再 set_ignore_cursor_events(true),\
             与 refresh 唯一决策点顺序对齐(避免 show 抢焦点清掉 WS_EX_TRANSPARENT)。\
             当前 show={}, ignore=true={}",
            pos_show,
            pos_ignore_true
        );
    }
}

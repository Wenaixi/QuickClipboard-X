use tauri::{AppHandle, Manager};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use super::state::{
    is_low_memory_mode,
    last_window_activity_at_ms,
    mark_window_activity,
    set_low_memory_mode,
    set_user_requested_exit,
    try_mark_auto_manager_started,
    try_start_exit_low_memory,
    finish_exit_low_memory,
    is_exiting_low_memory,
};

// 需要销毁的 WebView 窗口列表
const WEBVIEW_LABELS: &[&str] = &[
    "main",
    "quickpaste",
    "context-menu",
    "settings",
    "text-editor",
    "updater",
    "receive-box",
    "community",
    "drop-proxy",
    "preview-window",
];

const AUTO_LOW_MEMORY_WINDOW_LABELS: &[&str] = &[
    "settings",
    "text-editor",
    "updater",
    "preview-window",
    "context-menu",
    "receive-box",
    "community",
    "drop-proxy",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum AutoLowMemoryLogState {
    Disabled,
    Blocked(Vec<String>),
    Counting {
        idle_minutes: u64,
        started_at_ms: u64,
    },
}

static AUTO_LOW_MEMORY_LOG_STATE: Lazy<Mutex<Option<AutoLowMemoryLogState>>> =
    Lazy::new(|| Mutex::new(None));

// 进入低占用模式
pub fn enter_low_memory_mode(app: &AppHandle) -> Result<(), String> {
    if is_low_memory_mode() {
        return Ok(());
    }

    // 互斥缺口:退出流程(init 置位 EXITING_LOW_MEMORY → 重建 webview →
    // finish 复位)进行中,若此处直接进入,会摧毁/改写刚重建的主窗口形态。
    // 之前只有 try_start_exit_low_memory 防「并发退出」,没有防「退出中进入」。
    if super::state::is_exiting_low_memory() {
        return Err("正在退出低占用模式，请稍后再试".to_string());
    }

    let _ = super::hide_panel();
    set_user_requested_exit(false);
    set_low_memory_mode(true);

    if let Err(e) = crate::windows::tray::switch_to_native_menu(app) {
        set_low_memory_mode(false);
        return Err(e);
    }

    // 停止边缘监控
    crate::windows::main_window::stop_edge_monitoring();

    // 禁用鼠标监控
    crate::input_monitor::disable_mouse_monitoring();

    // 禁用导航键监听
    crate::input_monitor::disable_navigation_keys();

    destroy_all_webviews(app);

    // 清理内存
    crate::services::memory::cleanup_memory_respecting_settings();
    
    let _ = crate::services::notification::show_notification(
        app,
        "低占用模式",
        "已进入低占用模式，所有窗口已关闭。\n使用托盘菜单或使用快捷键可恢复。",
    );
    
    println!("[低占用模式] 已进入");
    Ok(())
}

// 退出低占用模式
pub fn exit_low_memory_mode(app: &AppHandle) -> Result<(), String> {
    if !is_low_memory_mode() {
        return Ok(());
    }

    if !try_start_exit_low_memory() {
        return Err("正在退出低占用模式，请稍后再试".to_string());
    }

    let _ = super::hide_panel();
    set_user_requested_exit(false);
    mark_window_activity();

    let _ = crate::services::notification::show_notification(
        app,
        "低占用模式",
        "已退出低占用模式，主窗口已恢复。",
    );

    let result = crate::windows::tray::switch_to_webview_menu(app)
        .and_then(|_| recreate_main_window(app))
        .and_then(|_| {
            let _ = crate::quickpaste::init_quickpaste_window(app);
            Ok(())
        });

    // 重建 quickpaste 后再整体刷新自身窗口排除列表——recreate_main_window
    // 内部那次刷新执行时 quickpaste 尚不存在,其新 hwnd 不在 EXCLUDED_HWNDS;
    // 不刷新的话聚焦快捷粘贴窗口会被记为外部窗口,恢复焦点设回隐藏的
    // quickpaste 窗口。
    #[cfg(windows)]
    crate::services::system::focus::refresh_excluded_hwnds(app);

    // 无论退出流程中间步骤成功与否,都必须复位低占用标记:
    // 只复位 EXITING_LOW_MEMORY 而把 LOW_MEMORY_MODE 留在 true,
    // lib.rs ExitRequested 会因 is_low_memory_mode() && !is_user_requested_exit()
    // 而 prevent_exit,用户被锁在"低占用标记但窗口未恢复"的撕裂态。
    // 重建失败时尽力兜底恢复窗口与监听,让用户至少回到可用状态。
    set_low_memory_mode(false);

    if let Err(e) = &result {
        crate::input_monitor::enable_navigation_keys();
        crate::input_monitor::enable_mouse_monitoring();
        eprintln!("退出低占用模式恢复主窗口失败: {}", e);
    }

    finish_exit_low_memory();

    if let Err(e) = result {
        let _ = ensure_main_window(app);
        return Err(e);
    }

    println!("[低占用模式] 已退出");
    Ok(())
}

pub fn init_auto_low_memory_manager(app: AppHandle) {
    if !try_mark_auto_manager_started() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));

        loop {
            interval.tick().await;

            if is_low_memory_mode() {
                continue;
            }

            let settings = crate::get_settings();
            if !settings.auto_low_memory_enabled {
                log_auto_low_memory_state(AutoLowMemoryLogState::Disabled);
                continue;
            }

            let visible_windows = collect_visible_windows_for_auto_low_memory(&app);
            if !visible_windows.is_empty() {
                log_auto_low_memory_state(AutoLowMemoryLogState::Blocked(visible_windows));
                mark_window_activity();
                continue;
            }

            let idle_minutes = settings.auto_low_memory_idle_minutes.max(1) as u64;
            let idle_threshold_ms = idle_minutes * 60 * 1000;
            let last_activity_at_ms = last_window_activity_at_ms();
            if last_activity_at_ms == 0 {
                continue;
            }

            log_auto_low_memory_state(AutoLowMemoryLogState::Counting {
                idle_minutes,
                started_at_ms: last_activity_at_ms,
            });

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(last_activity_at_ms);

            if now_ms.saturating_sub(last_activity_at_ms) < idle_threshold_ms {
                continue;
            }

            println!(
                "[低占用模式][自动检测] 所有相关窗口已隐藏，并已空闲 {} 分钟，准备自动进入低占用模式",
                idle_minutes
            );

            if let Err(error) = enter_low_memory_mode(&app) {
                eprintln!("自动进入低占用模式失败: {}", error);
                mark_window_activity();
                continue;
            }

            println!("[低占用模式] 因窗口空闲自动进入");
        }
    });
}

fn collect_visible_windows_for_auto_low_memory(app: &AppHandle) -> Vec<String> {
    let mut visible_windows = Vec::new();

    if is_main_window_visible_for_auto_low_memory() {
        visible_windows.push("main".to_string());
    }

    if crate::windows::quickpaste::is_visible() {
        visible_windows.push("quickpaste".to_string());
    }

    for label in AUTO_LOW_MEMORY_WINDOW_LABELS {
        let visible = app
            .get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);

        if visible {
            visible_windows.push((*label).to_string());
        }
    }

    // 文件盒窗口是 transfer-shelf-{id} 动态标签,静态表枚举不到——开着时
    // 若不被视为"相关窗口",空闲计时不会因它暂停,会带着文件盒自动进入低
    // 占用,与 destroy_all_webviews 的前缀销毁不对称。
    for (label, window) in app.webview_windows() {
        if label.starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)
            && window.is_visible().unwrap_or(false)
        {
            visible_windows.push(label);
        }
    }

    visible_windows
}

fn is_main_window_visible_for_auto_low_memory() -> bool {
    let state = crate::windows::main_window::get_window_state();

    if state.is_hidden {
        return false;
    }

    state.state == crate::windows::main_window::WindowState::Visible
}

fn log_auto_low_memory_state(next_state: AutoLowMemoryLogState) {
    let mut last_state = AUTO_LOW_MEMORY_LOG_STATE.lock();
    if last_state.as_ref() == Some(&next_state) {
        return;
    }

    match &next_state {
        AutoLowMemoryLogState::Disabled => {
            println!("[低占用模式][自动检测] 自动进入低占用模式未启用");
        }
        AutoLowMemoryLogState::Blocked(windows) => {
            println!(
                "[低占用模式][自动检测] 检测到窗口仍在显示，暂停空闲计时: {}",
                windows.join(", ")
            );
        }
        AutoLowMemoryLogState::Counting {
            idle_minutes,
            started_at_ms: _,
        } => {
            println!(
                "[低占用模式][自动检测] 所有相关窗口均已隐藏，开始空闲计时，{} 分钟后自动进入低占用模式",
                idle_minutes
            );
        }
    }

    *last_state = Some(next_state);
}

// 销毁所有 WebView 窗口
fn destroy_all_webviews(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        // pin-image-{uuid} 与 transfer-shelf-{id} 都是动态标签,静态数组
        // 枚举不到,必须按前缀遍历销毁——否则文件盒窗口开着时进入低占用,
        // 该窗口残留,退出后主窗口重建完成但文件盒仍在,与自动检测不对称。
        if label.starts_with("pin-image-")
            || label.starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)
        {
            let _ = window.destroy();
        }
    }

    // 文件盒窗口销毁后,清空内存 SHELVES 记录——静态 Vec 只增不减时,
    // 退出低占用后 list_shelves 会返回已销毁窗口的记录,前端按记录打开
    // 等于打开不存在的窗口,重开同名文件盒也会命中残留记录。
    crate::windows::transfer_shelf::clear_active_shelves();

    // 贴图窗口销毁后整体清空贴图数据——window.destroy() 走不到前端主动
    // 关窗的清理逻辑,失败路径下的 PIN_IMAGE_DATA_MAP 记录会残留、被改图
    // 等操作创建的独立临时文件也不会删除;整体清理还能覆盖"窗口已销毁但
    // 数据记录仍在"的漏网场景,保证退出低占用后数据与临时文件全部归零。
    crate::windows::pin_image_window::cleanup_all_pin_images();

    for label in WEBVIEW_LABELS {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.destroy();
        }
    }
}

// 确保主窗口可用：窗口对象还在但句柄失效时先销毁再重建。
// 供单实例启动、主窗口销毁后重建等路径复用，避免直接 get_webview_window
// 拿到一个句柄已失效的窗口对象。
pub fn ensure_main_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    // 复用或重建前取消旧自动弹出任务，避免异步回调作用于新窗口。
    crate::windows::main_window::invalidate_mouse_auto_popup();

    if let Some(window) = app.get_webview_window("main") {
        #[cfg(windows)]
        if window.hwnd().is_ok() {
            return Ok(window);
        }

        #[cfg(not(windows))]
        {
            return Ok(window);
        }

        let _ = window.destroy();
    }

    recreate_main_window(app)?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口重建后仍不可用".to_string())?;
    #[cfg(windows)]
    if window.hwnd().is_err() {
        return Err("主窗口重建后句柄仍不可用".to_string());
    }
    Ok(window)
}

fn recreate_main_window(app: &AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // 重建路径只处理失效对象，调用方负责在复用路径清理会话。
    crate::windows::main_window::invalidate_mouse_auto_popup();

    // 退出低占用模式必须对称恢复导航键/鼠标监听。即使主窗口已存在走
    // 早返分支,enter_low_memory_mode 已调过 disable_navigation_keys() /
    // disable_mouse_monitoring()(行 67/70),这里必须无条件 enable,否则
    // 退出后导航键/鼠标监控永久失效。原实现把 enable 放在早返检查之后,
    // 主窗口已存在时早返跳过恢复,与 enter_low_memory_mode 不对称。
    crate::input_monitor::enable_navigation_keys();
    crate::input_monitor::enable_mouse_monitoring();

    if let Some(window) = app.get_webview_window("main") {
        #[cfg(windows)]
        if window.hwnd().is_ok() {
            return Ok(());
        }

        let _ = window.destroy();
    }

    let settings = crate::get_settings();

    let (width, height) = if settings.remember_window_size {
        settings.saved_window_size.unwrap_or((360, 520))
    } else {
        (360, 520)
    };
    let window = WebviewWindowBuilder::new(
        app,
        "main",
        WebviewUrl::App("windows/main/index.html".into()),
    )
    .title("快速剪贴板")
    .inner_size(360.0, 520.0)
    .min_inner_size(350.0, 500.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false) 
    .resizable(true)
    .maximizable(false)
    .minimizable(false)
    .center()
    .focused(false)
    .visible_on_all_workspaces(true)
    .disable_drag_drop_handler() 
    .build()
    .map_err(|e| format!("重建主窗口失败: {}", e))?;

    if settings.remember_window_size {
        crate::windows::main_window::apply_saved_window_size(&window, width, height);
    }
    
    let _ = window.set_focusable(false);
    
    #[cfg(debug_assertions)]
    let _ = window.open_devtools();

    crate::input_monitor::update_main_window(window.clone());

    #[cfg(windows)]
    // 重建主窗口后按当前全部自身窗口重建排除列表,替换 add_excluded_hwnd
    // 的只增不减——旧 hwnd 已销毁,OS 复用其值后会把无关窗口当自身窗口。
    crate::services::system::focus::refresh_excluded_hwnds(app);

    crate::init_edge_monitor(window.clone());

    let _ = crate::windows::main_window::restore_edge_snap_on_startup(&window);

    crate::input_monitor::enable_mouse_monitoring();

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::services::system::hotkey::test_utils::strip_line_comments;

    fn manager_source() -> String {
        crate::services::system::hotkey::test_utils::source_file("src/services/low_memory/manager.rs")
    }

    // 退出低占用模式重建主窗口必须恢复导航键——进入时
    // enter_low_memory_mode 调了 disable_navigation_keys()，
    // 不恢复则退出后导航键永久失效。
    #[test]
    fn recreate_main_window_reenables_navigation_keys() {
        let src = strip_line_comments(&manager_source());
        let start = src
            .find("fn recreate_main_window(app: &AppHandle)")
            .expect("缺 recreate_main_window");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let b = &src[start..end];
        assert!(
            b.contains("enable_navigation_keys()"),
            "recreate_main_window 必须恢复导航键监听"
        );
        assert!(
            b.find("enable_navigation_keys()") < b.find("enable_mouse_monitoring()"),
            "导航键恢复必须先于鼠标监控恢复"
        );
        assert!(
            b.contains("invalidate_mouse_auto_popup()"),
            "重建失效窗口前必须清理旧自动弹出会话"
        );
    }

    // 早返分支 `if let Some(window) = app.get_webview_window("main")`
    // 必须保留对 enable_navigation_keys() / enable_mouse_monitoring() 的恢复调用。
    // 若两次 enable 写在早返检查之后,主窗口已存在时早返就跳过恢复——
    // 退出低占用模式后导航键/鼠标监控永久失效。与 enter_low_memory_mode 中
    // disable_navigation_keys() / disable_mouse_monitoring() (行 67/70) 对称必须。
    // 顺序类不变量,源码字面 find() 下标比较,无法用 contains 替代。
    #[test]
    fn recreate_main_window_early_return_still_restores_io() {
        let src = strip_line_comments(&manager_source());
        let start = src
            .find("fn recreate_main_window(app: &AppHandle)")
            .expect("缺 recreate_main_window");
        let after_start = &src[start..];
        // 早返检查必须先于下一次 enable 调用,故先定位早返下标
        let early_return_rel = after_start
            .find("if let Some(window) = app.get_webview_window(\"main\")")
            .expect("缺早返检查");
        let pos_enable_nav_rel = after_start
            .find("enable_navigation_keys()")
            .expect("缺 enable_navigation_keys()");
        let pos_enable_mouse_rel = after_start
            .find("enable_mouse_monitoring()")
            .expect("缺 enable_mouse_monitoring()");
        assert!(
            pos_enable_nav_rel < early_return_rel,
            "enable_navigation_keys() 必须在早返检查之前被调用。\
             当前 enable_navigation_keys 位于第 {} 字符,早返检查位于第 {} 字符。\
             退出低占用时主窗口已存在会导致早返跳过恢复,导航键永久失效。",
            pos_enable_nav_rel,
            early_return_rel,
        );
        assert!(
            pos_enable_mouse_rel < early_return_rel,
            "enable_mouse_monitoring() 必须在早返检查之前被调用。\
             当前 enable_mouse_monitoring 位于第 {} 字符,早返检查位于第 {} 字符。",
            pos_enable_mouse_rel,
            early_return_rel,
        );
        assert!(
            after_start[..early_return_rel].contains("invalidate_mouse_auto_popup()"),
            "重建函数必须在检查旧窗口前失效自动弹出会话"
        );
    }

    // §10.3 源码护栏：确保主窗口可用必须检查句柄，句柄失效的窗口要销毁重建，
    // 否则 Windows 上 WebView 句柄失效后窗口对象残留，无法重建。
    #[test]
    fn ensure_main_window_destroys_window_without_hwnd_and_rebuilds() {
        let src = strip_line_comments(&manager_source());
        let start = src
            .find("pub fn ensure_main_window")
            .expect("缺 ensure_main_window");
        let end = src[start..]
            .find("\nfn recreate_main_window")
            .map(|i| start + i)
            .unwrap_or(src.len());
        let body = &src[start..end];
        assert!(
            body.contains("get_webview_window(\"main\")"),
            "确保主窗口必须先尝试获取主窗口"
        );
        assert!(
            body.contains("hwnd().is_ok()"),
            "必须用句柄判断窗口是否可用"
        );
        assert!(
            body.contains("window.destroy()"),
            "句柄失效的窗口必须销毁才能重建"
        );
        assert!(
            body.contains("recreate_main_window(app)"),
            "必须重建主窗口"
        );
    }

    // 互斥护栏:退出低占用模式进行中(EXITING_LOW_MEMORY 置位)不得再次进入。
    // 此前只有 try_start_exit_low_memory 防「并发退出」,没有防「退出中进入」——
    // exit 流程重建主窗口期间被 enter 击穿,会摧毁/改写刚建好的窗口形态。
    #[test]
    fn enter_low_memory_blocks_while_exit_in_flight() {
        let src = strip_line_comments(&manager_source());
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "enter_low_memory_mode");
        assert!(
            body.contains("is_exiting_low_memory"),
            "进入低占用模式必须检查退出流程是否进行中"
        );
        let state_src = strip_line_comments(
            &crate::services::system::hotkey::test_utils::source_file("src/services/low_memory/state.rs"),
        );
        assert!(
            state_src.contains("pub fn is_exiting_low_memory"),
            "state.rs 必须提供退出进行中查询能力"
        );
    }

    // 退出低占用模式重建窗口后,排除列表刷新必须发生在全部窗口重建完成
    // (含 quickpaste 初始化)之后——recreate_main_window 内部已有一次刷新,
    // 但那次执行时 quickpaste 窗口还不存在;若退出流程随后创建 quickpaste
    // 而不再次刷新,新 hwnd 不在 EXCLUDED_HWNDS,聚焦快捷粘贴窗口会被记为
    // 外部窗口,恢复焦点把焦点设回隐藏的 quickpaste 窗口。
    #[test]
    fn exit_low_memory_refreshes_excluded_hwnds_after_quickpaste_rebuilt() {
        let src = strip_line_comments(&manager_source());
        let body =
            crate::services::system::hotkey::test_utils::fn_body(&src, "exit_low_memory_mode");
        let quickpaste_pos = body
            .find("init_quickpaste_window(app)")
            .expect("退出低占用必须重建 quickpaste 窗口");
        let refresh_pos = body
            .find("refresh_excluded_hwnds(app)")
            .expect("重建 quickpaste 后必须再次刷新排除列表,新 hwnd 才能入列表");
        assert!(
            quickpaste_pos < refresh_pos,
            "排除列表刷新必须晚于 quickpaste 重建——recreate_main_window 内部刷新时 quickpaste 尚不存在"
        );
    }

    // 退出低占用模式的失败路径必须复位 LOW_MEMORY_MODE——只复位
    // EXITING_LOW_MEMORY 而把 LOW_MEMORY_MODE 留在 true 时,lib.rs
    // ExitRequested 会 prevent_exit,用户被锁在"低占用标记但窗口未恢复"
    // 撕裂态;重建失败分支还必须兜底恢复导航键/鼠标监听。
    #[test]
    fn exit_low_memory_failure_path_resets_low_memory_mode() {
        let src = strip_line_comments(&manager_source());
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "exit_low_memory_mode");
        let set_pos = body
            .find("set_low_memory_mode(false)")
            .expect("退出流程必须复位低占用标记");
        let finish_pos = body
            .find("finish_exit_low_memory()")
            .expect("缺退出流程完成标记");
        assert!(
            set_pos < finish_pos,
            "复位 LOW_MEMORY_MODE 必须早于 finish 复位 EXITING——否则复位 EXITING 后\
             并发 enter 会击穿,而 LOW_MEMORY_MODE 仍 true 造成撕裂态"
        );
        // 失败兜底:重建失败分支内必须恢复监听
        let err_pos = body
            .find("return Err(e)")
            .expect("退出失败必须返回错误");
        let fail_seg = &body[..err_pos];
        assert!(
            fail_seg.contains("enable_navigation_keys()")
                && fail_seg.contains("enable_mouse_monitoring()"),
            "重建失败分支必须兜底恢复导航键与鼠标监听"
        );
        assert!(
            fail_seg.contains("ensure_main_window(app)"),
            "重建失败后必须再次尝试确保主窗口可用"
        );
    }

    // 退出低占用模式重建主窗口后,必须按当前全部自身窗口整体重建
    // 排除列表(refresh_excluded_hwnds),不得退回 add_excluded_hwnd 的只增
    // 不减——旧 hwnd 销毁后 OS 会复用其句柄值,若旧值仍留在 EXCLUDED_HWNDS,
    // 无关窗口的聚焦事件会被误过滤,导航键/悬浮行为错乱。
    #[test]
    fn recreate_main_window_refreshes_excluded_hwnds() {
        let src = strip_line_comments(&manager_source());
        let body =
            crate::services::system::hotkey::test_utils::fn_body(&src, "recreate_main_window");
        assert!(
            body.contains("refresh_excluded_hwnds(app)"),
            "重建主窗口后必须整体重建自身窗口排除列表"
        );
        assert!(
            !body.contains("add_excluded_hwnd("),
            "重建路径不得用 add_excluded_hwnd 只增不减——旧 hwnd 销毁后列表残留,OS 复用其值会误过滤无关窗口"
        );
    }

    // 源码护栏:进入低占用销毁全部分窗口时,必须覆盖收发盒/社区/拖放代理/
    // 预览窗口与全部 transfer-shelf-{id} 文件盒——静态数组枚举不到动态
    // 标签,漏掉它们会让窗口开着时仍自动进入低占用、进入后又销毁不了,
    // 与自动检测的可见性扫描不对称。标签断言限生产代码域,避免测试自身的
    // 字符串字面量误命中。
    #[test]
    fn destroy_all_webviews_covers_constant_and_dynamic_labels() {
        let src = strip_line_comments(&manager_source());
        let prod = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "destroy_all_webviews");
        for label in [
            "receive-box",
            "community",
            "drop-proxy",
            "preview-window",
        ] {
            assert!(
                prod.contains(label),
                "销毁相关表必须覆盖 {} 窗口,否则开着时进入低占用该窗口残留",
                label
            );
        }
        assert!(
            body.contains("starts_with(\"pin-image-\")")
                && body.contains("starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)"),
            "动态标签窗口(贴图/文件盒)必须按前缀遍历销毁"
        );
    }

    // 源码护栏:销毁文件盒窗口后必须清空内存 SHELVES 记录——静态 Vec 只增
    // 不减时,退出低占用后 list_shelves 返回"已销毁文件的盒"记录,前端按
    // 记录打开等于打开不存在的窗口;再加同名文件盒也会命中残留记录。
    #[test]
    fn destroy_all_webviews_clears_shelf_records_after_destroy() {
        let src = strip_line_comments(&manager_source());
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "destroy_all_webviews");
        let destroy_pos = body
            .find("window.destroy()")
            .expect("destroy_all_webviews 必须销毁文件盒窗口");
        let clear_pos = body
            .find("clear_active_shelves()")
            .expect("销毁文件盒窗口后必须清空内存 SHELVES 记录");
        assert!(
            destroy_pos < clear_pos,
            "必须先销毁窗口再清空记录——避免残留下已销毁窗口的文件盒记录"
        );
    }

    // 源码护栏:贴图窗口销毁后必须整体清空贴图数据——window.destroy()
    // 走不到前端主动关窗的清理逻辑,数据记录与独立临时文件会泄漏;整体
    // 清理放在销毁循环之后,还覆盖"窗口已销毁但数据仍在"的漏网场景。
    #[test]
    fn destroy_all_webviews_cleans_pin_image_data_after_destroy() {
        let src = strip_line_comments(&manager_source());
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "destroy_all_webviews");
        let destroy_pos = body
            .find("window.destroy()")
            .expect("destroy_all_webviews 必须销毁贴图窗口");
        let cleanup_pos = body
            .find("cleanup_all_pin_images()")
            .expect("销毁贴图窗口后必须整体清空贴图数据与临时文件");
        assert!(
            destroy_pos < cleanup_pos,
            "贴图数据整体清理必须发生在窗口销毁之后——直接 destroy 绕过清理造成残留"
        );
    }

    // 源码护栏:自动低占用检测的可见窗口扫描必须与销毁列表对称——收发盒/
    // 社区/拖放代理开着时仍会开始空闲计时;文件盒其实开着但静态表扫不到,
    // 空闲计时仍照走,带着文件盒自动进入低占用。护栏断言动态 transfer-shelf
    // 前缀窗口也要纳入可见性扫描。标签断言限生产代码域。
    #[test]
    fn collect_visible_windows_checks_constant_and_dynamic_labels() {
        let src = strip_line_comments(&manager_source());
        let prod = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        let body = crate::services::system::hotkey::test_utils::fn_body(&src, "collect_visible_windows_for_auto_low_memory");
        for label in [
            "receive-box",
            "community",
            "drop-proxy",
            "preview-window",
        ] {
            assert!(
                prod.contains(label),
                "自动检测相关表必须覆盖 {} 窗口,否则开着时仍开始空闲计时",
                label
            );
        }
        assert!(
            body.contains("starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)"),
            "自动检测必须把 transfer-shelf 前缀的文件盒窗口纳入可见性扫描"
        );
    }
}

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use super::global::{get_app, parse_shortcut};

static NAVIGATION_SHORTCUTS: Lazy<Mutex<Vec<NavigationShortcutRegistration>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static NAVIGATION_HOTKEYS_DESIRED: AtomicBool = AtomicBool::new(false);
static NAVIGATION_HOTKEYS_REGISTERED: AtomicBool = AtomicBool::new(false);
static NAVIGATION_REPEAT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

static NAVIGATION_THROTTLE_STATE: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static NAVIGATION_REPEAT_TOKENS: Lazy<Mutex<HashMap<String, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// 导航热键注册/注销串行锁：前台切换回调、窗口显隐、全局热键重注册可能从不同线程
// 同时触发导航热键的增删，RegisterHotKey 对同一裸键并发注册会返回
// AlreadyRegistered，失败后内部表与 Windows 层不一致，残留的裸 Enter/Tab/Esc
// 注册就是吞掉全局按键的"幽灵热键"。串行化后从根源消除并发撞车。
static NAVIGATION_SYNC_LOCK: Mutex<()> = Mutex::new(());

const NAVIGATION_REPEAT_INITIAL_DELAY: Duration = Duration::from_millis(300);
const NAVIGATION_FAST_REPEAT_INTERVAL: Duration = Duration::from_millis(45);

#[derive(Clone)]
struct NavigationShortcutRegistration {
    id: String,
    shortcut: String,
}

#[derive(Clone)]
struct NavigationShortcutConfig {
    id: &'static str,
    action: &'static str,
    shortcut: String,
}

pub fn enable_navigation_hotkeys() {
    // F4: 与 disable 对称——先持 NAVIGATION_SYNC_LOCK 再 store+sync，
    // 消除锁外 store 的 lost wakeup：并发 disable 在 store 与 sync 之间
    // 抢锁时，enable 的 sync 可能看到 desired=true 直接跳过注册。
    let _guard = NAVIGATION_SYNC_LOCK.lock();
    NAVIGATION_HOTKEYS_DESIRED.store(true, Ordering::SeqCst);
    sync_navigation_hotkeys_for_foreground_inner();
}

pub fn disable_navigation_hotkeys() {
    // F4: 与 enable 对称——先持 NAVIGATION_SYNC_LOCK 再 store+unregister，
    // 消除锁外 store 的 lost wakeup：并发 enable 在 store 与锁内 unregister
    // 之间抢锁时,enable 的 sync 可能看到 desired=true 跳过注册。
    let _guard = NAVIGATION_SYNC_LOCK.lock();
    NAVIGATION_HOTKEYS_DESIRED.store(false, Ordering::SeqCst);
    unregister_navigation_hotkeys();
}

pub fn sync_navigation_hotkeys_for_foreground() {
    // 串行化：注册/注销全程持锁，防止前台切换回调、窗口显隐、全局热键重注册
    // 并发对同一裸键 RegisterHotKey 撞车，失败后 Windows 层残留"幽灵热键"
    // 吞掉全局 Enter/Tab/Esc。
    let _guard = NAVIGATION_SYNC_LOCK.lock();
    sync_navigation_hotkeys_for_foreground_inner();
}

fn sync_navigation_hotkeys_for_foreground_inner() {
    if !NAVIGATION_HOTKEYS_DESIRED.load(Ordering::SeqCst) {
        unregister_navigation_hotkeys();
        return;
    }

    if should_suspend_navigation_hotkeys() {
        unregister_navigation_hotkeys();
        return;
    }

    if !NAVIGATION_HOTKEYS_REGISTERED.load(Ordering::SeqCst) {
        reload_navigation_hotkeys_from_settings_inner();
    }
}

pub fn reload_navigation_hotkeys_from_settings() {
    let _guard = NAVIGATION_SYNC_LOCK.lock();
    reload_navigation_hotkeys_from_settings_inner();
}

fn reload_navigation_hotkeys_from_settings_inner() {
    if !NAVIGATION_HOTKEYS_DESIRED.load(Ordering::SeqCst) {
        unregister_navigation_hotkeys();
        return;
    }

    if should_suspend_navigation_hotkeys() {
        unregister_navigation_hotkeys();
        return;
    }

    if let Err(error) = register_navigation_hotkeys_from_settings() {
        eprintln!("同步导航快捷键失败: {}", error);
    }
}

// 让内部注册状态整体失效，Windows 层注册由调用方负责摘除。
// 供 global 层"禁用热键/前台屏蔽"时插件级 unregister_all 后调用，
// 使下次前台切换能干净地重建导航热键。
pub fn invalidate_navigation_hotkeys() {
    let _guard = NAVIGATION_SYNC_LOCK.lock();
    NAVIGATION_HOTKEYS_REGISTERED.store(false, Ordering::SeqCst);
    NAVIGATION_SHORTCUTS.lock().clear();
    NAVIGATION_REPEAT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    NAVIGATION_REPEAT_TOKENS.lock().clear();
    NAVIGATION_THROTTLE_STATE.lock().clear();
}

fn register_navigation_hotkeys_from_settings() -> Result<(), String> {
    unregister_navigation_hotkeys();

    let app = get_app()?;
    let configs = navigation_shortcut_configs();
    let mut registrations = Vec::new();
    let mut has_failure = false;

    for config in configs {
        if config.shortcut.trim().is_empty() {
            continue;
        }

        let shortcut = match parse_shortcut(&config.shortcut) {
            Ok(shortcut) => shortcut,
            Err(error) => {
                eprintln!("解析导航快捷键 [{}] 失败: {}", config.id, error);
                continue;
            }
        };

        let id = config.id.to_string();
        let action = config.action.to_string();
        let shortcut_for_log = config.shortcut.clone();

        match app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
            match event.state {
                ShortcutState::Pressed => handle_navigation_pressed(&id, &action),
                ShortcutState::Released => handle_navigation_released(&id),
            }
        }) {
            Ok(_) => {
                println!("已注册导航快捷键 [{}]: {}", config.id, config.shortcut);
                // F2: 成功注册必须写 success 状态,覆盖同 id 的旧失败状态——
                // 用户改回合法键并保存后立即清除设置页红错。
                super::global::set_shortcut_status(&config.id, &config.shortcut, true, None);
                registrations.push(NavigationShortcutRegistration {
                    id: config.id.to_string(),
                    shortcut: shortcut_for_log,
                });
            }
            Err(error) => {
                eprintln!(
                    "注册导航快捷键 [{}] {} 失败: {}",
                    config.id, config.shortcut, error
                );
                // 插件可能在 Err 前部分写入 Windows 层，主动探测并注销清理，
                // 避免残留吞键的"幽灵热键"。与 global.rs register_shortcut 同款。
                super::global::safe_unregister(&app, shortcut);
                // F7: 注册失败必须写 SHORTCUT_STATUS 状态表——前端 navigation tab
                // 的 ShortcutInput 靠 backendId 查 getBackendError 展示冲突/失败，
                // 不写则用户配置错误时前端静默无提示。key 用导航 config id，
                // 错误码与 global.rs register_shortcut 同款区分 CONFLICT/REGISTRATION_FAILED。
                let error_msg = if error.to_string().contains("already registered") {
                    "CONFLICT".to_string()
                } else {
                    "REGISTRATION_FAILED".to_string()
                };
                super::global::set_shortcut_status(&config.id, &config.shortcut, false, Some(error_msg));
                // F7(单键失败治理):仅记录该键失败状态,不整体置 REGISTERED=false——
                // 避免一个键配错让全部导航键失效并清空其余成功注册;
                // 失败键由下次 reload 重试,其余成功键保持可用。
                has_failure = true;
            }
        }
    }

    let has_registration = !registrations.is_empty();
    *NAVIGATION_SHORTCUTS.lock() = registrations;

    // 有成功注册即视为已就绪；单键失败不影响其余成功键,避免雪崩。
    NAVIGATION_HOTKEYS_REGISTERED.store(has_registration, Ordering::SeqCst);
    Ok(())
}

// 安全摘除导航热键：使用插件 is_registered 探测后再注销，避免对未注册的
// 裸键 UnregisterHotKey 空跑后内部表与 Windows 层脱节，残留"幽灵热键"。
fn unregister_navigation_hotkeys() {
    let app = match get_app() {
        Ok(app) => app,
        Err(_) => return,
    };

    let registrations = std::mem::take(&mut *NAVIGATION_SHORTCUTS.lock());
    for registration in registrations {
        if let Ok(shortcut) = parse_shortcut(&registration.shortcut) {
            if app.global_shortcut().is_registered(shortcut) {
                super::global::safe_unregister(&app, shortcut);
                println!(
                    "已注销导航快捷键 [{}]: {}",
                    registration.id, registration.shortcut
                );
            }
        }
        // F2: 注销即清除该 id 的 SHORTCUT_STATUS——导航 id 不在
        // REGISTERED_SHORTCUTS,global 的 unregister_all 清理覆盖不到;
        // 不清除则用户改回合法键后设置页持续显示红错直到全局 reload/重启。
        super::global::clear_shortcut_status(&registration.id);
    }

    NAVIGATION_HOTKEYS_REGISTERED.store(false, Ordering::SeqCst);
    NAVIGATION_REPEAT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    NAVIGATION_REPEAT_TOKENS.lock().clear();
    NAVIGATION_THROTTLE_STATE.lock().clear();
}

fn navigation_shortcut_configs() -> Vec<NavigationShortcutConfig> {
    let settings = crate::get_settings();
    vec![
        NavigationShortcutConfig {
            id: "navigation_navigate_up",
            action: "navigate-up",
            shortcut: settings.navigate_up_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_navigate_down",
            action: "navigate-down",
            shortcut: settings.navigate_down_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_execute_item",
            action: "execute-item",
            shortcut: settings.paste_item_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_tab_left",
            action: "tab-left",
            shortcut: settings.tab_left_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_tab_right",
            action: "tab-right",
            shortcut: settings.tab_right_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_previous_group",
            action: "previous-group",
            shortcut: settings.previous_group_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_next_group",
            action: "next-group",
            shortcut: settings.next_group_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_focus_search",
            action: "focus-search",
            shortcut: settings.focus_search_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_hide_window",
            action: "hide-window",
            shortcut: settings.hide_window_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_toggle_pin",
            action: "toggle-pin",
            shortcut: settings.toggle_pin_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_filter_left",
            action: "filter-left",
            shortcut: settings.filter_left_shortcut,
        },
        NavigationShortcutConfig {
            id: "navigation_filter_right",
            action: "filter-right",
            shortcut: settings.filter_right_shortcut,
        },
    ]
}

fn handle_navigation_pressed(id: &str, action: &str) {
    if !NAVIGATION_HOTKEYS_DESIRED.load(Ordering::SeqCst)
        || !NAVIGATION_HOTKEYS_REGISTERED.load(Ordering::SeqCst)
        || should_suspend_navigation_hotkeys()
    {
        return;
    }

    if emit_navigation_action_if_ready(action) {
        start_navigation_repeat_if_needed(id, action);
    }
}

fn handle_navigation_released(id: &str) {
    NAVIGATION_REPEAT_TOKENS.lock().remove(id);
    NAVIGATION_REPEAT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
}

fn start_navigation_repeat_if_needed(id: &str, action: &str) {
    let interval = match get_repeat_interval(action) {
        Some(interval) => interval,
        None => return,
    };

    let token = NAVIGATION_REPEAT_SEQUENCE
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    NAVIGATION_REPEAT_TOKENS.lock().insert(id.to_string(), token);

    let id = id.to_string();
    let action = action.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(NAVIGATION_REPEAT_INITIAL_DELAY);

        while should_continue_repeat(&id, token) {
            emit_navigation_action(&action);
            std::thread::sleep(interval);
        }
    });
}

fn should_continue_repeat(id: &str, token: u64) -> bool {
    if !NAVIGATION_HOTKEYS_DESIRED.load(Ordering::SeqCst)
        || !NAVIGATION_HOTKEYS_REGISTERED.load(Ordering::SeqCst)
        || should_suspend_navigation_hotkeys()
    {
        return false;
    }

    NAVIGATION_REPEAT_TOKENS
        .lock()
        .get(id)
        .copied()
        == Some(token)
}

fn should_suspend_navigation_hotkeys() -> bool {
    !super::global::is_hotkeys_enabled()
        || !crate::get_settings().hotkeys_enabled
        || crate::services::system::is_front_app_globally_disabled_from_settings()
}

fn emit_navigation_action_if_ready(action: &str) -> bool {
    if should_throttle(action) {
        return false;
    }

    emit_navigation_action(action);
    true
}

fn emit_navigation_action(action: &str) {
    let action = action.to_string();
    if let Ok(app) = get_app() {
        let app_for_task = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = app_for_task.get_webview_window("main") {
                let _ = window.emit(
                    "navigation-action",
                    serde_json::json!({
                        "action": action
                    }),
                );
            }
        });
    }
}

fn should_throttle(action: &str) -> bool {
    let delay = match get_throttle_delay(action) {
        Some(delay) => delay,
        None => return false,
    };

    let mut throttle_state = NAVIGATION_THROTTLE_STATE.lock();
    let now = Instant::now();

    if let Some(last_time) = throttle_state.get(action) {
        if now.duration_since(*last_time) < delay {
            return true;
        }
    }

    throttle_state.insert(action.to_string(), now);
    false
}

fn get_throttle_delay(action: &str) -> Option<Duration> {
    match action {
        "navigate-up" | "navigate-down" => None,
        "tab-left" | "tab-right" | "filter-left" | "filter-right" => Some(Duration::from_millis(150)),
        "previous-group" | "next-group" => Some(Duration::from_millis(100)),
        "execute-item" | "focus-search" | "hide-window" | "toggle-pin" => {
            Some(Duration::from_millis(200))
        }
        _ => Some(Duration::from_millis(100)),
    }
}

fn get_repeat_interval(action: &str) -> Option<Duration> {
    match action {
        "navigate-up" | "navigate-down" => Some(NAVIGATION_FAST_REPEAT_INTERVAL),
        "tab-left" | "tab-right" | "filter-left" | "filter-right" => Some(Duration::from_millis(150)),
        "previous-group" | "next-group" => Some(Duration::from_millis(100)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{fn_body, strip_line_comments};

    fn navigation_source() -> String {
        super::super::test_utils::source_file("src/services/system/hotkey/navigation.rs")
    }

    // F3: 注册失败路径必须调 safe_unregister 清理幽灵热键（内部先
    // is_registered 探测再 unregister,与 global.rs register_shortcut 同款），
    // 并保留 eprintln 错误日志。
    #[test]
    fn navigation_failure_path_probes_before_unregister() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "register_navigation_hotkeys_from_settings");
        assert!(
            b.find("safe_unregister(&app, shortcut)").is_some(),
            "注册失败路径必须调 safe_unregister 清理幽灵热键"
        );
        assert!(
            b.contains("eprintln!"),
            "注册失败路径必须保留 eprintln 错误日志"
        );
    }

    // F7: 注册失败必须写 global 的 SHORTCUT_STATUS 状态表——
    // 前端 navigation tab 的 ShortcutInput 靠 backendId 查状态展示错误，
    // 不写则用户配置错误时前端静默无提示。错误码区分 CONFLICT/REGISTRATION_FAILED。
    #[test]
    fn navigation_failure_writes_shortcut_status() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "register_navigation_hotkeys_from_settings");
        let status_pos = b.find("set_shortcut_status");
        let conflict_pos = b.find("CONFLICT");
        let failed_pos = b.find("REGISTRATION_FAILED");
        assert!(
            status_pos.is_some(),
            "注册失败路径必须写 global::set_shortcut_status 状态表"
        );
        assert!(
            conflict_pos.is_some() && failed_pos.is_some(),
            "错误码必须区分 CONFLICT 与 REGISTRATION_FAILED"
        );
    }

    // F4: enable_navigation_hotkeys 必须先持 NAVIGATION_SYNC_LOCK 再 store+sync，
    // 与 disable_navigation_hotkeys 对称，消除锁外 store 的 lost wakeup。
    #[test]
    fn enable_navigation_hotkeys_holds_lock_before_store() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "enable_navigation_hotkeys");
        let lock_pos = b.find("NAVIGATION_SYNC_LOCK.lock()");
        let store_pos = b.find("NAVIGATION_HOTKEYS_DESIRED.store(true");
        assert!(
            lock_pos.is_some() && store_pos.is_some() && lock_pos < store_pos,
            "enable_navigation_hotkeys 必须持锁后 store"
        );
        assert!(
            b.find("sync_navigation_hotkeys_for_foreground()").is_none(),
            "enable_navigation_hotkeys 持锁内必须调 inner,禁止顶层 sync 再 lock"
        );
    }

    // F4(对称性护栏):disable_navigation_hotkeys 必须与 enable 对称——
    // 先持 NAVIGATION_SYNC_LOCK 再 store(false)，锁外 store 与并发
    // enable/sync 存在 lost wakeup 竞态（store 与锁内 unregister 之间
    // 被并发 enable 抢锁时,enable 的 sync 可能看到 desired=true 跳过注册）。
    #[test]
    fn disable_navigation_hotkeys_holds_lock_before_store() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "disable_navigation_hotkeys");
        let lock_pos = b.find("NAVIGATION_SYNC_LOCK.lock()");
        let store_pos = b.find("NAVIGATION_HOTKEYS_DESIRED.store(false");
        assert!(
            lock_pos.is_some() && store_pos.is_some() && lock_pos < store_pos,
            "disable_navigation_hotkeys 必须持锁后 store(false)"
        );
        assert!(
            b.find("unregister_navigation_hotkeys()").is_some(),
            "disable_navigation_hotkeys 持锁内必须调 unregister_navigation_hotkeys"
        );
    }

    // F7(连带雪崩治理):单键注册失败不得整体置 NAVIGATION_HOTKEYS_REGISTERED=false,
    // 否则一个键配错会让全部导航键失效——下次前台切换/显隐触发 reload 时
    // unregister 会摘除全部已成功注册的键,用户配错的键连同其余正常键一起
    // 全部失灵。失败键自身已写 SHORTCUT_STATUS 错误状态,由下次 reload 重试。
    #[test]
    fn navigation_single_key_failure_does_not_poison_global_state() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "register_navigation_hotkeys_from_settings");
        let store_pos = b.find("NAVIGATION_HOTKEYS_REGISTERED.store(");
        assert!(
            store_pos.is_some(),
            "register_navigation_hotkeys_from_settings 必须 store REGISTERED 状态"
        );
        let segment = &b[store_pos.unwrap()..b[store_pos.unwrap()..].find(';').unwrap() + store_pos.unwrap()];
        assert!(
            !segment.contains("has_failure"),
            "store 表达式不得依赖 has_failure,单键失败不能整体置 REGISTERED=false"
        );
    }

    // F2: 导航注册失败路径写入 global::SHORTCUT_STATUS 状态表后,成功/重试路径
    // 必须清除该 id 的失败状态——否则用户改回合法键并保存后,设置页持续显示
    // 红错直到全局 reload/重启。清除逻辑须同时覆盖 unregister 路径。
    #[test]
    fn navigation_success_path_clears_stale_failure_status() {
        let src = strip_line_comments(&navigation_source());
        let b = fn_body(&src, "register_navigation_hotkeys_from_settings");
        let ok_pos = b.find("Ok(_)");
        assert!(
            ok_pos.is_some() && b[ok_pos.unwrap()..].find("set_shortcut_status").is_some(),
            "成功注册分支必须写 set_shortcut_status(success) 清除旧失败状态"
        );
        let ub = fn_body(&src, "unregister_navigation_hotkeys");
        let status_pos = ub.find("set_shortcut_status");
        let clear_pos = ub.find("clear_shortcut_status");
        assert!(
            status_pos.is_some() || clear_pos.is_some(),
            "unregister_navigation_hotkeys 必须清除对应 id 的 SHORTCUT_STATUS 状态"
        );
    }
}

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
    NAVIGATION_HOTKEYS_DESIRED.store(true, Ordering::SeqCst);
    sync_navigation_hotkeys_for_foreground();
}

pub fn disable_navigation_hotkeys() {
    NAVIGATION_HOTKEYS_DESIRED.store(false, Ordering::SeqCst);
    let _guard = NAVIGATION_SYNC_LOCK.lock();
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
                has_failure = true;
                // 注册失败（多为 RegisterHotKey 并发撞车 AlreadyRegistered）：
                // 保持 NAVIGATION_HOTKEYS_REGISTERED=false，让下次前台切换/显隐时
                // 能重新走一遍完整注销+注册，清除 Windows 层可能残留的"幽灵热键"。
            }
        }
    }

    let has_registration = !registrations.is_empty();
    *NAVIGATION_SHORTCUTS.lock() = registrations;

    // 有失败则整体保持"未注册"状态，等待自愈重试；避免半注册状态被当作已就绪。
    NAVIGATION_HOTKEYS_REGISTERED.store(has_registration && !has_failure, Ordering::SeqCst);
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
                let _ = app.global_shortcut().unregister(shortcut);
                println!(
                    "已注销导航快捷键 [{}]: {}",
                    registration.id, registration.shortcut
                );
            }
        }
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

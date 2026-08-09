use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub(super) static APP_HANDLE: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));
static REGISTERED_SHORTCUTS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static HOTKEYS_ENABLED: AtomicBool = AtomicBool::new(true);
static FOREGROUND_GLOBALLY_DISABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyActivation {
    Active,
    Inactive,
}

#[derive(Debug)]
struct HotkeySyncState {
    current: HotkeyActivation,
    desired: HotkeyActivation,
    syncing: bool,
}

static HOTKEY_SYNC_STATE: Lazy<Mutex<HotkeySyncState>> = Lazy::new(|| {
    Mutex::new(HotkeySyncState {
        current: HotkeyActivation::Active,
        desired: HotkeyActivation::Active,
        syncing: false,
    })
});

static ACTIVE_PASTE_KEYS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

// 检查快捷键是否首次按下
fn try_activate_key(key_id: &str) -> bool {
    let mut active = ACTIVE_PASTE_KEYS.lock();
    if active.contains(key_id) {
        false
    } else {
        active.insert(key_id.to_string());
        true
    }
}

// 检查快捷键是否处于活跃状态（重复按下）
fn is_key_active(key_id: &str) -> bool {
    ACTIVE_PASTE_KEYS.lock().contains(key_id)
}

// 释放快捷键
fn deactivate_key(key_id: &str) {
    ACTIVE_PASTE_KEYS.lock().remove(key_id);
}

// 快捷键注册状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutStatus {
    pub id: String,
    pub shortcut: String,
    pub success: bool,
    pub error: Option<String>,
}

static SHORTCUT_STATUS: Lazy<Mutex<HashMap<String, ShortcutStatus>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// 全局热键注册/注销/重载串行锁：前台切换(sync_hotkeys_for_foreground)、
// 设置变更(reload_from_settings)、单键注册(register_shortcut)可能从不同线程
// 同时触发，RegisterHotKey 对同一组合键并发注册会返回 AlreadyRegistered，
// 失败后内部表与 Windows 层不一致，残留吞键的"幽灵热键"——
// 62e6b718 只给 navigation 加了锁，global.rs 的整体热键注册漏了，这是
// "所有快捷键失效"的根因之一。串行化后从根源消除并发撞车。
pub(super) static GLOBAL_HOTKEY_SYNC_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
// 锁序契约：GLOBAL 必须先于 NAVIGATION 获取。global 层入口
// (reload_from_settings/disable_hotkeys/unregister_all/enable_hotkeys)
// 内部会调 navigation::sync_navigation_hotkeys_for_foreground（后者持
// NAVIGATION_SYNC_LOCK），若某处先持 NAVIGATION 锁再调 global 层入口
// 就构成 AB-BA 死锁定时炸弹。任何持 NAVIGATION 锁调 global 层入口的
// 代码都是死锁，新增调用点前先对照本契约。

pub fn init_hotkey_manager(app: AppHandle, _window: WebviewWindow) {
    *APP_HANDLE.lock() = Some(app);
}

fn is_foreground_globally_disabled() -> bool {
    FOREGROUND_GLOBALLY_DISABLED.load(Ordering::Relaxed)
}

fn apply_activation(desired: HotkeyActivation) {
    match desired {
        HotkeyActivation::Active => {
            let _ = reload_from_settings();
        }
        HotkeyActivation::Inactive => {
            disable_all_shortcuts();
        }
    }
}

// 插件级整体注销 + 清空内部状态：利用 GlobalHotKeyManager::unregister_all
// 先注销 Windows 层全部注册再清空内部表，消除"半注册/幽灵热键"残留；
// 同时让导航热键内部状态整体失效，等下次前台切换/显隐时干净重建。
fn disable_all_shortcuts() {
    // F2: 必须持 GLOBAL_HOTKEY_SYNC_LOCK。apply_activation(Inactive) 从
    // sync_hotkeys_for_foreground 的 spawn 线程调用本函数，不持锁会与
    // 持锁的 register_* 并发留下半注册幽灵热键。
    let _guard = GLOBAL_HOTKEY_SYNC_LOCK.lock();
    if let Ok(app) = get_app() {
        use tauri_plugin_global_shortcut::GlobalShortcutExt as _;
        let _ = app.global_shortcut().unregister_all();
    }
    REGISTERED_SHORTCUTS.lock().clear();
    SHORTCUT_STATUS.lock().clear();
    super::navigation::invalidate_navigation_hotkeys();
}

pub fn sync_hotkeys_for_foreground() {
    let settings = crate::get_settings();
    let globally_disabled = crate::services::system::is_front_app_globally_disabled_from_settings();
    FOREGROUND_GLOBALLY_DISABLED.store(globally_disabled, Ordering::Relaxed);

    let desired = if !settings.hotkeys_enabled
        || !HOTKEYS_ENABLED.load(Ordering::Relaxed)
        || globally_disabled
    {
        HotkeyActivation::Inactive
    } else {
        HotkeyActivation::Active
    };

    {
        let mut state = HOTKEY_SYNC_STATE.lock();
        state.desired = desired;

        if state.syncing {
            return;
        }

        if state.current == state.desired {
            return;
        }

        state.syncing = true;
    }

    std::thread::spawn(|| loop {
        let desired_now = {
            let state = HOTKEY_SYNC_STATE.lock();
            state.desired
        };

        apply_activation(desired_now);

        let mut state = HOTKEY_SYNC_STATE.lock();
        state.current = desired_now;

        if state.current == state.desired {
            state.syncing = false;
            break;
        }
    });
}

pub(super) fn get_app() -> Result<AppHandle, String> {
    APP_HANDLE
        .lock()
        .clone()
        .ok_or_else(|| "热键管理器未初始化".to_string())
}

pub(super) fn parse_shortcut(shortcut_str: &str) -> Result<Shortcut, String> {
    let normalized = shortcut_str
        .replace("Win+", "Super+")
        .replace("Ctrl+", "Control+");
    
    normalized.parse::<Shortcut>()
        .map_err(|e| format!("解析快捷键失败: {}", e))
}

fn ensure_normal_mode_for_hotkey(app: &AppHandle, action_name: &str) -> Result<bool, String> {
    if !crate::services::low_memory::is_low_memory_mode() {
        return Ok(true);
    }

    if !crate::get_settings().auto_exit_low_memory_mode {
        return Ok(false);
    }

    crate::services::low_memory::exit_low_memory_mode(app)
        .map_err(|e| format!("{}前自动退出低占用模式失败: {}", action_name, e))?;
    Ok(true)
}

pub fn register_shortcut<F>(id: &str, shortcut_str: &str, handler: F) -> Result<(), String>
where
    F: Fn(&AppHandle) + Send + Sync + 'static,
{
    // F1: 假设调用方已持 GLOBAL_HOTKEY_SYNC_LOCK；同线程再 lock 会自死锁。
    let app = get_app()?;

    unregister_shortcut(id);

    let shortcut = match parse_shortcut(shortcut_str) {
        Ok(s) => s,
        Err(_e) => {
            update_shortcut_status(id, shortcut_str, false, Some("REGISTRATION_FAILED".to_string()));
            return Err("REGISTRATION_FAILED".to_string());
        }
    };

    match app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                handler(app);
            }
        }) {
        Ok(_) => {
            REGISTERED_SHORTCUTS.lock().push((id.to_string(), shortcut_str.to_string()));
            update_shortcut_status(id, shortcut_str, true, None);
            println!("已注册快捷键 [{}]: {}", id, shortcut_str);
            Ok(())
        }
        Err(e) => {
            // 注册失败：插件可能在 Err 前部分写入 Windows 层，主动探测并
            // 注销清理，避免残留吞键的"幽灵热键"。探测按组合键而非 id：
            // 若用户把两个快捷键配成相同组合键（配置错误），可能误摘
            // 同组合键的其他 id 条目——属配置错误可接受，UI 侧已有
            // duplicate 检测提示，不按 id 探测是刻意取舍（插件 API 只
            // 提供按 Shortcut 的 is_registered/unregister）。
            safe_unregister(&app, shortcut);
            let error_msg = if e.to_string().contains("already registered") {
                "CONFLICT".to_string()
            } else {
                "REGISTRATION_FAILED".to_string()
            };
            update_shortcut_status(id, shortcut_str, false, Some(error_msg.clone()));
            Err(format!("注册快捷键失败: {}", e))
        }
    }
}

// 探测后再注销：避免对未注册的裸键 UnregisterHotKey 空跑后，内部表与
// Windows 层脱节，残留吞键的"幽灵热键"。所有失败清理/注销路径共用。
// Shortcut 是 Copy 类型,值传递避免 &Shortcut 无法满足插件 From 约束。
pub(super) fn safe_unregister(app: &AppHandle, shortcut: Shortcut) {
    if app.global_shortcut().is_registered(shortcut) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

pub fn unregister_shortcut(id: &str) {
    let app = match get_app() {
        Ok(app) => app,
        Err(_) => return,
    };

    let mut shortcuts = REGISTERED_SHORTCUTS.lock();
    if let Some(pos) = shortcuts.iter().position(|(registered_id, _)| registered_id == id) {
        let (_, shortcut_str) = shortcuts.remove(pos);
        if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
            // 先探测再注销，避免对未注册的裸键 UnregisterHotKey 空跑后，
            // 内部表与 Windows 层脱节，残留吞键的"幽灵热键"。
            safe_unregister(&app, shortcut);
            println!("已注销快捷键 [{}]: {}", id, shortcut_str);
        }
    }

    clear_shortcut_status(id);
}

pub fn register_toggle_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("toggle", shortcut_str, |app| {
        if is_foreground_globally_disabled() {
            return;
        }
        let app = app.clone();
        std::thread::spawn(move || {
            let _ = crate::toggle_main_window_visibility(&app);
        });
    })
}

pub fn register_open_settings_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("open_settings", shortcut_str, |app| {
        if is_foreground_globally_disabled() {
            return;
        }

        let app_clone = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::windows::settings_window::open_settings_window(&app_clone) {
                eprintln!("打开设置窗口失败: {}", e);
            }
        });
    })
}

pub fn register_quickpaste_hotkey(shortcut_str: &str) -> Result<(), String> {
    // F1: 假设调用方已持 GLOBAL_HOTKEY_SYNC_LOCK；不在此 lock。
    let app = get_app()?;

    unregister_shortcut("quickpaste");

    let shortcut = parse_shortcut(shortcut_str)?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if crate::services::low_memory::is_low_memory_mode() {
                    return;
                }

                if is_foreground_globally_disabled() {
                    return;
                }

                let settings = crate::get_settings();
                let is_keyboard_mode = settings.quickpaste_paste_on_modifier_release;
                let is_visible = crate::windows::quickpaste::is_visible();
                
                if is_keyboard_mode && is_visible {
                    if let Some(window) = app.get_webview_window("quickpaste") {
                        let _ = window.emit("quickpaste-next", ());
                    }
                    crate::services::system::raw_input::start_quickpaste_secondary_key_hold();
                    return;
                }
                
                if let Err(e) = crate::windows::quickpaste::show_quickpaste_window(&app) {
                    eprintln!("显示便捷粘贴窗口失败: {}", e);
                } else if is_keyboard_mode {
                    crate::services::system::raw_input::start_quickpaste_secondary_key_hold();
                }
            } else if event.state == ShortcutState::Released {
                if crate::services::low_memory::is_low_memory_mode() {
                    return;
                }

                if is_foreground_globally_disabled() {
                    return;
                }

                let settings = crate::get_settings();
                if settings.quickpaste_paste_on_modifier_release {
                    return;
                }

                if let Some(window) = app.get_webview_window("quickpaste") {
                    let _ = window.emit("quickpaste-hide", ());
                }

                let app_clone = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if let Err(e) = crate::windows::quickpaste::hide_quickpaste_window(&app_clone) {
                        eprintln!("隐藏便捷粘贴窗口失败: {}", e);
                    }
                });
            }
        })
        .map_err(|e| {
            // 注册失败：插件可能在 Err 前部分写入 Windows 层，主动探测并
            // 注销清理，避免残留吞键的"幽灵热键"。
            safe_unregister(&app, shortcut);
            format!("注册便捷粘贴快捷键失败: {}", e)
        })?;
    
    REGISTERED_SHORTCUTS.lock().push(("quickpaste".to_string(), shortcut_str.to_string()));
    
    println!("已注册便捷粘贴快捷键: {}", shortcut_str);
    Ok(())
}

pub fn register_transfer_shelf_create_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("transfer_shelf_create", shortcut_str, |app| {
        if is_foreground_globally_disabled() {
            return;
        }

        let app = app.clone();
        std::thread::spawn(move || {
            if !matches!(ensure_normal_mode_for_hotkey(&app, "创建文件盒"), Ok(true)) {
                return;
            }

            if let Err(error) = crate::windows::transfer_shelf::open_or_create_shelf(&app) {
                eprintln!("快捷键创建文件盒失败: {}", error);
            }
        });
    })
}

fn run_webdav_hotkey_action(action_name: &'static str, mode: &'static str) {
    tauri::async_runtime::spawn(async move {
        let result = match mode {
            "push" => crate::services::webdav_sync::upload().await.map(|_| ()),
            "pull" => crate::services::webdav_sync::download(false).await.map(|_| ()),
            _ => Ok(()),
        };

        if let Err(error) = result {
            eprintln!("{} 失败: {}", action_name, error);
        }
    });
}

pub fn register_webdav_push_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("webdav_push", shortcut_str, |_app| {
        if is_foreground_globally_disabled() {
            return;
        }

        run_webdav_hotkey_action("快捷键推送到 WebDAV", "push");
    })
}

pub fn register_webdav_pull_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("webdav_pull", shortcut_str, |_app| {
        if is_foreground_globally_disabled() {
            return;
        }

        run_webdav_hotkey_action("快捷键从 WebDAV 拉取", "pull");
    })
}

#[cfg(feature = "screenshot-suite")]
pub fn register_screenshot_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("screenshot", shortcut_str, |app| {
        let app = app.clone();
        std::thread::spawn(move || {
            if !matches!(ensure_normal_mode_for_hotkey(&app, "启动截图"), Ok(true)) {
                return;
            }

            if is_foreground_globally_disabled() {
                return;
            }
            if let Err(e) = screenshot_suite::start_screenshot(&app) {
                eprintln!("启动截图窗口失败: {}", e);
            }
        });
    })
}

#[cfg(not(feature = "screenshot-suite"))]
pub fn register_screenshot_hotkey(_shortcut_str: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "screenshot-suite")]
pub fn register_screenshot_quick_save_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("screenshot_quick_save", shortcut_str, |app| {
        let app = app.clone();
        std::thread::spawn(move || {
            if !matches!(ensure_normal_mode_for_hotkey(&app, "启动快速保存截图"), Ok(true)) {
                return;
            }
            if is_foreground_globally_disabled() {
                return;
            }
            if let Err(e) = screenshot_suite::start_screenshot_quick_save(&app) {
                eprintln!("启动快速保存截图失败: {}", e);
            }
        });
    })
}

#[cfg(not(feature = "screenshot-suite"))]
pub fn register_screenshot_quick_save_hotkey(_shortcut_str: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "screenshot-suite")]
pub fn register_screenshot_quick_pin_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("screenshot_quick_pin", shortcut_str, |app| {
        let app = app.clone();
        std::thread::spawn(move || {
            if !matches!(ensure_normal_mode_for_hotkey(&app, "启动快速贴图截图"), Ok(true)) {
                return;
            }
            if is_foreground_globally_disabled() {
                return;
            }
            if let Err(e) = screenshot_suite::start_screenshot_quick_pin(&app) {
                eprintln!("启动快速贴图截图失败: {}", e);
            }
        });
    })
}

#[cfg(not(feature = "screenshot-suite"))]
pub fn register_screenshot_quick_pin_hotkey(_shortcut_str: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "screenshot-suite")]
pub fn register_screenshot_quick_ocr_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("screenshot_quick_ocr", shortcut_str, |app| {
        let app = app.clone();
        std::thread::spawn(move || {
            if !matches!(ensure_normal_mode_for_hotkey(&app, "启动快速OCR截图"), Ok(true)) {
                return;
            }
            if is_foreground_globally_disabled() {
                return;
            }
            if let Err(e) = screenshot_suite::start_screenshot_quick_ocr(&app) {
                eprintln!("启动快速OCR截图失败: {}", e);
            }
        });
    })
}

#[cfg(not(feature = "screenshot-suite"))]
pub fn register_screenshot_quick_ocr_hotkey(_shortcut_str: &str) -> Result<(), String> {
    Ok(())
}

pub fn register_toggle_clipboard_monitor_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("toggle_clipboard_monitor", shortcut_str, |app| {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::commands::settings::toggle_clipboard_monitor(&app_clone) {
                eprintln!("切换剪贴板监听状态失败: {}", e);
            }
        });
    })
}

pub fn register_toggle_paste_with_format_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("toggle_paste_with_format", shortcut_str, |app| {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::commands::settings::toggle_paste_with_format(&app_clone) {
                eprintln!("切换格式粘贴状态失败: {}", e);
            }
        });
    })
}

pub fn register_toggle_low_memory_mode_hotkey(shortcut_str: &str) -> Result<(), String> {
    register_shortcut("toggle_low_memory_mode", shortcut_str, |app| {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let result = if crate::services::low_memory::is_low_memory_mode() {
                crate::services::low_memory::exit_low_memory_mode(&app_clone)
            } else {
                crate::services::low_memory::enter_low_memory_mode(&app_clone)
            };

            if let Err(e) = result {
                eprintln!("切换低占用模式失败: {}", e);
            }
        });
    })
}

pub fn register_paste_plain_text_hotkey(shortcut_str: &str) -> Result<(), String> {
    // F1: 假设调用方已持 GLOBAL_HOTKEY_SYNC_LOCK；不在此 lock。
    let app = get_app()?;

    unregister_shortcut("paste_plain_text");

    let shortcut = parse_shortcut(shortcut_str)?;
    let key_id = "paste_plain_text".to_string();
    let shortcut_owned = shortcut_str.to_string();

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            match event.state {
                ShortcutState::Pressed => {
                    if try_activate_key(&key_id) {
                        // 首次按下
                        let app = app.clone();
                        let key_id = key_id.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = handle_paste_plain_text_press(&app) {
                                eprintln!("纯文本粘贴失败: {}", e);
                                deactivate_key(&key_id);
                            }
                        });
                    } else if is_key_active(&key_id) {
                        // 重复按下
                        let shortcut = shortcut_owned.clone();
                        std::thread::spawn(move || {
                            use crate::services::paste::keyboard::set_trigger_key_from_shortcut;
                            set_trigger_key_from_shortcut(&shortcut);
                            let _ = simulate_paste_only();
                        });
                    }
                }
                ShortcutState::Released => {
                    deactivate_key(&key_id);
                }
            }
        })
        .map_err(|e| {
            // 注册失败：插件可能在 Err 前部分写入 Windows 层，主动探测并
            // 注销清理，避免残留吞键的"幽灵热键"。
            safe_unregister(&app, shortcut);
            format!("注册纯文本粘贴快捷键失败: {}", e)
        })?;

    REGISTERED_SHORTCUTS
        .lock()
        .push(("paste_plain_text".to_string(), shortcut_str.to_string()));
    update_shortcut_status("paste_plain_text", shortcut_str, true, None);
    println!("已注册纯文本粘贴快捷键: {}", shortcut_str);
    Ok(())
}

// 首次按下
fn handle_paste_plain_text_press(app: &AppHandle) -> Result<(), String> {
    use crate::services::database::{query_clipboard_items, get_clipboard_item_by_id, QueryParams};
    use crate::services::paste::paste_handler::paste_clipboard_item_with_format;
    use crate::services::paste::PasteAction;
    use crate::services::paste::keyboard::set_trigger_key_from_shortcut;

    set_trigger_key_from_shortcut(&crate::get_settings().paste_plain_text_shortcut);

    let state = crate::get_window_state();
    let is_window_visible = state.state == crate::WindowState::Visible && !state.is_hidden;

    if is_window_visible {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.emit("paste-plain-text-selected", ());
        }
    } else {
        let items = query_clipboard_items(QueryParams {
            offset: 0,
            limit: 1,
            search: None,
            content_type: None,
        })?
        .items;

        if let Some(item) = items.first() {
            let full_item = get_clipboard_item_by_id(item.id)?
                .ok_or_else(|| format!("剪贴板项 {} 不存在", item.id))?;
            paste_clipboard_item_with_format(&full_item, Some(PasteAction::PlainText))?;
        }
    }

    Ok(())
}

pub fn register_number_shortcuts(modifier: &str) -> Result<(), String> {
    // F1: 假设调用方已持 GLOBAL_HOTKEY_SYNC_LOCK；不在此 lock。
    let app = get_app()?;

    unregister_number_shortcuts();

    {
        let mut status_map = SHORTCUT_STATUS.lock();
        status_map.remove("number_shortcuts");
    }

    let is_f_key = modifier.ends_with("F");
    let prefix = if is_f_key {
        modifier.strip_suffix("F").unwrap_or("").trim_end_matches('+')
    } else {
        modifier
    };

    let mut failed_shortcuts: Vec<String> = Vec::new();

    for num in 1..=9 {
        let id = format!("number_{}", num);
        let shortcut_str = if is_f_key {
            if prefix.is_empty() {
                format!("F{}", num)
            } else {
                format!("{}+F{}", prefix, num)
            }
        } else {
            format!("{}+{}", modifier, num)
        };

        if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
            let key_id = format!("number_{}", num);
            let index = (num - 1) as usize;

            match app
                .global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    match event.state {
                        ShortcutState::Pressed => {
                            if try_activate_key(&key_id) {
                                // 首次按下
                                let key_id = key_id.clone();
                                if let Err(e) = handle_number_shortcut_press(index) {
                                    eprintln!("执行数字快捷键 {} 失败: {}", index + 1, e);
                                    deactivate_key(&key_id);
                                }
                            } else if is_key_active(&key_id) {
                                // 重复按下
                                let vk = if is_f_key {
                                    0x70 + index as u16
                                } else {
                                    0x31 + index as u16
                                };
                                crate::services::paste::keyboard::set_trigger_key_raw(vk);
                                let _ = simulate_paste_only();
                            }
                        }
                        ShortcutState::Released => {
                            deactivate_key(&key_id);
                        }
                    }
                })
            {
                Ok(_) => {
                    REGISTERED_SHORTCUTS.lock().push((id, shortcut_str.clone()));
                    println!("已注册数字快捷键: {}", shortcut_str);
                }
                Err(e) => {
                    eprintln!(
                        "注册数字快捷键 {} 失败: {}，继续注册其他快捷键",
                        shortcut_str, e
                    );
                    // F6: 注册失败：插件可能在 Err 前部分写入 Windows 层，主动探测并
                    // 注销清理，避免残留吞键的"幽灵热键"。但清理前必须检查命中的
                    // 组合键是否属于其他条目——reload 顺序是用户自定义条目先注册
                    // 成功、数字快捷键后注册失败,失败清理探测到 Ctrl+1 已注册
                    // （用户条目的）即注销它→用户热键静默失效但状态表仍显示成功。
                    // 属于其他条目则跳过清理,仅记录失败状态。
                    if !belongs_to_other_shortcut(&shortcut) {
                        safe_unregister(&app, shortcut);
                    }
                    failed_shortcuts.push(shortcut_str);
                }
            }
        }
    }
    
    if !failed_shortcuts.is_empty() {
        let mut status_map = SHORTCUT_STATUS.lock();
        status_map.insert("number_shortcuts".to_string(), ShortcutStatus {
            id: "number_shortcuts".to_string(),
            shortcut: failed_shortcuts.join(", "),
            success: false,
            error: Some("REGISTRATION_FAILED".to_string()),
        });
    }
    
    Ok(())
}

// 判断组合键是否已属于"本次 reload 的其他条目"——数字快捷键注册失败时,
// 命中的组合键可能是刚注册成功的用户自定义热键(RELOAD 顺序用户条目在前),
// 若直接探测注销会误杀用户热键。是则跳过清理,仅记录失败状态。
fn belongs_to_other_shortcut(shortcut: &Shortcut) -> bool {
    let registered = REGISTERED_SHORTCUTS.lock();
    registered.iter().any(|(_, s)| {
        parse_shortcut(s).map(|registered_shortcut| registered_shortcut == *shortcut).unwrap_or(false)
    })
}
// ponytail: unregister_number_shortcuts 保留独立实现,不循环调
// unregister_shortcut——它 get_app 失败时提前 return 会漏掉 shortcuts.retain
// 的状态清理;且本函数先持 REGISTERED_SHORTCUTS 锁,循环调用会再拿同一把锁
// 自死锁。unregister_shortcut 按 position 移除+clear_shortcut_status 的
// 语义与数字键批量注销不同,保持现状。

pub fn unregister_number_shortcuts() {
    let mut shortcuts = REGISTERED_SHORTCUTS.lock();
    let number_shortcuts: Vec<_> = shortcuts
        .iter()
        .filter(|(id, _)| id.starts_with("number_"))
        .cloned()
        .collect();

    for (id, shortcut_str) in number_shortcuts {
        if let Ok(shortcut) = parse_shortcut(&shortcut_str) {
            if let Ok(app) = get_app() {
                // F5: 与 unregister_shortcut 同款——先 is_registered 探测再
                // 注销，避免对未注册的裸键 UnregisterHotKey 空跑后内部表
                // 与 Windows 层脱节，残留吞键的"幽灵热键"。
                if app.global_shortcut().is_registered(shortcut) {
                    safe_unregister(&app, shortcut);
                    println!("已注销数字快捷键: {}", shortcut_str);
                }
            }
        }
        shortcuts.retain(|(sid, _)| sid != &id);
    }
}

// 首次按下
fn handle_number_shortcut_press(index: usize) -> Result<(), String> {
    use crate::services::database::{query_clipboard_items, get_clipboard_item_by_id, QueryParams};
    use crate::services::paste::paste_handler::paste_clipboard_item_with_update;
    use crate::services::paste::keyboard;

    // 设置触发键虚拟键码，确保 simulate_paste 能释放正确的按键
    let settings = crate::get_settings();
    let is_f_key = settings.number_shortcuts_modifier.ends_with('F');
    let vk = if is_f_key {
        0x70 + index as u16 // F1-F9
    } else {
        0x31 + index as u16 // '1'-'9'
    };
    keyboard::set_trigger_key_raw(vk);

    let items = query_clipboard_items(QueryParams {
        offset: 0,
        limit: 9,
        search: None,
        content_type: None,
    })?
    .items;

    let item = items.get(index).ok_or_else(|| {
        format!(
            "剪贴板项索引 {} 超出范围（共 {} 项）",
            index + 1,
            items.len()
        )
    })?;

    let full_item = get_clipboard_item_by_id(item.id)?
        .ok_or_else(|| format!("剪贴板项 {} 不存在", item.id))?;

    paste_clipboard_item_with_update(&full_item)
}

// 重复按下
fn simulate_paste_only() -> Result<(), String> {
    use crate::services::paste::keyboard::simulate_paste;

    std::thread::sleep(std::time::Duration::from_millis(50));
    simulate_paste()?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    Ok(())
}

pub fn unregister_all() {
    // F1: 假设调用方已持 GLOBAL_HOTKEY_SYNC_LOCK；reload_from_settings_inner
    // 持锁贯穿整个 reload，此处再 lock 同一把 parking_lot Mutex 会
    // 同线程不可重入自死锁。
    let shortcuts = REGISTERED_SHORTCUTS.lock().clone();
    for (id, _) in shortcuts {
        unregister_shortcut(&id);
    }
}

pub fn enable_hotkeys() -> Result<(), String> {
    if HOTKEYS_ENABLED.load(Ordering::Relaxed) {
        return Ok(());
    }

    // F2: 与 disable_hotkeys 对称——先持锁再 store+reload，
    // 避免 store(true) 在锁外被并发 disable 覆盖或读到撕裂状态；
    // 持锁后必须调 reload_from_settings_inner（顶层 reload 会再 lock 自死锁）。
    let _guard = GLOBAL_HOTKEY_SYNC_LOCK.lock();
    HOTKEYS_ENABLED.store(true, Ordering::Relaxed);
    reload_from_settings_inner()?;
    println!("已启用全局热键");
    Ok(())
}

pub fn disable_hotkeys() {
    if !HOTKEYS_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // F1: outer 持 GLOBAL_HOTKEY_SYNC_LOCK 覆盖 unregister_all 假设锁已持的调用契约，
    // 否则 disable_hotkeys 内的 unregister_all 不持锁无法走完串行化。
    let _guard = GLOBAL_HOTKEY_SYNC_LOCK.lock();
    unregister_all();
    HOTKEYS_ENABLED.store(false, Ordering::Relaxed);
    println!("已禁用全局热键");
}

pub fn is_hotkeys_enabled() -> bool {
    HOTKEYS_ENABLED.load(Ordering::Relaxed)
}

// 更新快捷键状态
fn update_shortcut_status(id: &str, shortcut: &str, success: bool, error: Option<String>) {
    set_shortcut_status(id, shortcut, success, error);
}

// 更新快捷键状态（pub: navigation.rs 注册失败时写导航键状态表）
pub fn set_shortcut_status(id: &str, shortcut: &str, success: bool, error: Option<String>) {
    let mut status_map = SHORTCUT_STATUS.lock();
    status_map.insert(
        id.to_string(),
        ShortcutStatus {
            id: id.to_string(),
            shortcut: shortcut.to_string(),
            success,
            error,
        },
    );
}

// 获取所有快捷键状态
pub fn get_shortcut_statuses() -> Vec<ShortcutStatus> {
    let status_map = SHORTCUT_STATUS.lock();
    status_map.values().cloned().collect()
}

// 获取单个快捷键状态
pub fn get_shortcut_status(id: &str) -> Option<ShortcutStatus> {
    let status_map = SHORTCUT_STATUS.lock();
    status_map.get(id).cloned()
}

// 清除快捷键状态（pub: navigation.rs 注销导航键时清除对应 id 状态，
// 导航 id 不在 REGISTERED_SHORTCUTS，unregister_all 的清理覆盖不到）
pub fn clear_shortcut_status(id: &str) {
    let mut status_map = SHORTCUT_STATUS.lock();
    status_map.remove(id);
}

pub fn reload_from_settings() -> Result<(), String> {
    let _guard = GLOBAL_HOTKEY_SYNC_LOCK.lock();
    reload_from_settings_inner()
}

fn reload_from_settings_inner() -> Result<(), String> {
    let settings = crate::get_settings();
    
    unregister_all();
    {
        let mut status_map = SHORTCUT_STATUS.lock();
        status_map.clear();
    }
    
    if settings.hotkeys_enabled {
        if is_foreground_globally_disabled() {
            return Ok(());
        }

        if !settings.toggle_shortcut.is_empty() {
            if let Err(e) = register_toggle_hotkey(&settings.toggle_shortcut) {
                eprintln!("注册主窗口切换快捷键失败: {}", e);
            }
        }

        if !settings.open_settings_shortcut.is_empty() {
            if let Err(e) = register_open_settings_hotkey(&settings.open_settings_shortcut) {
                eprintln!("注册打开设置快捷键失败: {}", e);
            }
        }
        
        if settings.quickpaste_enabled && !settings.quickpaste_shortcut.is_empty() {
            if let Err(e) = register_quickpaste_hotkey(&settings.quickpaste_shortcut) {
                eprintln!("注册预览窗口快捷键失败: {}", e);
            }
        }

        if !settings.transfer_shelf_create_shortcut.is_empty() {
            if let Err(e) = register_transfer_shelf_create_hotkey(&settings.transfer_shelf_create_shortcut) {
                eprintln!("注册文件盒创建快捷键失败: {}", e);
            }
        }

        if !settings.webdav_push_shortcut.is_empty() {
            if let Err(e) = register_webdav_push_hotkey(&settings.webdav_push_shortcut) {
                eprintln!("注册 WebDAV 推送快捷键失败: {}", e);
            }
        }

        if !settings.webdav_pull_shortcut.is_empty() {
            if let Err(e) = register_webdav_pull_hotkey(&settings.webdav_pull_shortcut) {
                eprintln!("注册 WebDAV 拉取快捷键失败: {}", e);
            }
        }
        
        if settings.screenshot_enabled && !settings.screenshot_shortcut.is_empty() {
            if let Err(e) = register_screenshot_hotkey(&settings.screenshot_shortcut) {
                eprintln!("注册截图快捷键失败: {}", e);
            }
        }
        
        if settings.screenshot_enabled && !settings.screenshot_quick_save_shortcut.is_empty() {
            if let Err(e) = register_screenshot_quick_save_hotkey(&settings.screenshot_quick_save_shortcut) {
                eprintln!("注册快速保存截图快捷键失败: {}", e);
            }
        }
        
        if settings.screenshot_enabled && !settings.screenshot_quick_pin_shortcut.is_empty() {
            if let Err(e) = register_screenshot_quick_pin_hotkey(&settings.screenshot_quick_pin_shortcut) {
                eprintln!("注册快速贴图截图快捷键失败: {}", e);
            }
        }
        
        if settings.screenshot_enabled && !settings.screenshot_quick_ocr_shortcut.is_empty() {
            if let Err(e) = register_screenshot_quick_ocr_hotkey(&settings.screenshot_quick_ocr_shortcut) {
                eprintln!("注册快速OCR截图快捷键失败: {}", e);
            }
        }
        
        if !settings.toggle_clipboard_monitor_shortcut.is_empty() {
            if let Err(e) = register_toggle_clipboard_monitor_hotkey(&settings.toggle_clipboard_monitor_shortcut) {
                eprintln!("注册切换剪贴板监听快捷键失败: {}", e);
            }
        }
        
        if !settings.toggle_paste_with_format_shortcut.is_empty() {
            if let Err(e) = register_toggle_paste_with_format_hotkey(&settings.toggle_paste_with_format_shortcut) {
                eprintln!("注册切换格式粘贴快捷键失败: {}", e);
            }
        }

        if !settings.toggle_low_memory_mode_shortcut.is_empty() {
            if let Err(e) = register_toggle_low_memory_mode_hotkey(&settings.toggle_low_memory_mode_shortcut) {
                eprintln!("注册切换低占用模式快捷键失败: {}", e);
            }
        }
        
        if !settings.paste_plain_text_shortcut.is_empty() {
            if let Err(e) = register_paste_plain_text_hotkey(&settings.paste_plain_text_shortcut) {
                eprintln!("注册纯文本粘贴快捷键失败: {}", e);
            }
        }
        
        if settings.number_shortcuts && !settings.number_shortcuts_modifier.is_empty() {
            if let Err(e) = register_number_shortcuts(&settings.number_shortcuts_modifier) {
                eprintln!("注册数字快捷键失败: {}", e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{fn_body, strip_line_comments};

    fn global_source() -> String {
        super::super::test_utils::source_file("src/services/system/hotkey/global.rs")
    }

    // F5: unregister_number_shortcuts 必须先 is_registered 探测再注销,
    // 与 unregister_shortcut 同款——裸 UnregisterHotKey 空跑会让内部表
    // 与 Windows 层脱节,残留吞键的幽灵热键。统一走 safe_unregister。
    #[test]
    fn unregister_number_shortcuts_probes_before_unregister() {
        let src = strip_line_comments(&global_source());
        let start = src
            .find("pub fn unregister_number_shortcuts()")
            .expect("缺 unregister_number_shortcuts");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let b = &src[start..end];
        let probe_pos = b.find("is_registered(shortcut)");
        let unregister_pos = b.find("safe_unregister");
        assert!(
            probe_pos.is_some() && unregister_pos.is_some() && probe_pos < unregister_pos,
            "unregister_number_shortcuts 必须先 is_registered 探测再 unregister"
        );
    }

    // F6: register_number_shortcuts 失败清理不得误杀同组合键的其他条目——
    // 用户把自定义热键设为 Ctrl+1 时,reload 顺序是用户条目先注册成功,
    // 数字快捷键后注册失败,失败清理探测到 Ctrl+1 已注册(用户条目的)
    // 即注销它→用户热键静默失效但状态表仍显示成功。清理前必须先检查
    // 该组合键是否属于其他条目,属于则跳过清理。
    #[test]
    fn number_shortcut_failure_cleanup_does_not_kill_same_combo_other_id() {
        let src = strip_line_comments(&global_source());
        let b = fn_body(&src, "register_number_shortcuts");
        let guard_pos = b.find("belongs_to_other");
        let unregister_pos = b.find("safe_unregister(&app, shortcut)");
        assert!(
            guard_pos.is_some() && unregister_pos.is_some() && guard_pos < unregister_pos,
            "失败清理前必须检查同组合键是否属于其他条目(belongs_to_other),避免误杀用户热键"
        );
        assert!(
            b.find("failed_shortcuts.push").is_some(),
            "失败清理后必须记录 failed_shortcuts"
        );
    }

    // F1: reload 顶层入口持锁后调用的内层函数禁止再 lock，
    // 否则 parking_lot 同线程不可重入自死锁——启动/settings 保存/托盘/
    // 前台切换全挂死。内层契约：reload_from_settings 顶层持锁贯穿整个
    // reload 过程，unregister_all / register_* 全部假设锁已持。
    // 锁序契约（F8）见 GLOBAL_HOTKEY_SYNC_LOCK 定义处注释。
    // 负向 contains 必须剥注释,避免注释字面误命中(§10.3)。
    #[test]
    fn reload_inner_and_callees_must_not_reenter_sync_lock() {
        let src = strip_line_comments(&global_source());
        for name in [
            "reload_from_settings_inner",
            "unregister_all",
            "register_shortcut",
            "register_quickpaste_hotkey",
            "register_paste_plain_text_hotkey",
            "register_number_shortcuts",
        ] {
            let b = fn_body(&src, name);
            assert!(
                !b.contains("GLOBAL_HOTKEY_SYNC_LOCK.lock()"),
                "{name} 在 reload 持锁路径上禁止再 lock，否则同线程自死锁"
            );
        }
    }

    // F1 配套:reload 顶层入口 + 配套 disable_hotkeys 必须持锁,
    // 保证 reload/disable 过程仍串行。
    #[test]
    fn reload_outer_entries_hold_sync_lock() {
        let src = strip_line_comments(&global_source());
        for name in ["reload_from_settings", "disable_hotkeys", "enable_hotkeys"] {
            let b = fn_body(&src, name);
            assert!(
                b.contains("GLOBAL_HOTKEY_SYNC_LOCK.lock()"),
                "{name} 作为外层入口必须持 GLOBAL_HOTKEY_SYNC_LOCK"
            );
        }
    }

    // F2: enable_hotkeys 持锁后必须调 reload_from_settings_inner
    // （顶层 reload_from_settings 会再 lock 同一把锁，同线程自死锁），
    // 且锁位置早于 HOTKEYS_ENABLED.store(true)。
    #[test]
    fn enable_hotkeys_holds_lock_and_calls_inner_reload() {
        let src = strip_line_comments(&global_source());
        let b = fn_body(&src, "enable_hotkeys");
        let lock_pos = b.find("GLOBAL_HOTKEY_SYNC_LOCK.lock()");
        let store_pos = b.find("HOTKEYS_ENABLED.store(true");
        let inner_pos = b.find("reload_from_settings_inner()");
        assert!(
            lock_pos.is_some() && store_pos.is_some() && inner_pos.is_some() && lock_pos < store_pos,
            "enable_hotkeys 必须持锁后 store 并调 reload_from_settings_inner"
        );
        assert!(
            b.find("reload_from_settings()").is_none(),
            "enable_hotkeys 持锁内禁止调顶层 reload_from_settings（自死锁）"
        );
    }

    // F8: 锁序契约必须成文——global 层入口(持 GLOBAL)内部调 navigation
    // sync(持 NAVIGATION),反向顺序即 AB-BA 死锁。注释须在锁定义处。
    // 本测试故意匹配注释字面("锁序契约"/"NAVIGATION"),故保留 raw 源码不剥注释。
    #[test]
    fn lock_order_contract_documented_at_global_lock_definition() {
        // ponytail: 故意 raw——断言目标就是注释本身,strip 会让护栏永远红
        let src = global_source();
        let def_pos = src.find("static GLOBAL_HOTKEY_SYNC_LOCK");
        let doc_pos = src.find("锁序契约");
        let nav_pos = src.find("NAVIGATION");
        assert!(
            doc_pos.is_some() && def_pos.is_some() && def_pos < doc_pos,
            "GLOBAL_HOTKEY_SYNC_LOCK 定义处必须成文锁序契约注释"
        );
        assert!(
            nav_pos.is_some(),
            "锁序契约注释必须点名 NAVIGATION 锁"
        );
    }

    // F2: disable_all_shortcuts 必须持 GLOBAL_HOTKEY_SYNC_LOCK,
    // 且锁位置早于 REGISTERED_SHORTCUTS.lock().clear()。
    // apply_activation(Inactive) 在 spawn 线程调用它,不持锁会与持锁
    // 的 register_* 并发留下半注册幽灵热键。
    #[test]
    fn disable_all_shortcuts_holds_sync_lock_before_clear() {
        let src = strip_line_comments(&global_source());
        let b = fn_body(&src, "disable_all_shortcuts");
        let lock_pos = b.find("GLOBAL_HOTKEY_SYNC_LOCK.lock()");
        let clear_pos = b.find("REGISTERED_SHORTCUTS.lock().clear()");
        assert!(
            lock_pos.is_some() && clear_pos.is_some() && lock_pos < clear_pos,
            "disable_all_shortcuts 必须在清空状态前持 GLOBAL_HOTKEY_SYNC_LOCK"
        );
    }

    // 三个直连 on_shortcut 的注册函数失败路径必须清理幽灵热键：
    // 插件可能在 Err 前部分写入 Windows 层，若不探测注销会残留吞键。
    // 统一走 safe_unregister 公共函数（内含 is_registered 探测）。
    #[test]
    fn direct_registration_fns_cleanup_ghost_hotkey_on_failure() {
        let src = strip_line_comments(&global_source());
        for name in [
            "register_quickpaste_hotkey",
            "register_paste_plain_text_hotkey",
            "register_number_shortcuts",
        ] {
            let b = fn_body(&src, name);
            assert!(
                b.find("safe_unregister(&app, shortcut)").is_some(),
                "{name} 失败路径必须调 safe_unregister 清理幽灵热键"
            );
        }
    }
}


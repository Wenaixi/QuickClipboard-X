use crate::services::{AppSettings, get_settings, update_settings, get_data_directory};
use crate::services::settings::storage::SettingsStorage;
use tauri::Manager;
use serde_json::Value;

const ONE_TIME_PASTE_STORE_KEY: &str = "tool.one_time_paste_enabled";
const DEFAULT_MAIN_WINDOW_WIDTH: u32 = 360;
const DEFAULT_MAIN_WINDOW_HEIGHT: u32 = 520;

fn handle_disable_edge_hide(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let state = crate::windows::main_window::get_window_state();

        if state.is_snapped {
            if state.is_hidden {
                let _ = crate::windows::main_window::show_snapped_window(&window);
            }
            let _ = crate::windows::main_window::restore_from_snap(&window);
            crate::windows::main_window::stop_edge_monitoring();
        }
    }
}

// 切换"鼠标移到边缘弹出"开关后,若主窗口正处于贴边隐藏状态,立即刷新其形态:
// 开启 -> 露出透明触发条(保持置顶);关闭 -> 真正隐藏,不留透明条
fn refresh_edge_hover_mode(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let state = crate::windows::main_window::get_window_state();
        if state.is_snapped && state.is_hidden {
            // 穿透/置顶/显隐(含 show)由 refresh_hidden_snapped_window 统一决策
            let _ = crate::windows::main_window::refresh_hidden_snapped_window(&window);
        }
    }
}

fn normalize_update_check_interval(value: &str) -> String {
    match value {
        "every3days" => "every3days".to_string(),
        "weekly" => "weekly".to_string(),
        _ => "daily".to_string(),
    }
}

fn capture_main_window_logical_size(app: &tauri::AppHandle) -> Option<(u32, u32)> {
    let window = app.get_webview_window("main")?;
    let size = window.inner_size().ok()?;
    let scale_factor = window
        .scale_factor()
        .ok()
        .filter(|value| *value > 0.0)
        .unwrap_or(1.0);

    Some((
        ((size.width as f64) / scale_factor).round().max(1.0) as u32,
        ((size.height as f64) / scale_factor).round().max(1.0) as u32,
    ))
}

fn restore_main_window_default_size(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    crate::windows::main_window::apply_saved_window_size(
        &window,
        DEFAULT_MAIN_WINDOW_WIDTH,
        DEFAULT_MAIN_WINDOW_HEIGHT,
    );

    let state = crate::windows::main_window::get_window_state();
    if state.is_snapped {
        if state.is_hidden {
            let _ = crate::windows::main_window::refresh_hidden_snapped_window(&window);
        } else if state.snap_edge != crate::windows::main_window::SnapEdge::None {
            let _ = crate::windows::main_window::snap_to_edge(&window, state.snap_edge);
        }
    }
}

// 重新加载设置
#[tauri::command]
pub fn reload_settings() -> Result<AppSettings, String> {
    let settings = SettingsStorage::load()?;
    update_settings(settings.clone())?;
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(mut settings: AppSettings, app: tauri::AppHandle) -> Result<(), String> {
    // 守不变量:贴边悬浮弹出蕴含贴边隐藏,JSON 导入或非 UI 流程若留下 hover=true/hide=false
    // 会在下次开启 hide 时意外弹出触发条 —— 此处一次性规范化
    normalize_edge_hover_invariant(&mut settings);

    let old_settings = get_settings();
    let webdav_password = std::mem::take(&mut settings.webdav_password);
    if !webdav_password.is_empty() {
        crate::services::secure_credentials::set_webdav_password(
            &settings.webdav_url,
            &settings.webdav_username,
            &webdav_password,
        )?;
    }
    if settings.settings_migration_version.is_none()
        || settings.settings_migration_version < old_settings.settings_migration_version
    {
        settings.settings_migration_version = old_settings.settings_migration_version;
    }
    settings.normalize_app_filter_blocklist();
    let clipboard_monitor_changed = old_settings.clipboard_monitor != settings.clipboard_monitor;
    let edge_hide_changed = old_settings.edge_hide_enabled != settings.edge_hide_enabled;
    let edge_hover_changed = old_settings.edge_hover_popup_enabled != settings.edge_hover_popup_enabled;
    let quickpaste_enabled_changed = old_settings.quickpaste_enabled != settings.quickpaste_enabled;
    let remember_window_size_enabled =
        !old_settings.remember_window_size && settings.remember_window_size;
    let remember_window_size_disabled =
        old_settings.remember_window_size && !settings.remember_window_size;
    let webdav_crypto_scope_changed = old_settings.webdav_url != settings.webdav_url
        || old_settings.webdav_username != settings.webdav_username
        || old_settings.webdav_root_path != settings.webdav_root_path;
    let show_tray_icon_changed = old_settings.show_tray_icon != settings.show_tray_icon;

    if edge_hide_changed && !settings.edge_hide_enabled {
        settings.edge_snap_position = None;
        settings.edge_snap_edge = None;
        settings.edge_snap_ratio = None;
        settings.edge_snap_monitor_id = None;
        handle_disable_edge_hide(&app);
    }

    settings.update_check_interval = normalize_update_check_interval(&settings.update_check_interval);
    if remember_window_size_enabled {
        settings.saved_window_size = capture_main_window_logical_size(&app);
    } else if !settings.remember_window_size {
        settings.saved_window_size = None;
    }
    if webdav_crypto_scope_changed {
        crate::services::webdav_sync::crypto::clear_cached_keys();
    }

    update_settings(settings.clone())?;

    if edge_hover_changed {
        // 新值已落地,再按新开关刷新贴边隐藏形态
        refresh_edge_hover_mode(&app);
    }

    if remember_window_size_disabled {
        restore_main_window_default_size(&app);
    }
    
    if let Err(e) = crate::hotkey::reload_from_settings() {
        eprintln!("重新加载快捷键失败: {}", e);
    }
    
    if clipboard_monitor_changed {
        if settings.clipboard_monitor {
            crate::start_clipboard_monitor()?;
        } else {
            crate::stop_clipboard_monitor()?;
        }
        
        use tauri::Emitter;
        let _ = app.emit("settings-changed", serde_json::json!({
            "clipboardMonitor": settings.clipboard_monitor
        }));
    }
    
    if quickpaste_enabled_changed {
        if settings.quickpaste_enabled {
            let app_clone = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = crate::quickpaste::init_quickpaste_window(&app_clone);
            });
        } else if let Some(window) = app.get_webview_window("quickpaste") {
            let _ = window.close();
        }
    }

    if show_tray_icon_changed {
        if let Some(tray) = app.tray_by_id("main-tray") {
            if let Err(e) = tray.set_visible(settings.show_tray_icon) {
                eprintln!("切换托盘图标可见性失败: {}", e);
            }
        }
    }

    #[cfg(feature = "screenshot-suite")]
    {
        if let Ok(json) = serde_json::to_value(&settings) {
            screenshot_suite::config::update_config(json);
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn reset_settings_to_default(app: tauri::AppHandle) -> Result<(), String> {
    let defaults = AppSettings::default();
    save_settings(defaults, app)
}

#[tauri::command]
pub fn get_settings_cmd() -> AppSettings {
    get_settings()
}

// 守不变量:贴边悬浮弹出蕴含贴边隐藏。
// 委托给 AppSettings::normalize_edge_hover_invariant,
// 该方法是唯一决策点,load / JSON 导入路径也统一调用它。
fn normalize_edge_hover_invariant(settings: &mut AppSettings) {
    settings.normalize_edge_hover_invariant();
}

#[tauri::command]
pub fn set_edge_hide_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    let mut settings = get_settings();
    settings.edge_hide_enabled = enabled;
    normalize_edge_hover_invariant(&mut settings);

    if !enabled {
        settings.edge_snap_position = None;
        settings.edge_snap_edge = None;
        settings.edge_snap_ratio = None;
        settings.edge_snap_monitor_id = None;
        handle_disable_edge_hide(&app);
    }
    
    update_settings(settings)?;
    Ok(())
}

#[tauri::command]
pub fn get_all_windows_info_cmd() -> Result<Vec<crate::services::system::AppInfo>, String> {
    Ok(crate::services::system::get_all_windows_info())
}

#[tauri::command]
pub fn is_portable_mode() -> bool {
    if crate::services::is_portable_build() { return true; }
    use std::env;
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("portable.txt").exists()))
        .unwrap_or(false)
}

// 获取应用版本信息
#[tauri::command]
pub fn get_app_version() -> Result<Value, String> {
    let version = env!("CARGO_PKG_VERSION");
    Ok(serde_json::json!({
        "version": version,
        "name": env!("CARGO_PKG_NAME"),
    }))
}

// 获取数据目录路径
#[tauri::command]
pub fn get_data_directory_cmd() -> Result<String, String> {
    let path = get_data_directory()?;
    Ok(path.to_string_lossy().to_string())
}

// 设置开机自启动
#[tauri::command]
pub fn set_auto_start(enabled: bool) -> Result<(), String> {
    let mut settings = get_settings();
    crate::services::system::configure_auto_start(enabled, settings.run_as_admin)?;
    settings.auto_start = enabled;
    update_settings(settings)?;
    Ok(())
}

// 检查开机自启动状态
#[tauri::command]
pub fn get_auto_start_status() -> Result<bool, String> {
    let settings = get_settings();
    crate::services::system::get_auto_start_status(settings.run_as_admin)
}

// 重新加载快捷键
#[tauri::command]
pub fn reload_hotkeys() -> Result<(), String> {
    crate::hotkey::reload_from_settings()
}

// 启用快捷键
#[tauri::command]
pub fn enable_hotkeys() -> Result<(), String> {
    crate::hotkey::enable_hotkeys()
}

// 禁用快捷键
#[tauri::command]
pub fn disable_hotkeys() -> Result<(), String> {
    crate::hotkey::disable_hotkeys();
    Ok(())
}

// 检查快捷键是否启用
#[tauri::command]
pub fn is_hotkeys_enabled() -> bool {
    crate::hotkey::is_hotkeys_enabled()
}

// 获取所有快捷键状态
#[tauri::command]
pub fn get_shortcut_statuses() -> Vec<crate::hotkey::ShortcutStatus> {
    crate::hotkey::get_shortcut_statuses()
}

// 获取单个快捷键状态
#[tauri::command]
pub fn get_shortcut_status(id: String) -> Option<crate::hotkey::ShortcutStatus> {
    crate::hotkey::get_shortcut_status(&id)
}

// 切换剪贴板监听状态
pub fn toggle_clipboard_monitor(app: &tauri::AppHandle) -> Result<(), String> {
    let mut settings = get_settings();
    settings.clipboard_monitor = !settings.clipboard_monitor;
    let enabled = settings.clipboard_monitor;
    
    let result = save_settings(settings, app.clone());
    if crate::services::low_memory::is_low_memory_mode() {
        let _ = crate::windows::tray::native_menu::update_native_menu(app);
    }

    let message = if enabled { "剪贴板监听已启用" } else { "剪贴板监听已禁用" };
    let _ = crate::services::notification::show_notification(app, "QuickClipboard", message);
    
    result
}

// 切换格式粘贴状态
pub fn toggle_paste_with_format(app: &tauri::AppHandle) -> Result<(), String> {
    let mut settings = get_settings();
    settings.paste_with_format = !settings.paste_with_format;
    let enabled = settings.paste_with_format;
    
    use tauri::Emitter;
    let _ = app.emit("settings-changed", serde_json::json!({
        "pasteWithFormat": settings.paste_with_format
    }));
    
    let result = save_settings(settings, app.clone());
    if crate::services::low_memory::is_low_memory_mode() {
        let _ = crate::windows::tray::native_menu::update_native_menu(app);
    }

    let message = if enabled { "格式粘贴已启用" } else { "格式粘贴已禁用" };
    let _ = crate::services::notification::show_notification(app, "QuickClipboard", message);
    
    result
}

// 保存窗口位置
#[tauri::command]
pub fn save_window_position(x: i32, y: i32) -> Result<(), String> {
    let mut settings = get_settings();
    settings.saved_window_position = Some((x, y));
    update_settings(settings)?;
    Ok(())
}

// 保存窗口大小
#[tauri::command]
pub fn save_window_size(width: u32, height: u32) -> Result<(), String> {
    let mut settings = get_settings();
    if !settings.remember_window_size {
        return Ok(());
    }
    settings.saved_window_size = Some((width, height));
    update_settings(settings)?;
    Ok(())
}

#[tauri::command]
pub fn save_quickpaste_window_size(width: u32, height: u32) -> Result<(), String> {
    let mut settings = get_settings();
    settings.quickpaste_window_width = width;
    settings.quickpaste_window_height = height;
    update_settings(settings)?;
    Ok(())
}

// 设置管理员权限运行
#[tauri::command]
pub fn set_run_as_admin(enabled: bool) -> Result<(), String> {
    let mut settings = get_settings();
    if settings.run_as_admin == enabled {
        return Ok(());
    }

    if enabled {
        if crate::services::system::is_running_as_admin() {
            crate::services::system::configure_auto_start(settings.auto_start, true)?;
        }
    } else {
        crate::services::system::switch_to_standard_mode(settings.auto_start)?;
    }

    settings.run_as_admin = enabled;
    update_settings(settings)?;
    Ok(())
}

// 获取管理员权限运行状态配置
#[tauri::command]
pub fn get_run_as_admin_status() -> Result<bool, String> {
    Ok(get_settings().run_as_admin)
}

// 检查当前是否以管理员权限运行
#[tauri::command]
pub fn is_running_as_admin() -> bool {
    crate::services::system::is_running_as_admin()
}

// 检查计划任务是否存在
#[tauri::command]
pub fn is_admin_task_ready() -> Result<bool, String> {
    crate::services::system::is_admin_task_ready(get_settings().auto_start)
}

// 以管理员权限重启程序
#[tauri::command]
pub fn restart_as_admin(app: tauri::AppHandle) -> Result<(), String> {
    if crate::services::system::try_elevate_and_restart(get_settings().auto_start)? {
        app.exit(0);
        Ok(())
    } else {
        Err("请求管理员权限失败，用户取消了 UAC 提示".to_string())
    }
}

#[tauri::command]
pub fn get_one_time_paste_enabled() -> bool {
    crate::services::store::get::<bool>(ONE_TIME_PASTE_STORE_KEY).unwrap_or(false)
}

#[tauri::command]
pub fn set_one_time_paste_enabled(enabled: bool) -> Result<bool, String> {
    crate::services::store::set(ONE_TIME_PASTE_STORE_KEY, &enabled)?;
    Ok(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 回归:set_edge_hide_enabled(false) 必须连带关闭 hover,
    // 否则持久化 hide=false/hover=true 违规组合,下次开启 hide 时意外弹出触发条。
    // 注意:此测试只覆盖 normalize 本身的语义(单元级),
    // 端到端护栏(确认 set_edge_hide_enabled 函数体内真的调了 normalize)
    // 在 tdd_tests.rs 的 set_edge_hide_enabled_calls_normalize_in_body。
    #[test]
    fn disabling_edge_hide_also_disables_hover() {
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = true;

        // 模拟 set_edge_hide_enabled(false) 的写入路径
        settings.edge_hide_enabled = false;
        normalize_edge_hover_invariant(&mut settings);

        assert!(!settings.edge_hover_popup_enabled, "关闭贴边隐藏必须同时关闭悬浮弹出");
    }

    // hover 蕴含 hide:仅关闭 hover 不影响 hide
    #[test]
    fn disabling_hover_keeps_edge_hide() {
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = true;

        settings.edge_hover_popup_enabled = false;
        normalize_edge_hover_invariant(&mut settings);

        assert!(settings.edge_hide_enabled, "关闭悬浮弹出不得影响贴边隐藏");
    }

    // 开启 hide 不强制开启 hover(用户偏好)
    #[test]
    fn enabling_edge_hide_keeps_hover_off() {
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = false;
        settings.edge_hover_popup_enabled = false;

        settings.edge_hide_enabled = true;
        normalize_edge_hover_invariant(&mut settings);

        assert!(!settings.edge_hover_popup_enabled, "开启贴边隐藏不强制开启悬浮弹出");
    }
}


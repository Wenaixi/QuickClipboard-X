use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder, WebviewWindow, Manager};
use tauri::{Emitter, Listener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::time::interval;

static FORCE_UPDATE_MODE: AtomicBool = AtomicBool::new(false);
static UPDATE_BANNER_STATE: LazyLock<Mutex<Option<UpdateBannerState>>> = LazyLock::new(|| Mutex::new(None));
static UPDATE_WINDOW_PAYLOAD: LazyLock<Mutex<Option<serde_json::Value>>> = LazyLock::new(|| Mutex::new(None));
const AUTO_UPDATE_CHECK_INTERVAL_SECS: u64 = 60 * 60;
const LAST_AUTO_CHECK_AT_KEY: &str = "updater.last_auto_check_at";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBannerState {
    pub current_version: String,
    pub latest_version: String,
}

pub fn is_force_update_mode() -> bool {
    FORCE_UPDATE_MODE.load(Ordering::Relaxed)
}

pub fn get_update_banner_state() -> Option<UpdateBannerState> {
    UPDATE_BANNER_STATE
        .lock()
        .ok()
        .and_then(|state| state.clone())
}

fn set_update_banner_state(app: &AppHandle, state: Option<UpdateBannerState>) {
    if let Ok(mut guard) = UPDATE_BANNER_STATE.lock() {
        *guard = state.clone();
    }
    let _ = app.emit("update-banner-state-changed", state);
}

fn set_update_window_payload(payload: Option<serde_json::Value>) {
    if let Ok(mut guard) = UPDATE_WINDOW_PAYLOAD.lock() {
        *guard = payload;
    }
}

fn get_update_window_payload() -> Option<serde_json::Value> {
    UPDATE_WINDOW_PAYLOAD
        .lock()
        .ok()
        .and_then(|payload| payload.clone())
}

fn emit_update_payload(window: &WebviewWindow, payload: serde_json::Value) {
    let _ = window.emit("update-config", payload.clone());

    let win_for_emit = window.clone();
    window.once("updater-ready", move |_| {
        let _ = win_for_emit.emit("update-config", payload);
    });
}

fn parse_env_bool(key: &str) -> Option<bool> {
    let raw = std::env::var(key).ok()?;
    let v = raw.trim().to_lowercase();
    match v.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn is_prerelease(version: &str) -> bool {
    let v = version.to_lowercase();
    v.contains("alpha") || v.contains("beta") || v.contains("rc") || v.contains("dev")
}

fn update_check_interval_seconds(value: &str) -> u64 {
    match crate::services::AppSettings::normalize_update_check_interval(value) {
        "every3days" => 3 * 24 * 60 * 60,
        "weekly" => 7 * 24 * 60 * 60,
        _ => 24 * 60 * 60,
    }
}

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn resolve_use_beta_channel(settings: &crate::services::AppSettings, is_current_prerelease: bool) -> bool {
    if let Some(include_beta_updates) = settings.include_beta_updates {
        return include_beta_updates;
    }

    match std::env::var("QC_UPDATE_CHANNEL") {
        Ok(v) if v.trim().eq_ignore_ascii_case("beta") => true,
        Ok(v) if v.trim().eq_ignore_ascii_case("stable") => false,
        _ => is_current_prerelease,
    }
}

async fn check_updates_if_due(app: &AppHandle) -> Result<bool, String> {
    let settings = crate::services::get_settings();
    let now = current_unix_timestamp();
    let interval_secs = update_check_interval_seconds(&settings.update_check_interval);
    let last_check_at = crate::services::store::get::<u64>(LAST_AUTO_CHECK_AT_KEY).unwrap_or(0);

    if last_check_at > 0 && now.saturating_sub(last_check_at) < interval_secs {
        return Ok(false);
    }

    // 检查成功(含无更新)才推进时间戳——失败(网络/服务器抖动)不推进,
    // 让 60s tick 成为天然重试节拍,抖动期最多 1 分钟重试一次,恢复感知
    // 延迟 ≤ 60s;若先推进时间戳,失败后要等满 24h/72h 才重试(整周期空转)。
    let result = check_updates(app, !settings.disable_update_popup).await;
    if result.is_ok() {
        crate::services::store::set(LAST_AUTO_CHECK_AT_KEY, &now)?;
    }
    result
}

// 检测当前运行的程序是否为安装版
fn is_installed_version() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    use std::path::Path;
    
    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let current_dir = match current_exe.parent() {
        Some(p) => p.to_string_lossy().to_lowercase(),
        None => return false,
    };
    
    let reg_paths = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\QuickClipboard"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\QuickClipboard"),
    ];
    
    for (hkey, path) in reg_paths {
        if let Ok(key) = RegKey::predef(hkey).open_subkey(path) {
            if let Ok(loc) = key.get_value::<String, _>("InstallLocation") {
                let install_path = loc.trim_end_matches('\\').to_lowercase();
                if !install_path.is_empty() && current_dir == install_path {
                    return true;
                }
            }
            if let Ok(uninstall) = key.get_value::<String, _>("UninstallString") {
                let uninstall_clean = uninstall.trim_matches('"');
                if let Some(parent) = Path::new(uninstall_clean).parent() {
                    let install_path = parent.to_string_lossy().to_lowercase();
                    if current_dir == install_path {
                        return true;
                    }
                }
            }
            if let Ok(icon) = key.get_value::<String, _>("DisplayIcon") {
                let icon_clean = icon.split(',').next().unwrap_or("").trim_matches('"');
                if let Some(parent) = Path::new(icon_clean).parent() {
                    let install_path = parent.to_string_lossy().to_lowercase();
                    if current_dir == install_path {
                        return true;
                    }
                }
            }
        }
    }
    
    false
}

pub fn start_update_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let _ = check_updates_if_due(&app).await;
        
        let mut ticker = interval(Duration::from_secs(AUTO_UPDATE_CHECK_INTERVAL_SECS));
        ticker.tick().await;
        
        loop {
            ticker.tick().await;
            let _ = check_updates_if_due(&app).await;
        }
    });
}

pub fn open_updater_window(app: &AppHandle, force_update: bool) -> Result<WebviewWindow, String> {
    let window = WebviewWindowBuilder::new(
        app,
        "updater",
        WebviewUrl::App("windows/updater/index.html".into()),
    )
    .title("更新")
    .inner_size(360.0, 130.0)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(true)
    .focused(false)
    .focusable(false)
    .drag_and_drop(false)
    .build()
    .map_err(|e| format!("创建更新窗口失败: {}", e))?;

    if let Ok(size) = window.outer_size() {
        let margin: i32 = 12;

        if let Ok(monitor) = crate::screen::ScreenUtils::get_monitor_at_cursor(app) {
            let work_area = monitor.work_area();
            let x = work_area.position.x + work_area.size.width as i32 - size.width as i32 - margin;
            let y = work_area.position.y + work_area.size.height as i32 - size.height as i32 - margin;
            let _ = window.set_position(tauri::PhysicalPosition::new(x.max(work_area.position.x), y.max(work_area.position.y)));
        }
    }

    if force_update {
        let app_for_event = app.clone();
        window.on_window_event(move |event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                }
                tauri::WindowEvent::Destroyed => {
                    app_for_event.exit(0);
                }
                _ => {}
            }
        });
    }

    Ok(window)
}

pub async fn open_cached_update_window(app: &AppHandle) -> Result<bool, String> {
    let payload = match get_update_window_payload() {
        Some(payload) => payload,
        None => return Ok(false),
    };

    let force_update = payload
        .get("forceUpdate")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let window = if let Some(window) = app.get_webview_window("updater") {
        let _ = window.show();
        window
    } else {
        open_updater_window(app, force_update)?
    };

    emit_update_payload(&window, payload);
    Ok(true)
}

async fn check_updates(app: &AppHandle, should_open_window: bool) -> Result<bool, String> {
    use tauri_plugin_updater::UpdaterExt;
    use std::time::Duration;
    
    let settings = crate::services::get_settings();
    let current_version = app.package_info().version.to_string();
    let is_current_prerelease = is_prerelease(&current_version);

    let entry_url = "https://api.quickclipboard.cn/update/latest.json";

    let mut stable_json_url: Option<String> = None;
    let mut beta_json_url: Option<String> = None;

    if let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(15)).build() {
        if let Ok(resp) = client.get(entry_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                stable_json_url = json
                    .get("stableJson")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                beta_json_url = json
                    .get("betaJson")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    let stable_url = stable_json_url.unwrap_or_else(|| "https://api.quickclipboard.cn/update/stable_latest.json".to_string());
    let beta_url = beta_json_url.unwrap_or_else(|| "https://api.quickclipboard.cn/update/beta_latest.json".to_string());

    let use_beta_channel = resolve_use_beta_channel(&settings, is_current_prerelease);

    let chosen_manifest_url = if use_beta_channel { beta_url } else { stable_url };
    let chosen_manifest_url_str = chosen_manifest_url.clone();

    let mut force_update = false;
    let mut notes: Option<serde_json::Value> = None;

    if let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(15)).build() {
        if let Ok(resp) = client.get(&chosen_manifest_url_str).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                force_update = json.get("forceUpdate").and_then(|v| v.as_bool()).unwrap_or(false);
                notes = json.get("notes").cloned();
            }
        }
    }

    let chosen_manifest_endpoint = tauri::Url::parse(&chosen_manifest_url_str)
        .map_err(|e| format!("{}", e))?;

    let updater = app.updater_builder()
        .endpoints(vec![chosen_manifest_endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            let new_version = update.version.clone();

            if !use_beta_channel && is_prerelease(&new_version) {
                set_update_banner_state(app, None);
                set_update_window_payload(None);
                return Ok(false);
            }

            // 便携版/免安装版判定:统一走 is_portable_runtime,避免与
            // storage/commands/data_management 语义漂移。便携版不做强制
            // 更新——无法保证自动安装,锁死整个应用会让用户无法退出
            // 数据管理页被无限阻塞(trigger 下载前必须放行)。
            let mut is_portable = crate::services::is_portable_runtime()
                || !is_installed_version();

            if let Some(v) = parse_env_bool("QC_FORCE_PORTABLE") {
                is_portable = v;
            }

            // 强制更新在便携版降级为非强制:跳过锁死(关主窗/关 quickpaste/
            // 禁热键),仅弹更新窗口提示用户手动下载。
            let effective_force = force_update && !is_portable;

            set_update_banner_state(
                app,
                Some(UpdateBannerState {
                    current_version: current_version.clone(),
                    latest_version: new_version.clone(),
                }),
            );

            if effective_force {
                FORCE_UPDATE_MODE.store(true, Ordering::Relaxed);
                // 强制更新 = 用户确认了本次更新流程,退出时不得被低占用模式
                // 的 ExitRequested 保活拦住:低内存模式下进程只有强制更新窗
                // 可见,Destroyed 分支 exit(0) 若不先标记,会 prevent_exit
                // 僵死(唯一可见窗口已销毁、无任何 UI 可交互)。
                crate::services::low_memory::set_user_requested_exit(true);
                if let Some(main_window) = app.get_webview_window("main") {
                    crate::hide_main_window(&main_window);
                }
                if let Some(qp_window) = app.get_webview_window("quickpaste") {
                    let _ = qp_window.hide();
                }
                let _ = crate::hotkey::disable_hotkeys();
            }

            if !effective_force && !should_open_window {
                let payload = serde_json::json!({
                    "forceUpdate": false,
                    "version": new_version,
                    "notes": notes,
                    "isPortable": is_portable,
                });
                set_update_window_payload(Some(payload));
                return Ok(true);
            }

            let window = if let Some(w) = app.get_webview_window("updater") {
                let _ = w.show();
                w
            } else {
                open_updater_window(app, effective_force)?
            };

            let payload = serde_json::json!({
                "forceUpdate": if is_portable { false } else { effective_force },
                "version": new_version,
                "notes": notes,
                "isPortable": is_portable,
            });

            set_update_window_payload(Some(payload.clone()));
            emit_update_payload(&window, payload);

            Ok(true)
        }
        None => {
            set_update_banner_state(app, None);
            set_update_window_payload(None);
            Ok(false)
        }
    }
}

pub async fn check_updates_and_open_window(app: &AppHandle) -> Result<bool, String> {
    check_updates(app, true).await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::FORCE_UPDATE_MODE;

    fn creator_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/updater_window/creator.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取更新器源码失败")
    }

    fn stripped_source() -> String {
        creator_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 便携版强制更新护栏:check_updates 中携带着 for PORTABLE_MODE 的
    // 锁死动作(hide_main_window/hide quickpaste/disable_hotkeys)必须被
    // effective_force 的 is_portable 短路关断。顺序断言:effective_force
    // 求值必须早于锁死动作分支。
    #[test]
    fn portable_build_never_enters_force_lockdown() {
        let src = stripped_source();
        let effective_pos = src
            .find("let effective_force = force_update && !is_portable")
            .expect("缺 effective_force 定义");
        let bay = src.find("if effective_force {").expect("缺锁死动作分支");
        assert!(
            effective_pos < bay,
            "便携版短路必须先于锁死动作分支"
        );
        let mode_store = format!(
            "FORCE_UPDATE_MODE.store(true, Ordering::Relaxed)"
        );
        assert!(
            src.contains(&mode_store),
            "锁死动作分支必须设置强制更新标志"
        );
        let mark_pos = src.find("set_user_requested_exit(true)").expect("锁死动作分支必须标记用户请求退出");
        let mode_pos = src.find(&mode_store).expect("缺少强制更新标志写入");
        assert!(
            mode_pos < mark_pos,
            "强制更新标志写入必须先于用户请求退出标记(保证低占用模式 ExitRequested 不 prevent_exit)"
        );
    }

    // 便携版降级护栏:payload 的 forceUpdate 必须对便携版写 false
    // (手动下载提示,不锁死应用)。
    #[test]
    fn portable_build_force_update_is_downgraded_in_payload() {
        let src = stripped_source();
        let payload_line = src
            .split('\n')
            .find(|l| l.contains("is_portable { false }"))
            .unwrap_or("");
        assert!(
            !payload_line.is_empty(),
            "payload 必须对便携版写 false"
        );
        let _ = FORCE_UPDATE_MODE.swap(false, Ordering::Relaxed);
    }

    // 自动更新定时检查存活护栏:start_update_checker 必须在主入口挂载。
    // 该后台任务此前从未被调用——设置页"每日/每周"自动检查间隔全空转,
    // 只有手动"检查更新"才执行。若 lib.rs 的挂载行被删,自动更新偏好
    // 再次变为死壳。
    #[test]
    fn auto_update_checker_is_wired_in_lib_entry() {
        let lib_source = std::fs::read_to_string(format!(
            "{}/src/lib.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取主入口源码失败");
        assert!(
            lib_source.contains(
                "windows::updater_window::start_update_checker(app.handle().clone());"
            ),
            "主入口必须挂载自动更新定时检查"
        );
    }

    // 自动更新失败重试护栏:上次检查时间戳必须只在检查成功后推进。
    // 若先推进时间戳再检查,失败(网络/服务器抖动)后要等满整个间隔
    // (24h/72h)才重试,自动更新整周期空转;成功才推进让 60s tick
    // 成为天然重试节拍。顺序断言:store::set(LAST_AUTO_CHECK_AT_KEY
    // 必须位于 check_updates 调用之后(仅 contains 无法表达顺序,
    // 见 §10.3 顺序类不变量用 find 下标比较)。
    #[test]
    fn update_check_timestamp_advances_only_after_success() {
        let src = stripped_source();
        let body = src
            .find("async fn check_updates_if_due")
            .map(|start| src[start..].to_string())
            .unwrap_or_default();
        let check_pos = body
            .find("check_updates(app, !settings.disable_update_popup).await")
            .unwrap_or_else(|| panic!("缺 check_updates 调用"));
        let set_pos = body
            .find("crate::services::store::set(LAST_AUTO_CHECK_AT_KEY, &now)")
            .unwrap_or_else(|| panic!("缺时间戳推进写入"));
        assert!(
            set_pos > check_pos,
            "时间戳推进必须位于 check_updates 之后(失败不推进,60s tick 重试)"
        );
    }
}


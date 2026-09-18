use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

// 托盘双击去抖锁。Mutex 被 panic 污染后经 into_inner 恢复继续使用——
// 去抖是 UI 层语义,锁损坏不应让托盘点击从此失效(与 services/store.rs 的
// APP_HANDLE 锁、贴图数据表锁同一套 poison 恢复约定)。
fn lock_last_click_time(
    last_click_time: &Mutex<Instant>,
) -> std::sync::MutexGuard<'_, Instant> {
    last_click_time
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

pub fn handle_tray_click(app: &AppHandle) {
    if crate::windows::updater_window::is_force_update_mode() {
        if let Some(w) = app.get_webview_window("updater") {
            let _ = w.show();
            let _ = w.set_focus();
        }
        return;
    }
    crate::toggle_main_window_visibility(app);
}

pub fn create_click_handler(app_handle: AppHandle) -> impl Fn() + Send + 'static {
    let last_click_time = Arc::new(Mutex::new(Instant::now() - Duration::from_millis(1000)));

    move || {
        let now = Instant::now();
        let mut last_time = lock_last_click_time(&last_click_time);

        if now.duration_since(*last_time) < Duration::from_millis(50) {
            return;
        }

        *last_time = now;
        drop(last_time);

        handle_tray_click(&app_handle);
    }
}

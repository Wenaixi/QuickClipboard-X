//! core 事件总线 → tauri emit 桥（壳层适配，进不了 core 的 tauri 依赖在此落地）。
//!
//! core 业务域（paste/clipboard 等）统一经 core::events::post 发布事件，
//! gui(egui) 壳直接 drain 消费；Tauri 壳需要把同一事件翻译成前端可收的
//! tauri emit。本模块在 src-tauri 启动时注册重绘钩子 + 轮询 drain，
//! 把 core::events::AppEvent 映射为原有 tauri 事件名，保持前端零改动。

use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri::Manager;

/// 启动事件桥：注册重绘钩子并起一个后台线程轮询 drain 翻译成 tauri emit。
/// 事件名与旧 src-tauri 侧 app.emit 完全一致，前端无需感知来源变更。
pub fn start_event_bridge(app: AppHandle) {
    // core::events::post 会触发重绘钩子；Tauri 壳没有 egui 上下文，
    // 钩子设为空操作即可（drain 由轮询线程独占消费）。
    quickclipboard_core::events::set_repaint_hook(Box::new(|| {}));

    std::thread::Builder::new()
        .name("core-events-bridge".into())
        .spawn(move || loop {
            for event in quickclipboard_core::events::drain() {
                match event {
                    quickclipboard_core::events::AppEvent::ClipboardUpdated(payload) => {
                        let _ = app.emit("clipboard-updated", payload);
                    }
                    quickclipboard_core::events::AppEvent::PasteCountUpdated(id) => {
                        let _ = app.emit("paste-count-updated", id);
                    }
                    quickclipboard_core::events::AppEvent::FavoritePasteCountUpdated(id) => {
                        let _ = app.emit("favorite-paste-count-updated", id);
                    }
                    quickclipboard_core::events::AppEvent::TrayMenuRefresh => {
                        let _ = app.emit("tray-menu-refresh", ());
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        })
        .expect("启动事件桥线程失败");
}

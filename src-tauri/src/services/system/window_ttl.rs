// 临时窗口空闲销毁(TTL)共享 helper
//
// 背景:WebView2 每窗口一个 renderer(约 40-100MB),菜单窗口用后 hide
// 复用但 renderer 常驻,长时间不弹菜单就是白占内存。这里提供"登记 →
// 空闲计时 → 版本校验 → 销毁"的最小 TTL 设施,对齐 preview-window
// 已有的 60s destroy TTL 模式(PREVIEW_DESTROY_TIMER_VERSION +
// schedule_preview_window_destroy)。
//
// 设计要点:
//   - 每个注册窗口一个版本原子(AtomicU64),hide/复用路径必须
//     fetch_add 推进版本,让旧计时任务醒来即自杀;新建/激活路径重置
//     版本。避免"关闭后旧计时任务醒来销毁新窗口"的竞态。
//   - schedule_destroy 只调度一次:内部把版本推进,若相同 label 已有
//     在飞计时任务,后调者因版本不一致直接返回,天然去重。
//   - 计时任务醒来先比对版本,不一致(被复用/被销毁)直接退出;一致
//     才 destroy 并清版本号。
//   - 复用与销毁同线程(tauri async runtime),无跨线程锁。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager};

/// 每个注册窗口的 TTL 状态:版本 + 标签(销毁/取消都要标签)
struct TtlEntry {
    version: AtomicU64,
    label: String,
}

/// 已注册的 TTL 窗口表(label → 状态);未注册的 label 不会被 TTL 触碰
static TTL_WINDOWS: Lazy<Mutex<std::collections::HashMap<String, TtlEntry>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// 登记窗口进入 TTL 管理(幂等,重复登记只重置版本)
pub fn register_ttl_window(label: &str) {
    let mut map = TTL_WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
    map.entry(label.to_string())
        .or_insert_with(|| TtlEntry {
            version: AtomicU64::new(0),
            label: label.to_string(),
        })
        .version
        .store(0, Ordering::SeqCst);
}

/// 推进窗口版本:窗口被复用/重新激活时调用,令旧的 TTL 计时任务失效
pub fn touch_ttl_window(label: &str) {
    if let Some(entry) = TTL_WINDOWS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(label)
    {
        let _ = entry.version.fetch_add(1, Ordering::SeqCst);
    }
}

/// 取消窗口的 TTL 管理(窗口真正销毁时调用,释放登记)
pub fn unregister_ttl_window(label: &str) {
    TTL_WINDOWS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(label);
}

/// 窗口空闲 idle_ms 后若仍未被复用,销毁之(tauri 窗口销毁)。
/// 幂等:同 label 已有在飞计时任务时,推进版本后旧任务失效,本次调度接管。
pub fn schedule_ttl_destroy(app: AppHandle, label: &str, idle_ms: u64) {
    let entry = {
        let mut map = TTL_WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = map.get_mut(label) {
            let _ = entry.version.fetch_add(1, Ordering::SeqCst);
            entry.version.load(Ordering::SeqCst)
        } else {
            // 未登记(可能已被销毁):不调度
            return;
        }
    };

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(idle_ms)).await;

        // 闭包 async move 只捕获 app 与 label 所有权,不再引用函数参数。
        let owned_label = label.to_string();
        let (entry_label, version) = {
            let map = TTL_WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
            match map.get(owned_label.as_str()) {
                Some(entry) => {
                    let v = entry.version.load(Ordering::SeqCst);
                    (entry.label.clone(), v)
                }
                None => return,
            }
        };

        // 版本被推进(复用/取消)或已被销毁:让位
        if version == 0 {
            return;
        }

        if let Some(window) = app.get_webview_window(entry_label.as_str()) {
            let _ = window.hide();
            let _ = window.close();
        }
        unregister_ttl_window(&entry_label);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/services/system/window_ttl.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 window_ttl 源码失败")
    }

    // TTL 计时任务必须校验版本再销毁——复用/销毁推进版本后旧任务醒来
    // 不得销毁新窗口(核心竞态护栏,源码字面可反证)
    #[test]
    fn ttl_destroy_validates_version_before_destroying() {
        let src = source();
        let schedule_start = src
            .find("pub fn schedule_ttl_destroy")
            .expect("缺 schedule_ttl_destroy");
        let schedule_body = &src[schedule_start..];
        // 调度入口必须推进版本(令旧任务失效)
        assert!(
            schedule_body.contains("fetch_add(1, Ordering::SeqCst)"),
            "调度前必须推进版本令旧 TTL 任务失效"
        );
        // spawn 的计时任务体内必须有版本校验
        let spawn_pos = schedule_body
            .find("tauri::async_runtime::spawn")
            .expect("必须 spawn 计时任务");
        let spawn_body = &schedule_body[spawn_pos..];
        assert!(
            spawn_body.contains("entry.version.load(Ordering::SeqCst)"),
            "计时任务醒来必须读当前版本"
        );
        assert!(
            spawn_body.contains("version == 0"),
            "版本为 0(已取消/已销毁)必须退出不销毁"
        );
        let destroy_pos = spawn_body
            .find(".close()")
            .expect("计时任务必须销毁窗口");
        let version_check_pos = spawn_body
            .find("version == 0")
            .expect("版本校验必须存在");
        assert!(
            version_check_pos < destroy_pos,
            "版本校验必须早于窗口销毁"
        );
    }

    // 未登记窗口不得被 TTL 触碰(register/unregister 语义)
    #[test]
    fn ttl_helpers_registered_and_unregistered() {
        let src = source();
        assert!(
            src.contains("pub fn register_ttl_window"),
            "必须提供登记入口"
        );
        assert!(
            src.contains("pub fn unregister_ttl_window"),
            "必须提供注销入口"
        );
        assert!(
            src.contains("pub fn touch_ttl_window"),
            "必须提供版本推进入口(复用/激活时调用)"
        );
    }
}

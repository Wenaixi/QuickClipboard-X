// 持久化存储服务
// 封装 tauri-plugin-store，供 Rust 代码使用
//
// 锁的并发语义：进程内所有调用共享 APP_HANDLE 这把互斥锁，任何时刻至多一个
// 线程持锁执行 app.store() 流程。std Mutex 被 panic 污染后（另一线程 panic
// 时持锁）会返回 PoisonError——但这只是锁状态损坏，不表示进程数据损坏，此处
// 统一通过 into_inner 取回内部值继续使用，与仓库其它全局锁（贴图数据表、
// 主窗口状态）的 poison 恢复语义一致。不要改为裸 .unwrap()/expect()：一旦
// 未来有路径在持锁时 panic，恢复语义可以保证应用继续运行，而 panic 会直接
// 带崩整个事件循环。

use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

// 默认存储文件名
const DEFAULT_STORE_FILE: &str = "app-store.json";

// 全局 AppHandle 引用
static APP_HANDLE: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

// 初始化存储服务
pub fn init(app: &AppHandle) {
    let mut handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    *handle = Some(app.clone());
}

// 获取存储路径
fn get_store_path(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(DEFAULT_STORE_FILE)
}

// 获取值
pub fn get<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    let app = handle.as_ref()?;

    let store_path = get_store_path(app);
    let store = app.store(store_path).ok()?;

    store.get(key).and_then(|v| serde_json::from_value(v).ok())
}

// 设置值
pub fn set<T: serde::Serialize>(key: &str, value: &T) -> Result<(), String> {
    let handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    let app = handle.as_ref().ok_or("AppHandle 未初始化")?;

    let store_path = get_store_path(app);
    let store = app.store(store_path).map_err(|e| e.to_string())?;

    let json_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    store.set(key, json_value);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

// 删除值
pub fn delete(key: &str) -> Result<(), String> {
    let handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    let app = handle.as_ref().ok_or("AppHandle 未初始化")?;

    let store_path = get_store_path(app);
    let store = app.store(store_path).map_err(|e| e.to_string())?;

    store.delete(key);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

// 检查键是否存在
pub fn has(key: &str) -> bool {
    let handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    let Some(app) = handle.as_ref() else { return false };

    let store_path = get_store_path(app);
    let Ok(store) = app.store(store_path) else { return false };

    store.has(key)
}

// 获取所有键
pub fn keys() -> Vec<String> {
    let handle = APP_HANDLE.lock().unwrap_or_else(|p| p.into_inner());
    let Some(app) = handle.as_ref() else { return vec![] };

    let store_path = get_store_path(app);
    let Ok(store) = app.store(store_path) else { return vec![] };

    store.keys().into_iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 锁被 panic 污染后必须能继续使用(poison 恢复)——否则未来任一持锁
    // panic 会让 APP_HANDLE 永久锁定,全部 store 服务直接崩溃。
    #[test]
    fn store_lock_recovers_from_poison() {
        // 用 Arc 在两线程间共享同一把 Mutex:子线程持锁 panic 污染它,
        // 主线程 join 后仍通过共享句柄取回内部值,验证 poison 恢复语义。
        let poisoned = std::sync::Arc::new(std::sync::Mutex::new(()));
        let shared = std::sync::Arc::clone(&poisoned);
        let handle = std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("force store APP_HANDLE poison");
        });
        let _ = handle.join();

        // 不 panic 才能代表运行时行为(与 lock_pin_data_recovers_from_poison 同款)
        let guard = shared.lock().unwrap_or_else(|p| p.into_inner());
        assert_eq!(*guard, ());
    }

    // 源码护栏:APP_HANDLE 锁的每次获取都必须带 poison 恢复(unwrap_or_else),
    // 禁止裸 .unwrap()/expect()——恢复语义移除后行为测试仍会过(无法在测试里
    // 真实污染共享全局静),所以用源码字面钉死。读自身源码并剥行注释,避免
    // 护栏语句里的字面误命中自己(§10.4 自指陷阱)。
    #[test]
    fn app_handle_lock_always_recovers_from_poison() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/store.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 store 源码失败");
        let prod: String = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(&source)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        let lock_count = prod.matches("APP_HANDLE.lock()").count();
        assert!(lock_count >= 5, "store 门面应覆盖 get/set/delete/has/keys 锁点");
        let recover_count = prod.matches("unwrap_or_else(|p| p.into_inner())").count();
        assert_eq!(
            recover_count, lock_count,
            "每次 APP_HANDLE.lock() 都必须带 poison 恢复(共 {lock_count} 处)"
        );
        assert!(
            !prod.contains("APP_HANDLE.lock().unwrap()") && !prod.contains("APP_HANDLE.lock().expect("),
            "APP_HANDLE 锁禁止裸 unwrap/expect,必须 unwrap_or_else 恢复"
        );
    }
}
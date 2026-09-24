//! 应用级键值存储：替代 tauri-plugin-store 的薄封装。
//!
//! 与旧实现保持同一文件（`app-store.json`）与同一格式（顶层 JSON 对象），
//! 因此升级无需迁移。路径改走 `get_data_directory()`，便携版与自定义存储
//! 路径语义自动继承（旧实现走 tauri 的 app_data_dir 会忽略自定义路径）。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use once_cell::sync::OnceCell;
use serde_json::Value;

static STORE: LazyLock<Mutex<HashMap<String, Value>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static STORE_PATH: OnceCell<std::path::PathBuf> = OnceCell::new();

fn store_path() -> Option<&'static std::path::PathBuf> {
    STORE_PATH.get()
}

/// 初始化：从磁盘载入 `app-store.json`。文件不存在或损坏时以空表继续
/// （与旧实现一致——存储层故障不应阻断启动）。
pub fn init() {
    let Ok(dir) = crate::services::get_data_directory() else {
        return;
    };
    let path = dir.join("app-store.json");
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, Value>>(&content) {
            *STORE.lock().unwrap_or_else(|e| e.into_inner()) = map;
        }
    }
    let _ = STORE_PATH.set(path);
}

/// 全量落盘。序列化在持锁之前完成，避免磁盘 IO 拖长临界区。
fn persist() {
    let Some(path) = store_path() else {
        return;
    };
    let snapshot = {
        let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.clone()
    };
    let Ok(json) = serde_json::to_string_pretty(&snapshot) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, json);
}

pub fn get<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub fn set<T: serde::Serialize>(key: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_value(value).map_err(|e| e.to_string())?;
    {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.insert(key.to_string(), json);
    }
    persist();
    Ok(())
}

pub fn delete(key: &str) -> Result<(), String> {
    {
        let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        store.remove(key);
    }
    persist();
    Ok(())
}

pub fn has(key: &str) -> bool {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.contains_key(key)
}

pub fn keys() -> Vec<String> {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 设置-读取-删除的往返语义
    #[test]
    fn set_get_delete_round_trip() {
        set("__test_key", &"hello").unwrap();
        assert!(has("__test_key"));
        let value: Option<String> = get("__test_key");
        assert_eq!(value.as_deref(), Some("hello"));
        delete("__test_key").unwrap();
        assert!(!has("__test_key"));
    }

    // 读取不存在的键返回 None，而不是 panic
    #[test]
    fn get_missing_key_returns_none() {
        let value: Option<String> = get("__definitely_missing_key");
        assert!(value.is_none());
    }
}

// 文件盒的持久化层
//
// 复用 services::store 把多个 shelf 的暂存队列、目标设备、窗口几何写入 app-store.json。
// task 状态属于运行期数据，不在持久化范围。

use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex, MutexGuard};

const STATE_KEY: &str = "transfer_shelf_state";

// storage 写入口串行化锁:load→改→save 之间无内部事务语义,多窗口
// (文件盒拖文件 / 收件盒加入 / 挪窗几何)并发写同一 key 时整块覆盖
// 互相丢数据。锁放 storage 层一处覆盖全部写入口,调用方无需各自持锁。
static MUTATION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn lock_mutation() -> MutexGuard<'static, ()> {
    MUTATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShelfFilePersisted {
    pub path: String,
    #[serde(default)]
    pub added_at_ms: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShelfGeometryPersisted {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShelfPersisted {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub files: Vec<ShelfFilePersisted>,
    #[serde(default)]
    pub selected_peer_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShelfStatePersisted {
    #[serde(default)]
    pub shelves: Vec<ShelfPersisted>,
    #[serde(default)]
    pub geometries: std::collections::HashMap<String, ShelfGeometryPersisted>,
    #[serde(default)]
    pub name_counter: u32,
}

pub fn load() -> ShelfStatePersisted {
    crate::services::store::get::<ShelfStatePersisted>(STATE_KEY).unwrap_or_default()
}

pub fn save(state: &ShelfStatePersisted) -> Result<(), String> {
    crate::services::store::set(STATE_KEY, state)
}

/// 写入或更新单个 shelf 的暂存数据。
pub fn upsert_shelf(shelf: ShelfPersisted) -> Result<(), String> {
    let _guard = lock_mutation();
    let mut state = load();
    match state.shelves.iter_mut().find(|item| item.id == shelf.id) {
        Some(existing) => *existing = shelf,
        None => state.shelves.push(shelf),
    }
    save(&state)
}

pub fn remove_shelf(id: &str) -> Result<(), String> {
    let _guard = lock_mutation();
    let mut state = load();
    let before = state.shelves.len();
    state.shelves.retain(|item| item.id != id);
    state.geometries.remove(id);
    if state.shelves.len() == before && !state.geometries.contains_key(id) {
        return Ok(());
    }
    save(&state)
}

pub fn upsert_geometry(id: &str, geometry: ShelfGeometryPersisted) -> Result<(), String> {
    let _guard = lock_mutation();
    let mut state = load();
    state.geometries.insert(id.to_string(), geometry);
    save(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/transfer_shelf/storage.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取文件盒存储源码失败")
    }

    fn stripped_source() -> String {
        storage_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 文件盒并发写护栏(C3):三个写入口(upsert_shelf/remove_shelf/
    // upsert_geometry)必须各自先取 mutation 锁再做 load→改→save,
    // 否则多窗口并发写同一 key 整块覆盖互相丢数据。
    // 锁获取必须早于 load() 调用(临界区开头)。
    #[test]
    fn shelf_mutations_are_serialized_by_lock() {
        let src = stripped_source();
        for fn_name in ["upsert_shelf", "remove_shelf", "upsert_geometry"] {
            let pos = src
                .find(&format!("pub fn {fn_name}"))
                .unwrap_or_else(|| panic!("缺 {fn_name}"));
            let tail = &src[pos..];
            let end = tail.find("\npub fn ").unwrap_or(tail.len());
            let body = &tail[..end];
            assert!(
                body.contains("lock_mutation()"),
                "{fn_name} 必须取 mutation 锁(防并发整块覆盖)"
            );
            let lock_pos = body.find("lock_mutation()").expect("缺锁调用");
            let load_pos = body.find("load()").unwrap_or(usize::MAX);
            assert!(
                lock_pos < load_pos,
                "{fn_name} 锁获取必须先于 load()(临界区开头)"
            );
        }
    }
}


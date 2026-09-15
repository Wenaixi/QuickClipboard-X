use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::services::webdav_sync::types::{CloudGroup, CloudRecord};

// r6-sync-1 护栏:since 过滤必须 >=(含等)——拉取端 since=本地 MAX(updated_at),
// 严格大于会确定性漏掉对端 updated_at == MAX 的同秒记录(详见
// list_history_records_since 注释)。拉取端 max 锚点与其必须配对,否则 >= 改为
// 严格大于又回到同秒漏拉。§10.3 铁律,当场反证见红。
#[cfg(test)]
mod lan_same_second_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    #[test]
    fn since_filters_are_inclusive_of_local_max() {
        let src = strip_line_comments(&source_file("src/services/sync_transfer/lan/snapshot.rs"));
        for fn_name in ["list_history_records_since", "list_favorite_records_since"] {
            let body = fn_body(&src, fn_name);
            assert!(
                body.contains("record.updated_at >= since_updated_at"),
                "{} 必须 >= 过滤(含等),否则同秒新增记录永不补拉",
                fn_name
            );
            assert!(
                !body.contains("record.updated_at > since_updated_at"),
                "{} 禁止严格大于过滤(同秒漏拉)",
                fn_name
            );
        }
    }

    #[test]
    fn pull_anchor_stays_max_updated_at() {
        let src = strip_line_comments(&source_file("src/services/database/mod.rs"));
        for (fn_name, table) in [
            ("lan_local_history_max_updated_at", "FROM clipboard"),
            ("lan_local_favorites_max_updated_at", "FROM favorites"),
        ] {
            let body = fn_body(&src, fn_name);
            assert!(
                body.contains(&format!("MAX(updated_at)")) && body.contains(table),
                "{} 锚点必须仍是 MAX(updated_at)({} 表)",
                fn_name,
                table
            );
            assert!(
                !body.contains("MAX(updated_at) - 1"),
                "{} 锚点不得 -1(改锚点会把 MAX 本秒也漏掉)",
                fn_name
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanSyncSnapshot {
    pub device_id: String,
    pub history_states: HashMap<String, i64>,
    pub favorite_states: HashMap<String, i64>,
    pub groups: Vec<CloudGroup>,
    #[serde(default)]
    pub tombstone_states: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanRecordBatch {
    pub collection: String,
    pub records: Vec<CloudRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanGroupBatch {
    pub groups: Vec<CloudGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanTombstoneBatch {
    pub tombstones: Vec<crate::services::database::SyncTombstone>,
}

pub fn snapshot() -> Result<LanSyncSnapshot, String> {
    let device_id = super::runtime::device_id();
    Ok(LanSyncSnapshot {
        device_id: device_id.clone(),
        history_states: crate::services::database::webdav_history_record_states()?,
        favorite_states: crate::services::database::webdav_favorite_record_states()?,
        groups: crate::services::database::webdav_list_groups(&device_id)?,
        tombstone_states: crate::services::database::sync_tombstone_states()?,
    })
}

pub fn list_history_records_since(since_updated_at: Option<i64>) -> Result<LanRecordBatch, String> {
    let device_id = super::runtime::device_id();
    let mut records = crate::services::database::webdav_list_history_records(&device_id)?;
    // r6-sync-1:过滤必须 >=(含等)——拉取端 since 传本地 MAX(updated_at),
    // 严格大于会漏掉对端 updated_at 恰好等于该 MAX 的记录(同秒新增/更新),
    // 且本地 MAX 不变时该记录永不补拉。>= 只重传 MAX 那一秒的记录,upsert
    // 幂等(updated_at >= 现存即 skip),changed 计数不受影响,带宽可接受。
    if let Some(since_updated_at) = since_updated_at {
        records.retain(|record| record.updated_at >= since_updated_at);
    }
    let records = crate::services::database::filter_records_not_deleted(
        crate::services::database::COLLECTION_HISTORY,
        &records,
    )?;
    Ok(LanRecordBatch {
        collection: "history".to_string(),
        records,
    })
}

pub fn list_favorite_records_since(since_updated_at: Option<i64>) -> Result<LanRecordBatch, String> {
    let device_id = super::runtime::device_id();
    let mut records = crate::services::database::webdav_list_favorite_records(&device_id)?;
    // r6-sync-1:同上,>= 含等过滤,同秒记录不再漏拉。
    if let Some(since_updated_at) = since_updated_at {
        records.retain(|record| record.updated_at >= since_updated_at);
    }
    let records = crate::services::database::filter_records_not_deleted(
        crate::services::database::COLLECTION_FAVORITES,
        &records,
    )?;
    Ok(LanRecordBatch {
        collection: "favorites".to_string(),
        records,
    })
}

pub fn list_groups() -> Result<LanGroupBatch, String> {
    let device_id = super::runtime::device_id();
    let groups = crate::services::database::webdav_list_groups(&device_id)?;
    Ok(LanGroupBatch {
        groups: crate::services::database::filter_groups_not_deleted(&groups)?,
    })
}

pub fn list_tombstones_since(since_deleted_at: Option<i64>) -> Result<LanTombstoneBatch, String> {
    Ok(LanTombstoneBatch {
        tombstones: crate::services::database::list_sync_tombstones_since(since_deleted_at)?,
    })
}

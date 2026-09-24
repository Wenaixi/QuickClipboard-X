use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const CHUNK_RECORD_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct WebdavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavStatus {
    pub enabled: bool,
    pub configured: bool,
    pub auto_push: bool,
    pub auto_pull: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReport {
    pub pushed: u32,
    pub pulled: u32,
    pub errors: Vec<String>,
    pub pushed_clipboard: u32,
    pub pushed_favorites: u32,
    pub pushed_groups: u32,
    pub pulled_clipboard: u32,
    pub pulled_favorites: u32,
    pub pulled_groups: u32,
    pub pushed_items: Vec<SyncReportItem>,
    pub pulled_items: Vec<SyncReportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReportItem {
    pub category: String,
    pub id: String,
    pub summary: String,
    pub source_device_id: String,
    pub updated_at: i64,
}

impl CloudRecord {
    pub fn report_item(&self, category: &str) -> SyncReportItem {
        SyncReportItem {
            category: category.to_string(),
            id: self.uuid.clone(),
            summary: summarize_record(self),
            source_device_id: self.source_device_id.clone(),
            updated_at: self.updated_at,
        }
    }
}

fn summarize_record(record: &CloudRecord) -> String {
    let raw = if !record.title.trim().is_empty() {
        record.title.trim()
    } else {
        record.content.trim()
    };

    let mut summary = raw.chars().take(40).collect::<String>();
    if raw.chars().count() > 40 {
        summary.push('…');
    }
    summary
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncIndex {
    pub entries: HashMap<String, SyncIndexEntry>,
    pub next_chunk: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncIndexEntry {
    pub chunk: u32,
    pub updated_at: i64,
    pub source_device_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordChunk {
    pub records: HashMap<String, CloudRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudRecord {
    pub uuid: String,
    pub source_device_id: String,
    #[serde(default)]
    pub is_remote: bool,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_content: Option<String>,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_icon_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_count: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_group_name")]
    pub group_name: String,
    #[serde(default)]
    pub item_order: i64,
    #[serde(default)]
    pub paste_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct CloudRecordMeta {
    pub uuid: String,
    pub updated_at: i64,
    pub image_id: Option<String>,
}

fn default_group_name() -> String {
    "全部".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupList {
    pub groups: Vec<CloudGroup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TombstoneList {
    #[serde(default)]
    pub tombstones: Vec<crate::services::database::SyncTombstone>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageFileIndex {
    #[serde(default)]
    pub images: HashMap<String, ImageFileIndexEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageFileIndexEntry {
    pub uploaded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudGroup {
    pub name: String,
    pub icon: String,
    pub color: String,
    pub order: i32,
    pub source_device_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum SyncCollection {
    History,
    Favorites,
}

impl SyncCollection {
    pub fn dir(self) -> &'static str {
        match self {
            SyncCollection::History => "history",
            SyncCollection::Favorites => "favorites",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 摘要:优先取标题,标题空时取内容;超 40 字符截断加省略号。
    #[test]
    fn report_summary_prefers_title_then_content_with_ellipsis() {
        let record = CloudRecord {
            uuid: "u1".to_string(),
            source_device_id: "d1".to_string(),
            is_remote: false,
            content: "很长很长很长很长很长很长很长很长很长很长很长很长的内容".to_string(),
            html_content: None,
            content_type: "text".to_string(),
            image_id: None,
            source_app: None,
            source_icon_hash: None,
            char_count: None,
            title: "短标题".to_string(),
            group_name: "全部".to_string(),
            item_order: 0,
            paste_count: 0,
            created_at: 0,
            updated_at: 0,
        };
        let item = record.report_item("clipboard");
        assert_eq!(item.category, "clipboard");
        assert_eq!(item.id, "u1");
        assert_eq!(item.summary, "短标题", "标题非空必须优先取标题");

        let mut no_title = record;
        no_title.title = String::new();
        let item = no_title.report_item("favorites");
        assert!(
            item.summary.chars().next().unwrap() != '短',
            "标题空时摘要取内容"
        );
        assert!(
            item.summary.chars().count() <= 41,
            "超长内容摘要必须截断到 40 字符 + 省略号"
        );
        assert!(item.summary.ends_with('…'), "截断后必须带省略号");
    }

    // 摘要:40 字符以内的内容原样返回,不加省略号。
    #[test]
    fn report_summary_keeps_short_content_without_ellipsis() {
        let record = CloudRecord {
            uuid: "u2".to_string(),
            source_device_id: "d1".to_string(),
            is_remote: false,
            content: "短内容".to_string(),
            html_content: None,
            content_type: "text".to_string(),
            image_id: None,
            source_app: None,
            source_icon_hash: None,
            char_count: None,
            title: String::new(),
            group_name: "全部".to_string(),
            item_order: 0,
            paste_count: 0,
            created_at: 0,
            updated_at: 0,
        };
        let item = record.report_item("clipboard");
        assert_eq!(item.summary, "短内容", "40 字符内内容不截断不加省略号");
    }

    // CloudRecord 序列化:Option 空字段 skip_serializing(体积精简),
    // 反序列化时缺省字段回落默认值(兼容旧记录)。
    #[test]
    fn cloud_record_round_trip_skips_empty_options() {
        let record = CloudRecord {
            uuid: "u3".to_string(),
            source_device_id: "d1".to_string(),
            is_remote: false,
            content: "x".to_string(),
            html_content: None,
            content_type: "text".to_string(),
            image_id: None,
            source_app: None,
            source_icon_hash: None,
            char_count: None,
            title: String::new(),
            group_name: String::new(),
            item_order: 0,
            paste_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(
            !json.contains("html_content"),
            "None 字段必须 skip_serializing,不写进 JSON"
        );
        assert!(!json.contains("title"), "空字符串 title 不应写进 JSON");

        let restored: CloudRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.uuid, "u3");
        assert_eq!(restored.updated_at, 2);
        assert_eq!(restored.group_name, "全部", "缺省 group_name 回落默认值");
    }

    // SyncCollection 目录映射是同步路径的稳定锚点。
    #[test]
    fn sync_collection_dir_mapping_is_stable() {
        assert_eq!(SyncCollection::History.dir(), "history");
        assert_eq!(SyncCollection::Favorites.dir(), "favorites");
    }
}

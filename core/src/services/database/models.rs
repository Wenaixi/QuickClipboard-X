use serde::{Deserialize, Serialize};

// 剪贴板项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device_id: Option<String>,
    pub is_remote: bool,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_content: Option<String>,
    pub content_type: String,  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    pub item_order: i64,
    pub is_pinned: bool,
    pub paste_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,       
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_icon_hash: Option<String>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_count: Option<i64>,
    pub created_at: i64,  
    pub updated_at: i64, 
}

// 剪贴板原始格式数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardDataItem {
    pub id: i64,
    pub target_kind: String,
    pub target_id: String,
    pub format_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw_data: Vec<u8>,
    pub is_primary: bool,
    pub format_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

// 剪贴板原始格式写入项
#[derive(Debug, Clone)]
pub struct ClipboardDataSeed {
    pub format_name: String,
    pub raw_data: Vec<u8>,
    pub is_primary: bool,
    pub format_order: i64,
}

// 可粘贴选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteOption {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_format_name: Option<String>,
    pub is_primary: bool,
}

// 收藏项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteItem {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_content: Option<String>,
    pub content_type: String,  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    pub group_name: String,
    pub item_order: i64,
    pub paste_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_count: Option<i64>,
    pub created_at: i64,  
    pub updated_at: i64, 
}

// 分组信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub icon: String,
    pub color: String,
    pub order: i32,
    pub item_count: i32,
}

// 分页查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    // 总记录数
    pub total_count: i64,
    // 当前页数据
    pub items: Vec<T>,
    // 偏移量
    pub offset: i64,
    // 每页数量
    pub limit: i64,
    // 是否还有更多数据
    pub has_more: bool,
}

impl<T> PaginatedResult<T> {
    pub fn new(total_count: i64, items: Vec<T>, offset: i64, limit: i64) -> Self {
        let items_len = items.len() as i64;
        let has_more = offset + items_len < total_count;
        Self {
            total_count,
            items,
            offset,
            limit,
            has_more,
        }
    }
}

// 查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    // 偏移量
    pub offset: i64,
    // 每页数量
    pub limit: i64,
    // 搜索关键词（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    // 内容类型过滤（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub paste_status: Option<String>,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 50,
            search: None,
            content_type: None,
            paste_status: None,
        }
    }
}

// 收藏查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoritesQueryParams {
    // 偏移量
    pub offset: i64,
    // 每页数量
    pub limit: i64,
    // 分组名称（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    // 搜索关键词（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    // 内容类型过滤（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub paste_status: Option<String>,
}

impl Default for FavoritesQueryParams {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 50,
            group_name: None,
            search: None,
            content_type: None,
            paste_status: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // has_more:当且仅当 offset + 本页条数 < 总数时为真。
    // 分页的"是否还有下一页"判定的唯一决策点。
    #[test]
    fn paginated_has_more_is_true_only_when_more_pages_exist() {
        // 5 条总数,offset=0 取 3 条 → 还有 2 条
        let r = PaginatedResult::new(5, vec![1i32, 2, 3], 0, 3);
        assert!(r.has_more);
        // 最后一页正好取满(offset=3 取 2 条,总数 5) → 无更多
        let r = PaginatedResult::new(5, vec![4i32, 5], 3, 3);
        assert!(!r.has_more, "末页必须 has_more=false");
        // 空数据:第一条分页即空 → 无更多
        let r = PaginatedResult::new(0, Vec::<i32>::new(), 0, 50);
        assert!(!r.has_more);
    }

    // PaginatedResult 只读使用 new 构造,字段暴露为公开但仿真构造
    // 一致性:offset/limit/total_count 原样透传。
    #[test]
    fn paginated_result_passthrough_fields() {
        let r = PaginatedResult::new(100, vec!["a".to_string()], 0, 10);
        assert_eq!(r.total_count, 100);
        assert_eq!(r.offset, 0);
        assert_eq!(r.limit, 10);
        assert_eq!(r.items.len(), 1);
    }

    // QueryParams 默认值:首页 50 条、无过滤、无搜索。
    #[test]
    fn query_params_defaults_to_first_page_50() {
        let p = QueryParams::default();
        assert_eq!(p.offset, 0);
        assert_eq!(p.limit, 50);
        assert!(p.search.is_none());
        assert!(p.content_type.is_none());
        assert!(p.paste_status.is_none());
    }

    // FavoritesQueryParams 默认值:首页 50 条、无分组/搜索/过滤。
    #[test]
    fn favorites_query_params_defaults_to_all_groups_first_page() {
        let p = FavoritesQueryParams::default();
        assert_eq!(p.offset, 0);
        assert_eq!(p.limit, 50);
        assert!(p.group_name.is_none());
        assert!(p.search.is_none());
        assert!(p.content_type.is_none());
    }
}


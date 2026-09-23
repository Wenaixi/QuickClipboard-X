use super::models::{ClipboardDataItem, ClipboardDataSeed, ClipboardItem, PaginatedResult, QueryParams};
use super::connection::{with_connection, MAX_CONTENT_LENGTH};
use crate::services::webdav_sync::types::{CloudRecord, CloudRecordMeta};
use crate::utils::{is_textual_content_type, truncate_string, truncate_around_keyword, truncate_html, calculate_char_count};
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use chrono;
use uuid::Uuid;

// 字符数补齐后台线程的单飞守卫
static CHAR_COUNT_UPDATER_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

pub fn save_clipboard_data_items(
    target_kind: &str,
    target_id: &str,
    items: &[ClipboardDataSeed],
) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }

    with_connection(|conn| {
        let now = chrono::Local::now().timestamp();
        let tx = conn.unchecked_transaction()?;

        for item in items {
            tx.execute(
                "INSERT INTO clipboard_data (
                    target_kind, target_id, format_name, raw_data,
                    is_primary, format_order, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(target_kind, target_id, format_name)
                 DO UPDATE SET
                    raw_data = excluded.raw_data,
                    is_primary = excluded.is_primary,
                    format_order = excluded.format_order,
                    updated_at = excluded.updated_at",
                params![
                    target_kind,
                    target_id,
                    item.format_name,
                    item.raw_data,
                    if item.is_primary { 1 } else { 0 },
                    item.format_order,
                    now,
                    now,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    })
}

fn get_clipboard_data_items_by_target(
    target_kind: &str,
    target_id: &str,
) -> Result<Vec<ClipboardDataItem>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, target_kind, target_id, format_name, raw_data, is_primary, format_order, created_at, updated_at
             FROM clipboard_data
             WHERE target_kind = ?1 AND target_id = ?2
             ORDER BY format_order ASC, id ASC",
        )?;

        let items = stmt
            .query_map(params![target_kind, target_id], |row| {
                Ok(ClipboardDataItem {
                    id: row.get(0)?,
                    target_kind: row.get(1)?,
                    target_id: row.get(2)?,
                    format_name: row.get(3)?,
                    raw_data: row.get(4)?,
                    is_primary: row.get::<_, i64>(5)? != 0,
                    format_order: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(items)
    })
}

pub fn get_clipboard_data_items(
    target_kind: &str,
    target_id: &str,
) -> Result<Vec<ClipboardDataItem>, String> {
    get_clipboard_data_items_by_target(target_kind, target_id)
}

pub fn delete_clipboard_data_items(target_kind: &str, target_id: &str) -> Result<(), String> {
    with_connection(|conn| {
        conn.execute(
            "DELETE FROM clipboard_data WHERE target_kind = ?1 AND target_id = ?2",
            params![target_kind, target_id],
        )?;
        Ok(())
    })
}

pub fn delete_clipboard_data_items_by_kind(target_kind: &str) -> Result<(), String> {
    with_connection(|conn| {
        conn.execute(
            "DELETE FROM clipboard_data WHERE target_kind = ?1",
            params![target_kind],
        )?;
        Ok(())
    })
}

// 异步更新缺失的字符数
pub fn update_missing_char_counts(items: Vec<(i64, String, String)>) {
    if items.is_empty() { return; }
    // 单飞守卫:翻页过程中缺失 char_count 的行会不断经 query 收集到这里,
    // 每页都 spawn 一个后台线程会堆积成串行抢 DB 锁的线程群。一次只允许
    // 一个在飞线程处理,其余调用直接放弃(下次翻页仍会补齐,语义无损)。
    if CHAR_COUNT_UPDATER_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let _ = with_connection(|conn| {
            for (id, content, content_type) in items {
                if let Some(char_count) = calculate_char_count(&content, &content_type) {
                    conn.execute(
                        "UPDATE clipboard SET char_count = ?1 WHERE id = ?2",
                        params![char_count, id],
                    )?;
                }
            }
            Ok(())
        });
        CHAR_COUNT_UPDATER_IN_FLIGHT.store(false, Ordering::SeqCst);
    });
}

// 按逗号拆分图片ID,丢弃空段。拆分出的 id 后续会被白名单校验,路径穿越段不会进入文件操作
fn split_image_ids(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

// 检查图片ID是否仍被 clipboard 或 favorites 引用
fn is_image_id_referenced(conn: &rusqlite::Connection, image_id: &str) -> Result<bool, rusqlite::Error> {
    let exact = image_id;
    let p1 = format!("{},%", image_id);
    let p2 = format!("%,{},%", image_id);
    let p3 = format!("%,{}", image_id);

    let q = |table: &str| -> Result<bool, rusqlite::Error> {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE image_id = ?1 OR image_id LIKE ?2 OR image_id LIKE ?3 OR image_id LIKE ?4)",
            table
        );
        let exists: i64 = conn.query_row(&sql, params![exact, p1, p2, p3], |row| row.get(0))?;
        Ok(exists != 0)
    };

    Ok(q("clipboard")? || q("favorites")?)
}

// 删除图片文件。id 需先通过白名单校验,否则恶意 `../` 会被拒之门外
fn delete_image_files(image_ids: Vec<String>) -> Result<(), String> {
    if image_ids.is_empty() { return Ok(()); }
    let data_dir = crate::services::get_data_directory()?;
    let images_dir = data_dir.join("clipboard_images");
    for iid in image_ids {
        if !crate::services::webdav_sync::image_id::is_valid_image_id(&iid) {
            continue;
        }
        let p = images_dir.join(format!("{}.png", iid));
        if p.exists() {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(())
}

// 分页查询剪贴板历史
pub fn query_clipboard_items(params: QueryParams) -> Result<PaginatedResult<ClipboardItem>, String> {
    let search_keyword = params.search.clone();
    let has_filter = search_keyword.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
        || params.content_type.as_ref().map(|t| t != "all").unwrap_or(false)
        || params.paste_status.as_ref().map(|s| s.split(',').any(|v| v.trim() == "pasted" || v.trim() == "unpasted")).unwrap_or(false);
    
    with_connection(|conn| {
        // where_clauses 改 Vec<String>,不再 Box::leak 泄漏字符串
        let mut where_clauses = vec![];
        let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(ref search) = search_keyword {
            if !search.trim().is_empty() {
                where_clauses.push("content LIKE ? ESCAPE '\\'".to_string());
                let search_pattern = super::like_pattern(search);
                query_params.push(Box::new(search_pattern));
            }
        }

        if let Some(ref content_type) = params.content_type {
            let types: Vec<_> = content_type.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
            if !types.is_empty() && content_type != "all" {
                // 与搜索词路径一致:转义 %/_/\ + ESCAPE,避免 content_type 含通配符时误匹配(安全修复)
                let clauses = types.iter().map(|_| "content_type LIKE ? ESCAPE '\\'").collect::<Vec<_>>().join(" OR ");
                where_clauses.push(format!("({})", clauses));
                for content_type in types {
                    let pattern = if content_type == "text" { super::like_pattern("text") } else { super::like_pattern(content_type) };
                    query_params.push(Box::new(pattern));
                }
            }
        }
        if let Some(ref paste_status) = params.paste_status {
            let statuses: Vec<_> = paste_status.split(',').map(str::trim).filter(|status| *status == "pasted" || *status == "unpasted").collect();
            if statuses.len() == 1 {
                where_clauses.push(if statuses[0] == "pasted" { "paste_count > 0".to_string() } else { "paste_count = 0".to_string() });
            }
        }
        
        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };
        
        let total_count: i64 = if has_filter {
            let count_sql = format!("SELECT COUNT(*) FROM clipboard {}", where_clause);
            // COUNT 直接绑定 query_params 的真实值,不得先经 to_sql() 反射式
            // 深拷贝——旧实现对非文本参数(如 _sortId 深拷贝的数字项)会折叠成
            // 空串,让 COUNT 的 WHERE 比主查询的 WHERE 收到不同的值,分页总数
            // 与列表语义脱节。COUNT 执行时 query_params 只含 WHERE 文本参数,
            // 直接借用引用即可,limit/offset 在 count 之后才 push,互不干扰。
            conn.query_row(
                &count_sql,
                rusqlite::params_from_iter(query_params.iter().map(|p| p.as_ref())),
                |row| row.get(0)
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM clipboard", [], |row| row.get(0))?
        };
        
        if total_count == 0 {
            return Ok(PaginatedResult::new(0, vec![], params.offset, params.limit));
        }
        
        let query_sql = format!(
            "SELECT id, uuid, source_device_id, is_remote, content, html_content, content_type, image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash, created_at, updated_at, char_count,
                    (SELECT id FROM favorites WHERE source_clipboard_uuid = clipboard.uuid LIMIT 1) AS favorite_id
             FROM clipboard 
             {} 
             ORDER BY is_pinned DESC, item_order DESC, updated_at DESC 
             LIMIT ? OFFSET ?",
            where_clause
        );
        
        query_params.push(Box::new(params.limit));
        query_params.push(Box::new(params.offset));
        
        let mut stmt = conn.prepare(&query_sql)?;

        let mut items_to_update: Vec<(i64, String, String)> = vec![];
        
        let items = stmt.query_map(
            rusqlite::params_from_iter(query_params.iter().map(|p| p.as_ref())),
            |row| {
                let id: i64 = row.get(0)?;
                let uuid: Option<String> = row.get(1)?;
                let source_device_id: Option<String> = row.get(2)?;
                let is_remote: i64 = row.get(3)?;
                let content: String = row.get(4)?;
                let html_content: Option<String> = row.get(5)?;
                let content_type: String = row.get(6)?;
                let char_count: Option<i64> = row.get(15)?;
                
                let (truncated_content, truncated_html) = if is_textual_content_type(&content_type) {
                    let truncated_content = if content.len() > MAX_CONTENT_LENGTH {
                        if let Some(ref keyword) = search_keyword {
                            if !keyword.trim().is_empty() {
                                truncate_around_keyword(content.clone(), keyword, MAX_CONTENT_LENGTH)
                            } else {
                                truncate_string(content.clone(), MAX_CONTENT_LENGTH)
                            }
                        } else {
                            truncate_string(content.clone(), MAX_CONTENT_LENGTH)
                        }
                    } else {
                        content.clone()
                    };
                    
                    let truncated_html = html_content.map(|h| {
                        if h.len() > MAX_CONTENT_LENGTH {
                            truncate_html(h, MAX_CONTENT_LENGTH)
                        } else {
                            h
                        }
                    });
                    
                    (truncated_content, truncated_html)
                } else {
                    (content.clone(), html_content)
                };

                let calculated_char_count = calculate_char_count(&content, &content_type);
                let needs_update = char_count.is_none() && calculated_char_count.is_some();
                let final_char_count = char_count.or(calculated_char_count);
                
                Ok((ClipboardItem {
                    id,
                    uuid,
                    favorite_id: row.get(16)?,
                    source_device_id,
                    is_remote: is_remote != 0,
                    content: truncated_content,
                    html_content: truncated_html,
                    content_type: content_type.clone(),
                    image_id: row.get(7)?,
                    item_order: row.get(8)?,
                    is_pinned: row.get::<_, i64>(9)? != 0,
                    paste_count: row.get(10)?,
                    source_app: row.get(11)?,
                    source_icon_hash: row.get(12)?,
                    char_count: final_char_count,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                }, char_count.is_none() && calculated_char_count.is_some(), id, content, content_type))
            }
        )?
        .collect::<Result<Vec<_>, _>>()?;
        
        let mut result_items = vec![];
        for (item, needs_update, id, content, content_type) in items {
            if needs_update {
                items_to_update.push((id, content, content_type));
            }
            result_items.push(item);
        }

        if !items_to_update.is_empty() {
            update_missing_char_counts(items_to_update);
        }
        
        Ok(PaginatedResult::new(total_count, result_items, params.offset, params.limit))
    })
}

pub fn webdav_list_history_records(device_id: &str) -> Result<Vec<CloudRecord>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, uuid, source_device_id, is_remote, content, html_content, content_type,
                    image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash,
                    char_count, created_at, updated_at
             FROM clipboard
             ORDER BY item_order DESC, updated_at DESC, id DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let uuid_opt: Option<String> = row.get(1)?;
            let uuid = uuid_opt.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| id.to_string());
            let source_device_id = row
                .get::<_, Option<String>>(2)?
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| device_id.to_string());

            Ok(CloudRecord {
                uuid,
                source_device_id,
                is_remote: row.get::<_, i64>(3)? != 0,
                content: row.get(4)?,
                html_content: row.get(5)?,
                content_type: row.get(6)?,
                image_id: row.get(7)?,
                item_order: row.get(8)?,
                paste_count: row.get(10)?,
                source_app: row.get(11)?,
                source_icon_hash: row.get(12)?,
                char_count: row.get(13)?,
                title: String::new(),
                group_name: "全部".to_string(),
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        })?;

        Ok(rows.filter_map(|row| row.ok()).collect())
    })
}

pub fn webdav_list_history_record_metas() -> Result<Vec<CloudRecordMeta>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, uuid, updated_at, image_id
             FROM clipboard
             ORDER BY item_order DESC, updated_at DESC, id DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let uuid_opt: Option<String> = row.get(1)?;
            let uuid = uuid_opt.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| id.to_string());
            Ok(CloudRecordMeta {
                uuid,
                updated_at: row.get(2)?,
                image_id: row.get(3)?,
            })
        })?;

        Ok(rows.filter_map(|row| row.ok()).collect())
    })
}

pub fn webdav_get_history_record_by_uuid(uuid: &str, device_id: &str) -> Result<Option<CloudRecord>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, uuid, source_device_id, is_remote, content, html_content, content_type,
                    image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash,
                    char_count, created_at, updated_at
             FROM clipboard
             WHERE uuid = ?1 OR ((uuid IS NULL OR uuid = '') AND id = ?2)
             LIMIT 1",
        )?;
        let id = uuid.parse::<i64>().ok();
        let record = stmt.query_row(params![uuid, id], |row| {
            let id: i64 = row.get(0)?;
            let uuid_opt: Option<String> = row.get(1)?;
            let uuid = uuid_opt.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| id.to_string());
            let source_device_id = row
                .get::<_, Option<String>>(2)?
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| device_id.to_string());

            Ok(CloudRecord {
                uuid,
                source_device_id,
                is_remote: row.get::<_, i64>(3)? != 0,
                content: row.get(4)?,
                html_content: row.get(5)?,
                content_type: row.get(6)?,
                image_id: row.get(7)?,
                item_order: row.get(8)?,
                paste_count: row.get(10)?,
                source_app: row.get(11)?,
                source_icon_hash: row.get(12)?,
                char_count: row.get(13)?,
                title: String::new(),
                group_name: "全部".to_string(),
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        }).optional()?;

        Ok(record)
    })
}

pub fn webdav_list_own_history_records(device_id: &str) -> Result<Vec<CloudRecord>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, uuid, source_device_id, is_remote, content, html_content, content_type,
                    image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash,
                    char_count, created_at, updated_at
             FROM clipboard
             WHERE source_device_id IS NULL OR source_device_id = '' OR source_device_id = ?1
             ORDER BY item_order DESC, updated_at DESC, id DESC",
        )?;

        let rows = stmt.query_map(params![device_id], |row| {
            let id: i64 = row.get(0)?;
            let uuid_opt: Option<String> = row.get(1)?;
            let uuid = uuid_opt.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| id.to_string());

            Ok(CloudRecord {
                uuid,
                source_device_id: device_id.to_string(),
                is_remote: row.get::<_, i64>(3)? != 0,
                content: row.get(4)?,
                html_content: row.get(5)?,
                content_type: row.get(6)?,
                image_id: row.get(7)?,
                item_order: row.get(8)?,
                paste_count: row.get(10)?,
                source_app: row.get(11)?,
                source_icon_hash: row.get(12)?,
                char_count: row.get(13)?,
                title: String::new(),
                group_name: "全部".to_string(),
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        })?;

        Ok(rows.filter_map(|row| row.ok()).collect())
    })
}

pub fn webdav_history_record_states() -> Result<HashMap<String, i64>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT uuid, updated_at FROM clipboard WHERE uuid IS NOT NULL AND uuid != ''",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        let mut states = HashMap::new();
        for row in rows {
            let (uuid, updated_at) = row?;
            states.insert(uuid, updated_at);
        }
        Ok(states)
    })
}

pub fn lan_upsert_history_records(records: &[CloudRecord]) -> Result<Vec<CloudRecord>, String> {
    upsert_history_records(records, false)
}

pub fn webdav_repair_history_records(records: &[CloudRecord]) -> Result<Vec<CloudRecord>, String> {
    upsert_history_records(records, true)
}

fn upsert_history_records(records: &[CloudRecord], ignore_tombstones: bool) -> Result<Vec<CloudRecord>, String> {
    if records.is_empty() {
        return Ok(Vec::new());
    }

    with_connection(|conn| {
        let tx = conn.unchecked_transaction()?;
        let mut changed = Vec::new();

        for record in records {
            if record.uuid.trim().is_empty() {
                continue;
            }
            // 远端 content 不可信(files: 内路径可能指向本机任意文件),
            // 写库前净化,不放行绝对路径/含父目录段的条目
            let mut record = record.clone();
            if record.content_type == "file" || record.content_type == "image" {
                record.content = crate::services::sanitize_remote_files_content(&record.content);
            }
            let tombstone_deleted_at = super::tombstones::sync_tombstone_deleted_at_in_conn(
                &tx,
                super::tombstones::COLLECTION_HISTORY,
                &record.uuid,
            )?;
            if !ignore_tombstones && tombstone_deleted_at.map(|value| value >= record.updated_at).unwrap_or(false) {
                continue;
            }
            let restored_updated_at = if ignore_tombstones {
                super::tombstones::restored_record_updated_at(record.updated_at, tombstone_deleted_at)
            } else {
                record.updated_at
            };

            let existing = tx
                .query_row(
                    "SELECT COALESCE(source_device_id, ''), updated_at, content, html_content, content_type,
                            image_id, item_order, paste_count, source_app, source_icon_hash, char_count, created_at
                     FROM clipboard
                     WHERE uuid = ?1 OR ((uuid IS NULL OR uuid = '') AND CAST(id AS TEXT) = ?2) LIMIT 1",
                    params![record.uuid, record.uuid],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, Option<String>>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, Option<i64>>(10)?,
                            row.get::<_, i64>(11)?,
                        ))
                    },
                )
                .optional()?;

            if let Some((
                source_device_id,
                updated_at,
                content,
                html_content,
                content_type,
                image_id,
                item_order,
                paste_count,
                source_app,
                source_icon_hash,
                char_count,
                created_at,
            )) = existing {
                let same = source_device_id == record.source_device_id
                    && updated_at == restored_updated_at
                    && content == record.content
                    && html_content == record.html_content
                    && content_type == record.content_type
                    && image_id == record.image_id
                    && item_order == record.item_order
                    && paste_count == record.paste_count
                    && source_app == record.source_app
                    && source_icon_hash == record.source_icon_hash
                    && char_count == record.char_count
                    && created_at == record.created_at;

                if updated_at >= restored_updated_at || same {
                    if tombstone_deleted_at.map(|deleted_at| deleted_at < updated_at).unwrap_or(false) {
                        super::tombstones::delete_sync_tombstone_in_conn(
                            &tx,
                            super::tombstones::COLLECTION_HISTORY,
                            &record.uuid,
                        )?;
                    }
                    continue;
                }

                tx.execute(
                    "UPDATE clipboard SET
                        uuid = ?1,
                        source_device_id = ?2,
                        is_remote = 1,
                        content = ?3,
                        html_content = ?4,
                        content_type = ?5,
                        image_id = ?6,
                        item_order = ?7,
                        paste_count = ?8,
                        source_app = ?9,
                        source_icon_hash = ?10,
                        char_count = ?11,
                        created_at = ?12,
                        updated_at = ?13
                     WHERE uuid = ?14 OR ((uuid IS NULL OR uuid = '') AND CAST(id AS TEXT) = ?15)",
                    params![
                        record.uuid,
                        record.source_device_id,
                        record.content,
                        record.html_content,
                        record.content_type,
                        record.image_id,
                        record.item_order,
                        record.paste_count,
                        record.source_app,
                        record.source_icon_hash,
                        record.char_count,
                        record.created_at,
                        restored_updated_at,
                        record.uuid,
                        record.uuid,
                    ],
                )?;
                if tombstone_deleted_at.map(|deleted_at| deleted_at < restored_updated_at).unwrap_or(false) {
                    super::tombstones::delete_sync_tombstone_in_conn(
                        &tx,
                        super::tombstones::COLLECTION_HISTORY,
                        &record.uuid,
                    )?;
                }
                let mut changed_record = record.clone();
                changed_record.updated_at = restored_updated_at;
                changed.push(changed_record);
                continue;
            }

            tx.execute(
                "INSERT INTO clipboard (
                    uuid, source_device_id, is_remote, content, html_content, content_type,
                    image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash,
                    char_count, created_at, updated_at
                 ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    record.uuid,
                    record.source_device_id,
                    record.content,
                    record.html_content,
                    record.content_type,
                    record.image_id,
                    record.item_order,
                    record.paste_count,
                    record.source_app,
                    record.source_icon_hash,
                    record.char_count,
                    record.created_at,
                    restored_updated_at,
                ],
            )?;
            if tombstone_deleted_at.map(|deleted_at| deleted_at < restored_updated_at).unwrap_or(false) {
                super::tombstones::delete_sync_tombstone_in_conn(
                    &tx,
                    super::tombstones::COLLECTION_HISTORY,
                    &record.uuid,
                )?;
            }
            let mut changed_record = record.clone();
            changed_record.updated_at = restored_updated_at;
            changed.push(changed_record);
        }

        tx.commit()?;
        Ok(changed)
    })
}


// 获取剪贴板总数
pub fn get_clipboard_count() -> Result<i64, String> {
    with_connection(|conn| {
        conn.query_row("SELECT COUNT(*) FROM clipboard", [], |row| row.get(0))
    })
}

pub fn get_clipboard_item_position(id: i64) -> Result<Option<i64>, String> {
    with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order DESC, updated_at DESC",
        )?;

        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ids.iter().position(|item_id| *item_id == id).map(|index| index as i64))
    })
}

// 根据ID获取剪贴板项（完整内容，不截断）
pub fn get_clipboard_item_by_id(id: i64) -> Result<Option<ClipboardItem>, String> {
    get_clipboard_item_by_id_with_limit(id, None)
}

pub fn ensure_clipboard_item_uuid(id: i64) -> Result<String, String> {
    let maybe_uuid: Option<String> = with_connection(|conn| {
        let existing: Option<Option<String>> = conn
            .query_row(
                "SELECT uuid FROM clipboard WHERE id = ?1 LIMIT 1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;

        let existing = existing.flatten();

        if let Some(uuid) = existing.clone().filter(|u| !u.trim().is_empty()) {
            return Ok(Some(uuid));
        }

        let new_uuid = Uuid::new_v4().to_string();
        conn.execute(
            "UPDATE clipboard SET uuid = ?1 WHERE id = ?2 AND (uuid IS NULL OR uuid = '')",
            params![new_uuid, id],
        )?;

        let uuid: Option<Option<String>> = conn
            .query_row(
                "SELECT uuid FROM clipboard WHERE id = ?1 LIMIT 1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;

        Ok(uuid.flatten())
    })?;

    maybe_uuid
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| "生成 uuid 失败".to_string())
}

pub fn get_clipboard_item_id_by_uuid(uuid: &str) -> Result<Option<i64>, String> {
    with_connection(|conn| {
        conn.query_row(
            "SELECT id FROM clipboard WHERE uuid = ?1 LIMIT 1",
            params![uuid],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.into())
    })
}

// 根据ID获取剪贴板项（指定截断长度）
pub fn get_clipboard_item_by_id_with_limit(id: i64, max_content_length: Option<usize>) -> Result<Option<ClipboardItem>, String> {
    with_connection(|conn| {
        conn.query_row(
            "SELECT id, uuid, source_device_id, is_remote, content, html_content, content_type, image_id, item_order, is_pinned, paste_count, source_app, source_icon_hash, created_at, updated_at, char_count,
                    (SELECT id FROM favorites WHERE source_clipboard_uuid = clipboard.uuid LIMIT 1) AS favorite_id
             FROM clipboard WHERE id = ?",
            params![id],
            |row| {
                let uuid: Option<String> = row.get(1)?;
                let source_device_id: Option<String> = row.get(2)?;
                let is_remote: i64 = row.get(3)?;
                let content: String = row.get(4)?;
                let html_content: Option<String> = row.get(5)?;
                let content_type: String = row.get(6)?;
                let char_count: Option<i64> = row.get(15)?;
                let final_content = if let Some(max_len) = max_content_length {
                    let is_text_type = is_textual_content_type(&content_type);
                    if is_text_type && content.len() > max_len {
                        truncate_string(content.clone(), max_len)
                    } else {
                        content.clone()
                    }
                } else {
                    content.clone()
                };
                
                // 计算字符数
                let calculated_char_count = calculate_char_count(&content, &content_type);
                let needs_update = char_count.is_none() && calculated_char_count.is_some();
                let final_char_count = char_count.or(calculated_char_count);
                
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    uuid,
                    favorite_id: row.get(16)?,
                    source_device_id,
                    is_remote: is_remote != 0,
                    content: final_content,
                    html_content,
                    content_type,
                    image_id: row.get(7)?,
                    item_order: row.get(8)?,
                    is_pinned: row.get::<_, i64>(9)? != 0,
                    paste_count: row.get(10)?,
                    source_app: row.get(11)?,
                    source_icon_hash: row.get(12)?,
                    char_count: final_char_count,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                })
            }
        )
        .optional()
        .map_err(|e| e.into())
    })
}

pub fn increment_paste_count(id: i64) -> Result<(), String> {
    with_connection(|conn| {
        conn.execute(
            "UPDATE clipboard SET paste_count = paste_count + 1 WHERE id = ?",
            params![id],
        )?;
        Ok(())
    })
}

pub fn increment_paste_counts(ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }

    with_connection(|conn| {
        let tx = conn.unchecked_transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE clipboard SET paste_count = paste_count + 1 WHERE id = ?",
                params![id],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

// 限制剪贴板历史数量（删除超出限制的旧记录）
pub fn limit_clipboard_history(max_count: u64) -> Result<(), String> {
    if max_count >= 999999 {
        return Ok(());
    }
    // 边界:max_count=0 时 NOT IN (... LIMIT 0) 是空允许集,会把全部历史
    // (含置顶项)清空。前端输入框有 min=25 归一,但命令/升级/导入等直接
    // 调用路径可能传 0,这里统一兜底:0 视为 1,至少保留一条最近记录。
    let max_count = max_count.max(1);
    
    let (images_to_delete, deleted_ids): (Vec<String>, Vec<i64>) = with_connection(|conn| {
        let sql_ids = "SELECT image_id FROM clipboard WHERE id NOT IN (SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order DESC, updated_at DESC LIMIT ?1) AND image_id IS NOT NULL AND image_id <> ''";
        let mut stmt = conn.prepare(sql_ids)?;
        let ids_iter = stmt.query_map(params![max_count], |row| row.get::<_, String>(0))?;
        let mut set: HashSet<String> = HashSet::new();
        for r in ids_iter {
            if let Ok(s) = r {
                for iid in split_image_ids(&s) {
                    set.insert(iid);
                }
            }
        }
        drop(stmt);

        // 裁剪删除必须写 tombstone——先收集待删记录的 (id, uuid),
        // 删除超出上限的记录若不通知云端/peers,切换设备会把已裁剪内容拉回,
        // 且另一端删除也无法传播删除语义。
        let mut tombstone_ids = Vec::new();
        let mut delete_ids_stmt = conn.prepare(
            "SELECT id, uuid FROM clipboard WHERE id NOT IN (
                SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order DESC, updated_at DESC LIMIT ?1
            )",
        )?;
        let mut delete_iter = delete_ids_stmt.query_map(params![max_count], |row| {
            let id: i64 = row.get(0)?;
            let uuid: Option<String> = row.get(1)?;
            Ok((id, uuid))
        })?;
        let mut deleted_ids = Vec::new();
        while let Some(Ok((id, uuid_opt))) = delete_iter.next() {
            deleted_ids.push(id);
            tombstone_ids.push(
                uuid_opt
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| id.to_string()),
            );
        }
        drop(delete_iter);
        drop(delete_ids_stmt);

        // tombstone + DELETE 包同一事务,避免崩溃时只写墓碑不删记录
        // (本地幽灵删除)或只删记录不写墓碑(云端残留)。
        let tx = conn.unchecked_transaction()?;
        let deleted_at = chrono::Local::now().timestamp();
        let local_device_id = crate::services::sync_transfer::device_id();
        for tombstone_id in &tombstone_ids {
            super::tombstones::record_sync_tombstone_in_conn(
                &tx,
                super::tombstones::COLLECTION_HISTORY,
                tombstone_id,
                &local_device_id,
                deleted_at,
            )?;
        }
        tx.execute(
            "DELETE FROM clipboard WHERE id NOT IN (
                SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order DESC, updated_at DESC LIMIT ?1
            )",
            params![max_count],
        )?;
        tx.commit()?;

        let mut to_delete = Vec::new();
        for iid in set.into_iter() {
            if !is_image_id_referenced(conn, &iid)? {
                to_delete.push(iid);
            }
        }
        Ok((to_delete, deleted_ids))
    })?;

    for id in deleted_ids {
        let _ = delete_clipboard_data_items("clipboard", &id.to_string());
    }

    delete_image_files(images_to_delete)
}

// 删除单个剪贴板项
pub fn delete_clipboard_item(id: i64) -> Result<(), String> {
    let images_to_delete: Vec<String> = with_connection(|conn| {
        let item: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT image_id, uuid FROM clipboard WHERE id = ?",
                params![id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let Some((image_ids, uuid)) = item else {
            return Ok(Vec::new());
        };
        // tombstone + DELETE 包同一事务,与 delete_clipboard_items 口径
        // 一致——否则崩溃/异常时可能只写墓碑不删记录(本地幽灵删除,云端会
        // 删它但本地还在)或只删记录不写墓碑(云端残留,裁剪同款问题)。
        let tx = conn.unchecked_transaction()?;
        let deleted_at = chrono::Local::now().timestamp();
        let tombstone_id = uuid.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| id.to_string());
        super::tombstones::record_sync_tombstone_in_conn(
            &tx,
            super::tombstones::COLLECTION_HISTORY,
            &tombstone_id,
            &crate::services::sync_transfer::device_id(),
            deleted_at,
        )?;

        tx.execute("DELETE FROM clipboard WHERE id = ?1", params![id])?;
        tx.commit()?;

        let mut to_delete = Vec::new();
        if let Some(ids) = image_ids {
            for iid in split_image_ids(&ids) {
                if !is_image_id_referenced(conn, &iid)? {
                    to_delete.push(iid);
                }
            }
        }
        Ok(to_delete)
    })?;

    let _ = delete_clipboard_data_items("clipboard", &id.to_string());
    delete_image_files(images_to_delete)
}

pub fn delete_clipboard_items(ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }

    let unique_ids: Vec<i64> = ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let images_to_delete: Vec<String> = with_connection(|conn| {
        let mut image_id_set: HashSet<String> = HashSet::new();
        let mut tombstone_ids = Vec::new();
        for id in &unique_ids {
            let item: Option<(Option<String>, Option<String>)> = conn
                .query_row(
                    "SELECT image_id, uuid FROM clipboard WHERE id = ?",
                    params![id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?;

            if let Some((image_ids, uuid)) = item {
                if let Some(image_ids) = image_ids {
                    for image_id in split_image_ids(&image_ids) {
                        image_id_set.insert(image_id);
                    }
                }
                tombstone_ids.push(uuid.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| id.to_string()));
            }
        }

        let tx = conn.unchecked_transaction()?;
        let deleted_at = chrono::Local::now().timestamp();
        let local_device_id = crate::services::sync_transfer::device_id();
        for uuid in &tombstone_ids {
            super::tombstones::record_sync_tombstone_in_conn(
                &tx,
                super::tombstones::COLLECTION_HISTORY,
                uuid,
                &local_device_id,
                deleted_at,
            )?;
        }
        for id in &unique_ids {
            tx.execute("DELETE FROM clipboard WHERE id = ?1", params![id])?;
        }
        tx.commit()?;

        let mut to_delete = Vec::new();
        for image_id in image_id_set {
            if !is_image_id_referenced(conn, &image_id)? {
                to_delete.push(image_id);
            }
        }

        Ok(to_delete)
    })?;

    for id in &unique_ids {
        let _ = delete_clipboard_data_items("clipboard", &id.to_string());
    }
    delete_image_files(images_to_delete)
}

// 清空所有剪贴板历史
pub fn clear_clipboard_history() -> Result<(), String> {
    let images_to_delete: Vec<String> = with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, image_id, uuid FROM clipboard",
        )?;
        let ids_iter = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut set: HashSet<String> = HashSet::new();
        let mut tombstone_ids = Vec::new();
        for r in ids_iter {
            if let Ok((id, image_ids, uuid)) = r {
                if let Some(image_ids) = image_ids {
                    for iid in split_image_ids(&image_ids) {
                        set.insert(iid);
                    }
                }
                tombstone_ids.push(uuid.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| id.to_string()));
            }
        }
        drop(stmt);

        let tx = conn.unchecked_transaction()?;
        let deleted_at = chrono::Local::now().timestamp();
        let local_device_id = crate::services::sync_transfer::device_id();
        for uuid in tombstone_ids {
            super::tombstones::record_sync_tombstone_in_conn(
                &tx,
                super::tombstones::COLLECTION_HISTORY,
                &uuid,
                &local_device_id,
                deleted_at,
            )?;
        }
        tx.execute("DELETE FROM clipboard", [])?;
        tx.commit()?;

        let mut to_delete = Vec::new();
        for iid in set.into_iter() {
            if !is_image_id_referenced(conn, &iid)? {
                to_delete.push(iid);
            }
        }
        Ok(to_delete)
    })?;

    let _ = delete_clipboard_data_items_by_kind("clipboard");
    delete_image_files(images_to_delete)
}

// 排序逻辑
fn reorder_items(conn: &rusqlite::Connection, from_idx: usize, to_idx: usize, items: &[(i64, i64)]) -> Result<(), rusqlite::Error> {
    if from_idx == to_idx { return Ok(()); }
    
    let tx = conn.unchecked_transaction()?;
    let now = chrono::Local::now().timestamp();
    let moved_id = items[from_idx].0;
    let target_order = items[to_idx].1;

    if from_idx < to_idx {
        for i in (from_idx + 1)..=to_idx {
            tx.execute("UPDATE clipboard SET item_order = item_order + 1, updated_at = ?1 WHERE id = ?2", params![now, items[i].0])?;
        }
    } else {
        for i in to_idx..from_idx {
            tx.execute("UPDATE clipboard SET item_order = item_order - 1, updated_at = ?1 WHERE id = ?2", params![now, items[i].0])?;
        }
    }
    tx.execute("UPDATE clipboard SET item_order = ?1, updated_at = ?2 WHERE id = ?3", params![target_order, now, moved_id])?;
    tx.commit()
}

// 移动剪贴板项到顶部（非置顶区的顶部）
pub fn move_clipboard_item_to_top(id: i64) -> Result<(), String> {
    with_connection(|conn| {
        let now = chrono::Local::now().timestamp();
        let max_order: i64 = conn.query_row(
            "SELECT COALESCE(MAX(item_order), 0) FROM clipboard WHERE is_pinned = 0",
            [],
            |row| row.get(0)
        ).unwrap_or(0);
        
        conn.execute(
            "UPDATE clipboard SET item_order = ?1, updated_at = ?2 WHERE id = ?3 AND is_pinned = 0",
            params![max_order + 1, now, id],
        )?;
        Ok(())
    })
}

// 移动剪贴板项
pub fn move_clipboard_item_by_id(from_id: i64, to_id: i64) -> Result<(), String> {
    if from_id == to_id { return Ok(()); }

    with_connection(|conn| {
        let from_pinned: i64 = conn.query_row(
            "SELECT is_pinned FROM clipboard WHERE id = ?",
            params![from_id], |row| row.get(0)
        )?;
        let to_pinned: i64 = conn.query_row(
            "SELECT is_pinned FROM clipboard WHERE id = ?",
            params![to_id], |row| row.get(0)
        )?;
        
        if from_pinned != to_pinned {
            return Ok(());
        }
        
        let items: Vec<(i64, i64)> = conn.prepare("SELECT id, item_order FROM clipboard ORDER BY is_pinned DESC, item_order DESC, updated_at DESC")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let from_idx = items.iter().position(|(id, _)| *id == from_id)
            .ok_or_else(|| rusqlite::Error::InvalidParameterName(format!("ID {} 不存在", from_id)))?;
        let to_idx = items.iter().position(|(id, _)| *id == to_id)
            .ok_or_else(|| rusqlite::Error::InvalidParameterName(format!("ID {} 不存在", to_id)))?;
        
        reorder_items(conn, from_idx, to_idx, &items)
    })
}

// 更新剪贴板项的内容
pub fn update_clipboard_item(
    id: i64,
    content: String,
    html_content: Option<String>,
) -> Result<(), String> {
    // 旧格式清理必须在同一事务内完成——UPDATE 提交后再清 raw formats
    // 失败(独立连接)会留下"新 content 配旧 formats"的不一致态,前端按新
    // 格式请求 clipboard_data 时找不到对应 raw,内容回退显示错乱。
    // 事务内直接 DELETE,UPDATE + 清理任一失败整体回滚,原子落地。
    with_connection(|conn| {
        let tx = conn.unchecked_transaction()?;
        let (old_content, old_html_content): (String, Option<String>) = tx.query_row(
            "SELECT content, html_content FROM clipboard WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let content_changed = old_content != content;
        let html_changed = html_content
            .as_ref()
            .map(|new_html| old_html_content.as_deref() != Some(new_html.as_str()))
            .unwrap_or(false);

        let now = chrono::Local::now().timestamp();
        let rows = if let Some(ref html_content) = html_content {
            tx.execute(
                "UPDATE clipboard SET content = ?1, html_content = ?2, updated_at = ?3 WHERE id = ?4",
                params![&content, html_content, now, id],
            )?
        } else {
            // html_content=None 表示内容已变为纯文本,必须显式写 NULL——
            // 否则 UPDATE 不含该列,旧 HTML 残留,前端仍按 HTML 渲染旧富文本。
            tx.execute(
                "UPDATE clipboard SET content = ?1, html_content = NULL, updated_at = ?2 WHERE id = ?3",
                params![&content, now, id],
            )?
        };
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        if content_changed || html_changed {
            // 内容变了:旧 raw formats 必须同事务清掉,否则新内容配旧格式
            tx.execute(
                "DELETE FROM clipboard_data WHERE target_kind = 'clipboard' AND target_id = ?1",
                [id.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .map_err(|e| if e.contains("Query returned no rows") {
        format!("剪贴板项不存在: {}", id)
    } else { e })?;

    Ok(())
}

// 切换剪贴板项的置顶状态（置顶时放到置顶区第一位，取消置顶时移到非置顶区第一位）
pub fn toggle_pin_clipboard_item(id: i64) -> Result<bool, String> {
    with_connection(|conn| {
        let current_pinned: i64 = conn.query_row(
            "SELECT is_pinned FROM clipboard WHERE id = ?", params![id], |row| row.get(0)
        )?;

        // 置顶/取消置顶是纯本地排序操作,不推进 updated_at:排序语义与内容
        // 版本解耦。若置顶同时 bump updated_at,该记录会以最大时间戳触发
        // WebDAV/LAN 差量推送、把 item_order 排序传到对端互相踩踏,并成为
        // LWW 胜者压过对端真实内容更新——置顶一次就让整条记录抢跑同步。
        if current_pinned == 0 {
            let max_pinned_order: i64 = conn.query_row(
                "SELECT COALESCE(MAX(item_order), 0) FROM clipboard WHERE is_pinned = 1", [], |row| row.get(0)
            ).unwrap_or(0);
            conn.execute("UPDATE clipboard SET is_pinned = 1, item_order = ?1 WHERE id = ?2", params![max_pinned_order + 1, id])?;
            Ok(true)
        } else {
            let max_order: i64 = conn.query_row(
                "SELECT COALESCE(MAX(item_order), 0) FROM clipboard WHERE is_pinned = 0", [], |row| row.get(0)
            ).unwrap_or(0);
            conn.execute("UPDATE clipboard SET is_pinned = 0, item_order = ?1 WHERE id = ?2", params![max_order + 1, id])?;
            Ok(false)
        }
    })
}

#[cfg(test)]
mod limit_zero_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 字符数补齐必须单飞:翻页命中缺失 char_count 的行会每页触发一次
    // update_missing_char_counts,不加守卫则快速滚动时堆积互抢 DB 锁的
    // 线程群。单飞守卫要求 spawn 前 swap 置 true、线程收尾复位 false。
    #[test]
    fn char_count_updater_has_in_flight_guard() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "update_missing_char_counts");
        let swap_pos = body
            .find("CHAR_COUNT_UPDATER_IN_FLIGHT.swap(true, Ordering::SeqCst)")
            .expect("spawn 前必须单飞占用守卫");
        let spawn_pos = body
            .find("std::thread::spawn(move ||")
            .expect("必须 spawn 后台线程");
        assert!(swap_pos < spawn_pos, "必须先占用守卫再 spawn");
        let reset_pos = body
            .find("CHAR_COUNT_UPDATER_IN_FLIGHT.store(false, Ordering::SeqCst)")
            .expect("线程收尾必须复位守卫");
        assert!(
            reset_pos > spawn_pos,
            "守卫复位必须在 spawn 之后(线程体内),否则复位立即执行失去单飞意义"
        );
    }

    // 边界护栏:limit_clipboard_history 对 max_count=0 必须兜底,
    // 否则 NOT IN (... LIMIT 0) 空允许集把全部历史(含置顶)清空。
    #[test]
    fn limit_zero_clamps_to_one_not_empty_allowlist() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "limit_clipboard_history");
        assert!(
            body.contains("max_count.max(1)"),
            "max_count=0 必须钳制到至少 1,不得清空全部历史"
        );
    }

    // html_content=None 表示内容已变为纯文本,UPDATE 必须显式写 NULL——
    // 若 SQL 不含该列,旧 HTML 残留,前端仍按 HTML 渲染旧富文本。
    #[test]
    fn update_clipboard_item_writes_null_html_when_none() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "update_clipboard_item");
        assert!(
            body.contains("html_content = NULL"),
            "html_content=None 时必须显式写 NULL,不得让旧 HTML 残留"
        );
    }
}

#[cfg(test)]
mod upsert_null_uuid_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // NULL uuid 重复:upsert_history_records 的 existing 查询必须与
    // webdav_get_history_record_by_uuid 口径一致,含 NULL uuid 兜底分支——
    // 否则本地 uuid 为 NULL 的旧记录被上传(uuid 兜底成 id 字符串)后,对端
    // upsert 查 WHERE uuid=?1 匹配不到 NULL 行,INSERT 出重复行。
    // 修复只覆盖 NULL,漏 uuid='' 空串:reading 路径 :414 兜底是
    // (uuid IS NULL OR uuid = ''),upsert 也必须同样覆盖——某些导入/修复
    // 场景 uuid 会被写成空串而非 NULL,只兜 NULL 会漏,INSERT 出重复行。
    #[test]
    fn upsert_existing_query_has_null_uuid_fallback() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "upsert_history_records");
        let existing_pos = body
            .find("FROM clipboard")
            .expect("upsert 必须查 clipboard 表");
        let existing_seg = &body[existing_pos..];
        assert!(
            existing_seg.contains("(uuid IS NULL OR uuid = '') AND CAST(id AS TEXT) = ?2"),
            "existing 查询必须含 NULL/空串 uuid 兜底(uuid IS NULL OR uuid = '')"
        );
    }

    // UPDATE 分支的 WHERE 同样要覆盖 NULL uuid 行,否则 existing
    // 兜底命中的 NULL 行会被 UPDATE 更新 0 行,新值不落地。
    #[test]
    fn upsert_update_where_covers_null_uuid_rows() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "upsert_history_records");
        let update_pos = body
            .find("UPDATE clipboard SET")
            .expect("upsert 必须含 UPDATE 分支");
        let update_seg = &body[update_pos..];
        assert!(
            update_seg.contains("(uuid IS NULL OR uuid = '') AND CAST(id AS TEXT) = ?15"),
            "UPDATE 分支 WHERE 必须覆盖 NULL/空串 uuid 行"
        );
    }
}

// 友好错误映射必须匹配 rusqlite 的 Display 输出,而不是 Error 变体名——
// with_connection 已把错误 format 成 "数据库操作失败: {e}",此时 e 是
// Display 文本 "Query returned no rows"(小写 r)。若继续 contains 驼峰
// 变体名 QueryReturnedNoRows 永远不命中,用户只会看到生硬报错而拿不到
// "剪贴板项不存在: {id}" 的友好提示。
#[cfg(test)]
mod no_rows_friendly_message_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    #[test]
    fn update_clipboard_item_maps_no_rows_to_friendly_message() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "update_clipboard_item");
        assert!(
            body.contains("Query returned no rows"),
            "必须匹配 rusqlite 对 QueryReturnedNoRows 的 Display 输出(小写 r)"
        );
        let map_pos = body
            .find("Query returned no rows")
            .unwrap_or_else(|| panic!("找不到错误映射"));
        let map_seg = &body[map_pos..];
        assert!(
            map_seg.contains("剪贴板项不存在"),
            "不存在项必须映射为友好消息"
        );
        assert!(
            !body.contains("contains(\"QueryReturnedNoRows\")"),
            "不得拿驼峰变体名做字符串匹配(永远为 false)——只能匹配 Display 输出"
        );
    }
}

#[cfg(test)]
mod limit_tombstone_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 裁剪不写 tombstone:limit_clipboard_history 删除超出上限的记录
    // 必须写 tombstone(collection=history,item_id=uuid 兜底 id)——否则云端/
    // peers 不知道这些记录已删,切换设备把已裁剪内容拉回,且另一端删除无法
    // 传播删除语义。裁剪是常态路径,不写墓碑等于删除永不扩散。
    #[test]
    fn limit_clipboard_history_writes_tombstones_for_deleted() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "limit_clipboard_history");
        assert!(
            body.contains("record_sync_tombstone_in_conn"),
            "裁剪删除必须写 tombstone,否则云端/peers 永久残留被裁剪记录"
        );
    }
}

#[cfg(test)]
mod single_delete_tx_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 单删无事务:delete_clipboard_item 的 tombstone + DELETE 必须包在
    // 同一事务里——崩溃可能只写墓碑不删记录(本地幽灵删除)或只删记录不写
    // 墓碑(云端残留)。与 delete_clipboard_items 的既有事务路径对齐。
    #[test]
    fn delete_clipboard_item_wraps_tombstone_and_delete_in_tx() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "delete_clipboard_item");
        let tx_pos = body
            .find("unchecked_transaction")
            .expect("delete_clipboard_item 必须开事务");
        let tx_seg = &body[tx_pos..];
        let tombstone_pos = tx_seg
            .find("record_sync_tombstone_in_conn")
            .expect("事务内必须先写 tombstone");
        let tombstone_seg = &tx_seg[tombstone_pos..];
        assert!(
            tombstone_seg.contains("DELETE FROM clipboard WHERE id = ?1"),
            "tombstone 后必须在同一事务内 DELETE"
        );
        assert!(
            tombstone_seg.contains("tx.commit()"),
            "DELETE 后必须提交事务"
        );
    }
}

#[cfg(test)]
mod content_type_like_tests {
    use std::fs;
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    /// 护栏:content_type 过滤必须 like_pattern + ESCAPE,与搜索词路径一致。
    #[test]
    fn query_clipboard_content_type_uses_like_pattern_with_escape() {
        let source = fs::read_to_string(format!(
            "{}/src/services/database/clipboard.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 clipboard.rs");

        let body: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        // 定位 query_clipboard_items 函数体
        let start = body
            .find("pub fn query_clipboard_items")
            .expect("找不到 query_clipboard_items");
        let after = &body[start..];
        // 取到下一个 pub fn
        let end = after
            .find("\npub fn ")
            .or_else(|| after.find("\nfn "))
            .map(|i| start + i)
            .unwrap_or(body.len());
        // 限制在 content_type 分支附近,避免函数过长误扫
        let fn_body = &body[start..end.min(start + 2500)];

        assert!(
            fn_body.contains("content_type LIKE ? ESCAPE"),
            "content_type 过滤必须带 ESCAPE '\\\\'"
        );
        assert!(
            fn_body.contains("like_pattern(content_type)")
                || fn_body.contains("super::like_pattern(content_type)"),
            "content_type 必须走 like_pattern,禁止 format!(\"%{{}}%\")"
        );
        // 负向:裸 format!("%{}%") 拼 content_type 不得再出现
        let has_raw_format = fn_body
            .lines()
            .any(|l| l.contains("format!") && l.contains("%{}%") && l.contains("content_type"));
        assert!(
            !has_raw_format,
            "content_type 不得再用 format!(\"%{{}}%\") 裸拼"
        );
    }

    /// 护栏:update_clipboard_item 的旧格式清理必须包进同一事务。
    /// 原实现 UPDATE 提交后才调 delete_clipboard_data_items(独立连接),
    /// 清理失败时留下"新 content 配旧 formats"的不一致态;且 DELETE 必须
    /// 用 tx 内的执行器,禁止再调 with_connection 嵌套(死锁)。
    #[test]
    fn update_clipboard_item_clears_raw_formats_in_same_transaction() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "update_clipboard_item");
        let tx_pos = body
            .find("unchecked_transaction()")
            .expect("update_clipboard_item 必须开事务");
        let tx_seg = &body[tx_pos..];
        let del_pos = tx_seg
            .find("DELETE FROM clipboard_data WHERE target_kind = 'clipboard'")
            .expect("内容变化时必须同事务清掉旧 raw formats");
        let commit_pos = tx_seg
            .find("tx.commit()")
            .expect("UPDATE + 清理后必须提交事务");
        assert!(
            del_pos < commit_pos,
            "DELETE 清理必须先于提交事务"
        );
        // 负向:禁止再出现事务外的独立连接清理
        assert!(
            !body.contains("delete_clipboard_data_items"),
            "旧格式清理不得再走事务外的独立连接(with_connection 会死锁)"
        );
    }

    /// 护栏:COUNT 参数必须直接复用 query_params(深拷贝会折叠非文本)。
    /// 旧实现对每个参数 to_sql() 反射:文本拷贝、其余折叠空串——选中项含
    /// 数字/整型(_sortId 深拷贝)时,WHERE 按原始值过滤、COUNT 按空串计数,
    /// 分页总数与列表语义脱节。COUNT 执行时 query_params 只含 WHERE 文本
    /// 参数,直接借用即可。
    #[test]
    fn count_query_uses_same_params_as_where_clause() {
        let src = strip_line_comments(&source_file("src/services/database/clipboard.rs"));
        let body = fn_body(&src, "query_clipboard_items");
        let count_pos = body
            .find("SELECT COUNT(*) FROM clipboard")
            .expect("分页必须有 COUNT 查询");
        let count_seg = &body[count_pos..];
        assert!(
            count_seg.contains("params_from_iter(query_params.iter().map(|p| p.as_ref()))"),
            "COUNT 必须直接复用 query_params,禁止深拷贝折叠非文本"
        );
        assert!(
            !count_seg.contains("to_sql()"),
            "COUNT 不得再对参数做 to_sql 反射式深拷贝"
        );
    }

    /// 护栏:where_clauses 必须是 Vec<String>,禁止 Box::leak 泄漏字符串。
    #[test]
    fn query_clipboard_where_clauses_are_strings_not_leaked() {
        let source = fs::read_to_string(format!(
            "{}/src/services/database/clipboard.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 clipboard.rs");
        let body: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = body
            .find("pub fn query_clipboard_items")
            .expect("找不到 query_clipboard_items");
        let after = &body[start..];
        let end = after
            .find("\npub fn ")
            .or_else(|| after.find("\nfn "))
            .map(|i| start + i)
            .unwrap_or(body.len());
        let fn_body = &body[start..end.min(start + 2500)];
        assert!(
            fn_body.contains("let mut where_clauses = vec![];"),
            "where_clauses 必须用 Vec 收集"
        );
        assert!(
            !fn_body.contains("Box::leak"),
            "where_clauses 禁止 Box::leak 泄漏字符串,改为 Vec<String>"
        );
        assert!(
            fn_body.contains("where_clauses.join(\" AND \")"),
            "where_clauses 必须以 AND 拼接"
        );
    }

    // 置顶/取消置顶必须与内容版本解耦:置顶切的是排序位与 is_pinned,不得
    // 推进 updated_at。否则置顶一次就 bump 时间戳触发 WebDAV/LAN 差量推送,
    // item_order 排序语义被当内容同步、互相踩踏,且记录以最大时间戳成为
    // LWW 胜者压过对端真实内容更新。
    #[test]
    fn toggle_pin_does_not_bump_updated_at() {
        let source = fs::read_to_string(format!(
            "{}/src/services/database/clipboard.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 clipboard.rs");
        let body: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = body
            .find("pub fn toggle_pin_clipboard_item")
            .expect("找不到 toggle_pin_clipboard_item");
        let after = &body[start..];
        let end = after
            .find("\nfn ")
            .map(|i| start + i)
            .unwrap_or_else(|| after.find("\npub fn ").map(|i| start + i).unwrap_or(body.len()));
        let fn_body = &body[start..end];
        // 置顶/取消置顶两条 UPDATE 都必须只写 is_pinned 与 item_order,不带
        // updated_at 占位符。
        assert!(
            fn_body.matches("is_pinned = 1, item_order = ?1 WHERE").count() >= 1,
            "置顶 UPDATE 必须只写 is_pinned + item_order"
        );
        assert!(
            fn_body.matches("is_pinned = 0, item_order = ?1 WHERE").count() >= 1,
            "取消置顶 UPDATE 必须只写 is_pinned + item_order"
        );
        assert!(
            !fn_body.contains("updated_at = ?2"),
            "置顶选不得推进 updated_at(排序与内容版本解耦)"
        );
    }
}


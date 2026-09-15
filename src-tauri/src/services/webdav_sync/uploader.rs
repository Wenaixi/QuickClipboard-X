use std::collections::{BTreeMap, HashMap, HashSet};

use super::chunk_manager::{load_chunk, save_chunk};
use super::index_manager::{load_index, merge_index, save_index};
use super::types::{CloudRecord, CloudRecordMeta, ImageFileIndex, ImageFileIndexEntry, SyncCollection, SyncIndexEntry, SyncReport, CHUNK_RECORD_LIMIT};
use super::webdav_client::WebdavClient;

pub async fn upload_all(client: &WebdavClient, device_id: &str) -> Result<SyncReport, String> {
    upload_parts(client, device_id, true, true, true, true).await
}

pub async fn upload_parts(
    client: &WebdavClient,
    device_id: &str,
    upload_clipboard: bool,
    upload_favorites: bool,
    upload_groups: bool,
    upload_tombstones: bool,
) -> Result<SyncReport, String> {
    let mut report = SyncReport::default();
    let settings = crate::services::get_settings();
    let mut uploaded_records = Vec::new();

    let tombstone_states = if upload_tombstones {
        match super::tombstones_sync::upload_tombstones(client).await {
            Ok(result) => {
                report.pulled_clipboard += result.applied.history;
                report.pulled_favorites += result.applied.favorites;
                report.pulled_groups += result.applied.groups;
                report.pulled += result.applied.total();
                result.states
            }
            Err(e) => {
                report.errors.push(format!("删除记录推送失败: {}", e));
                crate::services::database::sync_tombstone_states().unwrap_or_default()
            }
        }
    } else {
        crate::services::database::sync_tombstone_states().unwrap_or_default()
    };

    let history_records = if settings.webdav_sync_clipboard && upload_clipboard {
        let index = load_index(client, SyncCollection::History).await?;
        let metas = crate::services::database::webdav_list_history_record_metas()?;
        let metas = crate::services::database::filter_record_metas_not_deleted_by_states(
            crate::services::database::COLLECTION_HISTORY,
            metas,
            &tombstone_states,
        );
        let metas = metas_newer_than_index(metas, &index.entries);
        let records = load_history_records(&metas, device_id)?;
        Some((index, records))
    } else {
        None
    };
    let favorite_records = if settings.webdav_sync_favorites && upload_favorites {
        let index = load_index(client, SyncCollection::Favorites).await?;
        let metas = crate::services::database::webdav_list_favorite_record_metas()?;
        let metas = crate::services::database::filter_record_metas_not_deleted_by_states(
            crate::services::database::COLLECTION_FAVORITES,
            metas,
            &tombstone_states,
        );
        let metas = metas_newer_than_index(metas, &index.entries);
        let records = load_favorite_records(&metas, device_id)?;
        Some((index, records))
    } else {
        None
    };

    if let Some((index, history_records)) = history_records {
        match upload_collection_incremental(client, SyncCollection::History, index, history_records.clone(), device_id).await {
            Ok(records) => {
                let count = records.len() as u32;
                report.pushed += count;
                report.pushed_clipboard = count;
                report
                    .pushed_items
                    .extend(records.iter().map(|record| record.report_item("clipboard")));
                uploaded_records.extend(records);
            }
            Err(e) => report.errors.push(format!("剪贴板历史推送失败: {}", e)),
        }
    }

    if settings.webdav_sync_favorites && (upload_favorites || upload_groups) {
        if let Some((index, favorite_records)) = favorite_records {
            match upload_collection_incremental(client, SyncCollection::Favorites, index, favorite_records.clone(), device_id).await {
                Ok(records) => {
                    let count = records.len() as u32;
                    report.pushed += count;
                    report.pushed_favorites = count;
                    report
                        .pushed_items
                        .extend(records.iter().map(|record| record.report_item("favorites")));
                    uploaded_records.extend(records);
                }
                Err(e) => report.errors.push(format!("收藏推送失败: {}", e)),
            }
        }

        if upload_groups {
            match super::groups_sync::upload_groups_with_tombstones(client, device_id, &tombstone_states).await {
                Ok(groups) => {
                    let count = groups.len() as u32;
                    report.pushed += count;
                    report.pushed_groups = count;
                    report.pushed_items.extend(groups.into_iter().map(|group| {
                        super::types::SyncReportItem {
                            category: "groups".to_string(),
                            id: group.name.clone(),
                            summary: group.name,
                            source_device_id: group.source_device_id,
                            updated_at: group.updated_at,
                        }
                    }));
                }
                Err(e) => report.errors.push(format!("分组推送失败: {}", e)),
            }
        }
    }

    if settings.webdav_sync_images {
        upload_images(client, &uploaded_records)
            .await
            .map_err(|e| format!("上传图片失败: {}", e))?;
    }

    Ok(report)
}

async fn upload_collection_incremental(
    client: &WebdavClient,
    collection: SyncCollection,
    mut index: super::types::SyncIndex,
    records: Vec<CloudRecord>,
    device_id: &str,
) -> Result<Vec<CloudRecord>, String> {
    let mut changed = Vec::new();
    let mut existing_by_chunk: HashMap<u32, Vec<CloudRecord>> = HashMap::new();
    let mut new_records = Vec::new();

    for record in records {
        let needs_upload = match index.entries.get(&record.uuid) {
            Some(entry) => entry.updated_at < record.updated_at,
            None => true,
        };

        if !needs_upload {
            continue;
        }

        if let Some(entry) = index.entries.get(&record.uuid) {
            existing_by_chunk.entry(entry.chunk).or_default().push(record.clone());
        } else {
            new_records.push(record.clone());
        }
        changed.push(record);
    }

    let mut new_records_by_chunk = Vec::<(u32, Vec<CloudRecord>)>::new();
    let mut chunk_counts = chunk_record_counts(&index.entries);
    let fillable_chunk_ids = chunk_counts
        .iter()
        .filter_map(|(chunk_id, count)| {
            if *count < CHUNK_RECORD_LIMIT {
                Some(*chunk_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut fillable_index = 0usize;
    let next_available_chunk = chunk_counts
        .keys()
        .next_back()
        .copied()
        .map(|chunk_id| chunk_id.saturating_add(1))
        .unwrap_or(0);
    let mut current_chunk_id = index.next_chunk.max(next_available_chunk);
    let mut current_chunk_records = Vec::new();

    for record in new_records {
        let fillable_chunk_id = loop {
            let Some(chunk_id) = fillable_chunk_ids.get(fillable_index).copied() else {
                break None;
            };
            let count = chunk_counts.get(&chunk_id).copied().unwrap_or(0);
            if count < CHUNK_RECORD_LIMIT {
                break Some(chunk_id);
            }
            fillable_index += 1;
        };

        if let Some(chunk_id) = fillable_chunk_id {
            existing_by_chunk.entry(chunk_id).or_default().push(record);
            *chunk_counts.entry(chunk_id).or_default() += 1;
            continue;
        }

        current_chunk_records.push(record);
        if current_chunk_records.len() >= CHUNK_RECORD_LIMIT {
            let count = current_chunk_records.len();
            new_records_by_chunk.push((current_chunk_id, current_chunk_records));
            chunk_counts.insert(current_chunk_id, count);
            current_chunk_id = current_chunk_id.saturating_add(1);
            current_chunk_records = Vec::new();
        }
    }
    if !current_chunk_records.is_empty() {
        let count = current_chunk_records.len();
        new_records_by_chunk.push((current_chunk_id, current_chunk_records));
        chunk_counts.insert(current_chunk_id, count);
        current_chunk_id = current_chunk_id.saturating_add(1);
    }

    if !existing_by_chunk.is_empty() || !new_records_by_chunk.is_empty() {
        client.ensure_collection_dirs(collection).await?;
    }

    for (chunk_id, records) in existing_by_chunk {
        let mut chunk = load_chunk(client, collection, chunk_id).await?;
        for record in records {
            chunk.records.insert(record.uuid.clone(), record.clone());
            index.entries.insert(
                record.uuid.clone(),
                SyncIndexEntry {
                    chunk: chunk_id,
                    updated_at: record.updated_at,
                    source_device_id: device_id.to_string(),
                },
            );
        }
        save_chunk(client, collection, chunk_id, &chunk).await?;
    }

    for (chunk_id, records) in new_records_by_chunk {
        // 新 chunk 必须先从远端 load 现有块再内存合并——直接以空块 PUT
        // 会整块覆盖对端设备并发写入同一 chunk 的记录(两设备各自推进
        // next_chunk 可指向同一 chunk 号)。load 后对不存在的新块返回空,
        // 对已存在的块则按 uuid 覆盖式合并,保留对端记录。
        let mut chunk = load_chunk(client, collection, chunk_id).await?;
        for record in records {
            chunk.records.insert(record.uuid.clone(), record.clone());
            index.entries.insert(
                record.uuid.clone(),
                SyncIndexEntry {
                    chunk: chunk_id,
                    updated_at: record.updated_at,
                    source_device_id: device_id.to_string(),
                },
            );
        }
        save_chunk(client, collection, chunk_id, &chunk).await?;
    }
    index.next_chunk = current_chunk_id;

    // index.json 并发整块覆盖——chunk 有"先 load 远端再合并",
    // index 却没有:直接以本地内存 index 裸 PUT 会整块覆盖对端设备并发
    // 写入的条目(next_chunk 也会被拉回旧值,下一轮 A/B 设备可能为同一
    // 个 chunk 编号各写各的)。写前 load 远端按 uuid 合并(本地条目覆盖
    // 同名、远端独有条目保留、next_chunk 取两者较大),仅当合并后确有
    // 变化才条件 PUT,避免每次同步都整块覆盖 index.json。
    if !changed.is_empty() {
        let (merged, has_change) = merge_index(client, collection, &index).await?;
        if has_change {
            save_index(client, collection, &merged).await?;
        }
    }

    Ok(changed)
}

fn metas_newer_than_index(
    metas: Vec<CloudRecordMeta>,
    entries: &HashMap<String, SyncIndexEntry>,
) -> Vec<CloudRecordMeta> {
    metas
        .into_iter()
        .filter(|meta| {
            entries
                .get(&meta.uuid)
                .map(|entry| entry.updated_at < meta.updated_at)
                .unwrap_or(true)
        })
        .collect()
}

fn load_history_records(
    metas: &[CloudRecordMeta],
    device_id: &str,
) -> Result<Vec<CloudRecord>, String> {
    let mut records = Vec::with_capacity(metas.len());
    for meta in metas {
        if let Some(record) = crate::services::database::webdav_get_history_record_by_uuid(&meta.uuid, device_id)? {
            records.push(record);
        }
    }
    Ok(records)
}

fn load_favorite_records(
    metas: &[CloudRecordMeta],
    device_id: &str,
) -> Result<Vec<CloudRecord>, String> {
    let mut records = Vec::with_capacity(metas.len());
    for meta in metas {
        if let Some(record) = crate::services::database::webdav_get_favorite_record_by_uuid(&meta.uuid, device_id)? {
            records.push(record);
        }
    }
    Ok(records)
}

fn chunk_record_counts(index_entries: &HashMap<String, SyncIndexEntry>) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for entry in index_entries.values() {
        *counts.entry(entry.chunk).or_default() += 1;
    }
    counts
}

async fn upload_images(client: &WebdavClient, records: &[CloudRecord]) -> Result<(), String> {
    let mut image_ids = HashSet::new();
    for record in records {
        collect_image_ids(&mut image_ids, record.image_id.as_deref());
    }
    // 与 downloader 侧对称的重扫:本批 records 之外,历史/收藏表里仍引用
    // 但从未上传(或曾上传失败)的图片也要补传。若只依赖本批 records,
    // 缺文件/上传失败的图片会永久保持缺失,而同步报告还记 success。
    for meta in crate::services::database::webdav_list_history_record_metas()? {
        collect_image_ids(&mut image_ids, meta.image_id.as_deref());
    }
    for meta in crate::services::database::webdav_list_favorite_record_metas()? {
        collect_image_ids(&mut image_ids, meta.image_id.as_deref());
    }

    if image_ids.is_empty() {
        return Ok(());
    }

    let mut index = load_image_file_index(client).await?;
    let mut changed = false;

    let data_dir = crate::services::get_data_directory()?;
    let images_dir = data_dir.join("clipboard_images");
    for image_id in image_ids {
        if index.images.contains_key(&image_id) {
            continue;
        }
        let path = images_dir.join(format!("{}.png", image_id));
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if !changed {
            client.ensure_files_dir().await?;
        }
        client.put_bytes(&format!("files/{}.png", image_id), bytes).await?;
        index.images.insert(
            image_id,
            ImageFileIndexEntry {
                uploaded_at: chrono::Utc::now().timestamp(),
            },
        );
        changed = true;
    }

    if changed {
        save_image_file_index(client, &index).await?;
    }

    Ok(())
}

async fn load_image_file_index(client: &WebdavClient) -> Result<ImageFileIndex, String> {
    let index = client.get_json("files/index.json").await?;
    if index.is_some() {
        client.mark_dir_ensured("");
        client.mark_dir_ensured("files");
    }
    Ok(index.unwrap_or_default())
}

async fn save_image_file_index(client: &WebdavClient, index: &ImageFileIndex) -> Result<(), String> {
    client.put_json("files/index.json", index).await
}

fn collect_image_ids(out: &mut HashSet<String>, raw: Option<&str>) {
    let Some(raw) = raw else { return; };
    for item in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if super::image_id::is_valid_image_id(item) {
            out.insert(item.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::collect_image_ids;
    use std::collections::HashSet;

    #[test]
    fn collect_image_ids_filters_path_traversal() {
        let mut out = HashSet::new();
        collect_image_ids(&mut out, Some("../evil.png,ok_1,..\\..\\x.png,img-2"));
        assert!(out.contains("ok_1"), "合法 id 必须保留");
        assert!(out.contains("img-2"), "合法 id 必须保留");
        assert!(!out.contains("../evil.png"), "../ 不得进入集合");
        assert!(!out.contains("..\\..\\x.png"), "反斜杠穿越不得进入集合");
    }

    #[test]
    fn collect_image_ids_none_and_empty_are_noop() {
        let mut out = HashSet::new();
        collect_image_ids(&mut out, None);
        collect_image_ids(&mut out, Some(""));
        collect_image_ids(&mut out, Some("   ,  "));
        assert!(out.is_empty());
    }
}

#[cfg(test)]
mod image_rescan_guards {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // upload_images 必须从历史/收藏 metas 全量重扫图片 ID,不能只依赖
    // 本批 records——否则缺文件/失败上传的图片永久缺失,同步报告还记 success。
    #[test]
    fn upload_images_rescans_history_and_favorite_metas() {
        let src = strip_line_comments(&source_file("src/services/webdav_sync/uploader.rs"));
        let body = fn_body(&src, "upload_images");
        assert!(
            body.contains("webdav_list_history_record_metas"),
            "必须全量扫历史表 image_id 补传"
        );
        assert!(
            body.contains("webdav_list_favorite_record_metas"),
            "必须全量扫收藏表 image_id 补传"
        );
    }

    // 并发 chunk 覆盖:新记录落块必须先从远端 load 现有块再内存合并——
    // 直接 RecordChunk::default() 空块 PUT 会整块覆盖对端设备并发写入同一
    // chunk 的记录。护栏断言新 chunk 分支含 load_chunk 调用,且无空块构造。
    #[test]
    fn new_chunk_uploads_merge_remote_chunk_instead_of_blank_overwrite() {
        let src = strip_line_comments(&source_file("src/services/webdav_sync/uploader.rs"));
        let body = fn_body(&src, "upload_collection_incremental");
        let new_chunk_pos = body
            .find("new_records_by_chunk")
            .expect("upload_collection_incremental 必须处理新记录分块");
        let new_chunk_seg = &body[new_chunk_pos..];
        assert!(
            new_chunk_seg.contains("load_chunk(client, collection, chunk_id)"),
            "新 chunk 分支必须先 load 远端现有块再合并,否则空块覆盖对端并发写入"
        );
        assert!(
            !new_chunk_seg.contains("RecordChunk::default()"),
            "新 chunk 分支不得用空块直接 PUT"
        );
    }

    // index.json 并发整块覆盖:upload_collection_incremental 的 index 写路径
    // 必须先 merge_index(load 远端合并)再按 has_change 条件 save_index——
    // 直接 save_index(本地内存 index) 会整块覆盖对端设备并发写入的条目,
    // next_chunk 也被拉回旧值。护栏断言:调用 merge_index、has_change 分支内
    // 才 save_index、禁止裸 save_index(client, collection, &index)。
    #[test]
    fn index_save_merges_remote_before_conditional_put() {
        let src = strip_line_comments(&source_file("src/services/webdav_sync/uploader.rs"));
        let body = fn_body(&src, "upload_collection_incremental");
        let merge_pos = body
            .find("merge_index(client, collection, &index)")
            .expect("写 index 前必须调用 merge_index 合并远端");
        let has_change_pos = body
            .find("if has_change")
            .expect("必须按 merge_index 返回的 has_change 条件写");
        assert!(
            merge_pos < has_change_pos,
            "merge_index 必须早于 has_change 判断"
        );
        let save_pos = body
            .find("save_index(client, collection, &merged)")
            .expect("条件 PUT 必须写合并后的 index");
        assert!(
            has_change_pos < save_pos,
            "save_index 必须位于 has_change 条件分支内"
        );
        // 负向:禁止裸 PUT 本地内存 index(未合并)
        let bare_save = body.find("save_index(client, collection, &index)");
        assert!(
            bare_save.is_none(),
            "禁止直接裸 PUT 本地内存 index,必须先 merge_index 合并远端"
        );
    }
}

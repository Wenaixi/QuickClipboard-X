use std::collections::{HashMap, HashSet};

use super::chunk_manager::load_chunk;
use super::index_manager::load_index;
use super::types::{CloudRecord, SyncCollection, SyncIndexEntry, SyncReport};
use super::webdav_client::WebdavClient;

pub async fn download_all(
    client: &WebdavClient,
    _device_id: &str,
    force_download: bool,
) -> Result<SyncReport, String> {
    let mut report = SyncReport::default();
    let settings = crate::services::get_settings();
    let remote_tombstone_report = match super::tombstones_sync::download_tombstones(client).await {
        Ok(report) => report,
        Err(e) => {
            report.errors.push(format!("删除记录拉取失败: {}", e));
            SyncReport::default()
        }
    };
    report.pulled_clipboard += remote_tombstone_report.pulled_clipboard;
    report.pulled_favorites += remote_tombstone_report.pulled_favorites;
    report.pulled_groups += remote_tombstone_report.pulled_groups;
    report.pulled += remote_tombstone_report.pulled;
    let tombstone_states = if force_download {
        super::tombstones_sync::remote_tombstone_states(client)
            .await?
    } else {
        crate::services::database::sync_tombstone_states()?
    };

    if settings.webdav_sync_clipboard {
        let local_states = crate::services::database::webdav_history_record_states()?;
        let mut records_for_images = Vec::new();
        match download_collection(
            client,
            SyncCollection::History,
            force_download,
            &local_states,
            &tombstone_states,
        )
        .await
        {
            Ok(records) => {
                records_for_images.extend(records.iter().cloned());
                let changed = if records.is_empty() {
                    Vec::new()
                } else {
                    if force_download {
                        crate::services::database::webdav_repair_history_records(&records)?
                    } else {
                        crate::services::database::lan_upsert_history_records(&records)?
                    }
                };
                records_for_images.extend(changed.iter().cloned());
                let count = changed.len() as u32;
                report.pulled += count;
                report.pulled_clipboard += count;
                report
                    .pulled_items
                    .extend(changed.iter().map(|record| record.report_item("clipboard")));
            }
            Err(e) => report.errors.push(format!("剪贴板历史拉取失败: {}", e)),
        }
        if settings.webdav_sync_images {
            let mut image_ids = collect_record_image_ids(&records_for_images);
            image_ids.extend(history_image_ids_from_metas()?);
            download_images(client, image_ids).await?;
        }
    }

    if settings.webdav_sync_favorites {
        let local_states = crate::services::database::webdav_favorite_record_states()?;
        let mut records_for_images = Vec::new();
        match download_collection(
            client,
            SyncCollection::Favorites,
            force_download,
            &local_states,
            &tombstone_states,
        )
        .await
        {
            Ok(records) => {
                records_for_images.extend(records.iter().cloned());
                let changed = if records.is_empty() {
                    Vec::new()
                } else {
                    if force_download {
                        crate::services::database::webdav_repair_favorite_records(&records)?
                    } else {
                        crate::services::database::lan_upsert_favorite_records(&records)?
                    }
                };
                records_for_images.extend(changed.iter().cloned());
                let count = changed.len() as u32;
                report.pulled += count;
                report.pulled_favorites += count;
                report
                    .pulled_items
                    .extend(changed.iter().map(|record| record.report_item("favorites")));
            }
            Err(e) => report.errors.push(format!("收藏拉取失败: {}", e)),
        }
        if settings.webdav_sync_images {
            let mut image_ids = collect_record_image_ids(&records_for_images);
            image_ids.extend(favorite_image_ids_from_metas()?);
            download_images(client, image_ids).await?;
        }

        match super::groups_sync::download_groups(client, force_download, &tombstone_states).await {
            Ok(groups) => {
                let count = groups.len() as u32;
                report.pulled += count;
                report.pulled_groups += count;
                report.pulled_items.extend(groups.into_iter().map(|group| {
                    super::types::SyncReportItem {
                        category: "groups".to_string(),
                        id: group.name.clone(),
                        summary: group.name,
                        source_device_id: group.source_device_id,
                        updated_at: group.updated_at,
                    }
                }));
            }
            Err(e) => report.errors.push(format!("分组拉取失败: {}", e)),
        }
    }

    Ok(report)
}

async fn download_collection(
    client: &WebdavClient,
    collection: SyncCollection,
    force_download: bool,
    local_states: &HashMap<String, i64>,
    tombstone_states: &HashMap<String, i64>,
) -> Result<Vec<CloudRecord>, String> {
    let index = load_index(client, collection).await?;
    if index.entries.is_empty() {
        return Ok(Vec::new());
    }
    let collection_name = collection.dir();
    let mut selected_entries = HashMap::<String, SyncIndexEntry>::new();
    for (uuid, entry) in index.entries {
        if tombstone_states
            .get(&crate::services::database::tombstone_state_key(collection_name, &uuid))
            .map(|deleted_at| *deleted_at >= entry.updated_at)
            .unwrap_or(false)
        {
            continue;
        }

        if !force_download {
            if let Some(local_updated_at) = local_states.get(&uuid) {
                if *local_updated_at >= entry.updated_at {
                    continue;
                }
            }
        }

        selected_entries.insert(uuid, entry);
    }

    if selected_entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut chunk_ids = selected_entries
        .values()
        .map(|entry| entry.chunk)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    chunk_ids.sort_unstable();

    let mut out = Vec::new();
    for chunk_id in chunk_ids {
        let chunk = load_chunk(client, collection, chunk_id).await?;
        for (uuid, record) in chunk.records {
            let Some(entry) = selected_entries.get(&uuid) else {
                continue;
            };
            if entry.chunk != chunk_id {
                continue;
            }
            if record.updated_at < entry.updated_at {
                continue;
            }
            if tombstone_states
                .get(&crate::services::database::tombstone_state_key(collection_name, &record.uuid))
                .map(|deleted_at| *deleted_at >= record.updated_at)
                .unwrap_or(false)
            {
                continue;
            }
            out.push(record);
        }
    }

    out.sort_by(|a, b| {
        b.item_order
            .cmp(&a.item_order)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.uuid.cmp(&b.uuid))
    });
    Ok(out)
}

async fn download_images(client: &WebdavClient, image_ids: HashSet<String>) -> Result<(), String> {
    if image_ids.is_empty() {
        return Ok(());
    }

    let data_dir = crate::services::get_data_directory()?;
    let images_dir = data_dir.join("clipboard_images");
    std::fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    for image_id in image_ids {
        // 已存在且内容完好的本地 png 跳过(校验 PNG 魔数,半写/损坏文件视为缺失重下)
        let path = images_dir.join(format!("{}.png", image_id));
        if path.exists() {
            if let Ok(bytes) = std::fs::read(&path) {
                if is_png_bytes(&bytes) {
                    continue;
                }
            }
            // 存在但损坏:删除后走重下
            let _ = std::fs::remove_file(&path);
        }
        let Some(bytes) = client.get_bytes(&format!("files/{}.png", image_id)).await? else {
            continue;
        };
        if is_png_bytes(&bytes) {
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        } else {
            eprintln!("[WebDAV同步] 跳过非 PNG 内容: {}", image_id);
        }
    }

    Ok(())
}

// 检查是否为有效 PNG 魔数(8 字节签名)
fn is_png_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
}

fn collect_record_image_ids(records: &[CloudRecord]) -> HashSet<String> {
    let mut image_ids = HashSet::new();
    for record in records {
        collect_image_ids(&mut image_ids, record.image_id.as_deref());
    }
    image_ids
}

fn history_image_ids_from_metas() -> Result<HashSet<String>, String> {
    let mut image_ids = HashSet::new();
    for meta in crate::services::database::webdav_list_history_record_metas()? {
        collect_image_ids(&mut image_ids, meta.image_id.as_deref());
    }
    Ok(image_ids)
}

fn favorite_image_ids_from_metas() -> Result<HashSet<String>, String> {
    let mut image_ids = HashSet::new();
    for meta in crate::services::database::webdav_list_favorite_record_metas()? {
        collect_image_ids(&mut image_ids, meta.image_id.as_deref());
    }
    Ok(image_ids)
}

// 收集记录里的图片 ID,仅保留通过白名单校验的——恶意云端/本地脏数据
// 带 `../` 等路径段时直接丢弃,杜绝目录穿越写出任意文件。
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

    #[test]
    fn is_png_bytes_rejects_truncated_and_non_png() {
        assert!(super::is_png_bytes(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3]));
        assert!(!super::is_png_bytes(&[0x89, b'P', b'N', b'G']), "短于 8 字节不是 png");
        assert!(!super::is_png_bytes(&[]));
        assert!(!super::is_png_bytes(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0B]), "魔数末尾错");
        assert!(!super::is_png_bytes(&b"hello world"[..]));
    }
}

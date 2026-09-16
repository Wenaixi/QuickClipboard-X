use crate::services::webdav_sync::types::{SyncReport, SyncReportItem};

pub async fn pull_from_peer(device_id: &str) -> Result<SyncReport, String> {
    let peer = super::peer_store::list_peers()
        .into_iter()
        .find(|peer| peer.device_id == device_id)
        .ok_or_else(|| "未找到已配对设备".to_string())?;

    let mut report = SyncReport::default();

    // 计数语义已决:删除记录与新增记录一样是数据变更,tombstone 应用数
    // 如实计入 pulled(与 uploader 的 upload_tombstones 口径一致),不另立
    // removed 字段——避免报告结构跨端膨胀,前端已有 errors 通道可区分异常。
    let tombstones = super::http_client::fetch_peer_tombstones(&peer).await?;
    let _ = crate::services::database::upsert_sync_tombstones(&tombstones.tombstones)?;
    let tombstone_report = crate::services::database::apply_sync_tombstones(&tombstones.tombstones)?;
    report.pulled_clipboard += tombstone_report.history;
    report.pulled_favorites += tombstone_report.favorites;
    report.pulled_groups += tombstone_report.groups;
    report.pulled += tombstone_report.total();

    // LAN pull 必须按 since 增量拉取——全量拉取在记录数增长后每次同步
    // 都拉全表,且对端 list_history_records_since(since) 已支持增量,客户端
    // 不带 since 就等于丢弃了服务端的差量能力。增量点取本地 MAX(updated_at):
    // 更早的已落地记录再拉也不会被 upsert 改动(幂等),只省带宽与会话时长。
    let history = super::http_client::fetch_peer_history_records(
        &peer,
        crate::services::database::lan_local_history_max_updated_at()?,
    )
    .await?;
    let history_records = crate::services::database::filter_records_not_deleted(
        crate::services::database::COLLECTION_HISTORY,
        &history.records,
    )?;
    let changed_history = crate::services::database::lan_upsert_history_records(&history_records)?;
    let changed_history_count = changed_history.len() as u32;
    report.pulled_clipboard += changed_history_count;
    report.pulled += changed_history_count;
    report
        .pulled_items
        .extend(changed_history.iter().map(|record| record.report_item("clipboard")));

    let favorites = super::http_client::fetch_peer_favorite_records(
        &peer,
        crate::services::database::lan_local_favorites_max_updated_at()?,
    )
    .await?;
    let favorite_records = crate::services::database::filter_records_not_deleted(
        crate::services::database::COLLECTION_FAVORITES,
        &favorites.records,
    )?;
    let changed_favorites = crate::services::database::lan_upsert_favorite_records(&favorite_records)?;
    let changed_favorites_count = changed_favorites.len() as u32;
    report.pulled_favorites += changed_favorites_count;
    report.pulled += changed_favorites_count;
    report
        .pulled_items
        .extend(changed_favorites.iter().map(|record| record.report_item("favorites")));

    let groups = super::http_client::fetch_peer_groups(&peer).await?;
    let groups = crate::services::database::filter_groups_not_deleted(&groups.groups)?;
    let changed_groups = crate::services::database::lan_save_groups(&groups)?;
    let changed_groups_count = changed_groups.len() as u32;
    report.pulled_groups += changed_groups_count;
    report.pulled += changed_groups_count;
    report.pulled_items.extend(changed_groups.into_iter().map(|group| {
        SyncReportItem {
            category: "groups".to_string(),
            id: group.name.clone(),
            summary: group.name,
            source_device_id: group.source_device_id,
            updated_at: group.updated_at,
        }
    }));

    let image_peer = peer.clone();
    tauri::async_runtime::spawn(async move {
        fetch_missing_images_best_effort(&image_peer, &history_records).await;
        fetch_missing_images_best_effort(&image_peer, &favorite_records).await;
        // 全表重扫补拉历史缺失图片——与 WebDAV 侧 uploader/downloader 对称:
        // 仅覆盖本次增量记录时,网络抖动导致某次下载失败的图片不会再被取回
        // (该记录 updated_at 不变,增量 pull 不再携带),缺失会永久化。这里
        // 对历史/收藏全表 metas 再扫一遍 image_id,本地已存在的文件会被
        // fetch_missing_images_best_effort 内部跳过,成本只限于缺失项。
        scan_and_fetch_missing_images(&image_peer).await;
    });

    Ok(report)
}

async fn scan_and_fetch_missing_images(peer: &super::peer_store::PairedPeer) {
    let mut image_ids = std::collections::HashSet::new();
    for meta in crate::services::database::webdav_list_history_record_metas()
        .into_iter()
        .flatten()
    {
        if let Some(raw) = meta.image_id.as_deref() {
            for image_id in raw.split(',').map(|item| item.trim()) {
                if !image_id.is_empty() && super::files::is_valid_image_id(image_id) {
                    image_ids.insert(image_id.to_string());
                }
            }
        }
    }
    for meta in crate::services::database::webdav_list_favorite_record_metas()
        .into_iter()
        .flatten()
    {
        if let Some(raw) = meta.image_id.as_deref() {
            for image_id in raw.split(',').map(|item| item.trim()) {
                if !image_id.is_empty() && super::files::is_valid_image_id(image_id) {
                    image_ids.insert(image_id.to_string());
                }
            }
        }
    }
    for image_id in image_ids {
        match super::files::read_image_file(&image_id) {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                eprintln!("[局域网同步] 检查本地图片失败 image_id={} 错误={}", image_id, e);
                continue;
            }
        }
        match super::http_client::fetch_peer_image(peer, &image_id).await {
            Ok(Some(bytes)) => {
                if let Err(e) = super::files::save_image_file(&image_id, &bytes) {
                    eprintln!("[局域网同步] 保存拉取图片失败 image_id={} 错误={}", image_id, e);
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("[局域网同步] 拉取图片失败 image_id={} 错误={}", image_id, e),
        }
    }
}

async fn fetch_missing_images_best_effort(
    peer: &super::peer_store::PairedPeer,
    records: &[crate::services::webdav_sync::types::CloudRecord],
) {
    for image_id in super::files::collect_record_image_ids(records) {
        match super::files::read_image_file(&image_id) {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                eprintln!("[局域网同步] 检查本地图片失败 image_id={} 错误={}", image_id, e);
                continue;
            }
        }
        match super::http_client::fetch_peer_image(peer, &image_id).await {
            Ok(Some(bytes)) => {
                if let Err(e) = super::files::save_image_file(&image_id, &bytes) {
                    eprintln!("[局域网同步] 保存拉取图片失败 image_id={} 错误={}", image_id, e);
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("[局域网同步] 拉取图片失败 image_id={} 错误={}", image_id, e),
        }
    }
}

#[cfg(test)]
mod lan_incremental_pull_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // (LAN pull 全量):pull_from_peer 必须按 since 增量——对端
    // list_*_records_since(since) 早已支持,客户端不带 since 每次同步拉全表,
    // 记录增长后带宽与会话时长线性膨胀。增量点 = 本地 MAX(updated_at)。
    #[test]
    fn pull_passess_local_max_updated_at_as_since() {
        let src = strip_line_comments(&source_file("src/services/sync_transfer/lan/pull.rs"));
        let body = fn_body(&src, "pull_from_peer");
        for (fetch, since_fn) in [
            (
                "fetch_peer_history_records",
                "lan_local_history_max_updated_at",
            ),
            (
                "fetch_peer_favorite_records",
                "lan_local_favorites_max_updated_at",
            ),
        ] {
            let fetch_pos = body.find(fetch).unwrap_or_else(|| {
                panic!("pull 必须调用 {}", fetch)
            });
            let since_pos = body
                .find(since_fn)
                .unwrap_or_else(|| panic!("{} 必须传入 {}", fetch, since_fn));
            assert!(
                since_pos > fetch_pos && since_pos < fetch_pos + 600,
                "{} 必须紧跟本地 MAX(updated_at) 增量起点",
                fetch
            );
        }
        // 负向:禁止裸全量调用(不带 since 参数)
        assert!(
            !body.contains("authorized_get(peer, \"/qc-sync/records/history\")"),
            "历史拉取禁止裸全量调用,必须带 since"
        );
        assert!(
            !body.contains("authorized_get(peer, \"/qc-sync/records/favorites\")"),
            "收藏拉取禁止裸全量调用,必须带 since"
        );
    }

    // 图片补拉必须覆盖全表 metas 重扫——只处理本次增量记录时,网络抖动
    // 导致某次下载失败的图片永不重试(记录 updated_at 不变,增量 pull 不再
    // 携带)。断言 pull_from_peer 的 spawn 范围里同时存在按记录差量拉取与
    // scan_and_fetch_missing_images 全表重扫两个环节。
    #[test]
    fn pull_scans_all_metas_for_missing_images_besides_increment() {
        let src = strip_line_comments(&source_file("src/services/sync_transfer/lan/pull.rs"));
        let body = fn_body(&src, "pull_from_peer");
        let spawn_pos = body
            .find("fetch_missing_images_best_effort")
            .unwrap_or_else(|| panic!("图片补拉必须先按本次增量记录差量拉取"));
        let scan_pos = body
            .find("scan_and_fetch_missing_images")
            .unwrap_or_else(|| panic!("pull_from_peer 必须再对全表 metas 重扫缺失图片,否则历史缺失图片永不重试"));
        assert!(
            scan_pos > spawn_pos,
            "全表重扫必须紧跟增量图片补拉之后,作为兜底环节"
        );
        let scan_fn = &src[src.find("async fn scan_and_fetch_missing_images").expect("缺扫描函数")..];
        assert!(
            scan_fn.contains("webdav_list_history_record_metas")
                && scan_fn.contains("webdav_list_favorite_record_metas"),
            "全表重扫必须读历史与收藏两张表的 metas"
        );
    }
}

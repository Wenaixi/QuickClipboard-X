use crate::services;

#[tauri::command]
pub fn sync_transfer_get_mode_infos() -> Result<Vec<services::sync_transfer::SyncTransferModeInfo>, String> {
    Ok(services::sync_transfer::mode_infos())
}

#[tauri::command]
pub fn sync_transfer_lan_get_status() -> Result<services::sync_transfer::lan::LanRuntimeStatus, String> {
    Ok(services::sync_transfer::lan_status())
}

#[tauri::command]
pub async fn sync_transfer_lan_refresh_pairing_code(app: tauri::AppHandle) -> Result<services::sync_transfer::lan::PairingCodeView, String> {
    services::sync_transfer::lan_start_http_server(app).await?;
    Ok(services::sync_transfer::lan_refresh_pairing_code())
}

#[tauri::command]
pub fn sync_transfer_lan_list_paired_peers() -> Result<Vec<services::sync_transfer::lan::PairedPeerInfo>, String> {
    Ok(services::sync_transfer::lan_list_paired_peers())
}

#[tauri::command]
pub fn sync_transfer_lan_remove_paired_peer(device_id: String) -> Result<bool, String> {
    services::sync_transfer::lan_remove_paired_peer(&device_id)
}

#[tauri::command]
pub async fn sync_transfer_lan_pair_with_peer(
    base_url: String,
    pairing_code: String,
    app: tauri::AppHandle,
) -> Result<services::sync_transfer::lan::PairedPeerInfo, String> {
    services::sync_transfer::lan_start_http_server(app).await?;
    services::sync_transfer::lan_pair_with_peer(base_url, pairing_code).await
}

#[tauri::command]
pub async fn sync_transfer_lan_fetch_peer_snapshot(device_id: String) -> Result<services::sync_transfer::lan::LanSyncSnapshot, String> {
    services::sync_transfer::lan_fetch_peer_snapshot(&device_id).await
}

#[tauri::command]
pub fn sync_transfer_lan_get_local_snapshot() -> Result<services::sync_transfer::lan::LanSyncSnapshot, String> {
    services::sync_transfer::lan_snapshot()
}

#[tauri::command]
pub async fn sync_transfer_lan_discover_peers(timeout_ms: Option<u64>) -> Result<Vec<services::sync_transfer::lan::DiscoveredLanPeer>, String> {
    services::sync_transfer::lan_discover_peers(timeout_ms.unwrap_or(1200)).await
}

#[tauri::command]
pub fn sync_transfer_lan_get_auto_sync_status() -> Result<services::sync_transfer::lan::LanAutoSyncStatus, String> {
    Ok(services::sync_transfer::lan_auto_sync_status())
}

#[tauri::command]
pub async fn sync_transfer_lan_update_auto_sync_settings(
    settings: services::sync_transfer::lan::LanAutoSyncSettings,
    app: tauri::AppHandle,
) -> Result<services::sync_transfer::lan::LanAutoSyncSettings, String> {
    let settings = services::sync_transfer::lan_update_auto_sync_settings(settings)?;
    if settings.receive_enabled {
        services::sync_transfer::lan_start_http_server(app.clone()).await?;
    } else {
        services::sync_transfer::lan_stop_http_server().await;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn sync_transfer_lan_push_to_peer(device_id: String) -> Result<services::webdav_sync::SyncReport, String> {
    services::sync_transfer::lan_push_to_peer(&device_id).await
}

// 手动拉取镜像命令:与 push 对称,让用户在设置面板主动从对端拉回数据。
// pull.rs 实现完整(增量 since + 全表补图 + tombstone),此前仅有服务层
// 定义而无命令/前端入口,属"实现完成待接线"。
#[tauri::command]
pub async fn sync_transfer_lan_pull_to_peer(device_id: String, app: tauri::AppHandle) -> Result<services::webdav_sync::SyncReport, String> {
    let report = services::sync_transfer::lan_pull_from_peer(&device_id).await?;
    // 拉回的数据已落库,主窗口必须刷新——否则用户在设置面板看到 pulled N 条
    // 但历史/收藏/分组侧栏零变化(对称面:对端下推/WebDAV 下载都会 mark+emit,
    // 唯独手动拉取此前不刷新)。拉回空数据无需触发。
    if report.pulled > 0 {
        services::sync_transfer::mark_pull_refresh(&app);
    }
    Ok(report)
}

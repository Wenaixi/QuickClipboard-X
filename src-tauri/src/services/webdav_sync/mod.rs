pub mod chunk_manager;
pub mod cloud_files;
pub mod crypto;
pub mod downloader;
pub mod image_id; // 剪贴板图片 ID 白名单校验(webdav 同步下载/上传共用)
pub mod groups_sync;
pub mod index_manager;
pub mod local_state;
pub mod sync_scheduler;
pub mod tombstones_sync;
pub mod types;
pub mod uploader;
pub mod webdav_client;

pub use types::{SyncReport, WebdavStatus};

use std::sync::LazyLock;
use tokio::sync::Mutex as AsyncMutex;

use types::WebdavConfig;
use webdav_client::WebdavClient;

// 云文件(cloud_files)索引串行化锁:cloud_files/index.json 的读-改-写
// 是整块 PUT 无 merge(与 uploader 的 merge_index 不同),upload/delete
// 等操作若并发交叠会整块覆盖丢失彼此的 manifest。锁把索引读写临界
// 区串行化,防并发覆盖。download 只读 index 不写,不持锁无碍。
static CLOUD_FILES_MUTATION_LOCK: LazyLock<AsyncMutex<()>> =
    LazyLock::new(|| AsyncMutex::new(()));

// 持有云文件索引锁后再进入 cloud_files 写路径(上传/删除)。
// 锁内不 await 网络 IO 的调用方请勿加锁——避免锁面扩大拖慢读路径。
async fn cloud_files_lock() -> tokio::sync::MutexGuard<'static, ()> {
    CLOUD_FILES_MUTATION_LOCK.lock().await
}

pub async fn test_connection() -> Result<(), String> {
    let client = build_client().await?;
    client.test_connection().await
}

pub async fn upload() -> Result<SyncReport, String> {
    let report = sync_scheduler::upload_selected_parts(false)
        .await?
        .unwrap_or_default();
    Ok(sync_scheduler::store_manual_report("push", report))
}

pub(super) async fn download_raw(force_download: bool) -> Result<SyncReport, String> {
    // 持有全局同步锁,调度拉取/窗口显示拉取不得与上传事务交错
    let _tx_guard = sync_scheduler::SYNC_TX_LOCK.lock().await;
    let client = build_client().await?;
    let device_id = crate::services::sync_transfer::device_id();
    downloader::download_all(&client, &device_id, force_download).await
}

pub async fn download(force_download: bool) -> Result<SyncReport, String> {
    let report = download_raw(force_download).await?;
    Ok(sync_scheduler::store_manual_report("pull", report))
}

pub async fn upload_parts(
    upload_clipboard: bool,
    upload_favorites: bool,
    upload_groups: bool,
    upload_tombstones: bool,
) -> Result<SyncReport, String> {
    let client = build_client().await?;
    let device_id = crate::services::sync_transfer::device_id();
    uploader::upload_parts(
        &client,
        &device_id,
        upload_clipboard,
        upload_favorites,
        upload_groups,
        upload_tombstones,
    ).await
}

pub async fn upload_cloud_files_with_progress(
    requests: Vec<cloud_files::CloudFileUploadRequest>,
) -> Result<Vec<cloud_files::CloudFileUploadBatchItem>, String> {
    // 索引写临界区:upload 与 delete 并发整块覆盖丢 manifest,锁内完成
    // load_index→改→save_index 全链(网络 IO 在锁内,低并发无害)。
    let _lock = cloud_files_lock().await;
    let client = build_client().await?;
    cloud_files::upload_files_with_progress(&client, requests).await
}

pub async fn list_cloud_files() -> Result<Vec<cloud_files::CloudFileListItem>, String> {
    let client = build_client().await?;
    cloud_files::list_files(&client).await
}

pub async fn download_cloud_file(file_id: &str) -> Result<cloud_files::CloudFileDownloadResult, String> {
    let client = build_client().await?;
    cloud_files::download_file(&client, file_id).await
}

pub async fn delete_cloud_file(file_id: &str) -> Result<(), String> {
    // 索引写临界区:与 upload_cloud_files 串行,防整块覆盖丢 manifest。
    let _lock = cloud_files_lock().await;
    let client = build_client().await?;
    cloud_files::delete_file(&client, file_id).await
}

pub fn status() -> WebdavStatus {
    sync_scheduler::status()
}

pub fn start_scheduler() {
    sync_scheduler::start();
}

pub fn stop_scheduler() {
    sync_scheduler::stop();
}

pub fn notify_local_change(app: tauri::AppHandle, reason: &'static str) {
    sync_scheduler::notify_local_change(app, reason);
}

pub fn notify_main_window_shown(app: tauri::AppHandle) {
    sync_scheduler::notify_main_window_shown(app);
}

async fn build_client() -> Result<WebdavClient, String> {
    let settings = crate::services::get_settings();
    let webdav_url = settings.webdav_url.trim().to_string();
    let webdav_username = settings.webdav_username.trim().to_string();
    let webdav_root_path = if settings.webdav_root_path.trim().is_empty() {
        "quickclipboard".to_string()
    } else {
        settings.webdav_root_path.clone()
    };
    let password = if settings.webdav_username.trim().is_empty() {
        String::new()
    } else {
        crate::services::secure_credentials::get_webdav_password(
            &webdav_url,
            &webdav_username,
        )?
        .ok_or_else(|| "请先在设置中保存 WebDAV 密码".to_string())?
    };
    let encryption_password = crate::services::secure_credentials::get_webdav_encryption_password(
        &webdav_url,
        &webdav_username,
        &webdav_root_path,
    )?
    .ok_or_else(|| "请先设置 WebDAV 云端加密密码".to_string())?;
    let config = WebdavConfig {
        url: webdav_url,
        username: webdav_username,
        password,
        root_path: webdav_root_path,
    };
    let mut client = WebdavClient::new(config)?;
    client.enable_encryption(&encryption_password).await?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mod_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/services/webdav_sync/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 webdav_sync/mod.rs 源码失败")
    }

    fn stripped_source() -> String {
        mod_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 云文件索引写锁护栏(C2):upload_cloud_files_with_progress 与
    // delete_cloud_file 两个写路径必须各持 cloud_files_lock 进入——
    // cloud_files/index.json 是整块 PUT 无 merge,写路径并发交叠会覆盖
    // 丢 manifest。顺序断言:锁获取必须先于 build_client 后的写调用
    // (锁在函数体开头)。
    #[test]
    fn cloud_file_mutations_hold_index_lock() {
        let src = stripped_source();
        for fn_name in [
            "upload_cloud_files_with_progress",
            "delete_cloud_file",
        ] {
            let pos = src
                .find(&format!("pub async fn {fn_name}"))
                .unwrap_or_else(|| panic!("缺 {fn_name}"));
            let tail = &src[pos..];
            let end = tail
                .find("\npub ")
                .unwrap_or(tail.len());
            let body = &tail[..end];
            assert!(
                body.contains("cloud_files_lock().await"),
                "{fn_name} 必须持 cloud_files_lock 进入(防整块覆盖丢 manifest)"
            );
            let lock_pos = body
                .find("cloud_files_lock().await")
                .expect("缺锁调用");
            assert!(
                lock_pos < body.find("build_client()").unwrap_or(body.len()),
                "{fn_name} 锁获取必须先于 build_client(锁在临界区开头)"
            );
        }
    }
}

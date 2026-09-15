use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;

pub const LAN_UNAUTHORIZED: &str = "局域网设备未授权（配对已失效）";
const FILE_TRANSFER_BUFFER_SIZE: usize = 1024 * 1024;
const SYNC_REQUEST_TIMEOUT_SECS: u64 = 180;
const TRANSFER_CONNECT_TIMEOUT_SECS: u64 = 10;
const IMAGE_REQUEST_MAX_ATTEMPTS: usize = 3;
const IMAGE_REQUEST_RETRY_DELAYS_MS: [u64; 2] = [300, 800];

// 直传客户端整体请求超时:覆盖发送请求体(读本地文件流)与接收响应全程。
// 此前 build_transfer_client 只设 connect_timeout,发送端卡在流式写一半或
// 读响应挂起时没有兜底,断网/死固件下传输任务永久占住任务与连接。
// 5 分钟窗口内必须走完一个直传请求;超过视为网络异常由上层重试/报错。
const TRANSFER_REQUEST_TIMEOUT_SECS: u64 = 300;

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(SYNC_REQUEST_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn build_transfer_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(TRANSFER_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(TRANSFER_REQUEST_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanHttpClientConfig {
    pub base_url: String,
    pub peer_token: String,
}

impl LanHttpClientConfig {
    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.peer_token)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanHelloResponse {
    pub device_id: String,
    pub device_name: String,
    pub protocol: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairingConfirmPayload {
    device_id: String,
    device_name: String,
    base_url: String,
    pairing_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairingConfirmOutput {
    peer_token: String,
}

pub async fn pair_with_peer(base_url: String, pairing_code: String) -> Result<super::PairedPeerInfo, String> {
    let base_url = normalize_base_url(&base_url)?;
    let client = build_client();
    let hello = client
        .get(format!("{}/qc-sync/hello", base_url))
        .send()
        .await
        .map_err(|e| format!("连接局域网设备失败: {}", e))?;
    if !hello.status().is_success() {
        return Err(format!("读取局域网设备信息失败: {}", hello.status()));
    }
    let hello = hello
        .json::<LanHelloResponse>()
        .await
        .map_err(|e| format!("解析局域网设备信息失败: {}", e))?;
    if hello.protocol != "quickclipboard-sync-transfer-lan-http" {
        return Err("对方不是兼容的 QuickClipboard 同步/传输服务".to_string());
    }
    if hello.device_id == super::runtime::device_id() {
        return Err("不能配对当前设备自身".to_string());
    }
    let payload = PairingConfirmPayload {
        device_id: super::runtime::device_id(),
        device_name: super::runtime::device_name(),
        base_url: local_base_url(),
        pairing_code,
    };
    let confirm = client
        .post(format!("{}/qc-sync/pairing/confirm", base_url))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("发送配对请求失败: {}", e))?;
    if !confirm.status().is_success() {
        let status = confirm.status();
        let message = confirm
            .text()
            .await
            .unwrap_or_else(|_| "配对失败".to_string());
        return Err(format!("配对失败: {} {}", status, message));
    }
    let output = confirm
        .json::<PairingConfirmOutput>()
        .await
        .map_err(|e| format!("解析配对响应失败: {}", e))?;
    let mut peer = super::peer_store::PairedPeer::new(
        hello.device_id,
        hello.device_name,
        base_url,
        output.peer_token,
    );
    peer.last_seen_at_ms = Some(chrono::Utc::now().timestamp_millis());
    let info = peer.info();
    super::peer_store::upsert_peer(peer)?;
    Ok(info)
}

pub async fn fetch_peer_snapshot(peer: &super::peer_store::PairedPeer) -> Result<super::LanSyncSnapshot, String> {
    authorized_get(peer, "/qc-sync/snapshot").await
}

pub async fn fetch_peer_history_records(
    peer: &super::peer_store::PairedPeer,
    since_updated_at: Option<i64>,
) -> Result<super::LanRecordBatch, String> {
    authorized_get_since(peer, "/qc-sync/records/history", since_updated_at).await
}

pub async fn fetch_peer_favorite_records(
    peer: &super::peer_store::PairedPeer,
    since_updated_at: Option<i64>,
) -> Result<super::LanRecordBatch, String> {
    authorized_get_since(peer, "/qc-sync/records/favorites", since_updated_at).await
}

pub async fn fetch_peer_groups(peer: &super::peer_store::PairedPeer) -> Result<super::LanGroupBatch, String> {
    authorized_get(peer, "/qc-sync/groups").await
}

pub async fn fetch_peer_tombstones(peer: &super::peer_store::PairedPeer) -> Result<super::LanTombstoneBatch, String> {
    authorized_get(peer, "/qc-sync/tombstones").await
}

pub async fn push_peer_history_records(
    peer: &super::peer_store::PairedPeer,
    batch: super::LanRecordBatch,
) -> Result<super::LanRecordBatch, String> {
    authorized_post(peer, "/qc-sync/records/history", &batch).await
}

pub async fn push_peer_favorite_records(
    peer: &super::peer_store::PairedPeer,
    batch: super::LanRecordBatch,
) -> Result<super::LanRecordBatch, String> {
    authorized_post(peer, "/qc-sync/records/favorites", &batch).await
}

pub async fn push_peer_groups(
    peer: &super::peer_store::PairedPeer,
    batch: super::LanGroupBatch,
) -> Result<super::LanGroupBatch, String> {
    authorized_post(peer, "/qc-sync/groups", &batch).await
}

pub async fn push_peer_tombstones(
    peer: &super::peer_store::PairedPeer,
    batch: super::LanTombstoneBatch,
) -> Result<super::LanTombstoneBatch, String> {
    authorized_post(peer, "/qc-sync/tombstones", &batch).await
}

pub async fn fetch_peer_image(peer: &super::peer_store::PairedPeer, image_id: &str) -> Result<Option<Vec<u8>>, String> {
    let client = build_transfer_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let url = format!("{}/qc-sync/files/{}.png", config.base_url.trim_end_matches('/'), image_id);
    for attempt in 0..IMAGE_REQUEST_MAX_ATTEMPTS {
        let response = match client
            .get(&url)
            .header("Authorization", config.authorization_header())
            .header("X-Device-Id", super::runtime::device_id())
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) if should_retry_transport_error(&e) && attempt + 1 < IMAGE_REQUEST_MAX_ATTEMPTS => {
                wait_before_image_retry(attempt).await;
                continue;
            }
            Err(e) => return Err(format!("读取局域网图片失败: {}", e)),
        };
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!("读取局域网图片失败: {}", response.status()));
        }
        match response.bytes().await {
            Ok(bytes) => return Ok(Some(bytes.to_vec())),
            Err(e) if should_retry_transport_error(&e) && attempt + 1 < IMAGE_REQUEST_MAX_ATTEMPTS => {
                wait_before_image_retry(attempt).await;
                continue;
            }
            Err(e) => return Err(format!("读取局域网图片内容失败: {}", e)),
        }
    }
    Err("读取局域网图片失败: 多次重试后仍无法连接".to_string())
}

pub async fn push_peer_image(peer: &super::peer_store::PairedPeer, image_id: &str, bytes: Vec<u8>) -> Result<(), String> {
    let client = build_transfer_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let url = format!("{}/qc-sync/files/{}.png", config.base_url.trim_end_matches('/'), image_id);
    for attempt in 0..IMAGE_REQUEST_MAX_ATTEMPTS {
        let response = match client
            .put(&url)
            .header("Authorization", config.authorization_header())
            .header("X-Device-Id", super::runtime::device_id())
            .body(bytes.clone())
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) if should_retry_transport_error(&e) && attempt + 1 < IMAGE_REQUEST_MAX_ATTEMPTS => {
                wait_before_image_retry(attempt).await;
                continue;
            }
            Err(e) => return Err(format!("推送局域网图片失败: {}", e)),
        };
        if !response.status().is_success() {
            return Err(format!("推送局域网图片失败: {}", response.status()));
        }
        return Ok(());
    }
    Err("推送局域网图片失败: 多次重试后仍无法连接".to_string())
}

pub async fn send_peer_file_stream(
    peer: &super::peer_store::PairedPeer,
    file_name: &str,
    path: PathBuf,
    size: u64,
    reporter: Option<super::transfer::FileTransferProgressReporter>,
) -> Result<super::FileTransferResult, String> {
    let client = build_transfer_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("打开待传输文件失败: {}", e))?;
    if let Some(reporter) = reporter.as_ref() {
        reporter.emit("sending", 0);
    }
    let hasher = Arc::new(Mutex::new(Sha256::new()));
    let reader = ProgressHashReader::new(file, size, reporter.clone(), hasher.clone());
    let stream = ReaderStream::with_capacity(reader, FILE_TRANSFER_BUFFER_SIZE);
    let body = reqwest::Body::wrap_stream(stream);
    let response = client
        .put(format!(
            "{}/qc-transfer/files/{}",
            config.base_url.trim_end_matches('/'),
            encode_path_segment(file_name)
        ))
        .header("Authorization", config.authorization_header())
        .header("X-Device-Id", super::runtime::device_id())
        .header("Content-Length", size)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("发送局域网文件失败: {}", e))?;
    if !response.status().is_success() {
        if let Some(reporter) = reporter.as_ref() {
            reporter.emit("failed", 0);
        }
        return Err(format!("发送局域网文件失败: {}", response.status()));
    }
    let mut result = response
        .json::<super::FileTransferResult>()
        .await
        .map_err(|e| format!("解析局域网文件传输结果失败: {}", e))?;
    let local_sha256 = {
        let guard = hasher.lock().map_err(|_| "局域网文件校验状态异常".to_string())?;
        hex::encode(guard.clone().finalize())
    };
    if result.size != 0 && result.size != size {
        if let Some(reporter) = reporter.as_ref() {
            reporter.emit("failed", size);
        }
        return Err(format!("局域网文件大小校验失败: 本地 {} 字节，对方 {} 字节", size, result.size));
    }
    if let Some(remote_sha256) = result.sha256.as_deref() {
        if !remote_sha256.eq_ignore_ascii_case(&local_sha256) {
            if let Some(reporter) = reporter.as_ref() {
                reporter.emit("failed", size);
            }
            return Err("局域网文件内容校验失败，请重新发送".to_string());
        }
    } else {
        result.sha256 = Some(local_sha256);
    }
    if let Some(reporter) = reporter.as_ref() {
        reporter.emit("done", size);
    }
    Ok(result)
}

struct ProgressHashReader<R> {
    inner: R,
    sent: u64,
    total: u64,
    last_reported: u64,
    reporter: Option<super::transfer::FileTransferProgressReporter>,
    hasher: Arc<Mutex<Sha256>>,
}

impl<R> ProgressHashReader<R> {
    fn new(
        inner: R,
        total: u64,
        reporter: Option<super::transfer::FileTransferProgressReporter>,
        hasher: Arc<Mutex<Sha256>>,
    ) -> Self {
        Self {
            inner,
            sent: 0,
            total,
            last_reported: 0,
            reporter,
            hasher,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressHashReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let read = buf.filled().len().saturating_sub(before) as u64;
            if read > 0 {
                if let Ok(mut hasher) = self.hasher.lock() {
                    hasher.update(&buf.filled()[before..]);
                }
                self.sent = self.sent.saturating_add(read);
                let should_report = self.sent == self.total
                    || self.sent.saturating_sub(self.last_reported) >= FILE_TRANSFER_BUFFER_SIZE as u64;
                if should_report {
                    self.last_reported = self.sent;
                    if let Some(reporter) = self.reporter.as_ref() {
                        reporter.emit("sending", self.sent);
                    }
                }
            }
        }
        poll
    }
}

async fn authorized_get_since<T: serde::de::DeserializeOwned>(
    peer: &super::peer_store::PairedPeer,
    path: &str,
    since_updated_at: Option<i64>,
) -> Result<T, String> {
    let url = match since_updated_at {
        Some(since) => format!("{}?since={}", path, since),
        None => path.to_string(),
    };
    authorized_get(peer, &url).await
}

async fn authorized_get<T: serde::de::DeserializeOwned>(peer: &super::peer_store::PairedPeer, path: &str) -> Result<T, String> {
    let client = build_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let response = client
        .get(format!("{}{}", config.base_url.trim_end_matches('/'), path))
        .header("Authorization", config.authorization_header())
        .header("X-Device-Id", super::runtime::device_id())
        .send()
        .await
        .map_err(|e| format!("读取局域网同步数据失败({}): {}", path, e))?;
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(LAN_UNAUTHORIZED.to_string());
    }
    if !response.status().is_success() {
        return Err(format!("读取局域网同步数据失败({}): {}", path, response.status()));
    }
    response.json::<T>().await.map_err(|e| format!("解析局域网同步数据失败({}): {}", path, e))
}

async fn authorized_post<T, B>(peer: &super::peer_store::PairedPeer, path: &str, body: &B) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
    B: Serialize + ?Sized,
{
    let client = build_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let response = client
        .post(format!("{}{}", config.base_url.trim_end_matches('/'), path))
        .header("Authorization", config.authorization_header())
        .header("X-Device-Id", super::runtime::device_id())
        .json(body)
        .send()
        .await
        .map_err(|e| format!("推送局域网同步数据失败({}): {}", path, e))?;
    if !response.status().is_success() {
        return Err(format!("推送局域网同步数据失败({}): {}", path, response.status()));
    }
    response.json::<T>().await.map_err(|e| format!("解析局域网推送结果失败({}): {}", path, e))
}

fn should_retry_transport_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout()
}

async fn wait_before_image_retry(attempt: usize) {
    let delay_ms = IMAGE_REQUEST_RETRY_DELAYS_MS
        .get(attempt)
        .copied()
        .unwrap_or(*IMAGE_REQUEST_RETRY_DELAYS_MS.last().unwrap_or(&800));
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("局域网设备地址不能为空".to_string());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    Ok(format!("http://{}", trimmed))
}

fn local_base_url() -> String {
    let port = super::http_server::running_port().unwrap_or(super::DEFAULT_HTTP_PORT);
    format!("http://127.0.0.1:{}", port)
}

fn encode_path_segment(raw: &str) -> String {
    raw.bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => vec![byte as char],
            b' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", byte).chars().collect::<Vec<_>>(),
        })
        .collect()
}

// 探测对端是否已有指定图片(差量推送前问一次,避免每次都全量推)
// 服务端 GET /qc-sync/files/<id>.png 存在=200 / 不存在=404。
pub async fn peer_image_exists(peer: &super::peer_store::PairedPeer, image_id: &str) -> Result<bool, String> {
    let client = build_transfer_client();
    let config = LanHttpClientConfig {
        base_url: peer.base_url.clone(),
        peer_token: peer.peer_token.clone(),
    };
    let url = format!("{}/qc-sync/files/{}.png", config.base_url.trim_end_matches('/'), image_id);
    for attempt in 0..IMAGE_REQUEST_MAX_ATTEMPTS {
        let response = match client
            .get(&url)
            .header("Authorization", config.authorization_header())
            .header("X-Device-Id", super::runtime::device_id())
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) if should_retry_transport_error(&e) && attempt + 1 < IMAGE_REQUEST_MAX_ATTEMPTS => {
                wait_before_image_retry(attempt).await;
                continue;
            }
            Err(e) => return Err(format!("探测局域网图片失败: {}", e)),
        };
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !response.status().is_success() {
            return Err(format!("探测局域网图片失败: {}", response.status()));
        }
        return Ok(true);
    }
    Err("探测局域网图片失败: 多次重试后仍无法连接".to_string())
}

#[cfg(test)]
mod transfer_timeout_guards {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 直传客户端必须设整体请求超时:此前只有 connect_timeout,
    // 发送端卡在流式写一半/读响应挂起时没有任何兜底。
    #[test]
    fn transfer_client_sets_overall_request_timeout() {
        let src = strip_line_comments(&source_file("src/services/sync_transfer/lan/http_client.rs"));
        let body = fn_body(&src, "build_transfer_client");
        assert!(
            body.contains("TRANSFER_REQUEST_TIMEOUT_SECS"),
            "直传客户端必须设置整体请求超时"
        );
    }

    // serve 侧流式读(请求头/请求体/直传内容)与写响应必须套 IO 超时,
    // 否则对端半开连接/死固件会永久挂住处理任务。
    #[test]
    fn server_side_streaming_io_has_timeout() {
        let src = strip_line_comments(&source_file("src/services/sync_transfer/lan/http_server.rs"));
        for function in [
            "read_request",
            "read_request_body",
            "copy_exact_to_file",
            "write_response",
        ] {
            let body = fn_body(&src, function);
            assert!(
                body.contains("tokio::time::timeout"),
                "{} 必须对 IO 读写套超时",
                function
            );
        }
    }

    // WebDAV 客户端必须设整体请求超时:此前 Client::new() 零超时,
    // 同步/上传任务断网即永久挂起。
    #[test]
    fn webdav_client_sets_overall_request_timeout() {
        let src = strip_line_comments(&source_file("src/services/webdav_sync/webdav_client.rs"));
        let body = fn_body(&src, "new");
        assert!(
            body.contains("WEBDAV_REQUEST_TIMEOUT_SECS"),
            "WebDAV 客户端必须设置整体请求超时"
        );
    }
}

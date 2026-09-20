use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::webdav_sync::types::CloudRecord;

pub const MAX_DIRECT_TRANSFER_FILE_SIZE: u64 = 512 * 1024 * 1024;

static RESERVED_RECEIVED_FILE_PATHS: Lazy<Mutex<HashSet<PathBuf>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static RECEIVED_FILE_INDEX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

const RECEIVED_FILE_INDEX_NAME: &str = "index.json";

#[derive(Debug, Clone)]
pub struct ReceivedFileReservation {
    pub final_path: PathBuf,
    pub temp_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedFileMetadata {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub source_device_id: String,
    pub source_device_name: String,
    pub received_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ReceivedFileIndex {
    #[serde(default)]
    files: HashMap<String, ReceivedFileMetadata>,
}

pub fn collect_record_image_ids(records: &[CloudRecord]) -> Vec<String> {
    let mut ids = HashSet::new();
    for record in records {
        let Some(raw) = record.image_id.as_deref() else { continue; };
        for image_id in raw.split(',').map(|item| item.trim()).filter(|item| !item.is_empty()) {
            if is_valid_image_id(image_id) {
                ids.insert(image_id.to_string());
            }
        }
    }
    ids.into_iter().collect()
}

pub fn read_image_file(image_id: &str) -> Result<Option<Vec<u8>>, String> {
    if !image_exists(image_id)? {
        return Ok(None);
    }
    let path = image_path(image_id)?;
    std::fs::read(path).map(Some).map_err(|e| format!("读取局域网同步图片失败: {}", e))
}

// 仅判断图片文件是否存在，读取元数据即可，避免每次全量读图进内存
// （拉取前检查本地 100 张 5MB 图若逐个 read 会多读 ~500MB 磁盘 IO）。
pub fn image_exists(image_id: &str) -> Result<bool, String> {
    let path = image_path(image_id)?;
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.is_file()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("检查局域网同步图片文件失败: {}", e)),
    }
}

pub fn save_image_file(image_id: &str, bytes: &[u8]) -> Result<(), String> {
    let path = image_path(image_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建局域网同步图片目录失败: {}", e))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("保存局域网同步图片失败: {}", e))
}

pub fn outgoing_file_info(path: &str) -> Result<(String, PathBuf, u64), String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path).map_err(|e| format!("读取待传输文件信息失败: {}", e))?;
    if !metadata.is_file() {
        return Err("只能传输普通文件".to_string());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "文件名无效".to_string())?
        .to_string();
    Ok((file_name, path, metadata.len()))
}

pub fn prepare_received_file(file_name: &str) -> Result<ReceivedFileReservation, String> {
    let safe_name = sanitize_file_name(file_name)?;
    let dir = received_files_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建接收文件目录失败: {}", e))?;
    let mut reserved = RESERVED_RECEIVED_FILE_PATHS
        .lock()
        .map_err(|_| "接收文件路径状态异常".to_string())?;
    let mut reserved_paths = reserved.clone();
    reserved_paths.insert(received_file_index_path()?);
    let final_path = unique_path(&dir, &safe_name, &reserved_paths);
    reserved.insert(final_path.clone());
    let temp_path = dir.join(format!(".{}.qcpart", Uuid::new_v4()));
    Ok(ReceivedFileReservation { final_path, temp_path })
}

pub fn commit_received_file(reservation: &ReceivedFileReservation) -> Result<PathBuf, String> {
    let mut reserved = RESERVED_RECEIVED_FILE_PATHS
        .lock()
        .map_err(|_| "接收文件路径状态异常".to_string())?;
    let dir = reservation
        .final_path
        .parent()
        .ok_or_else(|| "接收文件目录无效".to_string())?;
    let file_name = reservation
        .final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "文件名无效".to_string())?;
    let final_path = if reservation.final_path.exists() {
        unique_path(dir, file_name, &reserved)
    } else {
        reservation.final_path.clone()
    };
    std::fs::rename(&reservation.temp_path, &final_path)
        .map_err(|e| format!("完成接收文件保存失败: {}", e))?;
    reserved.remove(&reservation.final_path);
    Ok(final_path)
}

pub fn discard_received_file(reservation: &ReceivedFileReservation) {
    if let Ok(mut reserved) = RESERVED_RECEIVED_FILE_PATHS.lock() {
        reserved.remove(&reservation.final_path);
    }
    let _ = std::fs::remove_file(&reservation.temp_path);
}

pub fn record_received_file(
    path: &Path,
    size: u64,
    sha256: &str,
    source_device_id: &str,
    source_device_name: &str,
) -> Result<(), String> {
    let _guard = RECEIVED_FILE_INDEX_LOCK
        .lock()
        .map_err(|_| "接收文件索引状态异常".to_string())?;
    let mut index = load_received_file_index()?;
    let path_key = received_file_path_key(path);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("file")
        .to_string();
    index.files.insert(path_key.clone(), ReceivedFileMetadata {
        path: path_key,
        name,
        size,
        sha256: sha256.to_string(),
        source_device_id: source_device_id.trim().to_string(),
        source_device_name: source_device_name.trim().to_string(),
        received_at: chrono::Utc::now().timestamp_millis(),
    });
    save_received_file_index(&index)
}

pub fn list_received_file_metadata() -> Result<Vec<ReceivedFileMetadata>, String> {
    let _guard = RECEIVED_FILE_INDEX_LOCK
        .lock()
        .map_err(|_| "接收文件索引状态异常".to_string())?;
    let index = load_received_file_index()?;
    Ok(index.files.into_values().collect())
}

pub fn remove_received_file_metadata(path: &Path) -> Result<(), String> {
    let _guard = RECEIVED_FILE_INDEX_LOCK
        .lock()
        .map_err(|_| "接收文件索引状态异常".to_string())?;
    let mut index = load_received_file_index()?;
    let path_key = received_file_path_key(path);
    let before = index.files.len();
    index.files.remove(&path_key);
    index.files.retain(|_, item| item.path != path_key);
    if index.files.len() != before {
        save_received_file_index(&index)?;
    }
    Ok(())
}

pub fn file_name_from_transfer_path(path: &str) -> Result<String, String> {
    let raw = path
        .strip_prefix("/qc-transfer/files/")
        .ok_or_else(|| "无效的局域网传输路径".to_string())?;
    sanitize_file_name(raw)
}

pub fn image_id_from_file_path(path: &str) -> Result<String, String> {
    let raw = path
        .strip_prefix("/qc-sync/files/")
        .ok_or_else(|| "无效的局域网文件路径".to_string())?
        .strip_suffix(".png")
        .ok_or_else(|| "仅支持 png 图片文件".to_string())?;
    if !is_valid_image_id(raw) {
        return Err("无效的图片 ID".to_string());
    }
    Ok(raw.to_string())
}

pub fn received_files_dir() -> Result<PathBuf, String> {
    Ok(crate::services::get_data_directory()?.join("sync_transfer_files"))
}

// 清理接收目录残留的半写 .qcpart 临时文件——发送端中断/接收端进程崩溃
// 时 prepare_received_file 创建的临时文件不会走到 rename/remove,下次启动
// 服务前清扫,避免堆积占用磁盘。
pub fn sweep_orphan_qcpart_files() -> Result<usize, String> {
    let dir = received_files_dir()?;
    if !dir.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("读取接收文件目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取接收文件目录项失败: {}", e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".qcpart") {
            let _ = std::fs::remove_file(entry.path());
            removed += 1;
        }
    }
    if removed > 0 {
        eprintln!("[LAN] 已清理 {} 个残留 .qcpart 临时文件", removed);
    }
    Ok(removed)
}

pub fn is_received_file_internal(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|name| name == RECEIVED_FILE_INDEX_NAME || name.starts_with('.') || name.ends_with(".qcpart"))
        .unwrap_or(false)
}

fn received_file_index_path() -> Result<PathBuf, String> {
    Ok(received_files_dir()?.join(RECEIVED_FILE_INDEX_NAME))
}

fn load_received_file_index() -> Result<ReceivedFileIndex, String> {
    let path = received_file_index_path()?;
    if !path.exists() {
        return Ok(ReceivedFileIndex::default());
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("读取接收文件索引失败: {}", e))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析接收文件索引失败: {}", e))
}

fn save_received_file_index(index: &ReceivedFileIndex) -> Result<(), String> {
    let path = received_file_index_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建接收文件索引目录失败: {}", e))?;
    }
    let bytes = serde_json::to_vec_pretty(index)
        .map_err(|e| format!("序列化接收文件索引失败: {}", e))?;
    std::fs::write(&path, bytes)
        .map_err(|e| format!("保存接收文件索引失败: {}", e))
}

fn received_file_path_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn image_path(image_id: &str) -> Result<PathBuf, String> {
    if !is_valid_image_id(image_id) {
        return Err("无效的图片 ID".to_string());
    }
    Ok(crate::services::get_data_directory()?
        .join("clipboard_images")
        .join(format!("{}.png", image_id)))
}

fn sanitize_file_name(raw: &str) -> Result<String, String> {
    let decoded = percent_decode(raw)?;
    let name = Path::new(&decoded)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "文件名无效".to_string())?
        .trim()
        .to_string();
    if name.is_empty() || name == "." || name == ".." {
        return Err("文件名无效".to_string());
    }
    // 拒绝点开头文件名(.env/.hidden 等):系统内部文件(index.json/.qcpart)由
    // 代码自建不走本函数,收件盒内以点开头一律视为内部文件,放行会造成"落盘
    // 可见但全部操作被拒"的幽灵文件(对端可控触发)。
    if name.starts_with('.') {
        return Err("文件名无效".to_string());
    }
    // 拒绝 Windows 保留设备名与结尾句点/空格:CON/COM1/NUL 等无法在
    // Windows 创建/重命名,结尾句点会被系统吞掉造成落盘名与显示名不符;
    // cloud_files.rs 已处理同族,LAN 侧对齐保持一致。
    let trimmed_name = name.trim_end_matches([' ', '.']);
    if trimmed_name != name {
        return Err("文件名无效".to_string());
    }
    let stem = trimmed_name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    ) {
        return Err("文件名无效".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains(':') {
        return Err("文件名包含非法字符".to_string());
    }
    Ok(name)
}

fn percent_decode(raw: &str) -> Result<String, String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("文件名编码无效".to_string());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| "文件名编码无效".to_string())?;
            let value = u8::from_str_radix(hex, 16).map_err(|_| "文件名编码无效".to_string())?;
            out.push(value);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).map_err(|_| "文件名编码无效".to_string())
}

fn unique_path(dir: &Path, file_name: &str, reserved: &HashSet<PathBuf>) -> PathBuf {
    let base = Path::new(file_name)
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("file");
    let ext = Path::new(file_name).extension().and_then(|ext| ext.to_str());
    let mut path = dir.join(file_name);
    let mut index = 1u32;
    while path.exists() || reserved.contains(&path) {
        let candidate = match ext {
            Some(ext) if !ext.is_empty() => format!("{} ({}).{}", base, index, ext),
            _ => format!("{} ({})", base, index),
        };
        path = dir.join(candidate);
        index = index.saturating_add(1);
    }
    path
}

pub(super) fn is_valid_image_id(image_id: &str) -> bool {
    !image_id.is_empty()
        && image_id.len() <= 128
        && image_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    // 护栏:启动清扫函数必须存在且只清 .qcpart,不动 index.json 与正常文件。
    #[test]
    fn sweep_orphan_qcpart_files_is_defined_and_targeted() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/sync_transfer/lan/files.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 files.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub fn sweep_orphan_qcpart_files")
            .expect("必须有 sweep_orphan_qcpart_files 清理函数");
        let rest = &stripped[start..];
        let end = rest
            .find("\nfn ")
            .or_else(|| rest.find("\n#["))
            .map(|i| start + i)
            .unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains(".qcpart"),
            "清扫函数必须按 .qcpart 后缀识别半写临时文件"
        );
        assert!(
            body.contains("ends_with(\".qcpart\")"),
            "清扫函数必须 ends_with .qcpart,不得误删 index.json 或正常文件"
        );
        assert!(
            body.contains("remove_file"),
            "清扫函数必须实际删除文件"
        );
    }

    // 护栏:start 必须绑定监听前清扫残留 .qcpart,防止旧会话半写文件
    // 在下次接收时被当作正常文件进入收件盒。
    #[test]
    fn http_server_start_sweeps_orphan_qcpart_first() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/sync_transfer/lan/http_server.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 http_server.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub async fn start")
            .expect("缺 http_server start");
        let rest = &stripped[start..];
        let stop_pos = rest
            .find("pub async fn stop")
            .expect("缺 http_server stop");
        // 以 stop 函数开头为界,取完整 start 函数体,不用固定字符窗口
        // (函数内注释剥除后行数不固定,魔数窗口会误切到监听行之外)。
        let body = &stripped[start..start + stop_pos];
        let sweep_pos = body
            .find("sweep_orphan_qcpart_files()")
            .expect("start 必须调用 .qcpart 清扫函数");
        let listen_pos = body
            .find("TcpListener::bind")
            .expect("start 必须绑定监听");
        assert!(
            sweep_pos < listen_pos,
            "清扫必须早于监听,否则新会话接收期间残留文件仍在"
        );
    }

    // 护栏:判断图片存在必须走元数据(image_exists),不得在拉取前把整图
    // 读进内存(read_image_file)——LAN 全表重扫时逐张全量读会多出百 MB
    // 级磁盘 IO。拉取路径改回 read 判存在即见红。
    #[test]
    fn image_existence_check_uses_metadata_not_full_read() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/sync_transfer/lan/pull.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 pull.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let body_start = stripped
            .find("fn scan_and_fetch_missing_images")
            .or_else(|| stripped.find("pub async fn scan_and_fetch_missing_images"))
            .expect("缺拉取扫描函数");
        let rest = &stripped[body_start..];
        let body_end = rest
            .find("\nfn ")
            .map(|i| body_start + i)
            .unwrap_or(stripped.len());
        let body = &stripped[body_start..body_end];
        assert!(
            body.contains("files::image_exists(&image_id)") || body.contains("files::image_exists(image_id)"),
            "拉取前判存在必须走 image_exists 元数据判断"
        );
        assert!(
            !body.contains("files::read_image_file(&image_id)"),
            "拉取判存在不得全量读图进内存"
        );
    }

    // 护栏:文件名净化必须拒绝点开头(与收件盒内部文件命名空间隔离)——
    // 放行会让 .env 等落盘可见但操作全拒,呈"幽灵文件"。删该守卫即见红。
    #[test]
    fn sanitize_rejects_dot_prefixed_file_names() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/sync_transfer/lan/files.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 files.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("fn sanitize_file_name")
            .expect("缺文件名净化函数");
        let rest = &stripped[start..];
        let end = rest
            .find("\nfn ")
            .map(|i| start + i)
            .unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains("name.starts_with('.')"),
            "净化必须拒绝点开头文件名"
        );
        assert!(
            body.contains("trim_end_matches([' ', '.'])"),
            "净化必须拒绝结尾句点/空格"
        );
        assert!(
            body.contains("\"CON\" | \"PRN\" | \"AUX\" | \"NUL\""),
            "净化必须拒绝 Windows 保留设备名"
        );
        assert!(
            !body.contains("name.contains('/')") || body.contains("name.contains('/') || name.contains('\\\\')"),
            "净化必须保留分隔符拒绝"
        );
    }
}

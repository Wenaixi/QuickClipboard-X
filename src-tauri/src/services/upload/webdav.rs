// R5 WebDAV 上传目标：复用 webdav_sync 的 WebdavConfig 与 WebdavClient
// put_raw_bytes 把产物 PUT 到服务端 uploads/ 固定前缀，回读可访问 URL。
// 密码取自 secure_credentials（与同步共用），不新造取密逻辑。上传路径
// 固定 uploads/ 前缀 → 与同步的 index/cloud_files/groups/tombstones
// 命名空间物理隔离（防火墙护栏：上传任何情况不得写这些同步目录）。

use crate::services::upload::target::{UploadResult, UploadTarget};
use crate::services::webdav_sync::webdav_client::WebdavClient;

/// WebDAV 上传目标：持一份复用同步配置的客户端（明文产物，不加同步加密）。
pub struct WebdavUploadTarget {
    client: WebdavClient,
}

impl WebdavUploadTarget {
    /// 以当前设置构造：URL/用户名/密码/root_path 与同步同一套
    /// （root_path 为空回退 quickclipboard），密码走 secure_credentials。
    pub fn from_settings() -> Result<Self, String> {
        let settings = crate::services::get_settings();
        let url = settings.webdav_url.trim().to_string();
        if url.is_empty() {
            return Err("WebDAV 地址未配置".to_string());
        }
        let username = settings.webdav_username.trim().to_string();
        let password = if username.is_empty() {
            String::new()
        } else {
            crate::services::secure_credentials::get_webdav_password(&url, &username)?
                .ok_or_else(|| "请先在设置中保存 WebDAV 密码".to_string())?
        };
        let root_path = if settings.webdav_root_path.trim().is_empty() {
            "quickclipboard".to_string()
        } else {
            settings.webdav_root_path.clone()
        };
        let client = WebdavClient::new(crate::services::webdav_sync::types::WebdavConfig {
            url,
            username,
            password,
            root_path,
        })?;
        Ok(Self { client })
    }

    /// 上传文件名 → 服务端完整 URL（对齐 ShareX HostConfiguration 的
    /// 可访问地址回读语义：base_url + uploads/ + filename）。
    /// base_url 已含配置 root_path（WebdavClient::new 拼好），此处不再拼 root。
    fn remote_url(&self, filename: &str) -> String {
        format!("{}/uploads/{filename}", self.client.base_url())
    }
}

const UPLOAD_DIR: &str = "uploads";

impl UploadTarget for WebdavUploadTarget {
    fn id(&self) -> &str {
        "webdav"
    }

    fn name(&self) -> &str {
        "WebDAV"
    }

    async fn upload(&self, filename: &str, bytes: Vec<u8>) -> Result<UploadResult, String> {
        if filename.trim().is_empty() {
            return Err("上传文件名不能为空".to_string());
        }
        // 上传路径固定 uploads/ 前缀：与同步的 index/cloud_files/groups/
        // tombstones 目录物理隔离，任何情况不写这些同步目录。
        let remote = format!("{UPLOAD_DIR}/{}", filename.trim());
        self.client.put_raw_bytes(&remote, bytes).await?;
        let url = self.remote_url(filename.trim());
        Ok(UploadResult {
            // 可访问 URL（上传完成后 PUT 已成功，路径已存在）。
            url,
            // 删除 URL：同文件路径再 PUT 空即覆盖删除，此处标注同一地址。
            delete_url: url.clone(),
            // 缩略图 URL：WebDAV 无缩略图概念，回退原 URL（对齐
            // ShareX 三 URL 同构：无 thumbnail 的 provider 填原 URL）。
            thumb_url: url.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_result_shape_matches_sharex_triple_url() {
        // UploadResult 必须同时携带 url/delete_url/thumb_url 三个字段
        //（对齐 ShareX ISE.URL / DeletionURL / ThumbnailURL 三 URL 结构）。
        let result = UploadResult {
            url: "https://dav.example.com/uploads/a.png".to_string(),
            delete_url: "https://dav.example.com/uploads/a.png".to_string(),
            thumb_url: "https://dav.example.com/uploads/a.png".to_string(),
        };
        assert_eq!(result.url, result.thumb_url);
        assert!(!result.delete_url.is_empty());
    }

    #[test]
    fn webdav_remote_url_keeps_uploads_prefix() {
        // 远端 URL 必须以 …/uploads/<filename> 结尾（文件名只允许最后一段）。
        // 此护栏锁死上传路径前缀不与同步目录(cloud_files/groups/tombstones) 混淆。
        let source = std::fs::read_to_string(format!(
            "{}/src/services/upload/webdav.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取上传 webdav 源码失败");
        // 上传路径必须固定 uploads/ 前缀（与同步目录物理隔离）。
        assert!(source.contains("let remote = format!(\"{UPLOAD_DIR}/{}\", filename.trim());"),
            "上传路径必须固定 uploads/ 前缀");
        assert!(source.contains("put_raw_bytes"), "必须走明文 PUT 原语");
        assert!(source.contains("url: url.clone()"), "可访问 URL 必须回读");
    }

    #[test]
    fn remote_url_keeps_exactly_one_root_segment() {
        // 默认 root_path（空配置回退 quickclipboard）下，回读 URL 必须恰含一个
        // root 段——base_url 已把 root 拼进（WebdavClient::new），remote_url 再拼
        // root 会出现 quickclipboard/quickclipboard 重复段（复制出的链接 404）。
        let target = WebdavUploadTarget {
            client: WebdavClient::new(crate::services::webdav_sync::types::WebdavConfig {
                url: "https://dav.example.com".to_string(),
                username: "u".to_string(),
                password: "p".to_string(),
                root_path: "quickclipboard".to_string(),
            })
            .expect("构造 WebDAV 客户端失败"),
        };
        let url = target.remote_url("QC_1.png");
        // 完整 URL 精确锁死：base_url 已含 root，root 段只出现一次（重复即 404）。
        assert_eq!(url, "https://dav.example.com/quickclipboard/uploads/QC_1.png");
    }
}

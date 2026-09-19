// R5 上传目标抽象（对齐 ShareX UploadersLib UploadResult 三 URL 同构）：
// trait UploadTarget 定义 id/name/upload，WebDAV target 复用 webdav_sync
// 的 WebdavConfig + WebdavClient::put_raw_bytes 做明文 PUT（产物直接落
// 服务端 uploads/ 目录，回读可访问 URL）。upload 返回 UploadResult
// { url, delete_url, thumb_url }；后续 GitHub/Imgur/自建 provider 按
// trait 扩展。护栏：上传路径固定 uploads/ 前缀，绝不触碰同步 index/
// tombstones/ 目录（同步与上传两个域的命名空间物理隔离）。

use std::fmt::Display;

/// 一次上传的结果（对齐 ShareX ISE.URL / DeletionURL / ThumbnailURL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadResult {
    pub url: String,
    pub delete_url: String,
    pub thumb_url: String,
}

impl Display for UploadResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

/// 上传目标：实现方给出稳定的 id/name，upload 把本地文件字节推送到
/// 目标并回读可访问 URL。
pub trait UploadTarget: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    async fn upload(&self, filename: &str, bytes: Vec<u8>) -> Result<UploadResult, String>;
}

// R5 上传目标抽象（对齐 ShareX UploadersLib UploadResult 三 URL 同构）：
// trait UploadTarget 定义 id/name/upload，WebDAV target 复用 webdav_sync
// 的 WebdavConfig + WebdavClient::put_raw_bytes 做明文 PUT（产物直接落
// 服务端 uploads/ 目录，回读可访问 URL）。upload 返回 UploadResult
// { url, delete_url, thumb_url }；后续 GitHub/Imgur/自建 provider 按
// trait 扩展。护栏：上传路径固定 uploads/ 前缀，绝不触碰同步 index/
// tombstones/ 目录（同步与上传两个域的命名空间物理隔离）。

use std::{fmt::Display, future::Future, pin::Pin};

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
/// 目标并回读可访问 URL。upload 返回 Boxed Future 而非 async fn——
/// async fn 在 trait 里默认不可 dyn 兼容（无法建 vtable），而派发
/// target_for 恰好要按 id 返回 Box<dyn UploadTarget>，box 化未来后
/// trait 可 dyn 化，调用面不变（调用方照常 .await）。
pub trait UploadTarget: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// upload 借用的 future 同时捕获 &self 与 &filename，二者必须共享
    /// 同一生命周期参数，Boxed Future 的 `'a` 才与借用一致（dyn 兼容）。
    fn upload<'a>(
        &'a self,
        filename: &'a str,
        bytes: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<UploadResult, String>> + Send + 'a>>;
}

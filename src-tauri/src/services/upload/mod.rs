// R5 上传框架模块入口：UploadTarget trait + WebDAV 目标的第一实现。
// 对外暴露 dispatch 用的目标构造与 UploadResult 类型；护栏锁死
// UploadResult 三 URL 结构 + 上传路径 uploads/ 前缀。

pub mod target;
pub mod webdav;

pub use target::{UploadResult, UploadTarget};

use webdav::WebdavUploadTarget;

/// 按目标 id 构造上传目标：当前仅 webdav 一个实现（后续 GitHub/Imgur/
/// 自建按 target.rs 的 trait 扩展）。未配置 WebDAV 时返回明确错误。
pub fn target_for(id: &str) -> Result<Box<dyn UploadTarget>, String> {
    match id {
        "webdav" => Ok(Box::new(WebdavUploadTarget::from_settings()?)),
        other => Err(format!("不支持的上传目标: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dispatch_rejects_unknown_ids() {
        // 未实现的目标必须明确报错（webdav 分发接线由模块护栏锁死）。
        assert!(target_for("github").is_err(), "未实现的目标必须拒绝");
        assert!(target_for("").is_err(), "空目标 id 必须拒绝");
    }

    #[test]
    fn upload_module_guards_trait_shape_and_uploads_prefix() {
        let module = std::fs::read_to_string(format!(
            "{}/src/services/upload/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取上传模块源码失败");
        let target = std::fs::read_to_string(format!(
            "{}/src/services/upload/target.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取上传目标源码失败");
        let webdav = std::fs::read_to_string(format!(
            "{}/src/services/upload/webdav.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取上传 webdav 源码失败");
        // UploadResult 必须三 URL 同构（对齐 ShareX ISE.URL/DeletionURL/
        // ThumbnailURL）。
        assert!(target.contains("pub struct UploadResult"), "必须定义 UploadResult");
        assert!(target.contains("pub url: String") && target.contains("pub delete_url: String")
            && target.contains("pub thumb_url: String"), "UploadResult 必须含三 URL 字段");
        // trait 必须可被后续 provider 扩展。
        assert!(target.contains("pub trait UploadTarget"), "必须定义 UploadTarget trait");
        // WebDAV 上传路径必须固定 uploads/ 前缀，不得写同步 index/tombstones。
        assert!(webdav.contains("UPLOAD_DIR"), "必须有 uploads 目录常量");
        // 派发必须把 webdav 交给 WebdavUploadTarget。
        assert!(module.contains("\"webdav\" => Ok(Box::new(WebdavUploadTarget::from_settings()?))"),
            "分发必须把 webdav 交给 WebdavUploadTarget");
    }
}
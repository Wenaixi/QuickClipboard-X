//! 剪贴板图片 ID 白名单校验 re-export core（生产代码逐字一致，避免双份源码漂移）。

pub use quickclipboard_core::services::webdav_sync::image_id::*;

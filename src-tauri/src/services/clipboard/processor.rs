//! 剪贴板内容处理层 re-export core（生产代码逐字一致，避免双份源码漂移）。

pub use quickclipboard_core::services::clipboard::processor::*;

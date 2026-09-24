//! 设置层 re-export core（生产代码逐字一致，避免双份源码漂移）。
//! storage 子模块仍以 pub mod 暴露，命令/维护层经 storage::SettingsStorage 引用。

pub use quickclipboard_core::services::settings::*;
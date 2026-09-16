// 文件盒窗口
//
// 多实例独立浮窗，承担 dropshelf 风格的文件暂存、发送与拖出。
// 阶段一只提供窗口骨架与基础命令，后续阶段补齐发送、紧凑态、热区。

pub mod commands;
mod manager;
mod storage;
mod types;
mod window;

pub use manager::{
    append_files_to_recent_or_new_shelf, clear_active_shelves, open_or_create_shelf,
    persisted_file_paths, schedule_startup_restore_persisted_shelves,
};
// 标签前缀供焦点排除列表按 transfer-shelf-{id} 枚举窗口——
// 文件盒标题可被重命名,标题过滤失效,只能按标签前缀识别自身窗口。
pub use types::LABEL_PREFIX;

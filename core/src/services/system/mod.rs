//! 系统层纯 Win32 支撑面（core 裁剪版）。
//!
//! 这里只保留第二阶段各域实际引用的最小 API，不做全量搬运：
//!   - `get_clipboard_source`：剪贴板来源（clipboard/processor.rs 采集元信息）
//!   - `raw_input::PASTE_INPUT_MARKER` + `get_physical_modifier_keys_state`：
//!     粘贴模拟注入标记与物理修饰键读取（paste/keyboard.rs）
//! 被裁掉的监视器线程、窗口枚举、应用过滤规则等后续按需迁回。

pub mod app_filter;
pub mod raw_input;

pub use app_filter::{get_clipboard_source, ClipboardSourceInfo, ClipboardSourceType};
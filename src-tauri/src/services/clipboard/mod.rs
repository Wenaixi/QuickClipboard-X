mod capture;
mod content_type;
mod monitor;
mod processor;
mod storage;

pub use monitor::{
    clear_last_content_cache, get_app_handle, is_monitor_running, pause_clipboard_monitor_for,
    set_app_handle, set_last_hash_contents, set_last_hash_file, set_last_hash_files,
    set_last_hash_text, start_clipboard_monitor, stop_clipboard_monitor,
};

pub(crate) use processor::ProcessedContent;
pub(crate) use storage::store_clipboard_item;

pub const INTERNAL_IMAGE_PATH_FORMAT: &str = "__QC_IMAGE_PNG_PATH__";

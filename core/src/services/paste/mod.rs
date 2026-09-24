pub mod paste_handler;
pub mod options;
pub mod text;
pub mod keyboard;
pub mod clipboard_content;
pub mod merge;

pub use options::PasteAction;
pub use paste_handler::{
    copy_clipboard_item, copy_favorite_item, paste_text_direct, paste_image_file,
    paste_clipboard_item_with_format, paste_clipboard_item_with_update,
    paste_favorite_item_with_format, paste_favorite_item_with_update,
};
pub use clipboard_content::{
    FilesData,
    set_clipboard_from_item, set_clipboard_text, set_clipboard_files, set_clipboard_image_file,
};
pub use merge::{copy_merged_items, paste_merged_items};
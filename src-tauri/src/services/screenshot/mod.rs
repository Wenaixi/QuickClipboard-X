mod actions;
mod ai_vision;
mod image_store;
mod session;

#[cfg(target_os = "windows")]
pub mod capture;

pub use actions::{choose_and_save_screenshot, copy_screenshot, copy_screenshot_text, emit_screenshot_history_update, prepare_clipboard_content, save_screenshot, ScreenshotActionError};
pub use ai_vision::{recognize_image, AiVisionError, AiVisionResult};
pub use image_store::{encode_and_store_png, prepare_pin_path, ImageStoreError, StoredScreenshot};

pub use session::{
    CleanupPlan,
    MainWindowVisibilityRevision,
    MonitorRect,
    SessionPhase,
    ScreenshotSessionManager,
    StartSessionResult,
};

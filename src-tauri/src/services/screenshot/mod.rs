mod actions;
mod ai_vision;
mod image_store;
mod session;

#[cfg(target_os = "windows")]
pub mod capture;

pub use actions::{choose_screenshot_save_destination, copy_screenshot, copy_screenshot_text, emit_screenshot_history_update, prepare_clipboard_content, save_screenshot, ScreenshotActionError};
pub use ai_vision::{recognize_image, test_configuration, validate_configuration, AiVisionError, AiVisionResult};
pub use image_store::{encode_and_store_png, prepare_pin_path, ImageStoreError, StoredScreenshot};

pub use session::{
    CleanupPlan,
    MainWindowVisibilityRevision,
    MonitorRect,
    SessionPhase,
    ScreenshotSessionManager,
    StartSessionResult,
};

use std::path::{Path, PathBuf};

use tauri_plugin_dialog::DialogExt;

use crate::commands::window::{emit_clipboard_updated_event, ClipboardUpdatedEventPayload};
use crate::services::clipboard::{store_clipboard_item, ProcessedContent};
use crate::services::database::{get_clipboard_count, get_clipboard_item_by_id, get_clipboard_item_position};
use super::image_store::image_raw_format;
use super::StoredScreenshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenshotActionError {
    Clipboard(String),
    Save(String),
    Pin(String),
    InvalidAction(String),
}

impl std::fmt::Display for ScreenshotActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard(error) => write!(f, "复制截图失败: {error}"),
            Self::Save(error) => write!(f, "保存截图失败: {error}"),
            Self::Pin(error) => write!(f, "贴图失败: {error}"),
            Self::InvalidAction(action) => write!(f, "不支持的截图动作: {action}"),
        }
    }
}

impl std::error::Error for ScreenshotActionError {}

fn stored_absolute_path(stored: &StoredScreenshot) -> &Path {
    &stored.absolute_path
}

pub fn prepare_clipboard_content(stored: &StoredScreenshot) -> ProcessedContent {
    ProcessedContent {
        content: format!("files:[{{\"path\":\"{}\",\"name\":\"{}.png\",\"size\":{},\"is_directory\":false,\"file_type\":\"image/png\"}}]", stored.relative_path, stored.image_id, stored.png_bytes),
        html_content: None,
        content_type: "image".to_string(),
        image_id: Some(stored.image_id.clone()),
        source_app: Some("QuickClipboard Screenshot".to_string()),
        source_icon_hash: None,
        raw_formats: vec![image_raw_format(&stored.relative_path)],
    }
}

fn store_screenshot_history(stored: &StoredScreenshot) -> Result<i64, ScreenshotActionError> {
    store_clipboard_item(prepare_clipboard_content(stored)).map_err(ScreenshotActionError::Clipboard)
}

pub fn copy_screenshot_text(text: &str) -> Result<i64, ScreenshotActionError> {
    if text.trim().is_empty() {
        return Err(ScreenshotActionError::Clipboard("AI 未识别出文本".to_string()));
    }
    let context = clipboard_rs::ClipboardContext::new()
        .map_err(|error| ScreenshotActionError::Clipboard(format!("创建剪贴板上下文失败: {error}")))?;
    crate::services::paste::set_clipboard_text(&context, text)
        .map_err(ScreenshotActionError::Clipboard)?;
    crate::services::clipboard::set_last_hash_text(text);
    store_clipboard_item(ProcessedContent {
        content: text.to_string(),
        html_content: None,
        content_type: "text".to_string(),
        image_id: None,
        source_app: Some("QuickClipboard Screenshot AI".to_string()),
        source_icon_hash: None,
        raw_formats: Vec::new(),
    })
    .map_err(ScreenshotActionError::Clipboard)
}

pub fn emit_screenshot_history_update(app: &tauri::AppHandle, clipboard_id: i64) -> Result<(), ScreenshotActionError> {
    let mut item = get_clipboard_item_by_id(clipboard_id)
        .map_err(ScreenshotActionError::Clipboard)?
        .ok_or_else(|| ScreenshotActionError::Clipboard("截图历史记录写入后无法读取".to_string()))?;
    crate::commands::clipboard::hydrate_clipboard_item_for_ui(&mut item);
    let payload = ClipboardUpdatedEventPayload {
        kind: "created".to_string(),
        item: Some(item),
        insert_index: get_clipboard_item_position(clipboard_id).map_err(ScreenshotActionError::Clipboard)?,
        total_count: Some(get_clipboard_count().map_err(ScreenshotActionError::Clipboard)?),
    };
    emit_clipboard_updated_event(app, Some(payload)).map_err(ScreenshotActionError::Clipboard)
}

pub fn copy_screenshot(stored: &StoredScreenshot) -> Result<i64, ScreenshotActionError> {
    let _monitor_guard = crate::services::clipboard::pause_clipboard_monitor_for(500);
    let path = stored_absolute_path(stored)
        .to_str()
        .ok_or_else(|| ScreenshotActionError::Clipboard("截图路径无效".to_string()))?;
    crate::services::paste::clipboard_content::set_clipboard_image_file(path)
        .map_err(ScreenshotActionError::Clipboard)?;
    crate::services::clipboard::set_last_hash_file(path);
    store_screenshot_history(stored)
}

pub fn save_screenshot(stored: &StoredScreenshot, destination: &Path) -> Result<(), ScreenshotActionError> {
    if destination.as_os_str().is_empty() {
        return Err(ScreenshotActionError::Save("目标路径为空".to_string()));
    }
    super::image_store::copy_file_atomic(stored_absolute_path(stored), destination)
        .map_err(|error| ScreenshotActionError::Save(error.to_string()))
}

pub fn choose_screenshot_save_destination(
    stored: &StoredScreenshot,
    app: &tauri::AppHandle,
) -> Result<Option<PathBuf>, ScreenshotActionError> {
    let filename = format!("QC_{}.png", stored.image_id);
    let Some(save_path) = app
        .dialog()
        .file()
        .set_file_name(filename)
        .blocking_save_file()
    else {
        return Ok(None);
    };

    save_path
        .as_path()
        .map(Path::to_path_buf)
        .map(Some)
        .ok_or_else(|| ScreenshotActionError::Save("无效的文件路径".to_string()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn screenshot_action_rejects_empty_save_destination_before_io() {
        let stored = StoredScreenshot {
            relative_path: "clipboard_images/0123456789abcdef.png".to_string(),
            absolute_path: PathBuf::from("missing.png"),
            image_id: "0123456789abcdef".to_string(),
            width: 1,
            height: 1,
            png_bytes: 70,
        };
        let error = save_screenshot(&stored, Path::new(""));
        assert!(matches!(error, Err(ScreenshotActionError::Save(_))));
    }

    #[test]
    fn screenshot_copy_preloads_monitor_hash_only_after_clipboard_write() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/screenshot/actions.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取截图动作源码失败");
        let copy_start = source.find("pub fn copy_screenshot").expect("缺少截图复制动作");
        let copy_body = &source[copy_start..source.find("pub fn save_screenshot").expect("缺少截图保存动作")];
        let write_index = copy_body
            .find("set_clipboard_image_file(path)")
            .expect("截图复制必须写入图片剪贴板");
        let hash_index = copy_body
            .find("set_last_hash_file(path)")
            .expect("截图复制必须预置监听器哈希");
        assert!(write_index < hash_index, "只有成功写入剪贴板后才能预置监听器去重哈希");
    }

    #[test]
    fn history_update_event_uses_store_id_and_notifies_frontend() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/screenshot/actions.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取截图动作源码失败");
        // emit_screenshot_history_update 必须用 store 返回的 clipboard_id 读取历史项。
        // 切片终点锚定测试模块：尾部含测试自身断言字面量会自指误命中（§10.4）。
        let emit_start = source.find("pub fn emit_screenshot_history_update").expect("缺少历史事件函数");
        let emit_end = source[emit_start..]
            .find("#[cfg(test)]")
            .map(|offset| emit_start + offset)
            .expect("缺少测试模块锚点");
        let emit_body = &source[emit_start..emit_end];
        assert!(emit_body.contains("get_clipboard_item_by_id(clipboard_id)"), "必须用 store id 读取历史项");
        assert!(emit_body.contains("kind: \"created\""), "必须发出 created 事件");
        assert!(emit_body.contains("emit_clipboard_updated_event"), "必须通知前端主窗口");
    }

    #[test]
    fn save_action_uses_atomic_copy_and_content_addressed_filename() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/screenshot/actions.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取截图动作源码失败");
        let save_start = source.find("pub fn save_screenshot").expect("缺少保存动作");
        let save_body = &source[save_start..source.find("pub fn choose_screenshot_save_destination").expect("缺少选择保存函数")];
        // 保存必须走原子复制：普通 fs::copy 半写入会损坏目标文件。
        assert!(save_body.contains("copy_file_atomic"), "保存必须走原子复制");
        // 默认文件名必须内容寻址，避免不同截图互相覆盖。
        let choose_start = source.find("pub fn choose_screenshot_save_destination").expect("缺少选择保存函数");
        let choose_body = &source[choose_start..source.find("#[cfg(test)]").expect("缺少测试锚点")];
        assert!(choose_body.contains("QC_{}.png"), "默认文件名必须使用内容寻址 id");
        assert!(choose_body.contains("blocking_save_file()"), "必须弹出系统保存对话框");
    }

    #[test]
    fn screenshot_history_content_contains_content_addressed_image_reference() {
        let stored = StoredScreenshot {
            relative_path: "clipboard_images/0123456789abcdef.png".to_string(),
            absolute_path: PathBuf::from("image.png"),
            image_id: "0123456789abcdef".to_string(),
            width: 16,
            height: 8,
            png_bytes: 70,
        };
        let content = prepare_clipboard_content(&stored);
        assert_eq!(content.content_type, "image");
        assert_eq!(content.image_id.as_deref(), Some("0123456789abcdef"));
        assert!(content.content.contains("clipboard_images/0123456789abcdef.png"));
        assert_eq!(content.raw_formats.len(), 1);
    }
}

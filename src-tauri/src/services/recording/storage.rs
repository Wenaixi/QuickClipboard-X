// R4 屏幕录制产物管理：录制结束后由调用方把帧序列编码为 PNG 单帧
// 快照（录制结果是逐帧 RGBA），落剪贴板历史走既有截图存储链路
// encode_and_store_png_bytes + copy_screenshot（剪贴板图片 + 历史入库）。
// GIF 字节由 gif_writer 单独负责编码；本模块负责产物落历史与 GIF 落盘。

use crate::services::screenshot::{encode_and_store_png_bytes, StoredScreenshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingResult {
    pub gif_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RecordingResult {
    /// 结果是否为空：GIF 字节或尺寸任一缺失即视为空（录制没产出有效帧）。
    pub fn is_empty(&self) -> bool {
        self.gif_bytes.is_empty() || self.width == 0 || self.height == 0
    }
}

// 录制结果校验（纯函数便于单测）：空结果与像素长度不匹配都拒绝落盘。
fn validate_result(result: &RecordingResult) -> Result<(), String> {
    if result.is_empty() {
        return Err("录制结果为空".to_string());
    }
    let expected = (result.width as usize) * (result.height as usize) * 4;
    if result.rgba.len() != expected {
        return Err("录制帧像素数据长度无效".to_string());
    }
    Ok(())
}

/// 录制产物落剪贴板历史：把首帧 RGBA 编码为 PNG 走内容寻址落盘
/// （复用截图存储），再走 copy_screenshot 语义（剪贴板图片 + 暂停
/// 监听 + 历史入库 + 事件通知）。单帧即录制代表帧，历史可预览/复制。
/// 完成后把 GIF 动图写入数据目录 recordings/，产物闭环不再只留在内存。
pub fn store_recording_to_history(
    app: &tauri::AppHandle,
    result: &RecordingResult,
) -> Result<StoredScreenshot, String> {
    validate_result(result)?;
    let stored = encode_and_store_png_bytes(encode_snapshot(result)?)
        .map_err(|error| format!("录制帧存储失败: {error}"))?;
    let clipboard_id = crate::services::screenshot::actions::copy_screenshot(&stored)
        .map_err(|error| format!("录制产物写入剪贴板失败: {error}"))?;
    crate::services::screenshot::actions::emit_screenshot_history_update(app, clipboard_id)
        .map_err(|error| format!("录制产物历史事件通知失败: {error}"))?;
    persist_gif(result)?;
    Ok(stored)
}

// 把 GIF 字节写入数据目录 recordings/。录制动图随记录持久化,不再只在
// 内存缓冲里编码后无处安放(停止录制后用户在文件系统也能取回动图)。
fn persist_gif(result: &RecordingResult) -> Result<(), String> {
    let data_dir = crate::services::get_data_directory()
        .map_err(|error| format!("获取数据目录失败: {error}"))?;
    let recordings_dir = data_dir.join("recordings");
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("创建录制动图目录失败: {error}"))?;
    let file_name = format!("record-{}.gif", chrono::Utc::now().timestamp_millis());
    let target = recordings_dir.join(file_name);
    std::fs::write(&target, &result.gif_bytes)
        .map_err(|error| format!("写入录制动图失败: {error}"))?;
    Ok(())
}

// 把首帧 RGBA 编码为 PNG 快照（复用截图编码器：encode_and_store_png_bytes
// 内部再解析尺寸；此处先把 RGBA 编码为 PNG 字节）。
fn encode_snapshot(result: &RecordingResult) -> Result<Vec<u8>, String> {
    use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(&result.rgba, result.width, result.height, ColorType::Rgba8.into())
        .map_err(|error| format!("录制帧 PNG 编码失败: {error}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_result_rejected() {
        // 空结果（无 GIF 字节）必须被校验拒绝，不落盘。
        let empty = RecordingResult { gif_bytes: vec![], width: 800, height: 600, rgba: vec![0; 800 * 600 * 4] };
        assert!(empty.is_empty());
        assert!(validate_result(&empty).is_err(), "空 GIF 必须拒绝");
    }

    #[test]
    fn zero_dimension_result_rejected() {
        // 尺寸为零即使带 GIF 前缀字节也是无效产物，必须拒绝。
        let zero_side = RecordingResult { gif_bytes: vec![1], width: 0, height: 600, rgba: Vec::new() };
        assert!(validate_result(&zero_side).is_err(), "零尺寸必须拒绝");
    }

    #[test]
    fn invalid_pixel_length_rejected() {
        // 像素长度必须与宽高吻合：不一致的数据断言存储前校验存在。
        let malformed = RecordingResult { gif_bytes: vec![1], width: 2, height: 2, rgba: vec![1, 2, 3] };
        assert!(!malformed.is_empty());
        assert!(validate_result(&malformed).is_err(), "像素长度不一致必须拒绝");
    }

    #[test]
    fn valid_result_passes_validation() {
        // 合法结果（GIF 非空 + 尺寸吻合四通道像素）必须通过校验。
        let okay = RecordingResult { gif_bytes: vec![1], width: 2, height: 2, rgba: vec![0; 16] };
        assert!(validate_result(&okay).is_ok(), "合法结果必须通过");
    }

    #[test]
    fn storage_guards_require_history_path_and_clipboard_call() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/recording/storage.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取录制存储源码失败");
        // 产物必须走到截图历史存储链路。
        assert!(source.contains("encode_and_store_png_bytes"), "产物必须走截图历史存储");
        assert!(source.contains("copy_screenshot(&stored)"), "产物必须走剪贴板+历史链路");
        assert!(source.contains("PngEncoder::new(&mut bytes)"), "必须编码 PNG 快照");
        // 历史事件通知必须用 copy 返回的真实记录 ID，不得用假 0 号记录。
        assert!(
            source.contains("emit_screenshot_history_update(app, clipboard_id)"),
            "历史事件必须用 copy 的真实记录 ID"
        );
    }

    // 录制动图必须落盘持久化:gif_writer 编码出的 GIF 字节不能只在内存
    // 缓冲里编码后无处安放——停止录制后用户应在文件系统能取回动图。
    #[test]
    fn recording_persists_gif_to_data_dir() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/recording/storage.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取录制存储源码失败");
        // 存储入口必须先落历史再把动图落盘(闭环:快照入历史 + 动图持久化)。
        let store_body = {
            let start = source
                .find("pub fn store_recording_to_history")
                .expect("缺 store_recording_to_history");
            let end = source
                .find("// 把首帧 RGBA 编码为 PNG 快照")
                .expect("缺 encode_snapshot 注释");
            &source[start..end]
        };
        assert!(
            store_body.contains("persist_gif(result)"),
            "落历史后必须把 GIF 动图落盘"
        );
        // 动图落盘必须写到数据目录 recordings/ 下,带独立子目录不污染图片库。
        let persist_body = {
            let start = source
                .find("fn persist_gif(result: &RecordingResult)")
                .expect("缺 persist_gif");
            let end = start + 900;
            &source[start..end]
        };
        assert!(
            persist_body.contains("data_dir.join(\"recordings\")"),
            "GIF 必须落盘到数据目录 recordings/"
        );
        assert!(
            persist_body.contains("std::fs::write(&target, &result.gif_bytes)"),
            "必须把 GIF 字节写入落盘文件"
        );
    }
}
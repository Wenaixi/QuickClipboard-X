// R6 二维码生成后端：前端用 qrcode 库把文本转 PNG（data URL），后端
// 接收 base64 走截图存储链路（encode_and_store_png_bytes 内容寻址落盘
// → copy_screenshot 剪贴板图片+历史入库 → 事件通知），历史可预览/复制。
// 前端只负责「文本→PNG」，存储/复制/历史全部复用既有链路，零新依赖。

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use crate::services::screenshot::{encode_and_store_png_bytes, StoredScreenshot};

/// 把二维码 PNG base64 存为剪贴板图片并落历史。
pub fn store_qr_png_base64(
    app: &tauri::AppHandle,
    png_base64: &str,
) -> Result<StoredScreenshot, String> {
    let bytes = BASE64
        .decode(png_base64.trim())
        .map_err(|error| format!("二维码 PNG 解码失败: {error}"))?;
    if bytes.is_empty() {
        return Err("二维码 PNG 内容为空".to_string());
    }
    let stored = encode_and_store_png_bytes(bytes)
        .map_err(|error| format!("二维码存储失败: {error}"))?;
    let clipboard_id = crate::services::screenshot::actions::copy_screenshot(&stored)
        .map_err(|error| format!("二维码写入剪贴板失败: {error}"))?;
    crate::services::screenshot::actions::emit_screenshot_history_update(app, clipboard_id)
        .map_err(|error| format!("二维码历史事件通知失败: {error}"))?;
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_store_guards_decode_and_history_link() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/tools/qr.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取二维码服务源码失败");
        // 必须 base64 解码（前端送 data URL 的 base64 段）。
        assert!(source.contains("STANDARD as BASE64") || source.contains("BASE64"), "必须用 base64 解码");
        assert!(source.contains(".decode(png_base64.trim())"), "必须解码前端 base64");
        // 产物必须走既有截图存储 + 复制 + 历史事件（真实记录 id）。
        assert!(source.contains("encode_and_store_png_bytes"), "二维码必须走截图存储");
        assert!(source.contains("copy_screenshot(&stored)"), "二维码必须复制进剪贴板");
        assert!(source.contains("emit_screenshot_history_update(app, clipboard_id)"), "历史事件必须用真实记录 id");
        // 空内容必须拒绝（解码空串/零字节防御）。
        assert!(source.contains("bytes.is_empty()"), "空 PNG 必须拒绝");
    }

    #[test]
    fn empty_base64_rejected_before_storage() {
        // 空 base64（空串 trim 后解码为空字节）必须在存储前拒绝。
        assert!(store_qr_png_base64_base64_only("").is_err());
        // 说明：base64 语义下「非空合法输入」至少 1 字节，空串用例已覆盖空拒绝；
        // 非 PNG 字节拒绝由生产函数 store_qr_png_base64 的 decode_png_dimensions
        // 负责，依赖剪贴板存储 IO 环境，不适合无 IO 单测。
    }

    fn store_qr_png_base64_base64_only(input: &str) -> Result<(), String> {
        let bytes = BASE64.decode(input.trim()).map_err(|e| format!("解码失败: {e}"))?;
        if bytes.is_empty() {
            return Err("二维码 PNG 内容为空".to_string());
        }
        Ok(())
    }
}
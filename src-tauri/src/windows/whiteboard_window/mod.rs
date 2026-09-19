// R6 白板窗口（对齐 ShareX RegionCaptureTasks 的截图白板标注语义）：
// 透明全屏画布窗，可画（钢笔/直线/箭头/矩形/椭圆）+ 清空 + 保存当前
// 白板为 PNG（复制进剪贴板历史，复用截图存储链路）+ Esc 关闭。
// 生命周期同取色器：按需创建、前端自关、标签防重入。

use tauri::Manager;

pub const WHITEBOARD_WINDOW_LABEL: &str = "whiteboard";

// 取全局 AppHandle（setup 阶段由 store::init 注册；未注册即尚未就绪）。
fn app_handle() -> Result<tauri::AppHandle, String> {
    crate::services::store::app_handle_raw().ok_or_else(|| "应用尚未初始化".to_string())
}

/// 打开白板窗口（已存在则聚焦不重建）。
pub fn open_whiteboard() -> Result<(), String> {
    let app = app_handle()?;
    if let Some(window) = app.get_webview_window(WHITEBOARD_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        WHITEBOARD_WINDOW_LABEL,
        tauri::WebviewUrl::App("windows/whiteboard/index.html".into()),
    )
    .title("白板")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .focused(true)
    .fullscreen(true)
    .shadow(false)
    .build()
    .map(|_| ())
    .map_err(|error| format!("创建白板窗口失败: {error}"))?;

    // 新窗口加入自身窗口排除列表——白板窗全屏透明置顶 focused 建窗,
    // 不排除则聚焦白板会被记为 LAST_FOCUS_HWND,关闭后句柄失效,
    // 恢复焦点只清记录,外部应用焦点归还失败。
    #[cfg(windows)]
    crate::services::system::focus::refresh_excluded_hwnds(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whiteboard_guards_fullscreen_transparent_and_reuse_label() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/whiteboard_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取白板窗口源码失败");
        // 白板必须全屏（覆盖整屏画布，对齐 ShareX 白板语义）。
        assert!(source.contains(".fullscreen(true)"), "白板窗必须全屏");
        assert!(source.contains(".transparent(true)"), "白板窗必须透明");
        // 复用标签防重入：已存在只聚焦不重建。
        assert!(source.contains("WHITEBOARD_WINDOW_LABEL"), "必须复用标签防重入");
    }

    // 新窗口必须加入自身窗口排除列表——白板窗全屏透明置顶 focused 建窗,
    // 不排除则聚焦白板会被记为 LAST_FOCUS_HWND,关闭后句柄失效,
    // 恢复焦点只清记录,外部应用焦点归还失败。
    #[test]
    fn whiteboard_window_refreshes_excluded_hwnds_after_build() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/whiteboard_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取白板窗口源码失败");
        assert!(
            source.contains("refresh_excluded_hwnds(&app)"),
            "创建白板窗口后必须刷新自身窗口排除列表"
        );
    }
}
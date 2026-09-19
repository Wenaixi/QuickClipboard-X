// R6 屏幕标尺窗口（对齐 ShareX ScreenRuler）：透明置顶可调整大小的
// 小窗，前端画横纵刻度 + 显示当前宽高像素（跟随窗口大小实时变化）。
// 窗口生命周期与取色器同款：按需创建、前端自关、标签防重入。

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub const RULER_WINDOW_LABEL: &str = "ruler";

// 取全局 AppHandle（setup 阶段由 store::init 注册；未注册即尚未就绪）。
fn app_handle() -> Result<tauri::AppHandle, String> {
    crate::services::store::app_handle_raw().ok_or_else(|| "应用尚未初始化".to_string())
}

/// 打开标尺窗口（复用标签防重入：已存在则聚焦不重建）。
pub fn open_ruler() -> Result<(), String> {
    let app = app_handle()?;
    if let Some(window) = app.get_webview_window(RULER_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        RULER_WINDOW_LABEL,
        WebviewUrl::App("windows/ruler/index.html".into()),
    )
    .title("屏幕标尺")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .inner_size(400.0, 300.0)
    .min_inner_size(120.0, 80.0)
    .visible(true)
    .focused(true)
    .shadow(false)
    .build()
    .map(|_| ())
    .map_err(|error| format!("创建屏幕标尺窗口失败: {error}"))?;

    // 新窗口加入自身窗口排除列表——标尺窗透明置顶 focused 建窗,不排除
    // 则聚焦标尺会被记为 LAST_FOCUS_HWND,恢复焦点把焦点设回隐藏自身窗口。
    #[cfg(windows)]
    crate::services::system::focus::refresh_excluded_hwnds(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_guards_transparent_topmost_resizable() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/ruler_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取标尺窗口源码失败");
        // 标尺窗必须透明（可透视桌面）置顶（常显于其他窗口上方）可调整
        // 大小（用户拉窗即改测量范围，对齐 ShareX 标尺语义）。
        assert!(source.contains(".transparent(true)"), "标尺窗必须透明");
        assert!(source.contains(".always_on_top(true)"), "标尺窗必须置顶");
        assert!(source.contains(".resizable(true)"), "标尺窗必须可调大小");
        // 复用标签防重入：已存在只聚焦不重建。
        assert!(source.contains("RULER_WINDOW_LABEL"), "必须复用标签防重入");
    }

    // 新窗口必须加入自身窗口排除列表——标尺窗透明置顶 focused 建窗,
    // 不排除则聚焦标尺会被记为 LAST_FOCUS_HWND,恢复焦点把焦点设回隐藏
    // 自身窗口。
    #[test]
    fn ruler_window_refreshes_excluded_hwnds_after_build() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/ruler_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取标尺窗口源码失败");
        assert!(
            source.contains("refresh_excluded_hwnds(&app)"),
            "创建标尺窗口后必须刷新自身窗口排除列表"
        );
    }
}

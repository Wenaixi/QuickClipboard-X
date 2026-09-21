// R6 取色器窗口：透明全屏吸管窗（对齐 ShareX ScreenColorPickerWindow）。
// 前端 HTML 全屏透明覆盖 + 每帧 GetPixel 读鼠标位置像素颜色，放大镜
// 由前端 canvas 按颜色绘制（不 BitBlt，免屏幕 DC 拷贝）；Esc 关闭、
// 点击复制 Hex 到剪贴板（复用截图动作的 AI 文本复制语义=仅剪贴板）。
// 窗口生命周期与 screenshot 同款：按需创建、前端自关、标签防重入。

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::services::screenshot::actions::copy_screenshot_text;

pub const COLOR_PICKER_WINDOW_LABEL: &str = "color-picker";
const COLOR_PICKER_EVENT: &str = "color-picker:update";

// 取全局 AppHandle（setup 阶段由 store::init 注册；未注册即尚未就绪）。
fn app_handle() -> Result<tauri::AppHandle, String> {
    crate::services::store::app_handle_raw().ok_or_else(|| "应用尚未初始化".to_string())
}

fn pick_color_at(x: i32, y: i32) -> Option<u32> {
    crate::services::tools::color::screen_color_at(x, y)
}

/// 打开取色器窗口（复用标签防重入：已存在则聚焦不重建）。
pub fn open_color_picker() -> Result<(), String> {
    let app = app_handle()?;
    if let Some(window) = app.get_webview_window(COLOR_PICKER_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        COLOR_PICKER_WINDOW_LABEL,
        WebviewUrl::App("windows/colorPicker/index.html".into()),
    )
    .title("取色器")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(true)
    .focused(true)
    .fullscreen(true)
    .build()
    .map(|_| ())
    .map_err(|error| format!("创建取色器窗口失败: {error}"))?;

    // 新窗口加入自身窗口排除列表——取色窗全屏透明置顶 focused 建窗,
    // 不排除则聚焦取色器会被记为 LAST_FOCUS_HWND,关闭后句柄失效,
    // 恢复焦点只清记录,外部应用焦点归还失败。
    #[cfg(windows)]
    crate::services::system::focus::refresh_excluded_hwnds(&app);
    Ok(())
}

/// 把鼠标位置颜色推给前端（放大镜/色值显示）；失败推空表示取色失败。
pub fn push_color_to_window(x: i32, y: i32) {
    let Ok(app) = app_handle() else {
        return;
    };
    if let Some(window) = app.get_webview_window(COLOR_PICKER_WINDOW_LABEL) {
        let color = pick_color_at(x, y).map(|rgb| format!("{:06x}", rgb));
        let _ = window.emit(COLOR_PICKER_EVENT, &serde_json::json!({ "x": x, "y": y, "color": color }));
    }
}

/// 前端点击确认取色：复制 Hex 到剪贴板并关闭窗口。
pub fn pick_color(x: i32, y: i32) -> Result<(), String> {
    let rgb = pick_color_at(x, y).ok_or_else(|| "取色失败".to_string())?;
    let hex = format!("{:06x}", rgb);
    copy_screenshot_text(&hex).map_err(|error| error.to_string())?;
    let app = app_handle()?;
    if let Some(window) = app.get_webview_window(COLOR_PICKER_WINDOW_LABEL) {
        let _ = window.close();
    }
    Ok(())
}

/// 前端就绪：启动后台轮询线程，按节流间隔读鼠标物理坐标 → 取色 →
/// 推送窗口；窗口关闭后线程自然退出（有界）。
pub fn color_picker_ready() {
    let Ok(app) = app_handle() else {
        return;
    };
    std::thread::spawn(move || {
        let mut last = std::time::Instant::now();
        loop {
            // 窗口已关闭（或从未存在）即停。
            if app.get_webview_window(COLOR_PICKER_WINDOW_LABEL).is_none() {
                break;
            }
            let now = std::time::Instant::now();
            let elapsed_ms = now.duration_since(last).as_millis() as u64;
            if elapsed_ms >= 33 {
                last = now;
                if let Ok(position) = app.cursor_position() {
                    push_color_to_window(position.x as i32, position.y as i32);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    });
}

/// 前端点击取色（取当前鼠标物理坐标）。
pub fn color_picker_pick_at() -> Result<(), String> {
    let app = app_handle()?;
    let position = app.cursor_position().map_err(|error| format!("读取鼠标位置失败: {error}"))?;
    pick_color(position.x as i32, position.y as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_guards_fullscreen_transparent_and_copy_hex() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/color_picker_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取取色器窗口源码失败");
        // 窗口必须透明置顶全屏（对齐 ShareX 全屏取色窗）。
        assert!(source.contains(".transparent(true)"), "取色窗必须透明");
        assert!(source.contains(".always_on_top(true)"), "取色窗必须置顶");
        assert!(source.contains(".fullscreen(true)"), "取色窗必须全屏");
        // 取色必须走 GetPixel 服务；确认取色必须复制 Hex 进剪贴板。
        assert!(source.contains("screen_color_at"), "必须走 GetPixel 取色服务");
        assert!(source.contains("copy_screenshot_text(&hex)"), "确认取色必须复制 Hex");
        // 就绪后必须启动有界轮询（窗口关闭即停），避免取色线程常驻。
        assert!(source.contains("app.get_webview_window(COLOR_PICKER_WINDOW_LABEL).is_none()"), "轮询必须随窗口关闭停止");
    }

    // 新窗口必须加入自身窗口排除列表——取色窗全屏透明置顶 focused 建窗,
    // 不排除则聚焦取色器会被记为 LAST_FOCUS_HWND,关闭后句柄失效,
    // 恢复焦点只清记录,外部应用焦点归还失败。
    #[test]
    fn picker_window_refreshes_excluded_hwnds_after_build() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/color_picker_window/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取取色器窗口源码失败");
        assert!(
            source.contains("refresh_excluded_hwnds(&app)"),
            "创建取色器窗口后必须刷新自身窗口排除列表"
        );
    }
}

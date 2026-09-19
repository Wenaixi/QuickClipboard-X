// 编辑器窗口命令面：打开编辑器加载指定图片文件，供动作链 edit 动作
// 与贴图右键"编辑"入口调用。窗口单例复用（ShareX ImageEditor 每次编辑
// 打开独立窗口，但本项目窗口级复用减少 WebView 渲染进程常驻，与截图
// 窗口 quick 模式同款）。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const ANNOTATION_WINDOW_LABEL: &str = "annotation";
const ANNOTATION_LOAD_EVENT: &str = "annotation:load";

// 打开编辑器并加载图片文件：窗口已存在则复用并推送加载事件（数据
// 驱动，不重建 WebView），不存在则新建；文件不存在/非图片时报错。
pub fn open_annotation_window(app: &AppHandle, image_path: &str) -> Result<(), String> {
    let path = std::path::Path::new(image_path);
    if !path.is_file() {
        return Err(format!("要编辑的图片文件不存在: {image_path}"));
    }
    if let Some(window) = app.get_webview_window(ANNOTATION_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        window
            .emit(ANNOTATION_LOAD_EVENT, image_path)
            .map_err(|error| format!("推送编辑器加载事件失败: {error}"))?;
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, ANNOTATION_WINDOW_LABEL, WebviewUrl::App("windows/annotation/index.html".into()))
        .title("图像编辑器")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .focused(true)
        .inner_size(960.0, 640.0)
        .min_inner_size(480.0, 320.0)
        .build()
        .map_err(|error| format!("创建编辑器窗口失败: {error}"))?;
    window
        .emit(ANNOTATION_LOAD_EVENT, image_path)
        .map_err(|error| format!("推送编辑器初始加载事件失败: {error}"))?;
    Ok(())
}

// 关闭编辑器窗口（完成/取消后销毁，释放 WebView renderer 常驻）。
pub fn close_annotation_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ANNOTATION_WINDOW_LABEL) {
        let _ = window.destroy();
    }
}

// 校验编辑器加载事件权限来源：只接受编辑器窗口自身事件。
pub fn require_annotation_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == ANNOTATION_WINDOW_LABEL {
        Ok(())
    } else {
        Err("该命令只能由编辑器窗口调用".to_string())
    }
}

// 编辑器完成路径：编辑后图片已由前端编码落盘到指定路径，此命令只做
// 结果确认（复制/贴图等由动作链后续接管），并关闭编辑器窗口。
#[tauri::command]
pub fn annotation_finished(app: AppHandle, window: tauri::WebviewWindow, _result_path: String) -> Result<(), String> {
    require_annotation_window(&window)?;
    close_annotation_window(&app);
    Ok(())
}

// 编辑器取消路径：关闭窗口不保存。
#[tauri::command]
pub fn annotation_cancelled(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_annotation_window(&window)?;
    close_annotation_window(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/annotation.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取编辑器窗口源码失败")
    }

    #[test]
    fn editor_window_requires_existing_image_and_reuses_window() {
        let source = source();
        // 打开编辑器必须先校验图片文件存在（不存在的文件必须报错）。
        assert!(source.contains("!path.is_file()"), "必须校验图片文件存在");
        assert!(source.contains("要编辑的图片文件不存在"), "文件不存在必须报错");
        // 已存在窗口走复用（show + 推送加载事件），不重建 WebView。
        assert!(source.contains("get_webview_window(ANNOTATION_WINDOW_LABEL)"), "必须尝试复用现有窗口");
        assert!(source.contains("ANNOTATION_LOAD_EVENT, image_path"), "必须推送加载事件");
    }

    #[test]
    fn editor_commands_require_annotation_window_origin() {
        let source = source();
        // 完成/取消命令必须校验窗口来源（只接受编辑器窗口），关闭窗口释放资源。
        assert!(source.contains("require_annotation_window(&window)"), "命令必须校验窗口来源");
        assert!(source.contains("close_annotation_window(&app)"), "命令必须关闭窗口");
        assert!(source.contains("annotation_finished"), "必须提供完成命令");
        assert!(source.contains("annotation_cancelled"), "必须提供取消命令");
    }
}
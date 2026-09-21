// 编辑器窗口命令面：打开编辑器加载指定图片文件，供动作链 edit 动作
// 与贴图右键"编辑"入口调用。窗口单例复用（ShareX ImageEditor 每次编辑
// 打开独立窗口，但本项目窗口级复用减少 WebView 渲染进程常驻，与截图
// 窗口 quick 模式同款）。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use std::sync::Mutex;
use once_cell::sync::Lazy;

pub const ANNOTATION_WINDOW_LABEL: &str = "annotation";
const ANNOTATION_LOAD_EVENT: &str = "annotation:load";

// 待编辑器页面就绪后重放的加载事件缓存:窗口首次创建时页面脚本尚未来
// 得及注册监听,直接 emit 会因投递即弃而丢失(首开白屏)。照抄截图窗口的
// window_ready + pending 握手模式:新建窗口缓存 image_path,页面 listen
// 成功后 invoke annotation_window_ready 触发重放。
static PENDING_LOAD: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

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
        // 复用路径:上次编辑器未保存直接关窗时,截图会话仍停留在处理中终态,
        // 下次截图会被 Existing 守卫拒绝重入。复用前先尝试把残留会话按失败
        // 收口(会话不存在时静默跳过),保证编辑-取消-再截图链路不吞截图。
        crate::windows::screenshot_window::abandon_stale_edit_session(app);
        // 复用路径:窗口页面已就绪(上次已触发 ready),直接 emit。
        window
            .emit(ANNOTATION_LOAD_EVENT, image_path)
            .map_err(|error| format!("推送编辑器加载事件失败: {error}"))?;
        return Ok(());
    }
    let _window = WebviewWindowBuilder::new(app, ANNOTATION_WINDOW_LABEL, WebviewUrl::App("windows/annotation/index.html".into()))
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
    // 新窗口加入自身窗口排除列表——编辑器窗透明置顶 focused 建窗,不排除
    // 则聚焦编辑器会被记为 LAST_FOCUS_HWND,恢复焦点把焦点设回编辑器自身。
    #[cfg(windows)]
    crate::services::system::focus::refresh_excluded_hwnds(app);
    // 首次创建:页面未就绪,先缓存路径等页面 ready 后重放,避免事件丢弃。
    *PENDING_LOAD.lock().unwrap() = Some(image_path.to_string());
    Ok(())
}

// 编辑器页面就绪回调:置位就绪并重放缓存的首开加载事件(若有)。
#[tauri::command]
pub fn annotation_window_ready(app: AppHandle) -> Result<(), String> {
    let pending = { PENDING_LOAD.lock().unwrap().take() };
    if let Some(image_path) = pending {
        let window = app
            .get_webview_window(ANNOTATION_WINDOW_LABEL)
            .ok_or_else(|| "编辑器窗口尚未创建".to_string())?;
        window
            .emit(ANNOTATION_LOAD_EVENT, &image_path)
            .map_err(|error| format!("重放编辑器加载事件失败: {error}"))?;
    }
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

    // 首开竞态防护:新建窗口不得直接 emit(页面未就绪事件丢弃),必须先把
    // 待加载路径缓存进 pending,等页面 ready 命令触发重放。
    #[test]
    fn first_open_buffers_load_event_until_window_ready() {
        let source = source();
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 新建路径不再裸 emit,而是写 pending 缓存。
        let build_start = stripped
            .find("WebviewWindowBuilder::new")
            .expect("必须新建编辑器窗口");
        let new_path_seg = &stripped[build_start..];
        assert!(
            new_path_seg.contains("PENDING_LOAD.lock().unwrap() = Some(image_path.to_string())"),
            "新建窗口必须把待加载路径缓存进 pending 等待重放"
        );
        assert!(
            !new_path_seg[..new_path_seg.find("annotation_window_ready").unwrap_or(new_path_seg.len())].contains("emit(ANNOTATION_LOAD_EVENT"),
            "新建路径不得在页面就绪前直接 emit 加载事件"
        );
        // 复用路径仍直接 emit(页面已就绪),ready 命令负责重放。
        assert!(source.contains("pub fn annotation_window_ready"), "必须提供就绪命令");
        assert!(source.contains("PENDING_LOAD.lock().unwrap().take()"), "就绪命令必须取走缓存并重放");
    }

    // 新窗口必须加入自身窗口排除列表——编辑器窗透明置顶 focused 建窗,
    // 不排除则聚焦编辑器会被记为 LAST_FOCUS_HWND,恢复焦点把焦点设回
    // 编辑器自身(单例复用窗口,开着期间 restore 会把焦点设回置顶编辑窗)。
    #[test]
    fn editor_window_refreshes_excluded_hwnds_after_build() {
        let source = source();
        assert!(
            source.contains("refresh_excluded_hwnds(app)"),
            "创建编辑器窗口后必须刷新自身窗口排除列表"
        );
    }

    // 复用路径必须先清理遗留截图会话:编辑器未保存直接关窗时会话停留
    // Processing,下次截图被 Existing 守卫拒绝重入(吞截图)。复用前必须
    // 调用 abandon_stale_edit_session 按失败收口残留会话。
    #[test]
    fn editor_reuse_abandons_stale_screenshot_session() {
        let source = source();
        let reuse_pos = source
            .find("get_webview_window(ANNOTATION_WINDOW_LABEL)")
            .expect("必须尝试复用现有窗口");
        let reuse_seg = &source[reuse_pos..];
        assert!(
            reuse_seg.contains("abandon_stale_edit_session(app)"),
            "复用路径必须先清理遗留截图会话,否则编辑-取消-再截图被吞"
        );
    }
}
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

pub fn create_community_window(app: &AppHandle) -> Result<(), String> {
    let _window = WebviewWindowBuilder::new(
        app,
        "community",
        WebviewUrl::App("windows/community/index.html".into()),
    )
    .title("社区交流 - QuickClipboard")
    .inner_size(570.0, 350.0)
    .center()
    .resizable(true)
    .maximizable(false)
    .decorations(true)
    .transparent(true)
    .skip_taskbar(false)
    .visible(true)
    .focused(true)
    .drag_and_drop(false)
    .shadow(false)
    .build()
    .map_err(|e| format!("创建社区交流窗口失败: {}", e))?;

    // 社区窗口聚焦事件必须被排除列表过滤——否则聚焦社区窗口会被记为
    // LAST_FOCUS_HWND,恢复焦点时把焦点设回隐藏自身窗口。
    crate::services::system::focus::refresh_excluded_hwnds(app);

    Ok(())
}

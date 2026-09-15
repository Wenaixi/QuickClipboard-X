use super::window::{show_menu, ContextMenuRequest};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition};

#[cfg(test)]
mod close_all_sync_guard {
    // C2(菜单关后 200ms 阻塞):close_all_context_menus hide 后必须同步清
    // session,不得 spawn 延迟 200ms——延迟期间 is_context_menu_visible()
    // 恒 true,edge_monitor 延迟隐藏与主窗口 hide 检查它时被阻塞 200ms,
    // 每次撮/关菜单都引入抖动。submit_context_menu 的结果选择延迟可保留。
    #[test]
    fn close_all_context_menus_clears_session_synchronously() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/plugins/context_menu/commands.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 commands.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // §10.4 自指陷阱:测试代码里若直接写完整锚点字符串字面量会自命中
        // 永远红(pin_image 测试同款),锚点用 concat 拆分拼接构造。
        let anchor = ["pub fn close_all_", "context_menus("].concat();
        let start = stripped.find(&anchor).expect("缺 close_all_context_menus");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        let hide_pos = body
            .find("w.hide()")
            .expect("close_all_context_menus 必须 hide 窗口");
        let after_hide = &body[hide_pos..];
        // hide 后必须直接同步清 session(先于任何延迟/spawn)
        assert!(
            after_hide.contains("clear_active_menu_session"),
            "hide 后必须同步清 session,不得延迟"
        );
        assert!(
            !after_hide.contains("thread::spawn"),
            "close_all_context_menus 不得 spawn 延迟清理"
        );
        assert!(
            !after_hide.contains("sleep("),
            "close_all_context_menus 不得在 hide 后 sleep"
        );
    }
}

#[tauri::command]
pub fn get_context_menu_options() -> Result<ContextMenuRequest, String> {
    super::get_options().ok_or_else(|| "配置未初始化".into())
}

#[tauri::command]
pub fn update_context_menu_regions(main_menu: super::MenuRegion, submenus: Vec<super::MenuRegion>) {
    super::update_menu_regions(main_menu, submenus);
}

#[tauri::command]
pub fn submit_context_menu(item_id: Option<String>) {
    let session_id = super::get_active_menu_session();
    super::set_result(item_id);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        super::clear_active_menu_session(session_id);
        super::clear_options_for_session(session_id);
    });
}

#[tauri::command]
pub async fn show_context_menu(
    app: AppHandle,
    request: ContextMenuRequest,
) -> Result<Option<String>, String> {
    let _ = crate::windows::pin_image_window::close_image_preview(app.clone());
    let _ = crate::windows::preview_window::close_preview_window(app.clone());
    show_menu(app, request).await
}

#[tauri::command]
pub fn close_all_context_menus(app: AppHandle) {
    let _ = crate::windows::pin_image_window::close_image_preview(app.clone());
    let _ = crate::windows::preview_window::close_preview_window(app.clone());

    if let Some(w) = app.get_webview_window("context-menu") {
        let _ = w.hide();
        // C2:hide 后同步清 session,不延迟 200ms——延迟期间
        // is_context_menu_visible() 恒 true,edge_monitor 延迟隐藏与主窗口
        // hide 检查它时被阻塞 200ms,每次撮/关菜单都引入抖动。
        let sid = super::get_active_menu_session();
        super::clear_active_menu_session(sid);
        super::clear_options_for_session(sid);
    }
}

#[tauri::command]
pub fn resize_context_menu(app: AppHandle, width: f64, height: f64, x: f64, y: f64) {
    if let Some(w) = app.get_webview_window("context-menu") {
        let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
        let text_scale = crate::utils::get_text_scale_factor();
        let _ = w.set_size(LogicalSize::new(width * text_scale, height * text_scale));
    }
}

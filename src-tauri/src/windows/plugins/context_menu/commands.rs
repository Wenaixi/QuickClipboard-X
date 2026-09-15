use super::window::{show_menu, ContextMenuRequest};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition};

#[cfg(test)]
mod close_all_sync_guard {
    // 菜单关后 200ms 阻塞:close_all_context_menus 必须同步清
    // session,不得 spawn 延迟 200ms——延迟期间 is_context_menu_visible()
    // 恒 true,edge_monitor 延迟隐藏与主窗口 hide 检查它时被阻塞 200ms,
    // 每次撮/关菜单都引入抖动。submit_context_menu 的结果选择延迟可保留。
    // 窗口销毁后 session 残留:session 清理必须无条件执行且先于窗口
    // hide——低内存模式 destroy_all_webviews 销毁 context-menu 窗口后
    // get_webview_window 返回 None,旧实现把清理放进 if-let 内导致 session
    // 残留(visible 恒 true 阻塞 hide 判定),重建后旧会话仍挂起。反证:
    // 把清理移回 if-let 内 → FAILED。
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
        // session 清理必须无条件执行(先于窗口 hide 判断)
        let clear_pos = body
            .find("clear_active_menu_session")
            .expect("close_all_context_menus 必须同步清 session");
        let hide_pos = body
            .find("w.hide()")
            .expect("close_all_context_menus 必须 hide 窗口");
        assert!(
            clear_pos < hide_pos,
            "session 清理必须先于 hide 并位于 if-let 窗口判断之外(无条件执行)"
        );
        // 负向:清理不得挂在 hide 之后(旧实现形态,窗口销毁时残留)
        let after_hide = &body[hide_pos..];
        assert!(
            !after_hide.contains("clear_active_menu_session"),
            "session 清理必须无条件执行,不得位于 hide 之后的 if-let 分支内"
        );
        assert!(
            !body.contains("thread::spawn"),
            "close_all_context_menus 不得 spawn 延迟清理"
        );
        assert!(
            !body.contains("sleep("),
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

    // session 清理必须无条件执行,不得依赖窗口存在——低内存模式
    // destroy_all_webviews 已销毁 context-menu 窗口后 get_webview_window
    // 返回 None,旧实现把它放进 if-let 内导致 session 残留(visible 恒 true
    // 阻塞 edge_monitor 延迟隐藏与主窗口 hide 判定),恢复重建后旧会话仍挂起。
    // 此处同步清 session,不延迟——延迟期间 is_context_menu_visible()
    // 恒 true,每次撮/关菜单都引入抖动。窗口 hide 保留判断(可能已被销毁)。
    let sid = super::get_active_menu_session();
    super::clear_active_menu_session(sid);
    super::clear_options_for_session(sid);

    if let Some(w) = app.get_webview_window("context-menu") {
        let _ = w.hide();
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

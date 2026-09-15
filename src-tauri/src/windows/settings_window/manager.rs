use tauri::{AppHandle, Manager};
use super::creator::create_settings_window;

pub fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        if window.is_minimized().unwrap_or(false) {
            window.unminimize().map_err(|e| format!("取消最小化设置窗口失败: {}", e))?;
        }
        window.show().map_err(|e| format!("显示设置窗口失败: {}", e))?;
        window.set_focus().map_err(|e| format!("聚焦设置窗口失败: {}", e))?;
    } else {
        create_settings_window(app)?;
        if let Some(window) = app.get_webview_window("settings") {
            window.set_focus().map_err(|e| format!("聚焦设置窗口失败: {}", e))?;
        }
    }

    // hk1:设置窗口打开即禁用导航键——主窗口的 show/hide 路径才启禁导航键,
    // 打开设置不经过主窗口显隐,导航键保持注册时,设置窗口内按 Tab/方向键
    // 会被 RegisterHotKey 全局截走,设置界面无法键盘移动光标。禁用前先快照
    // DESIRED(主窗口当前形态),关闭后由 creator 按快照精确恢复,不误启用
    // 自动弹出(MouseAuto)隐藏态下本就禁用的导航键。
    crate::hotkey::snapshot_navigation_hotkeys_desired();
    crate::input_monitor::disable_navigation_keys();
    Ok(())
}

#[cfg(test)]
mod hk1_settings_window_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // hk1(设置窗口吞导航键):打开设置必须先快照 DESIRED 再禁用导航键——
    // 主窗口 show/hide 路径才启禁导航键,设置窗口打开不经过主窗口显隐,
    // 若不禁用,设置窗口内 Tab/方向键被 RegisterHotKey 全局截走。
    #[test]
    fn open_settings_window_snapshots_before_disabling_navigation_keys() {
        let src = strip_line_comments(&source_file("src/windows/settings_window/manager.rs"));
        let body = fn_body(&src, "open_settings_window");
        let snapshot_pos = body
            .find("snapshot_navigation_hotkeys_desired()")
            .expect("打开设置前必须先快照导航键期望状态");
        let disable_pos = body
            .find("disable_navigation_keys()")
            .expect("打开设置必须禁用导航键");
        assert!(
            snapshot_pos < disable_pos,
            "快照必须先于禁用,否则关闭后无法精确恢复"
        );
    }

    // hk1(恢复路径):设置窗口关闭(CloseRequested/Destroyed)必须按快照恢复
    // 导航键——打开时已禁用,不恢复则设置窗口关闭后主窗口导航键静默缺失。
    #[test]
    fn settings_window_close_restores_navigation_keys_from_snapshot() {
        let src = strip_line_comments(&source_file("src/windows/settings_window/creator.rs"));
        assert!(
            src.contains("restore_navigation_hotkeys_from_snapshot()"),
            "设置窗口关闭路径必须按快照恢复导航键"
        );
        let close_pos = src
            .find("WindowEvent::CloseRequested")
            .expect("关闭事件分支必须存在");
        let restore_pos = src
            .find("restore_navigation_hotkeys_from_snapshot()")
            .expect("恢复必须在关闭事件处理内");
        assert!(
            close_pos < restore_pos,
            "恢复调用必须位于关闭/销毁事件处理内"
        );
    }
}


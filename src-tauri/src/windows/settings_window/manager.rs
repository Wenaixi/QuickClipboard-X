use tauri::{AppHandle, Manager};
use super::creator::create_settings_window;

pub fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    // 设置窗口打开即禁用导航键——主窗口的 show/hide 路径才启禁导航键,
    // 打开设置不经过主窗口显隐,导航键保持注册时,设置窗口内按 Tab/方向键
    // 会被 RegisterHotKey 全局截走,设置界面无法键盘移动光标。禁用前先快照
    // DESIRED(主窗口当前形态),关闭后由 creator 按快照精确恢复,不误启用
    // 自动弹出(MouseAuto)隐藏态下本就禁用的导航键。
    // 快照+禁用必须放在最前、早于一切 `?`——re-show 路径上
    // unminimize()?/show()?/set_focus()? 任一失败即提前 return 时,若快照/禁用
    // 排在后面,设置窗口已打开但 Tab/方向仍被吞(该缺陷在错误路径复现),
    // 且关闭时 restore 按旧快照误判。
    crate::hotkey::snapshot_navigation_hotkeys_desired();
    crate::input_monitor::disable_navigation_keys();

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

    Ok(())
}

#[cfg(test)]
mod hk1_settings_window_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 设置窗口吞导航键:打开设置必须先快照 DESIRED 再禁用导航键——
    // 主窗口 show/hide 路径才启禁导航键,设置窗口打开不经过主窗口显隐,
    // 若不禁用,设置窗口内 Tab/方向键被 RegisterHotKey 全局截走。
    // 快照+禁用必须在函数最前,早于一切 `?`——re-show 路径上
    // unminimize()?/show()?/set_focus()? 任一失败提前 return 时,若排在后面
    // 设置窗口已打开但 Tab/方向仍被吞(该缺陷在错误路径复现),且关闭时
    // restore 按旧快照误判。
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
        // 快照+禁用必须早于一切可失败的窗口操作(? 早返)
        for marker in ["unminimize()", "set_focus()", "create_settings_window(app)"] {
            let marker_pos = body.find(marker);
            if let Some(marker_pos) = marker_pos {
                assert!(
                    disable_pos < marker_pos,
                    "禁用导航键必须早于 {}——失败提前 return 时窗口已打开但导航键仍注册",
                    marker
                );
            }
        }
    }

    // 恢复路径:设置窗口关闭(CloseRequested/Destroyed)必须按快照恢复
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


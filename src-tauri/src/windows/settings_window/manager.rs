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
mod settings_window_guard {
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

    // 设置窗口截图设置节接线必须完整:App.jsx 的渲染 switch 引用 ScreenshotSection
    // 组件,就必须存在对应 import——上游合并把 import 行与 case 分支拆散,
    // 只剩 case 没有 import,一旦有入口把 activeSection 切到 'screenshot',
    // 渲染函数内对未声明的 ScreenshotSection 求值会抛 ReferenceError。
    // 护栏断言 import 与 JSX 引用共存,且 import 位于文件顶部先于引用。
    #[test]
    fn settings_app_jsx_imports_screenshot_section_for_render_switch() {
        let src = std::fs::read_to_string(format!(
            "{}/../src/windows/settings/App.jsx",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取设置窗口 App.jsx 失败");
        let import_pos = src
            .find("import ScreenshotSection from './sections/ScreenshotSection'")
            .unwrap_or_else(|| panic!("App.jsx 必须 import ScreenshotSection,否则渲染截图设置节崩溃"));
        let use_pos = src
            .find("<ScreenshotSection settings={snap}")
            .unwrap_or_else(|| panic!("App.jsx 渲染 switch 必须引用 ScreenshotSection"));
        assert!(
            import_pos < use_pos,
            "ScreenshotSection 的 import 必须位于引用之前"
        );
    }

    // 设置侧边栏必须保留截图节导航项:App.jsx 渲染 switch 有 case 'screenshot'
    // 分支、ScreenshotSection 组件/语言包/后端命令全链路存留,唯独导航项在
    // 上游合并时被裁决丢出——上游真删了截图,合并逐块裁决把本地这一项也
    // 一并丢成"与上游一致"。侧边栏与设置搜索(sections 语言包同缺)双双
    // 失去入口后,截图设置节成为死分支,14 个截图设置项永久无法配置。
    #[test]
    fn settings_sidebar_keeps_screenshot_navigation_item() {
        let src = strip_line_comments(&source_file(
            "../src/windows/settings/components/SettingsSidebar.jsx",
        ));
        assert!(
            src.contains("id: 'screenshot'"),
            "侧边栏导航必须保留截图节入口,否则 case 'screenshot' 分支不可达"
        );
        for (path, expected) in [
            ("../src/shared/locales/zh-CN.json", "\"screenshot\": \"截图设置\""),
            ("../src/shared/locales/en-US.json", "\"screenshot\": \"Screenshot\""),
        ] {
            let locale = std::fs::read_to_string(format!(
                "{}/{}",
                env!("CARGO_MANIFEST_DIR"),
                path
            ))
            .expect("读取语言包失败");
            assert!(
                locale.contains(expected),
                "语言包 {} 必须保留截图分组名,否则设置搜索与侧边栏无法展示",
                path
            );
        }
    }

    // 设置搜索高亮必须转义正则特殊字符:SettingsSearch 用用户输入的查询串
    // 构造 `(query)` 正则做 split 高亮,若不经 escapeRegExp 直接拼接,用户
    // 输入半角括号/星号/左方括号等字符会抛 "Invalid regular expression"
    // 运行时异常,界面立即崩溃。共享 highlightText 已导出转义能力,这里
    // 断言必须复用(不得本地重复实现,也不得裸拼接)。
    #[test]
    fn settings_search_escapes_regex_special_characters() {
        let src = strip_line_comments(&source_file(
            "../src/windows/settings/components/SettingsSearch.jsx",
        ));
        assert!(
            src.contains("from '@shared/utils/highlightText'"),
            "必须从共享 highlightText 导入转义实现"
        );
        assert!(
            !src.contains("function escapeRegExp"),
            "不得在本地重复定义 escapeRegExp"
        );
        // 高亮分支前必须先完成转义:调用点必须是 escapeRegExp(query) 直呼,
        // 下标顺序断言转义调用早于正则构造,防止"导入但未在构造前使用"。
        let call_pos = src
            .find("escapeRegExp(query)")
            .expect("高亮前必须调用 escapeRegExp(query) 转义搜索查询");
        let regex_pos = src
            .find("new RegExp")
            .expect("设置搜索必须构造高亮正则");
        assert!(
            call_pos < regex_pos,
            "escapeRegExp(query) 必须位于 RegExp 构造之前"
        );
        assert!(
            !src.contains("new RegExp(`(${query})`"),
            "禁止以未转义的 query 直接构造高亮正则"
        );
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

    // 主窗口收藏页的返回顶部与导航重置必须接线完整:收藏页的滚动到顶
    // 会调用 navigationStore.resetNavigation() 重置键盘导航状态,就必须存在
    // 对应 import——合并历史上游 v0.5 曾把两个 Tab 组件的 import 行与
    // 引用拆散(剪贴板页同款缺 import 已在合并修复中补齐,收藏页这一份
    // 漏网),缺 import 时点击"返回顶部"即 ReferenceError 抛错,列表无法
    // 回到顶部且导航状态残留。
    #[test]
    fn favorites_tab_imports_navigation_store_for_scroll_to_top() {
        let src = std::fs::read_to_string(format!(
            "{}/../src/windows/main/components/FavoritesTab.jsx",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 FavoritesTab.jsx 失败");
        let import_pos = src
            .find("import { navigationStore } from '@shared/store/navigationStore'")
            .unwrap_or_else(|| panic!("FavoritesTab.jsx 必须 import navigationStore,否则返回顶部重置导航抛错"));
        let use_pos = src
            .find("navigationStore.resetNavigation()")
            .unwrap_or_else(|| panic!("FavoritesTab.jsx 返回顶部必须调用 resetNavigation"));
        assert!(
            import_pos < use_pos,
            "navigationStore 的 import 必须位于引用之前"
        );
    }
}


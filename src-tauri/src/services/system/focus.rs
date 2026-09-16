use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WebviewWindow};

static LAST_FOCUS_HWND: Mutex<Option<isize>> = Mutex::new(None);
static LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);

static LAST_FOREGROUND_CACHE: Mutex<Option<(isize, ForegroundAppInfo)>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct ForegroundAppInfo {
    pub process_name: String,
    pub process_path: String,
    pub window_title: String,
}

#[cfg(windows)]
static EXCLUDED_HWNDS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

// 启动焦点变化监听器
pub fn start_focus_listener(app_handle: tauri::AppHandle) {
    #[cfg(windows)]
    {
        if LISTENER_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        refresh_excluded_hwnds(&app_handle);

        crate::services::system::hotkey::sync_hotkeys_for_foreground();

        std::thread::spawn(|| {
            start_win_event_hook();
        });
    }

    #[cfg(not(windows))]
    {
        let _ = app_handle;
    }
}

// 停止焦点变化监听器
pub fn stop_focus_listener() {
    LISTENER_RUNNING.store(false, Ordering::SeqCst);
}

// 按窗口标签整体重建"自身窗口排除列表",替换 add_excluded_hwnd 的只增
// 不减——低内存模式退出重建主窗口时旧 hwnd 已销毁,若仍留在列表里,OS 复用
// 该 hwnd 值后会把无关窗口当自身窗口,其聚焦事件被误过滤。重建保证列表
// 始终只含现存自身窗口,且不随重建次数无限增长。
#[cfg(windows)]
pub fn refresh_excluded_hwnds(app_handle: &tauri::AppHandle) {
    let mut excluded = Vec::new();
    // 排除列表必须覆盖全部自身窗口——缺 quickpaste 时便捷粘贴窗口的聚焦
    // 事件不被过滤,可被记为 LAST_FOCUS_HWND,恢复焦点时把焦点设回隐藏窗口。
    // 预览窗口真实标签是 PREVIEW_WINDOW_LABEL "preview-window",
    // 此前 "preview" 标签 get_webview_window 恒 None,预览窗 hwnd 从未进列表;
    // 改用标签常量消除误导。
    for label in [
        "main",
        "context-menu",
        crate::windows::preview_window::PREVIEW_WINDOW_LABEL,
        "quickpaste",
        "community",
    ] {
        if let Some(win) = app_handle.get_webview_window(label) {
            if let Ok(hwnd) = win.hwnd() {
                excluded.push(hwnd.0 as isize);
            }
        }
    }
    // 文件盒窗口按 transfer-shelf-{id} 动态标签创建,rename_shelf 允许任意
    // 名称,重命名后标题过滤(name.starts_with("文件盒"))失效,只能按标签
    // 前缀枚举进排除列表;贴图窗口按 pin-image-{uuid} 动态标签创建,target
    // 为图片内容标题不固定,同样只能按标签前缀枚举——否则重命名后聚焦
    // 文件盒/聚焦贴图污染 LAST_FOCUS_HWND,恢复焦点把焦点设回隐藏窗口。
    for (label, win) in app_handle.webview_windows() {
        if label.starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)
            || label.starts_with("pin-image-")
        {
            if let Ok(hwnd) = win.hwnd() {
                excluded.push(hwnd.0 as isize);
            }
        }
    }
    *EXCLUDED_HWNDS.lock() = excluded;
}

#[cfg(windows)]
pub fn add_excluded_hwnd(hwnd: isize) {
    let mut excluded = EXCLUDED_HWNDS.lock();
    if !excluded.contains(&hwnd) {
        excluded.push(hwnd);
    }
}

// 聚焦剪贴板窗口
pub fn focus_clipboard_window(window: WebviewWindow) -> Result<(), String> {
    window.set_focus().map_err(|e| format!("设置窗口焦点失败: {}", e))?;
    crate::hotkey::suspend_execute_item_hotkey();
    Ok(())
}

// 仅保存当前焦点（手动）
pub fn save_current_focus(_app_handle: tauri::AppHandle) -> Result<(), String> {
    // 由 no-op 改为真正捕获当前前台窗口——focus_callback 的前台切换
    // 事件在自身窗口聚焦时被过滤,LAST_FOCUS_HWND 停留"最后外部窗口"状态;
    // 但连续快速切换或事件钩子未送达时,主动抓一次保证 restore 目标最新。
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetWindowTextW,
        };
        unsafe {
            let hwnd = GetForegroundWindow();
            if !hwnd.0.is_null() {
                let hwnd_val = hwnd.0 as isize;
                if !EXCLUDED_HWNDS.lock().contains(&hwnd_val) {
                    let mut class_buf = [0u16; 256];
                    let mut name_buf = [0u16; 256];
                    let class_len = GetClassNameW(hwnd, &mut class_buf);
                    let name_len = GetWindowTextW(hwnd, &mut name_buf);
                    let class_name =
                        String::from_utf16_lossy(&class_buf[..class_len as usize]);
                    let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                    if !is_ignored_foreground_window(&class_name, &name) {
                        *LAST_FOCUS_HWND.lock() = Some(hwnd_val);
                    }
                }
            }
        }
    }
    Ok(())
}

// 恢复上次焦点窗口
pub fn restore_last_focus() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};
        use std::ffi::c_void;

        if let Some(hwnd_val) = *LAST_FOCUS_HWND.lock() {
            // 恢复前校验句柄仍有效——主窗口销毁重建后旧 hwnd 已失效,
            // 直接 SetForegroundWindow 到无效句柄静默失败,焦点归还被吞。
            // windows crate 的 IsWindow 接收 Option<HWND>,None 表示无效。
            let valid = unsafe { IsWindow(Some(HWND(hwnd_val as *mut c_void))) };
            if valid.as_bool() {
                // SetForegroundWindow 返回值必须判定——windows 前台
                // 锁限制 / 非前台线程调用 / hwnd 跨虚拟桌面时,句柄有效但归还失败,
                // 此时仍 resume 会让 SUSPENDED 被静默清掉、execute-item/方向键重新
                // 全局注册,输入框仍聚焦,正好是要修的输入被截 bug 本身。
                // 归还成功才恢复;失败保持挂起并保留记录待下次重试。
                let brought = unsafe { SetForegroundWindow(HWND(hwnd_val as *mut c_void)) };
                if brought.as_bool() {
                    // 仅在焦点真正归还给外部窗口后恢复执行粘贴热键——输入框
                    // 聚焦期间的挂起(SUSPENDED)记录的是"输入域仍需独占 Enter";
                    // 归还失败时挂起保持,避免输入框仍聚焦时全局 Enter 截走
                    // 输入法组合提交。
                    crate::hotkey::resume_execute_item_hotkey();
                }
            } else {
                // 句柄已失效(主窗口销毁重建/外部窗口已关),记录作废
                // 清空前先显式 resume 执行粘贴热键——resume_execute_item_hotkey
                // 全仓唯一调用方就是本函数,若此处只清空记录不 resume,挂起的
                // EXECUTE_ITEM_HOTKEY_SUSPENDED 将永久无释放路径,Enter/方向键
                // 被持续门闩。目标窗口已不存在,恢复独占键无冲突。
                crate::hotkey::resume_execute_item_hotkey();
                *LAST_FOCUS_HWND.lock() = None;
            }
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        crate::hotkey::resume_execute_item_hotkey();
        Ok(())
    }
}

// 获取当前记录的焦点窗口句柄
pub fn get_last_focus_hwnd() -> Option<isize> {
    *LAST_FOCUS_HWND.lock()
}

pub fn get_foreground_app_info() -> Option<ForegroundAppInfo> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }

            let hwnd_val = hwnd.0 as isize;
            let mut title_buf = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            let window_title = if title_len > 0 {
                String::from_utf16_lossy(&title_buf[..title_len as usize])
            } else {
                String::new()
            };

            if let Some((cached_hwnd, cached_info)) = LAST_FOREGROUND_CACHE.lock().clone() {
                // 缓存只在句柄与标题都未变化时有效——浏览器切标签等场景
                // 句柄不变但标题变,若只按句柄命中会返回旧标题,标题 wildcard
                // 过滤规则随之失效/误判。
                if cached_hwnd == hwnd_val && cached_info.window_title == window_title {
                    return Some(cached_info);
                }
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }

            let mut process_path = String::new();
            let mut process_name = String::new();

            if let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                let mut buffer = [0u16; 260];
                let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
                if len > 0 {
                    process_path = String::from_utf16_lossy(&buffer[..len as usize]);
                    process_name = process_path
                        .split('\\')
                        .last()
                        .unwrap_or(&process_path)
                        .to_string();
                }
                let _ = CloseHandle(handle);
            }

            if process_name.is_empty() {
                if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    let mut buffer = [0u16; 260];
                    let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
                    if len > 0 {
                        process_path = String::from_utf16_lossy(&buffer[..len as usize]);
                        process_name = process_path
                            .split('\\')
                            .last()
                            .unwrap_or(&process_path)
                            .to_string();
                    }
                    let _ = CloseHandle(handle);
                }
            }

            // UWP 应用前台进程统一是 ApplicationFrameHost.exe 宿主,不解析
            // 真实应用名的话,前台来源过滤按宿主名匹配对所有 UWP 应用失效
            // (规则写商店应用名永远不命中)。复用 app_filter 的子窗口枚举
            // 解析器,失败兜底回宿主名——解析不到时保持宿主名而非清空。
            if process_name.to_lowercase() == "applicationframehost.exe" {
                process_name = crate::services::system::app_filter::get_uwp_app_name(hwnd)
                    .unwrap_or(process_name);
            }

            if process_name.is_empty() {
                return None;
            }

            let info = ForegroundAppInfo {
                process_name,
                process_path,
                window_title,
            };

            *LAST_FOREGROUND_CACHE.lock() = Some((hwnd_val, info.clone()));
            Some(info)
        }
    }

    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn start_win_event_hook() {
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, TranslateMessage, DispatchMessageW, MSG,
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    };
    
    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(focus_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        
        if hook.0.is_null() {
            LISTENER_RUNNING.store(false, Ordering::SeqCst);
            return;
        }
        
        let mut msg = MSG::default();
        while LISTENER_RUNNING.load(Ordering::SeqCst) {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        
        let _ = UnhookWinEvent(hook);
    }
}

// 前台窗口忽略过滤器——focus_callback 与 save_current_focus 共用。
// 除系统托盘/弹层/主窗口/设置/菜单外,补齐全部自身窗口标题:文本编辑器、
// 贴图、收件盒、便捷粘贴、文件盒、更新、拖放接收层、快速剪贴板(低内存面板)。
// 缺失时这些窗口聚焦会被记为 LAST_FOCUS_HWND,恢复焦点把焦点设回隐藏自身窗口。
#[cfg(windows)]
fn is_ignored_foreground_window(class_name: &str, name: &str) -> bool {
    class_name == "Shell_TrayWnd"
        || class_name == "Shell_SecondaryTrayWnd"
        || class_name == "NotifyIconOverflowWindow"
        || class_name == "TopLevelWindowForOverflowXamlIsland"
        || class_name == "tray_icon_app"
        || class_name.starts_with("Windows.UI.")
        || class_name == "#32768"
        || class_name == "DropDown"
        || class_name == "Xaml_WindowedPopupClass"
        || name == "快速剪贴板"
        // 设置窗口标题含"设置 - 快速剪贴板",名字以"设置"开头即视为自身
        // 窗口——主窗口在设置页聚焦时聚焦事件若被过滤,导航键仍注册,
        // Tab/方向键会被 RegisterHotKey 吞掉,设置界面无法键盘移动光标。
        || name.starts_with("设置")
        || name == "菜单"
        || name.starts_with("文本编辑器")
        || name == "贴图"
        || name == "收件盒"
        || name == "便捷粘贴"
        || name.starts_with("文件盒")
        || name.starts_with("社区交流")
        || name == "更新"
        || name == "拖放接收层"
}

#[cfg(windows)]
unsafe extern "system" fn focus_callback(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    _hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetClassNameW, GetWindowTextW};

    if crate::services::low_memory::is_low_memory_mode()
        && crate::services::low_memory::is_panel_visible()
    {
        return;
    }

    let hwnd = GetForegroundWindow();
    if hwnd.0.is_null() {
        return;
    }
    
    let hwnd_val = hwnd.0 as isize;

    if EXCLUDED_HWNDS.lock().contains(&hwnd_val) {
        return;
    }
 
    let mut class_buf = [0u16; 256];
    let mut name_buf = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buf);
    let name_len = GetWindowTextW(hwnd, &mut name_buf);
    let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
    let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);

    // 过滤窗口:所有自身窗口 + 系统托盘/弹层统一走 is_ignored_foreground_window
    if is_ignored_foreground_window(&class_name, &name) {
        return;
    }

    *LAST_FOCUS_HWND.lock() = Some(hwnd_val);

    crate::services::system::hotkey::sync_hotkeys_for_foreground();
}

#[cfg(test)]
mod tests {
    use super::super::hotkey::test_utils::{fn_body, strip_line_comments};

    fn focus_source() -> String {
        super::super::hotkey::test_utils::source_file("src/services/system/focus.rs")
    }

    // §10.3 源码护栏：前台应用信息查询里的 OpenProcess 必须配对 CloseHandle。
    #[test]
    fn foreground_app_info_closes_process_handles() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/focus.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 focus.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let impl_src = &stripped[..stripped.find("#[cfg(test)]").unwrap_or(stripped.len())];
        let open_count = impl_src.matches("OpenProcess(").count();
        let close_count = impl_src.matches("CloseHandle(").count();
        assert!(open_count > 0, "护栏必须能发现 OpenProcess 调用");
        assert!(
            close_count >= open_count,
            "OpenProcess 调用必须配对 CloseHandle，当前 open={open_count} close={close_count}"
        );
    }

    // 焦点污染:排除列表必须覆盖全部自身窗口——缺失 quickpaste 时便捷
    // 粘贴窗口可被记为上次焦点,恢复时把焦点设回隐藏窗口。护栏断言 label 数组
    // 同时含 quickpaste 与 main/context-menu/preview(用标签常量,防字符串漂移)。
    #[test]
    fn excluded_hwnds_cover_all_own_windows_including_quickpaste() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "refresh_excluded_hwnds");
        for label in [
            "main",
            "context-menu",
            "PREVIEW_WINDOW_LABEL",
            "quickpaste",
            "community",
        ] {
            assert!(
                b.contains(label),
                "排除列表必须覆盖 {} 窗口,否则其聚焦事件污染 LAST_FOCUS_HWND",
                label
            );
        }
        assert!(
            b.contains("starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)")
                && b.contains("starts_with(\"pin-image-\")"),
            "排除列表必须按标签前缀覆盖文件盒与贴图窗口,标题可被重命名/变图内容标题过滤失效"
        );
        assert!(
            b.contains("webview_windows()"),
            "必须遍历全部 webview 窗口取文件盒窗口"
        );
    }

    // 焦点未归还仍挂起执行键:restore_last_focus 必须在焦点成功归还外部
    // 窗口后(SetForegroundWindow 之后)才恢复执行粘贴热键——输入框聚焦期间
    // 挂起的 EXECUTE_ITEM_HOTKEY_SUSPENDED 记录"输入域仍需独占 Enter";
    // 若 LAST_FOCUS_HWND 无有效窗口(记录为空),挂起保持,避免全局 Enter
    // 在输入框仍聚焦时截走输入法组合提交。
    // 修复不彻底:旧实现丢弃 SetForegroundWindow 返回值,只受
    // IsWindow 有效保护——句柄有效但前台归还失败(前台锁/非前台线程/跨虚拟
    // 桌面)时仍 resume,输入被截复现。修复:判定返回值,false 保持挂起。
    #[test]
    fn restore_last_focus_resumes_execute_item_only_after_successful_foreground() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "restore_last_focus");
        let resume_pos = b
            .find("resume_execute_item_hotkey()")
            .expect("restore_last_focus 必须含恢复执行键调用");
        let set_pos = b
            .find("SetForegroundWindow(")
            .expect("restore_last_focus 必须设置前台窗口");
        assert!(
            set_pos < resume_pos,
            "SetForegroundWindow 必须早于 resume(先归还焦点再恢复执行键)"
        );
        let after_set = &b[set_pos..];
        // 返回值必须被判定:SetForegroundWindow 的返回值(brought.as_bool())分支
        let brought_branch = after_set
            .find("brought.as_bool()")
            .expect("SetForegroundWindow 返回值必须被判定");
        let resume_after_brought = after_set.find("resume_execute_item_hotkey()");
        assert!(
            resume_after_brought.is_some() && brought_branch < resume_after_brought.unwrap(),
            "resume 必须位于 brought.as_bool() 成功分支内(归还失败保持挂起)"
        );
        let iswindow_pos = b
            .find("IsWindow(")
            .expect("restore_last_focus 必须校验句柄有效性");
        assert!(
            iswindow_pos < resume_pos,
            "执行键恢复必须位于 IsWindow 有效分支内"
        );
    }

    // save_current_focus 原为空操作:手动保存焦点必须真正把当前前台窗口写进
    // LAST_FOCUS_HWND——旧实现是空函数,前端 3 处调用(main 显隐/鼠标进入/
    // 便捷粘贴)全部无效果,restore 目标停留在 focus_callback 的旧记录。
    #[test]
    fn save_current_focus_writes_foreground_window_to_last_focus() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "save_current_focus");
        assert!(
            b.contains("GetForegroundWindow()"),
            "save_current_focus 必须获取当前前台窗口"
        );
        assert!(
            b.contains("LAST_FOCUS_HWND.lock() = Some(hwnd_val)"),
            "save_current_focus 必须把前台窗口写进 LAST_FOCUS_HWND"
        );
        assert!(
            b.contains("is_ignored_foreground_window"),
            "save_current_focus 必须过滤自身窗口,不能把自身窗口记为外部焦点"
        );
    }

    // 护栏:restore_last_focus 设置焦点前必须校验句柄仍有效——
    // 主窗口销毁重建后旧 hwnd 失效,直接 SetForegroundWindow 静默失败;
    // 无效时清空记录,避免把焦点设回已销毁/隐藏窗口。
    #[test]
    fn restore_last_focus_guards_against_invalid_hwnd() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "restore_last_focus");
        let set_pos = b
            .find("SetForegroundWindow(")
            .expect("restore_last_focus 必须设置前台窗口");
        // 有效性校验必须发生在 SetForegroundWindow 之前(顺序不变量)
        assert!(
            b[..set_pos].contains("IsWindow("),
            "SetForegroundWindow 前必须校验 hwnd 有效性"
        );
        assert!(
            b.contains("LAST_FOCUS_HWND.lock() = None"),
            "无效句柄必须清空 LAST_FOCUS_HWND 记录"
        );
        // 句柄失效清空记录前必须先 resume 执行粘贴热键——否则挂起
        // 的 EXECUTE_ITEM_HOTKEY_SUSPENDED 无释放路径,Enter/方向键永久门闩。
        // 断言 else(无效句柄)分支内、清空记录之前紧邻含 resume;成功分支的
        // resume(brought.as_bool() 内)不能顶替本断言,必须锚定无效分支。
        let clear_pos = b
            .find("LAST_FOCUS_HWND.lock() = None")
            .expect("无效句柄必须清空记录");
        let else_pos = b[..clear_pos]
            .rfind("} else {")
            .expect("必须有 else(无效句柄)分支");
        let invalid_branch = &b[else_pos..clear_pos];
        assert!(
            invalid_branch.contains("resume_execute_item_hotkey()"),
            "无效句柄(else)分支内清空记录前必须 resume 执行粘贴热键,否则挂起永久无释放路径"
        );
    }

    // 护栏:focus_callback 过滤块必须把设置窗口当作自身窗口过滤——
    // 设置窗口标题为"设置 - 快速剪贴板",若聚焦事件不被过滤,前台切到设置页
    // 时 sync_hotkeys_for_foreground 会把导航键注册上,Tab/方向键被吞掉,
    // 设置界面无法键盘移动光标。抽 helper 后,护栏改为断言回调调用
    // helper,且 helper 的过滤项覆盖设置窗口与全部自身窗口。
    #[test]
    fn focus_callback_filters_settings_window_as_own() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/focus.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 focus.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("unsafe extern \"system\" fn focus_callback")
            .expect("缺 focus_callback");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        let sync_pos = body
            .find("sync_hotkeys_for_foreground()")
            .expect("focus_callback 必须同步前台热键");
        let filter_seg = &body[..sync_pos];
        // 回调过滤段必须调用统一 helper
        assert!(
            filter_seg.contains("is_ignored_foreground_window(&class_name, &name)"),
            "focus_callback 必须调用统一的前台窗口忽略过滤器"
        );

        // helper 过滤项必须包含:自身主窗口标题 + 设置窗口 + 菜单(且顺序
        // 设置早于菜单,与旧内联块一致);补齐的其余自身窗口也应在列。
        let helper_start = stripped
            .find("fn is_ignored_foreground_window")
            .expect("缺 is_ignored_foreground_window");
        let helper_rest = &stripped[helper_start..];
        let helper_end = helper_rest
            .find("\n}\n")
            .map(|i| helper_start + i)
            .unwrap_or(stripped.len());
        let helper = &stripped[helper_start..helper_end];
        assert!(
            helper.contains("快速剪贴板"),
            "helper 必须过滤主窗口标题(含低内存面板)"
        );
        assert!(
            helper.contains("name.starts_with(\"设置\")"),
            "helper 必须把设置窗口(标题以 设置 开头)当自身窗口过滤"
        );
        assert!(
            helper.contains("name == \"菜单\""),
            "helper 必须过滤菜单窗口"
        );
        let settings_pos = helper
            .find("name.starts_with(\"设置\")")
            .expect("设置窗口过滤条件必须存在");
        let menu_pos = helper
            .find("name == \"菜单\"")
            .expect("菜单窗口条件必须存在");
        let text_editor_pos = helper
            .find("name.starts_with(\"文本编辑器\")")
            .expect("文本编辑器标题过滤必须存在");
        let drop_pos = helper
            .find("name == \"拖放接收层\"")
            .expect("拖放接收层标题过滤必须存在");
        assert!(
            settings_pos < menu_pos,
            "设置窗口过滤必须早于菜单条件"
        );
        assert!(
            text_editor_pos < drop_pos,
            "补齐的自身窗口过滤项必须在 helper 内"
        );
        assert!(
            helper.contains("name.starts_with(\"社区交流\")"),
            "社区交流窗口(标题:社区交流 - QuickClipboard)必须按前缀过滤"
        );
    }

    // 源码护栏:get_foreground_app_info 的前台信息缓存必须同时校验句柄
    // 与标题——浏览器切标签/文件另存为重命名等场景句柄不变但标题变化,
    // 只按句柄命中会返回旧标题,标题 wildcard 过滤规则失效/误判。
    #[test]
    fn foreground_cache_validates_title_as_well_as_hwnd() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "get_foreground_app_info");
        let title_read = b
            .find("GetWindowTextW(hwnd")
            .expect("必须读取前台窗口标题");
        let cache_hit = b[title_read..]
            .find("cached_info.window_title == window_title")
            .map(|i| title_read + i)
            .expect("缓存命中必须同时校验标题");
        assert!(
            title_read < cache_hit,
            "缓存命中校验必须在读取标题之后"
        );
        let cache_write = b
            .find("LAST_FOREGROUND_CACHE.lock() = Some(")
            .expect("必须更新缓存");
        let cached_hwnd_cond = b[..cache_write]
            .rfind("cached_hwnd == hwnd_val")
            .expect("缓存匹配仍须校验句柄");
        assert!(
            cached_hwnd_cond < cache_hit,
            "缓存命中必须同时校验句柄与标题"
        );
    }

    // 文件盒窗口标题可被 rename_shelf 改成任意名,标题过滤
    // (name.starts_with("文件盒"))随之失效,只能按 transfer-shelf-{id} 标签
    // 前缀枚举进排除列表;贴图窗口按 pin-image-{uuid} 标签动态创建,加上
    // 托盘删除项校验——否则重命名后聚焦文件盒/聚焦贴图污染
    // LAST_FOCUS_HWND,恢复焦点把焦点设回隐藏的文件盒/贴图窗口。
    #[test]
    fn excluded_hwnds_cover_transfer_shelf_windows_by_label_prefix() {
        let src = strip_line_comments(&focus_source());
        let body = fn_body(&src, "refresh_excluded_hwnds");
        assert!(
            body.contains("starts_with(crate::windows::transfer_shelf::LABEL_PREFIX)"),
            "排除列表必须按 transfer-shelf 标签前缀枚举文件盒窗口"
        );
        assert!(
            body.contains("starts_with(\"pin-image-\")"),
            "排除列表必须按 pin-image- 标签前缀枚举贴图窗口"
        );
        assert!(
            body.contains("webview_windows()"),
            "必须遍历全部 webview 窗口取文件盒窗口"
        );
    }

    // 护栏:get_foreground_app_info 必须解析 UWP 真实应用名——UWP 应用
    // 前台进程都是 ApplicationFrameHost.exe 宿主,不解析则所有 UWP 来源
    // 过滤规则按宿主名匹配全部失效。必须复用 app_filter 的子窗口枚举
    // 解析器,并先判定宿主进程再调用,解析失败兜底回宿主名。
    #[test]
    fn foreground_app_info_resolves_uwp_real_name() {
        let src = strip_line_comments(&focus_source());
        let b = fn_body(&src, "get_foreground_app_info");
        let framehost = b
            .find("applicationframehost.exe")
            .expect("必须判定 ApplicationFrameHost 宿主进程");
        let resolve = b
            .find("get_uwp_app_name(hwnd)")
            .expect("必须复用 app_filter 的 UWP 真实应用名解析");
        assert!(
            framehost < resolve,
            "必须首先判定宿主进程再调用真实应用名解析"
        );
        assert!(
            b.contains("unwrap_or(process_name)"),
            "UWP 解析失败必须兜底回宿主名,不得清空应用名"
        );
    }
}

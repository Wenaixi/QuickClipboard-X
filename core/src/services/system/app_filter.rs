//! 剪贴板来源信息（core 裁剪版）。
//!
//! 原 app_filter.rs 的窗口枚举、应用过滤规则、来源监视器线程等
//! 不在本轮迁入面，这里只保留 processor.rs 采集元信息所需的
//! `get_clipboard_source` 与支撑类型/函数。语义与原实现一致。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub struct ClipboardSourceInfo {
    pub process_name: String,
    pub process_path: String,
    pub window_title: String,
    pub source_type: ClipboardSourceType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardSourceType {
    ClipboardOwner,
    ForegroundWindow,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub process: String,
    pub path: String,
    pub icon: Option<String>,
}

/// 剪贴板来源缓存：监视器（原 start_clipboard_source_monitor）不迁入时，
/// 每次调用直接读前台/所有者窗口。保留缓存结构便于后续迁回监视器。
static CLIPBOARD_SOURCE_CACHE: Lazy<Mutex<Option<ClipboardSourceInfo>>> =
    Lazy::new(|| Mutex::new(None));

#[cfg(target_os = "windows")]
pub fn get_clipboard_source() -> ClipboardSourceInfo {
    if let Some(cached) = CLIPBOARD_SOURCE_CACHE.lock().clone() {
        if !cached.process_name.is_empty() {
            return cached;
        }
    }
    get_clipboard_source_internal()
}

#[cfg(target_os = "windows")]
fn get_clipboard_source_internal() -> ClipboardSourceInfo {
    use windows::Win32::System::DataExchange::GetClipboardOwner;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        if let Ok(owner) = GetClipboardOwner() {
            if !owner.is_invalid() && owner.0 as usize != 0 {
                if let Some(info) = get_process_info_from_hwnd(owner, ClipboardSourceType::ClipboardOwner) {
                    if !info.process_name.is_empty() {
                        return info;
                    }
                }
            }
        }

        let foreground = GetForegroundWindow();
        if !foreground.is_invalid() && foreground.0 as usize != 0 {
            if let Some(info) = get_process_info_from_hwnd(foreground, ClipboardSourceType::ForegroundWindow) {
                return info;
            }
        }

        ClipboardSourceInfo {
            process_name: String::new(),
            process_path: String::new(),
            window_title: String::new(),
            source_type: ClipboardSourceType::Unknown,
        }
    }
}

// 从窗口句柄获取进程信息
#[cfg(target_os = "windows")]
unsafe fn get_process_info_from_hwnd(
    hwnd: windows::Win32::Foundation::HWND,
    source_type: ClipboardSourceType,
) -> Option<ClipboardSourceInfo> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId};

    let mut process_id: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    if process_id == 0 {
        return None;
    }

    let mut title_buffer = [0u16; 512];
    let title_len = GetWindowTextW(hwnd, &mut title_buffer);
    let window_title = if title_len > 0 {
        String::from_utf16_lossy(&title_buffer[..title_len as usize])
    } else {
        String::new()
    };

    let (process_path, process_name) = get_process_name_by_id(process_id);

    // UWP 应用特殊处理
    let final_name = if process_name.to_lowercase() == "applicationframehost.exe" {
        get_uwp_app_name(hwnd).unwrap_or(process_name)
    } else {
        process_name
    };

    Some(ClipboardSourceInfo {
        process_name: final_name,
        process_path,
        window_title,
        source_type,
    })
}

// 通过进程ID获取进程名称
#[cfg(target_os = "windows")]
fn get_process_name_by_id(process_id: u32) -> (String, String) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;

    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, process_id) {
            let mut buffer = [0u16; 260];
            let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
            let result = if len > 0 {
                let path = String::from_utf16_lossy(&buffer[..len as usize]);
                let name = path.split('\\').last().unwrap_or(&path).to_string();
                Some((path, name))
            } else {
                None
            };
            let _ = CloseHandle(handle);
            if let Some(result) = result {
                return result;
            }
        }

        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
            let mut buffer = [0u16; 260];
            let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
            let result = if len > 0 {
                let path = String::from_utf16_lossy(&buffer[..len as usize]);
                let name = path.split('\\').last().unwrap_or(&path).to_string();
                Some((path, name))
            } else {
                None
            };
            let _ = CloseHandle(handle);
            if let Some(result) = result {
                return result;
            }
        }

        (String::new(), String::new())
    }
}

// 获取 UWP 应用真实名称
#[cfg(target_os = "windows")]
pub fn get_uwp_app_name(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
    use windows::core::BOOL;

    struct Context {
        result: Option<String>,
        parent_pid: u32,
    }

    unsafe {
        let mut parent_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut parent_pid));

        let mut ctx = Context { result: None, parent_pid };

        unsafe extern "system" fn callback(child: windows::Win32::Foundation::HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Context);
            let mut child_pid: u32 = 0;
            GetWindowThreadProcessId(child, Some(&mut child_pid));

            if child_pid > 0 && child_pid != ctx.parent_pid {
                if let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, child_pid) {
                    let mut buffer = [0u16; 260];
                    let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
                    let result = if len > 0 {
                        let path = String::from_utf16_lossy(&buffer[..len as usize]);
                        let name = path.split('\\').last().unwrap_or(&path).to_string();
                        if !name.is_empty() && name.to_lowercase() != "applicationframehost.exe" {
                            Some(name)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let _ = CloseHandle(handle);
                    if let Some(name) = result {
                        ctx.result = Some(name);
                        return BOOL(0);
                    }
                }
            }
            BOOL(1)
        }

        let _ = EnumChildWindows(Some(hwnd), Some(callback), LPARAM(&mut ctx as *mut _ as isize));
        ctx.result
    }
}

/// 非 Windows 平台返回空来源（与上游剪枝面一致，本项目仅 Windows 构建）。
#[cfg(not(target_os = "windows"))]
pub fn get_clipboard_source() -> ClipboardSourceInfo {
    ClipboardSourceInfo {
        process_name: String::new(),
        process_path: String::new(),
        window_title: String::new(),
        source_type: ClipboardSourceType::Unknown,
    }
}
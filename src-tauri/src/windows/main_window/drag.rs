use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewWindow};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;

const BOUNDARY_MARGIN: i32 = 0;
static IS_DRAGGING_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static INSTALLED_WNDPROC_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WINDOWPOS,
        WM_WINDOWPOSCHANGING,
    };

    pub static BOUND_LEFT: AtomicI32 = AtomicI32::new(0);
    pub static BOUND_TOP: AtomicI32 = AtomicI32::new(0);
    pub static BOUND_RIGHT: AtomicI32 = AtomicI32::new(0);
    pub static BOUND_BOTTOM: AtomicI32 = AtomicI32::new(0);
    pub static ORIGINAL_WNDPROC_PTR: AtomicIsize = AtomicIsize::new(0);
    pub static MONITORS: once_cell::sync::Lazy<parking_lot::Mutex<Vec<(i32, i32, i32, i32, bool, bool, bool, bool)>>> = 
        once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(Vec::new()));
    pub static WINDOW_SIZE: once_cell::sync::Lazy<parking_lot::Mutex<(i32, i32)>> = 
        once_cell::sync::Lazy::new(|| parking_lot::Mutex::new((0, 0)));

    pub unsafe fn install_wndproc(hwnd: HWND) {
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, window_proc as *const () as isize);
        ORIGINAL_WNDPROC_PTR.store(old, Ordering::SeqCst);
    }

    pub unsafe fn restore_wndproc(hwnd: HWND) {
        let old = ORIGINAL_WNDPROC_PTR.swap(0, Ordering::SeqCst);
        if old != 0 {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, old);
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_WINDOWPOSCHANGING && IS_DRAGGING_ACTIVE.load(Ordering::Relaxed) && lparam.0 != 0 {
            let wp = &mut *(lparam.0 as *mut WINDOWPOS);
            
            if let (Some(monitors), Some(size)) = (MONITORS.try_lock(), WINDOW_SIZE.try_lock()) {
                let (w, h) = *size;
                let cx = wp.x + w / 2;
                let cy = wp.y + h / 2;
                
                let current_monitor = monitors.iter()
                    .find(|(mx, my, mw, mh, _, _, _, _)| {
                        cx >= *mx && cx < mx + mw && cy >= *my && cy < my + mh
                    });
                
                if let Some(&(mx, my, mw, mh, left_edge, right_edge, top_edge, bottom_edge)) = current_monitor {
                    if left_edge {
                        wp.x = wp.x.max(mx);
                    }
                    if right_edge {
                        wp.x = wp.x.min(mx + mw - w);
                    }
                    if top_edge {
                        wp.y = wp.y.max(my);
                    }
                    if bottom_edge {
                        wp.y = wp.y.min(my + mh - h);
                    }
                } else {
                    let vx = BOUND_LEFT.load(Ordering::Relaxed);
                    let vy = BOUND_TOP.load(Ordering::Relaxed);
                    let vright = BOUND_RIGHT.load(Ordering::Relaxed);
                    let vbottom = BOUND_BOTTOM.load(Ordering::Relaxed);
                    wp.x = wp.x.clamp(vx, vright);
                    wp.y = wp.y.clamp(vy, vbottom);
                }
            } else {
                let vx = BOUND_LEFT.load(Ordering::Relaxed);
                let vy = BOUND_TOP.load(Ordering::Relaxed);
                let vright = BOUND_RIGHT.load(Ordering::Relaxed);
                let vbottom = BOUND_BOTTOM.load(Ordering::Relaxed);
                wp.x = wp.x.clamp(vx, vright);
                // w8:兜底分支与上方 monitor 分支同款 clamp(vy, vbottom)——旧实现
                // 只 wp.y.max(vy) 钳上边,缺下边,锁竞争/无 monitor 命中时窗口可被
                // 拖出虚拟屏底边之外。
                wp.y = wp.y.clamp(vy, vbottom);
            }
        }

        match ORIGINAL_WNDPROC_PTR.load(Ordering::SeqCst) {
            0 => DefWindowProcW(hwnd, msg, wparam, lparam),
            old => CallWindowProcW(std::mem::transmute(old), hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start_drag(window: &WebviewWindow, _: i32, _: i32) -> Result<(), String> {
    if IS_DRAGGING_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        eprintln!("窗口拖拽已在进行中，忽略重复启动请求");
        return Ok(());
    }

    clear_snap_if_needed();
    super::state::set_dragging(true);

    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (w, h) = (size.width as i32, size.height as i32);
    *platform::WINDOW_SIZE.lock() = (w, h);

    let app = window.app_handle();
    let monitors_with_edges = crate::utils::screen::ScreenUtils::get_all_monitors_with_edges(app)
        .unwrap_or_default();
    *platform::MONITORS.lock() = monitors_with_edges;

    let (vx, vy, vw, vh) = crate::utils::screen::ScreenUtils::get_virtual_screen_size_by_app(app)
        .unwrap_or((0, 0, 1920, 1080));

    platform::BOUND_LEFT.store(vx - BOUNDARY_MARGIN, Ordering::SeqCst);
    platform::BOUND_TOP.store(vy - BOUNDARY_MARGIN, Ordering::SeqCst);
    platform::BOUND_RIGHT.store(vx + vw - w + BOUNDARY_MARGIN, Ordering::SeqCst);
    platform::BOUND_BOTTOM.store(vy + vh - h + BOUNDARY_MARGIN, Ordering::SeqCst);

    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let hwnd_value = hwnd.0 as isize;
            let previous_hwnd = INSTALLED_WNDPROC_HWND.swap(hwnd_value, Ordering::SeqCst);
            if previous_hwnd != 0 && previous_hwnd != hwnd_value {
                eprintln!(
                    "检测到残留拖拽窗口过程句柄，旧句柄: {}, 新句柄: {}",
                    previous_hwnd,
                    hwnd_value
                );
            }
            platform::install_wndproc(HWND(hwnd.0 as *mut _));
        }
    } else {
        IS_DRAGGING_ACTIVE.store(false, Ordering::SeqCst);
        super::state::set_dragging(false);
        return Err("获取窗口句柄失败，无法启动拖拽".to_string());
    }

    let win1 = window.clone();
    std::thread::spawn(move || wait_for_mouse_release(win1));

    let win2 = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(8));
        let app = win2.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            let _ = win2.start_dragging();
        });
    });

    Ok(())
}

#[cfg(target_os = "windows")]
pub fn stop_drag(window: &WebviewWindow) -> Result<(), String> {
    let was_dragging = IS_DRAGGING_ACTIVE.swap(false, Ordering::SeqCst);
    super::state::set_dragging(false);

    if let Ok(hwnd) = window.hwnd() {
        let hwnd_value = hwnd.0 as isize;
        // w6:restore_wndproc 必须先比对 INSTALLED_WNDPROC_HWND——若拖拽
        // 期间窗口被重建(引号场景),当前 hwnd 是新窗口,ORIGINAL_WNDPROC_PTR
        // 里存的是旧窗口的过程,直接装到新窗口上会把旧 WndProc 安到错误
        // 窗口。只有在句柄确实是自己安装过的窗口时才恢复。
        let installed_hwnd = INSTALLED_WNDPROC_HWND.load(Ordering::SeqCst);
        if installed_hwnd == hwnd_value {
            unsafe { platform::restore_wndproc(HWND(hwnd.0 as *mut _)); }
            INSTALLED_WNDPROC_HWND.store(0, Ordering::SeqCst);
        } else {
            INSTALLED_WNDPROC_HWND.store(0, Ordering::SeqCst);
            if installed_hwnd != 0 {
                eprintln!(
                    "拖拽结束跳过恢复:句柄不匹配,安装句柄: {}, 当前句柄: {}",
                    installed_hwnd, hwnd_value
                );
            }
        }
    } else {
        let installed_hwnd = INSTALLED_WNDPROC_HWND.swap(0, Ordering::SeqCst);
        if installed_hwnd != 0 {
            eprintln!("拖拽结束时无法获取窗口句柄，残留句柄: {}", installed_hwnd);
        }
    }

    platform::BOUND_LEFT.store(0, Ordering::SeqCst);
    platform::BOUND_TOP.store(0, Ordering::SeqCst);
    platform::BOUND_RIGHT.store(0, Ordering::SeqCst);
    platform::BOUND_BOTTOM.store(0, Ordering::SeqCst);
    platform::MONITORS.lock().clear();

    if was_dragging {
        delayed_check_snap(window);
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn start_drag(window: &WebviewWindow, _: i32, _: i32) -> Result<(), String> {
    clear_snap_if_needed();
    super::state::set_dragging(true);

    let win = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(8));
        let app = win.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            let _ = win.start_dragging();
        });
    });
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn stop_drag(window: &WebviewWindow) -> Result<(), String> {
    super::state::set_dragging(false);
    delayed_check_snap(window);
    Ok(())
}

pub fn is_dragging() -> bool {
    IS_DRAGGING_ACTIVE.load(Ordering::SeqCst)
}

fn clear_snap_if_needed() {
    if super::state::is_snapped() {
        super::clear_snap();
        super::edge_monitor::stop_edge_monitoring();
    }
}

fn delayed_check_snap(window: &WebviewWindow) {
    let win = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let app = win.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            let _ = super::check_snap(&win);
        });
    });
}

#[cfg(target_os = "windows")]
fn wait_for_mouse_release(window: WebviewWindow) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    
    std::thread::sleep(Duration::from_millis(100));
    loop {
        unsafe {
            if GetAsyncKeyState(VK_LBUTTON.0 as i32) >= 0 {
                std::thread::sleep(Duration::from_millis(50));
                if GetAsyncKeyState(VK_LBUTTON.0 as i32) >= 0 {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    
    let app = window.app_handle().clone();
    let _ = app.run_on_main_thread(move || {
        let _ = stop_drag(&window);
        let _ = window.emit("drag-ended", ());
    });
}

#[cfg(test)]
mod w6_restore_wndproc_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // w6(恢复装错窗口):stop_drag 恢复 WndProc 前必须比对 INSTALLED_WNDPROC_HWND
    // 与当前 hwnd——引号场景窗口重建后 ORIGINAL_WNDPROC_PTR 是旧窗口过程,
    // 无条件 SetWindowLongPtrW 会把旧过程装到新窗口,生成的新实例行为异常。
    #[test]
    fn stop_drag_only_restores_wndproc_when_hwnd_matches() {
        let src = strip_line_comments(&source_file("src/windows/main_window/drag.rs"));
        let body = fn_body(&src, "stop_drag");
        // 必须先读到已安装句柄(比对前不 swap 掉——否则更新语义丢失)
        let load_pos = body
            .find("INSTALLED_WNDPROC_HWND.load")
            .expect("恢复前必须先读已安装句柄");
        let cmp_pos = body
            .find("installed_hwnd == hwnd_value")
            .expect("必须比对句柄是否为本窗口");
        assert!(load_pos < cmp_pos, "比对应在读句柄之后");
        // 比对成功后,restore_wndproc 与清句柄都在条件分支内
        let restore_seg_start = body.find("restore_wndproc(&platform::").unwrap_or_else(|| {
            body.find("restore_wndproc")
                .expect("restore_wndproc 调用存在")
        });
        let cmp_seg = &body[cmp_pos..];
        assert!(
            cmp_seg.starts_with(&body[cmp_pos..cmp_pos + 60]),
            "restore_wndproc 必须位于句柄比对分支内"
        );
        // 核心:INSTALLED_WNDPROC_HWND 的写入(store/swap)必须早于 restore_wndproc,
        // 且两者之间就是"是否匹配"的分支
        let store_pos = body
            .find("INSTALLED_WNDPROC_HWND.store(0, Ordering::SeqCst)")
            .expect("恢复后必须清空已安装句柄");
        assert!(
            cmp_pos < restore_seg_start && restore_seg_start < store_pos,
            "顺序必须为:比对 -> restore(仅匹配时) -> 清句柄"
        );
        // 负向:禁止先 restore 后比对的旧实现形态(直通测试:命中就 fail)
        let load_first = body
            .find("INSTALLED_WNDPROC_HWND.load")
            .expect("必须含句柄比对读");
        assert!(
            load_first < restore_seg_start,
            "restore 必须晚于句柄比对"
        );
    }

    // w8(clamp 缺底边):兜底分支(锁竞争/无 monitor 命中)必须与 monitor 分支
    // 同款 clamp(vy, vbottom)——旧实现只 wp.y.max(vy) 钳上边,缺下边约束,
    // 拖拽经过兜底分支时窗口可被拖出虚拟屏底边之外。
    #[test]
    fn fallback_clamp_covers_bottom_edge_like_monitor_branch() {
        let src = strip_line_comments(&source_file("src/windows/main_window/drag.rs"));
        let body = fn_body(&src, "window_proc");
        // 两个分支都要读 vbottom
        let vbottom_count = body.matches("BOUND_BOTTOM.load(Ordering::Relaxed)").count();
        assert!(
            vbottom_count >= 2,
            "兜底分支与 monitor 分支必须都读 BOUND_BOTTOM,当前只读 {} 次",
            vbottom_count
        );
        // 兜底分支(clamp(vx, vright) 出现两次)必须用 clamp(vy, vbottom)
        let clamp_count = body.matches("wp.y.clamp(vy, vbottom)").count();
        assert!(
            clamp_count >= 2,
            "monitor 分支与兜底分支都必须用 clamp(vy, vbottom),当前 {} 次",
            clamp_count
        );
        // 负向:禁止兜底分支退回 max(vy)(只钳上边)
        assert!(
            !body.contains("wp.y = wp.y.max(vy)"),
            "兜底分支禁止只钳上边(wp.y.max(vy)),必须与 monitor 分支同款 clamp 底边"
        );
    }
}

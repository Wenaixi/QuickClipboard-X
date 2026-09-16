#[cfg(target_os = "windows")]
mod windows_display_change_monitor {
    use super::super::input_common;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use std::mem::size_of;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PeekMessageW,
        RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG,
        PM_NOREMOVE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE,
        WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    };

    static DISPLAY_MONITOR_ACTIVE: AtomicBool = AtomicBool::new(false);
    static DISPLAY_MONITOR_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static DISPLAY_REFRESH_VERSION: AtomicU64 = AtomicU64::new(0);
    // 节流时间基准:用单调时钟(Instant),不随系统墙钟回拨/调整漂移
    static DISPLAY_REFRESH_LAST_RUN: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

    const DISPLAY_REFRESH_DELAY_MS: u64 = 450;
    const DISPLAY_REFRESH_THROTTLE_MS: u64 = 150;

    pub(crate) fn start_display_change_monitor_if_needed() {
        if DISPLAY_MONITOR_ACTIVE.swap(true, Ordering::SeqCst) {
            return;
        }

        thread::spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            DISPLAY_MONITOR_THREAD_ID.store(tid, Ordering::SeqCst);

            let h_module = match GetModuleHandleW(PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => {
                    eprintln!("[DisplayChange] GetModuleHandleW 失败");
                    DISPLAY_MONITOR_ACTIVE.store(false, Ordering::SeqCst);
                    DISPLAY_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            };

            let mut init_msg = MSG::default();
            let _ = PeekMessageW(&mut init_msg, None, 0, 0, PM_NOREMOVE);

            let class_name = w!("QuickClipboardDisplayChangeSink");
            let wnd_class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(display_change_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_module.into(),
                hIcon: Default::default(),
                hCursor: Default::default(),
                hbrBackground: Default::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: Default::default(),
            };

            if RegisterClassExW(&wnd_class) == 0 {
                let err = windows::Win32::Foundation::GetLastError();
                if err != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS {
                    eprintln!("[DisplayChange] RegisterClassExW 失败：{:?}", err);
                    DISPLAY_MONITOR_ACTIVE.store(false, Ordering::SeqCst);
                    DISPLAY_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            }

            let _hwnd = match CreateWindowExW(
                Default::default(),
                class_name,
                w!(""),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(h_module.into()),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(_) => {
                    eprintln!("[DisplayChange] CreateWindowExW 失败");
                    DISPLAY_MONITOR_ACTIVE.store(false, Ordering::SeqCst);
                    DISPLAY_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            };

            println!("[DisplayChange] 显示器变化监听已启动");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            println!("[DisplayChange] 显示器变化监听线程退出");
            DISPLAY_MONITOR_ACTIVE.store(false, Ordering::SeqCst);
            DISPLAY_MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
        });
    }

    unsafe extern "system" fn display_change_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DISPLAYCHANGE => {
                schedule_hidden_snap_refresh();
                // A screenshot session stores the original monitor rectangle. Once
                // Windows reports a topology change that rectangle is no longer a
                // safe capture target, so cancel it on the app's main thread.
                input_common::run_on_main_thread(|| {
                    if let Some(app) = input_common::try_get_app_handle() {
                        if let Err(error) = crate::windows::screenshot_window::cancel_active_screenshot(&app) {
                            eprintln!("[Screenshot] 显示器变化取消截图失败: {error}");
                        }
                    }
                });
                LRESULT(0)
            }
            WM_DPICHANGED | WM_SETTINGCHANGE => {
                schedule_hidden_snap_refresh();
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn schedule_hidden_snap_refresh() {
        let now = Instant::now();
        {
            let last_run = DISPLAY_REFRESH_LAST_RUN.lock();
            if now.duration_since(*last_run) < Duration::from_millis(DISPLAY_REFRESH_THROTTLE_MS) {
                return;
            }
        }

        // 通过节流闸立即推进基准:显示变化风暴期间,后到事件由闸门直接
        // 丢弃。基准若等延迟线程真正执行才推进,风暴里每个事件都会通过
        // 旧基准的闸门,各自 spawn 一个 450ms 空睡线程,线程堆积。
        *DISPLAY_REFRESH_LAST_RUN.lock() = now;

        let version = DISPLAY_REFRESH_VERSION.fetch_add(1, Ordering::SeqCst) + 1;

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(DISPLAY_REFRESH_DELAY_MS));

            if DISPLAY_REFRESH_VERSION.load(Ordering::SeqCst) != version {
                return;
            }

            input_common::run_on_main_thread(|| {
                handle_display_change_impl();
            });
        });
    }

    fn handle_display_change_impl() {
        let Some(window) = input_common::try_get_main_window() else {
            return;
        };

        match crate::windows::main_window::needs_hidden_snap_refresh(&window) {
            Ok(true) => {
                let _ = crate::windows::main_window::refresh_hidden_snapped_window(&window);
            }
            Ok(false) => {}
            Err(_) => {}
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) use windows_display_change_monitor::start_display_change_monitor_if_needed;

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_display_change_monitor_if_needed() {}

#[cfg(target_os = "windows")]
#[cfg(test)]
mod display_change_monitor_tests {
    // 护栏:节流基准必须在调度点(通过节流闸时)就推进,不能在延迟线程
    // 真正执行时才推进——显示变化风暴期间,滞后基准会让每个风暴事件都
    // 通过闸门,各自 spawn 一个 450ms 空睡线程,线程堆积。
    #[test]
    fn display_refresh_throttle_baseline_advances_at_schedule_time() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/display_change_monitor.rs"));
        let body = fn_body(&src, "schedule_hidden_snap_refresh");
        // 调度点必须立即推进基准(DISPLAY_REFRESH_LAST_RUN 写 now)
        let baseline_pos = body
            .find("DISPLAY_REFRESH_LAST_RUN.lock() = now")
            .expect("通过节流闸后必须立即推进节流基准");
        // spawn 延迟线程必须发生在基准推进之后
        let spawn_pos = body
            .find("thread::spawn(move ||")
            .expect("必须 spawn 延迟刷新线程");
        assert!(
            baseline_pos < spawn_pos,
            "节流基准必须在 spawn 之前推进——延迟线程内推进会让风暴期间线程堆积"
        );
    }
}

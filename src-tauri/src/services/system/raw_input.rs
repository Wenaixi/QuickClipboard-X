use super::input_common;

#[cfg(target_os = "windows")]
pub(crate) const PASTE_INPUT_MARKER: usize = 0x5143_4C50;

#[cfg(target_os = "windows")]
mod windows_raw_input {
    use super::{input_common, PASTE_INPUT_MARKER};
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use std::mem::size_of;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use windows::core::w;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::{
        GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
        RID_INPUT, RIDEV_INPUTSINK,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PeekMessageW,
        RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG, PM_NOREMOVE,
        WM_DESTROY, WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSEXW,
        WS_OVERLAPPEDWINDOW,
    };

    use crate::services::sound::AppSounds;

    static RAW_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
    static RAW_INPUT_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
    static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
    static ALT_DOWN: AtomicBool = AtomicBool::new(false);
    static LWIN_DOWN: AtomicBool = AtomicBool::new(false);
    static RWIN_DOWN: AtomicBool = AtomicBool::new(false);
    static MIDDLE_BUTTON_DOWN: AtomicBool = AtomicBool::new(false);
    static MIDDLE_BUTTON_PRESS_ID: AtomicU64 = AtomicU64::new(0);
    static PREVIEW_GUARD_PENDING: AtomicBool = AtomicBool::new(false);
    static PREVIEW_GUARD_LAST_RUN_MS: Lazy<Mutex<u64>> = Lazy::new(|| Mutex::new(0));
    static QUICKPASTE_KEYBOARD_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
    static QUICKPASTE_HIDE_TRIGGERED: AtomicBool = AtomicBool::new(false);
    static QUICKPASTE_REQUIRED_MODIFIER_MASK: AtomicU8 = AtomicU8::new(0);
    static QUICKPASTE_SECONDARY_KEY_VK: AtomicU32 = AtomicU32::new(0);
    static QUICKPASTE_SECONDARY_KEY_DOWN: AtomicBool = AtomicBool::new(false);
    static QUICKPASTE_SECONDARY_KEY_PRESS_ID: AtomicU64 = AtomicU64::new(0);
    static LAST_TRAY_RECT: Lazy<Mutex<Option<(i32, i32, i32, i32)>>> = Lazy::new(|| Mutex::new(None));
    const VK_CONTROL_CODE: u32 = 0x11;
    const VK_LCONTROL_CODE: u32 = 0xA2;
    const VK_RCONTROL_CODE: u32 = 0xA3;
    const VK_SHIFT_CODE: u32 = 0x10;
    const VK_LSHIFT_CODE: u32 = 0xA0;
    const VK_RSHIFT_CODE: u32 = 0xA1;
    const VK_MENU_CODE: u32 = 0x12;
    const VK_LMENU_CODE: u32 = 0xA4;
    const VK_RMENU_CODE: u32 = 0xA5;
    const VK_LWIN_CODE: u32 = 0x5B;
    const VK_RWIN_CODE: u32 = 0x5C;
    const VK_INSERT_CODE: u32 = 0x2D;
    const PREVIEW_GUARD_THROTTLE_MS: u64 = 50;
    const QUICKPASTE_REPEAT_INITIAL_DELAY_MS: u64 = 300;
    const QUICKPASTE_REPEAT_INTERVAL_MS: u64 = 120;

    pub(crate) fn get_physical_modifier_keys_state() -> Option<(bool, bool, bool, bool, bool)> {
        if !RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
            return None;
        }

        // 直接读物理按键实时状态,而非用 CTRL_DOWN/SHIFT_DOWN 等 tracker:
        // 粘贴注入键(PASTE_INPUT_MARKER)走 SendInput,raw_input 在 marker
        // 分支直接 return,不更新 tracker;若恢复依赖 tracker,注入键的合成
        // keyup 不会反映到 tracker 里,快速连按粘贴后 Ctrl/Shift/Win 会
        // 被记成"仍按下",直到下一次真实物理按键才纠正(粘滞修饰键)。
        // GetAsyncKeyState 读的是真实物理/合成按键的当前状态,注入键本就
        // 没有改变物理键,直接读物理态即可消除漂移。
        unsafe {
            let ctrl = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            let shift = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let alt = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
            let lwin = (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0;
            let rwin = (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0;
            Some((ctrl, shift, alt, lwin, rwin))
        }
    }

    pub(crate) fn guard_tray_click_region(x: i32, y: i32, width: u32, height: u32) {
        let right = x.saturating_add(width.min(i32::MAX as u32) as i32);
        let bottom = y.saturating_add(height.min(i32::MAX as u32) as i32);
        *LAST_TRAY_RECT.lock() = Some((x, y, right, bottom));
    }

    pub(crate) fn start_raw_input_if_needed() {
        if RAW_INPUT_ACTIVE.swap(true, Ordering::SeqCst) {
            return;
        }

        thread::spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            RAW_INPUT_THREAD_ID.store(tid, Ordering::SeqCst);

            let h_module = match GetModuleHandleW(PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => {
                    eprintln!("[RawInput] GetModuleHandleW 失败");
                    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                    RAW_INPUT_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            };

            // 初始化线程消息队列（Windows 的消息队列是惰性创建的）
            let mut init_msg = MSG::default();
            let _ = PeekMessageW(&mut init_msg, None, 0, 0, PM_NOREMOVE);

            let class_name = w!("QuickClipboardRawInputSink");

            let wnd_class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(raw_input_wnd_proc),
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
                    eprintln!("[RawInput] RegisterClassExW 失败：{:?}", err);
                    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                    RAW_INPUT_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            }

            let hwnd = match CreateWindowExW(
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
                Ok(h) => h,
                Err(_) => {
                    eprintln!("[RawInput] CreateWindowExW 失败");
                    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                    RAW_INPUT_THREAD_ID.store(0, Ordering::SeqCst);
                    return;
                }
            };

            let rid = [
                RAWINPUTDEVICE {
                    usUsagePage: 0x01,
                    usUsage: 0x06,
                    dwFlags: RIDEV_INPUTSINK,
                    hwndTarget: hwnd,
                },
                RAWINPUTDEVICE {
                    usUsagePage: 0x01,
                    usUsage: 0x02,
                    dwFlags: RIDEV_INPUTSINK,
                    hwndTarget: hwnd,
                },
            ];

            if let Err(_) = RegisterRawInputDevices(&rid, size_of::<RAWINPUTDEVICE>() as u32) {
                let err = windows::Win32::Foundation::GetLastError();
                eprintln!("[RawInput] RegisterRawInputDevices 失败：{:?}", err);
                // 销毁已创建的 sink 窗口,避免句柄泄漏与残留消息队列
                DestroyWindow(hwnd);
                RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                RAW_INPUT_THREAD_ID.store(0, Ordering::SeqCst);
                return;
            }

            println!("[RawInput] Raw Input 已启动");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            println!("[RawInput] Raw Input 线程退出");
            RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
            RAW_INPUT_THREAD_ID.store(0, Ordering::SeqCst);
        });
    }

    unsafe extern "system" fn raw_input_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_INPUT => {
                handle_raw_input(lparam);
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn handle_raw_input(lparam: LPARAM) {
        let mut size: u32 = 0;

        let res = GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_INPUT,
            None,
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        );

        if res == u32::MAX || size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];

        let res = GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_INPUT,
            Some(buf.as_mut_ptr() as *mut _),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        );

        if res == u32::MAX {
            return;
        }

        let raw: &RAWINPUT = &*(buf.as_ptr() as *const RAWINPUT);

        match raw.header.dwType {
            // 鼠标
            0 => {
                let mouse = raw.data.mouse;
                let button_flags = mouse.Anonymous.Anonymous.usButtonFlags;

                const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
                const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
                const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
                const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
                const RI_MOUSE_WHEEL: u16 = 0x0400;

                if (button_flags & (RI_MOUSE_LEFT_BUTTON_DOWN | RI_MOUSE_RIGHT_BUTTON_DOWN)) != 0 {
                    input_common::run_on_main_thread(|| {
                        handle_click_outside_impl();
                    });
                }

                schedule_preview_guard_check();

                if (button_flags & RI_MOUSE_MIDDLE_BUTTON_UP) != 0 {
                    MIDDLE_BUTTON_DOWN.store(false, Ordering::SeqCst);
                    MIDDLE_BUTTON_PRESS_ID.fetch_add(1, Ordering::SeqCst);
                }

                if (button_flags & RI_MOUSE_MIDDLE_BUTTON_DOWN) != 0 {
                    handle_middle_button_down_impl();
                }
            }
            // 键盘
            1 => {
                let kb = raw.data.keyboard;
                let vkey = kb.VKey as u32;
                let message = kb.Message;

                let is_keydown = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
                let is_keyup = message == WM_KEYUP || message == WM_SYSKEYUP;

                if kb.ExtraInformation as usize == PASTE_INPUT_MARKER {
                    if is_keydown && (vkey == b'V' as u32 || vkey == VK_INSERT_CODE) {
                        AppSounds::play_paste_immediate();
                    }
                    return;
                }

                handle_quickpaste_keyboard_event(vkey, is_keydown, is_keyup);

                if vkey == VK_CONTROL_CODE || vkey == VK_LCONTROL_CODE || vkey == VK_RCONTROL_CODE {
                    if is_keydown {
                        CTRL_DOWN.store(true, Ordering::Relaxed);
                    } else if is_keyup {
                        CTRL_DOWN.store(false, Ordering::Relaxed);
                    }
                    return;
                }

                if vkey == VK_SHIFT_CODE || vkey == VK_LSHIFT_CODE || vkey == VK_RSHIFT_CODE {
                    if is_keydown {
                        SHIFT_DOWN.store(true, Ordering::Relaxed);
                    } else if is_keyup {
                        SHIFT_DOWN.store(false, Ordering::Relaxed);
                    }
                    return;
                }

                if vkey == VK_MENU_CODE || vkey == VK_LMENU_CODE || vkey == VK_RMENU_CODE {
                    if is_keydown {
                        ALT_DOWN.store(true, Ordering::Relaxed);
                    } else if is_keyup {
                        ALT_DOWN.store(false, Ordering::Relaxed);
                    }
                    return;
                }

                if vkey == VK_LWIN_CODE || vkey == VK_RWIN_CODE {
                    let state = if vkey == VK_LWIN_CODE { &LWIN_DOWN } else { &RWIN_DOWN };
                    if is_keydown {
                        state.store(true, Ordering::Relaxed);
                    } else if is_keyup {
                        state.store(false, Ordering::Relaxed);
                    }
                    return;
                }

                if is_keydown {
                    let is_ctrl_v = CTRL_DOWN.load(Ordering::Relaxed)
                        && (vkey == b'V' as u32 || vkey == b'v' as u32);
                    let is_shift_insert = SHIFT_DOWN.load(Ordering::Relaxed) && vkey == VK_INSERT_CODE;

                    if is_ctrl_v || is_shift_insert {
                        AppSounds::play_paste_immediate();
                    }
                }
            }
            _ => {}
        }
    }

    use tauri::{Emitter, Manager, WebviewWindow};
    use tauri_plugin_global_shortcut::{Code, Shortcut};

    #[cfg(target_os = "windows")]
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU,
        VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN,
    };

    pub(crate) fn enable_quickpaste_keyboard_mode() {
        let settings = crate::get_settings();
        if !settings.quickpaste_enabled || !settings.quickpaste_paste_on_modifier_release {
            disable_quickpaste_keyboard_mode();
            return;
        }

        QUICKPASTE_REQUIRED_MODIFIER_MASK.store(
            quickpaste_modifier_mask_from_shortcut(&settings.quickpaste_shortcut),
            Ordering::Relaxed,
        );
        QUICKPASTE_SECONDARY_KEY_VK.store(
            quickpaste_secondary_key_vk_from_shortcut(&settings.quickpaste_shortcut).unwrap_or(0),
            Ordering::Relaxed,
        );
        QUICKPASTE_SECONDARY_KEY_DOWN.store(false, Ordering::SeqCst);
        QUICKPASTE_SECONDARY_KEY_PRESS_ID.fetch_add(1, Ordering::SeqCst);
        QUICKPASTE_HIDE_TRIGGERED.store(false, Ordering::SeqCst);
        QUICKPASTE_KEYBOARD_MODE_ENABLED.store(true, Ordering::SeqCst);
    }

    pub(crate) fn disable_quickpaste_keyboard_mode() {
        QUICKPASTE_KEYBOARD_MODE_ENABLED.store(false, Ordering::SeqCst);
        QUICKPASTE_REQUIRED_MODIFIER_MASK.store(0, Ordering::Relaxed);
        QUICKPASTE_SECONDARY_KEY_VK.store(0, Ordering::Relaxed);
        QUICKPASTE_SECONDARY_KEY_DOWN.store(false, Ordering::SeqCst);
        QUICKPASTE_SECONDARY_KEY_PRESS_ID.fetch_add(1, Ordering::SeqCst);
        QUICKPASTE_HIDE_TRIGGERED.store(false, Ordering::SeqCst);
    }

    // 仅复位"本会话已触发隐藏"标记,不动键盘模式的任何状态。
    // show_quickpaste_window 显示路径调用:上次会话该标记残留为 true 时,
    // 本次会话松开修饰键的隐藏判断会被跳过,便捷粘贴窗口卡住不关。
    pub(crate) fn reset_quickpaste_hide_triggered() {
        QUICKPASTE_HIDE_TRIGGERED.store(false, Ordering::SeqCst);
    }

    // 非键盘模式(global.rs 快捷键 Released 路径)发起延迟隐藏请求时
    // 先置位隐藏触发标记,与键盘模式共用同一枚原子——show 路径复用
    // reset_quickpaste_hide_triggered 复位,延迟回调由 take_ 消费。
    pub(crate) fn mark_quickpaste_hide_triggered() {
        QUICKPASTE_HIDE_TRIGGERED.store(true, Ordering::SeqCst);
    }

    // 一次性判定"应否执行隐藏"——swap(false) 取走标记:若期间有
    // 新会话 show 路径复位过(值 false)则放弃,否则消费并执行隐藏。
    pub(crate) fn take_quickpaste_hide_triggered() -> bool {
        QUICKPASTE_HIDE_TRIGGERED.swap(false, Ordering::SeqCst)
    }

    pub(crate) fn start_quickpaste_secondary_key_hold() {
        if !QUICKPASTE_KEYBOARD_MODE_ENABLED.load(Ordering::SeqCst) {
            return;
        }

        let secondary_vk = QUICKPASTE_SECONDARY_KEY_VK.load(Ordering::Relaxed);
        if secondary_vk != 0 && is_vk_pressed(secondary_vk) {
            start_quickpaste_secondary_repeat(secondary_vk);
        }
    }

    fn quickpaste_modifier_mask_from_shortcut(shortcut: &str) -> u8 {
        let mut mask = 0;
        for part in shortcut.split('+') {
            match part.trim() {
                "Ctrl" | "Control" => mask |= 0x01,
                "Alt" => mask |= 0x02,
                "Shift" => mask |= 0x04,
                "Win" | "Super" | "Meta" | "Cmd" | "Command" => mask |= 0x08,
                _ => {}
            }
        }
        mask
    }

    fn quickpaste_modifier_mask_from_vk(vk: u32) -> u8 {
        if vk == VK_CONTROL.0 as u32 || vk == VK_LCONTROL.0 as u32 || vk == VK_RCONTROL.0 as u32 {
            return 0x01;
        }
        if vk == VK_MENU.0 as u32 || vk == VK_LMENU.0 as u32 || vk == VK_RMENU.0 as u32 {
            return 0x02;
        }
        if vk == VK_SHIFT.0 as u32 || vk == VK_LSHIFT.0 as u32 || vk == VK_RSHIFT.0 as u32 {
            return 0x04;
        }
        if vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32 {
            return 0x08;
        }
        0
    }

    fn is_quickpaste_secondary_key_candidate(vk: u32) -> bool {
        vk > 0
            && quickpaste_modifier_mask_from_vk(vk) == 0
            && !matches!(vk, 0x01..=0x06)
    }

    fn quickpaste_secondary_key_vk_from_shortcut(shortcut: &str) -> Option<u32> {
        let shortcut = shortcut
            .replace("Win+", "Super+")
            .parse::<Shortcut>()
            .ok()?;
        quickpaste_code_to_vk(shortcut.key)
    }

    fn quickpaste_code_to_vk(code: Code) -> Option<u32> {
        let key = code.to_string();

        if let Some(value) = key.strip_prefix("Key") {
            if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() {
                return Some(value.as_bytes()[0].to_ascii_uppercase() as u32);
            }
        }

        if let Some(value) = key.strip_prefix("Digit") {
            if value.len() == 1 && value.as_bytes()[0].is_ascii_digit() {
                return Some(value.as_bytes()[0] as u32);
            }
        }

        if let Some(num) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
            if (1..=24).contains(&num) {
                return Some(0x6F + num);
            }
        }

        if let Some(num) = key.strip_prefix("Numpad").and_then(|n| n.parse::<u32>().ok()) {
            if num <= 9 {
                return Some(0x60 + num);
            }
        }

        const NAMED_KEY_VKS: &[(&str, u32)] = &[
            ("Backquote", 0xC0), ("Minus", 0xBD), ("Equal", 0xBB), ("BracketLeft", 0xDB),
            ("BracketRight", 0xDD), ("Backslash", 0xDC), ("Semicolon", 0xBA), ("Quote", 0xDE),
            ("Comma", 0xBC), ("Period", 0xBE), ("Slash", 0xBF), ("Backspace", 0x08),
            ("Insert", 0x2D), ("Delete", 0x2E), ("Home", 0x24), ("End", 0x23),
            ("PageUp", 0x21), ("PageDown", 0x22), ("Space", 0x20), ("Tab", 0x09),
            ("Enter", 0x0D), ("Escape", 0x1B), ("ArrowUp", 0x26), ("ArrowDown", 0x28),
            ("ArrowLeft", 0x25), ("ArrowRight", 0x27),
        ];

        NAMED_KEY_VKS
            .iter()
            .find_map(|(name, vk)| (*name == key).then_some(*vk))
    }

    fn is_vk_pressed(vk: u32) -> bool {
        unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
    }

    fn handle_quickpaste_keyboard_event(vk: u32, is_keydown: bool, is_keyup: bool) {
        if !QUICKPASTE_KEYBOARD_MODE_ENABLED.load(Ordering::SeqCst) || (!is_keydown && !is_keyup) {
            return;
        }

        if is_keydown {
            handle_quickpaste_secondary_key_down(vk);
            return;
        }

        handle_quickpaste_secondary_key_up(vk);
        handle_quickpaste_modifier_release(vk);
    }

    fn handle_quickpaste_secondary_key_down(vk: u32) {
        if !is_quickpaste_secondary_key_candidate(vk) {
            return;
        }

        let secondary_vk = QUICKPASTE_SECONDARY_KEY_VK.load(Ordering::Relaxed);
        if secondary_vk == 0 {
            // 快捷键只有修饰键组合时无副键——不允许把任意按键兜底充当
            // 触发键,否则按住空格/字母即开始切换条目,误触频发。
            return;
        }
        if vk != secondary_vk {
            return;
        }

        start_quickpaste_secondary_repeat(vk);
    }

    fn start_quickpaste_secondary_repeat(vk: u32) {
        if !crate::windows::quickpaste::is_visible() {
            return;
        }

        if QUICKPASTE_SECONDARY_KEY_DOWN.swap(true, Ordering::SeqCst) {
            return;
        }

        let press_id = QUICKPASTE_SECONDARY_KEY_PRESS_ID
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(QUICKPASTE_REPEAT_INITIAL_DELAY_MS));

            while QUICKPASTE_KEYBOARD_MODE_ENABLED.load(Ordering::SeqCst)
                && QUICKPASTE_SECONDARY_KEY_DOWN.load(Ordering::SeqCst)
                && QUICKPASTE_SECONDARY_KEY_PRESS_ID.load(Ordering::SeqCst) == press_id
                && crate::windows::quickpaste::is_visible()
                && is_vk_pressed(vk)
            {
                handle_quickpaste_next_request_impl();
                thread::sleep(Duration::from_millis(QUICKPASTE_REPEAT_INTERVAL_MS));
            }

            if QUICKPASTE_SECONDARY_KEY_PRESS_ID.load(Ordering::SeqCst) == press_id {
                QUICKPASTE_SECONDARY_KEY_DOWN.store(false, Ordering::SeqCst);
            }
        });
    }

    fn handle_quickpaste_secondary_key_up(vk: u32) {
        let secondary_vk = QUICKPASTE_SECONDARY_KEY_VK.load(Ordering::Relaxed);
        if secondary_vk != 0 && vk == secondary_vk {
            QUICKPASTE_SECONDARY_KEY_DOWN.store(false, Ordering::SeqCst);
            QUICKPASTE_SECONDARY_KEY_PRESS_ID.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn handle_quickpaste_next_request_impl() {
        input_common::run_on_main_thread(|| {
            let settings = crate::get_settings();
            if !settings.quickpaste_enabled || !settings.quickpaste_paste_on_modifier_release {
                return;
            }

            if !crate::windows::quickpaste::is_visible() {
                return;
            }

            if let Some(app) = input_common::try_get_app_handle() {
                if let Some(window) = app.get_webview_window("quickpaste") {
                    let _ = window.emit("quickpaste-next", ());
                }
            }
        });
    }

    fn handle_quickpaste_modifier_release(vk: u32) {
        let required = QUICKPASTE_REQUIRED_MODIFIER_MASK.load(Ordering::Relaxed);
        let released_mask = quickpaste_modifier_mask_from_vk(vk);

        // 本次 keyup 既不是必需要、必需要也未全抬时提前退出;否则继续走
        // 全空求值——先松必需要后松非必需键(如 Ctrl 先松 Shift 后松)时,
        // 后松的 Shift keyup 仍会重新求值 all_released,隐藏不再被跳过。
        let (ctrl, alt, shift, meta) = get_modifier_keys_state_impl();
        let all_released = ((required & 0x01) == 0 || !ctrl)
            && ((required & 0x02) == 0 || !alt)
            && ((required & 0x04) == 0 || !shift)
            && ((required & 0x08) == 0 || !meta);
        let is_required_release = (released_mask & required) != 0;
        if !is_required_release && !all_released {
            return;
        }

        if all_released && !QUICKPASTE_HIDE_TRIGGERED.swap(true, Ordering::SeqCst) {
            handle_quickpaste_hide_request_impl();
        }
    }

    fn handle_quickpaste_hide_request_impl() {
        input_common::run_on_main_thread(|| {
            let settings = crate::get_settings();
            if !settings.quickpaste_enabled || !settings.quickpaste_paste_on_modifier_release {
                return;
            }

            if !crate::windows::quickpaste::is_visible() {
                return;
            }

            if let Some(app) = input_common::try_get_app_handle() {
                if let Some(window) = app.get_webview_window("quickpaste") {
                    let _ = window.emit("quickpaste-hide", ());
                }
            }

            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                input_common::run_on_main_thread(|| {
                    if let Some(app) = input_common::try_get_app_handle() {
                        // 50ms 延迟期间用户可能再次唤出 quickpaste(新会话),
                        // show 路径已 reset_quickpaste_hide_triggered 复位标记。
                        // 旧 hide 请求若直接 hide 会把新窗口立即误隐藏——因此
                        // 此刻用 swap(false) 一次性判定:标记仍为 true(发起置位
                        // 后既无新会话也无已执行隐藏)才执行 hide,否则放弃由
                        // 新会话的显式隐藏流程接管。不能以 is_visible 为前置
                        // 条件(重开后窗口重新可见,反而误关新窗口)。
                        if QUICKPASTE_HIDE_TRIGGERED.swap(false, Ordering::SeqCst) {
                            let _ = crate::windows::quickpaste::hide_quickpaste_window(&app);
                        }
                    }
                });
            });
        });
    }

    fn should_handle_click_outside_impl() -> bool {
        if input_common::is_mouse_monitoring_enabled() {
            return true;
        }

        if crate::services::low_memory::is_low_memory_mode()
            && crate::services::low_memory::is_panel_visible()
        {
            return true;
        }

        if let Some(app) = input_common::try_get_app_handle() {
            if let Some(win) = app.get_webview_window("screenshot") {
                return win.is_visible().unwrap_or(false);
            }
        }

        false
    }

    fn current_unix_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn is_in_tray_click_region() -> bool {
        let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
        LAST_TRAY_RECT
            .lock()
            .map(|(left, top, right, bottom)| {
                cursor_x >= left && cursor_x <= right && cursor_y >= top && cursor_y <= bottom
            })
            .unwrap_or(false)
    }

    fn schedule_preview_guard_check() {
        if !input_common::is_mouse_monitoring_enabled() {
            return;
        }

        let now_ms = current_unix_ms();
        {
            let last_run_ms = PREVIEW_GUARD_LAST_RUN_MS.lock();
            if now_ms.saturating_sub(*last_run_ms) < PREVIEW_GUARD_THROTTLE_MS {
                return;
            }
        }

        if PREVIEW_GUARD_PENDING.swap(true, Ordering::SeqCst) {
            return;
        }

        input_common::run_on_main_thread(|| {
            handle_preview_guard_check_impl();
            *PREVIEW_GUARD_LAST_RUN_MS.lock() = current_unix_ms();
            PREVIEW_GUARD_PENDING.store(false, Ordering::SeqCst);
        });
    }

    fn check_modifier_requirement_impl(required: &str) -> bool {
        let (ctrl, alt, shift, meta) = get_modifier_keys_state_impl();

        if required == "None" || required.is_empty() {
            return true;
        }

        let parts: Vec<&str> = required.split('+').collect();
        let need_ctrl = parts.contains(&"Ctrl");
        let need_alt = parts.contains(&"Alt");
        let need_shift = parts.contains(&"Shift");
        let need_meta = parts
            .iter()
            .any(|&p| matches!(p, "Win" | "Super" | "Meta" | "Cmd" | "Command"));

        (!need_ctrl || ctrl)
            && (!need_alt || alt)
            && (!need_shift || shift)
            && (!need_meta || meta)
            && (need_ctrl || !ctrl)
            && (need_alt || !alt)
            && (need_shift || !shift)
            && (need_meta || !meta)
    }

    fn get_modifier_keys_state_impl() -> (bool, bool, bool, bool) {
        unsafe {
            let ctrl = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            let alt = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
            let shift = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let meta = (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0
                || (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0;
            (ctrl, alt, shift, meta)
        }
    }

    fn handle_middle_button_action_impl() {
        let settings = crate::get_settings();
        if !settings.mouse_middle_button_enabled {
            return;
        }

        if crate::services::system::is_front_app_globally_disabled_from_settings() {
            return;
        }

        if !check_modifier_requirement_impl(&settings.mouse_middle_button_modifier) {
            return;
        }

        if let Some(app) = input_common::try_get_app_handle() {
            crate::toggle_main_window_visibility(&app);
        }
    }

    fn handle_middle_button_down_impl() {
        let settings = crate::get_settings();
        if !settings.mouse_middle_button_enabled {
            return;
        }

        if crate::services::system::is_front_app_globally_disabled_from_settings() {
            return;
        }

        if !check_modifier_requirement_impl(&settings.mouse_middle_button_modifier) {
            return;
        }

        if settings.mouse_middle_button_trigger != "long_press" {
            input_common::run_on_main_thread(|| {
                handle_middle_button_action_impl();
            });
            return;
        }

        let threshold_ms = settings.mouse_middle_button_long_press_ms.max(1) as u64;
        let press_id = MIDDLE_BUTTON_PRESS_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        MIDDLE_BUTTON_DOWN.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(threshold_ms));

            if !MIDDLE_BUTTON_DOWN.load(Ordering::SeqCst) {
                return;
            }

            if MIDDLE_BUTTON_PRESS_ID.load(Ordering::SeqCst) != press_id {
                return;
            }

            input_common::run_on_main_thread(move || {
                if !MIDDLE_BUTTON_DOWN.load(Ordering::SeqCst) {
                    return;
                }

                if MIDDLE_BUTTON_PRESS_ID.load(Ordering::SeqCst) != press_id {
                    return;
                }

                handle_middle_button_action_impl();
            });
        });
    }

    fn is_mouse_outside_window_impl(window: &WebviewWindow) -> bool {
        let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();

        let (win_x, win_y, win_width, win_height) = match crate::get_window_bounds(window) {
            Ok(bounds) => bounds,
            Err(_) => return false,
        };

        cursor_x < win_x || cursor_x > win_x + win_width as i32
            || cursor_y < win_y || cursor_y > win_y + win_height as i32
    }

    fn handle_preview_guard_check_impl() {
        let Some(app) = input_common::try_get_app_handle() else {
            return;
        };

        if app.get_webview_window("preview-window").is_none() {
            return;
        }

        let Some(main_window) = input_common::try_get_main_window() else {
            crate::windows::preview_window::force_close_preview_window(&app);
            return;
        };

        let state = crate::get_window_state();
        if state.state != crate::WindowState::Visible || state.is_hidden {
            crate::windows::preview_window::force_close_preview_window(&app);
            return;
        }

        let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
        let in_menu_region = crate::is_context_menu_visible()
            && crate::windows::plugins::context_menu::is_point_in_menu_region(cursor_x, cursor_y);

        if in_menu_region {
            crate::windows::preview_window::force_close_preview_window(&app);
            return;
        }

        if is_mouse_outside_window_impl(&main_window) {
            crate::windows::preview_window::force_close_preview_window(&app);
        }
    }

    fn handle_click_outside_impl() {
        if is_in_tray_click_region() {
            return;
        }

        if crate::services::low_memory::is_low_memory_mode()
            && crate::services::low_memory::is_panel_visible()
        {
            let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
            if !crate::services::low_memory::is_point_in_panel(cursor_x, cursor_y) {
                let _ = crate::services::low_memory::hide_panel();
            }
            return;
        }

        if crate::is_context_menu_visible() {
            if let Some(main_window) = input_common::try_get_main_window() {
                if let Some(menu_window) = main_window.app_handle().get_webview_window("context-menu") {
                    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
                    if menu_window.is_visible().unwrap_or(false)
                        && !crate::windows::plugins::context_menu::is_point_in_menu_region(cursor_x, cursor_y)
                    {
                        let _ = menu_window.emit("close-context-menu", ());
                    }
                }
            }
            return;
        }

        if !should_handle_click_outside_impl() {
            return;
        }

        if let Some(window) = input_common::try_get_main_window() {
            let state = crate::get_window_state();

            if state.is_hidden {
                return;
            }

            if state.is_pinned {
                return;
            }

            if window.is_visible().unwrap_or(false) && is_mouse_outside_window_impl(&window) {
                let _ = crate::check_snap(&window);
                crate::hide_main_window(&window);
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) use windows_raw_input::{
    disable_quickpaste_keyboard_mode,
    enable_quickpaste_keyboard_mode,
    get_physical_modifier_keys_state,
    guard_tray_click_region,
    mark_quickpaste_hide_triggered,
    reset_quickpaste_hide_triggered,
    start_quickpaste_secondary_key_hold,
    start_raw_input_if_needed,
    take_quickpaste_hide_triggered,
};

#[cfg(target_os = "windows")]
mod windows_raw_input_tests {
    // §10.3 源码护栏(会话重检):延迟隐藏线程的回调必须以隐藏触发标记
    // (QUICKPASTE_HIDE_TRIGGERED)的 swap(false) 判定是否仍应隐藏,先于
    // hide_quickpaste_window;且禁止以 is_visible() 作前置条件——重开后窗口
    // 重新可见,若以可见性作守卫反而把新会话窗口立即误隐藏(方向反转)。
    #[test]
    fn delayed_hide_guard_rechecks_visibility_before_hide() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/raw_input.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 raw_input.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 提取 handle_quickpaste_hide_request_impl 函数体(到闭合 } 为止)
        let start = stripped
            .find("fn handle_quickpaste_hide_request_impl")
            .expect("缺 handle_quickpaste_hide_request_impl");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        // 延迟线程回调(50ms sleep 之后)必须先 swap 判定标记再 hide
        let sleep_pos = body
            .find("sleep(std::time::Duration::from_millis(50))")
            .expect("延迟隐藏必须 sleep 50ms");
        let swap_pos = body[sleep_pos..]
            .find("QUICKPASTE_HIDE_TRIGGERED.swap(false, Ordering::SeqCst)")
            .map(|i| sleep_pos + i)
            .expect("延迟回调必须用 swap(false) 一次性判定隐藏触发标记");
        let hide_pos = body[sleep_pos..]
            .find("hide_quickpaste_window(&app)")
            .map(|i| sleep_pos + i)
            .expect("延迟回调必须调 hide_quickpaste_window");
        assert!(
            swap_pos < hide_pos,
            "延迟 hide 回调必须先判定隐藏标记再隐藏"
        );
        // 方向断言:swap 判定到 hide 之间不得以 is_visible() 作前置条件——
        // 重开后窗口重新可见,若以可见性作守卫反而误关新窗口。
        assert!(
            !body[sleep_pos..hide_pos].contains("quickpaste::is_visible()"),
            "延迟回调禁止以 is_visible() 作为 hide 前置条件(重开后可见,会把新窗口误隐藏)"
        );
    }

    // 护栏:显示路径必须复位 QUICKPASTE_HIDE_TRIGGERED,否则上次会话
    // 残留的标记会让本次会话松开修饰键时跳过隐藏,窗口卡住。
    #[test]
    fn show_quickpaste_resets_hide_triggered_flag() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/quickpaste/manager.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 manager.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub fn show_quickpaste_window")
            .expect("缺 show_quickpaste_window");
        let rest = &stripped[start..];
        let end = rest.find("\nfn ").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains("reset_quickpaste_hide_triggered()"),
            "显示窗口路径必须调用 reset_quickpaste_hide_triggered 复位隐藏触发标记"
        );
        let reset_pos = body
            .find("reset_quickpaste_hide_triggered()")
            .expect("显示路径必须复位标记");
        let show_pos = body
            .find("window.show()")
            .expect("显示路径应调 window.show");
        assert!(
            reset_pos < show_pos,
            "复位必须在显示窗口之前完成,避免窗口显示期间隐藏判断被跳过"
        );
    }

    // 护栏:复位函数只复位标记,不得改动键盘模式其他状态。
    #[test]
    fn reset_hide_triggered_only_touches_flag() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/raw_input.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 raw_input.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub(crate) fn reset_quickpaste_hide_triggered")
            .expect("缺 reset_quickpaste_hide_triggered");
        // 模块内函数闭合花括号有缩进,不能用顶层 \n}\n 锚;花括号配对扫描,
        // 与缩进无关,精确截到本函数末尾的 }。
        let mut depth = 0i32;
        let mut end = stripped.len();
        for (idx, ch) in stripped[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + idx + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &stripped[start..end];
        assert!(
            body.contains("QUICKPASTE_HIDE_TRIGGERED.store(false"),
            "复位函数必须把标记写回 false"
        );
        assert!(
            !body.contains("QUICKPASTE_KEYBOARD_MODE_ENABLED"),
            "复位函数不得改动键盘模式开关"
        );
    }

    // 护栏:非键盘模式延迟隐藏路径必须与键盘模式同款会话守卫——
    // 发起隐藏请求时置位标记(mark),延迟回调用 swap(false) 一次性判定(take),
    // 且 take 必须先于 hide_quickpaste_window;禁止以 is_visible() 作前置条件。
    #[test]
    fn released_path_delayed_hide_uses_session_guard() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/hotkey/global.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 global.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub fn register_quickpaste_hotkey")
            .expect("缺 register_quickpaste_hotkey");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];

        // Released 分支(快捷方式松开)必须置位标记
        let released_pos = body
            .find("ShortcutState::Released")
            .expect("快捷键 Released 分支必须存在");
        let mark_pos = body[released_pos..]
            .find("mark_quickpaste_hide_triggered()")
            .map(|i| released_pos + i)
            .expect("Released 分支必须先置位隐藏触发标记(mark)");
        // 置位必须在 50ms 延迟 spawn 之前
        let sleep_pos = body[mark_pos..]
            .find("sleep(std::time::Duration::from_millis(50))")
            .map(|i| mark_pos + i)
            .expect("延迟隐藏必须 sleep 50ms");
        assert!(
            mark_pos < sleep_pos,
            "隐藏触发标记置位必须先于延迟隐藏线程 spawn"
        );
        // 延迟回调用 swap(false) 一次性判定(take)且先于 hide_quickpaste_window
        let take_pos = body[sleep_pos..]
            .find("take_quickpaste_hide_triggered()")
            .map(|i| sleep_pos + i)
            .expect("延迟回调必须用 take(swap(false)) 一次性判定隐藏触发标记");
        let hide_pos = body[sleep_pos..]
            .find("hide_quickpaste_window(&app_clone)")
            .map(|i| sleep_pos + i)
            .expect("延迟回调必须调 hide_quickpaste_window");
        assert!(
            take_pos < hide_pos,
            "延迟 hide 回调必须先判定隐藏标记再隐藏"
        );
        assert!(
            !body[sleep_pos..hide_pos].contains("quickpaste::is_visible()"),
            "延迟回调禁止以 is_visible() 作为 hide 前置条件(重开后可见,会把新窗口误隐藏)"
        );
    }

    // 恢复物理修饰键必须读实时按键状态,禁止读 tracker:
    // tracker 会被 PASTE_INPUT_MARKER 分支的早退跳过低层修饰键 keyup,
    // 快速连按粘贴后 Ctrl/Shift/Win 被记成"仍按下"(粘滞修饰键)。
    #[test]
    fn physical_restore_reads_live_key_state_not_trackers() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let body = fn_body(&src, "get_physical_modifier_keys_state");
        assert!(
            body.contains("GetAsyncKeyState(VK_CONTROL"),
            "必须用 GetAsyncKeyState 读实时物理状态"
        );
        assert!(
            body.contains("GetAsyncKeyState(VK_MENU"),
            "必须包含 Alt(VK_MENU) 实时读取"
        );
        assert!(
            !body.contains("CTRL_DOWN.load"),
            "禁止回退到 tracker——tracker 会被注入键的 marker 早退污染"
        );
        assert!(
            !body.contains("SHIFT_DOWN.load"),
            "禁止回退到 tracker——tracker 会被注入键的 marker 早退污染"
        );
    }

    // 护栏:修饰键释放必须重求值全空——先松必需要后松非必需键时,
    // 后松的键 keyup 仍要触达隐藏判定,不得被"非必需要早退"跳过。断言:
    // 函数体内出现对本次释放掩码的判断且早退条件同时要求"非必需要且未全空"。
    #[test]
    fn modifier_release_reevaluates_all_released_on_non_required_keyup() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/raw_input.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 raw_input.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("fn handle_quickpaste_modifier_release")
            .expect("缺 handle_quickpaste_modifier_release");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        // 早退条件必须同时包含"非必需要释放"与"未全空",缺一即会吞掉后松场景
        let non_required_pos = body
            .find("released_mask & required) != 0")
            .expect("必须判断本次释放键是否属于必需要组合");
        assert!(
            body[..non_required_pos].contains("get_modifier_keys_state_impl"),
            "全空求值必须发生在本函数内(实时读修饰键状态)"
        );
        // 早退只能在"非必需要且未全空"时发生——断言函数内没有出现把
        // 非必需要释放直接 return 的旧逻辑
        assert!(
            !body.contains("return;\n        }\n\n        let (ctrl"),
            "非必需要释放不得直接 return(会跳过先松必需要后松非必需键场景)"
        );
        // 隐藏触发必须在全空成立时,且以 QUICKPASTE_HIDE_TRIGGERED 消费
        let hide_pos = body
            .find("QUICKPASTE_HIDE_TRIGGERED.swap(true")
            .expect("隐藏判定必须消费 QUICKPASTE_HIDE_TRIGGERED");
        let all_pos = body
            .find("all_released")
            .expect("必须求值 all_released");
        assert!(
            all_pos < hide_pos,
            "必须先在 all_released 成立后才触发隐藏"
        );
    }

    // 护栏:快捷键只有修饰键组合(无副键)时,不得把任意按键兜底存为
    // 副键——否则按住空格/字母即开始切换条目。必须:副键为 0 时直接
    // 返回,且函数体内不得出现把 vk 写进副键寄存器的兜底代码。
    #[test]
    fn no_secondary_key_does_not_fallback_to_arbitrary_key() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let body = fn_body(&src, "handle_quickpaste_secondary_key_down");
        // 无副键早退必须发生在 repeat 之前
        let zero_pos = body
            .find("secondary_vk == 0")
            .expect("必须判断无副键情况");
        let repeat_pos = body
            .find("start_quickpaste_secondary_repeat")
            .expect("候选副键按下后应进入切换逻辑");
        assert!(
            zero_pos < repeat_pos,
            "无副键早退必须发生在 repeat 之前"
        );
        // 禁止任意键兜底:不得把任意按键 vk 写进副键寄存器
        assert!(
            !body.contains("QUICKPASTE_SECONDARY_KEY_VK.store(vk"),
            "禁止把任意按键兜底存为副键——按住任意键即切换条目"
        );
    }

    // 护栏:配置 Win/Super/Meta/Cmd 家族修饰键不得被忽略——若解构时
    // 丢弃 meta 状态,按 Win 配置的判定会与真实键盘状态无关(方向反转),
    // 需要的键与禁止的键都会被跳过。
    #[test]
    fn win_modifier_requirement_not_ignored() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let body = fn_body(&src, "check_modifier_requirement_impl");
        // 解构必须接收 meta,禁止 _meta 丢弃
        assert!(
            body.contains("let (ctrl, alt, shift, meta) = get_modifier_keys_state_impl()"),
            "必须接收 meta 键状态,禁止 _meta 丢弃"
        );
        // 必须把 Win 家族解析进 need_meta
        let need_meta_pos = body
            .find("need_meta")
            .expect("必须解析 Win/Super/Meta 家族需求");
        let parse_pos = body
            .find("\"Win\" | \"Super\" | \"Meta\"")
            .expect("必须识别 Win/Super/Meta/Cmd/Command 家族写法");
        // need_meta 必须同时用于"需要则必须按下"与"不需要则禁止按下"
        let require_pos = body
            .find("need_meta || meta")
            .expect("必须包含 need_meta 满足蕴含");
        let forbid_pos = body
            .find("need_meta || !meta")
            .expect("必须包含未配置时禁止 meta 按下");
        assert!(
            need_meta_pos < require_pos && need_meta_pos < forbid_pos && parse_pos < forbid_pos,
            "need_meta 必须先解析得出,再用于满足与禁止双向判定"
        );
    }

    // 护栏:注册失败路径必须销毁已创建的隐藏窗口——若只复位状态标记
    // 就返回,CreateWindowExW 创建的 sink 窗口残留,句柄泄漏且线程
    // 消息队列仍挂着未被销毁的窗口。
    #[test]
    fn raw_input_register_failure_destroys_sink_window() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let body = fn_body(&src, "start_raw_input_if_needed");
        // CreateWindowExW 必须存在,失败路径才有能销毁的对象
        body.find("CreateWindowExW")
            .expect("必须创建 raw input sink 窗口");
        // RegisterRawInputDevices 失败分支内必须先销毁窗口再 reset 状态
        let register_pos = body
            .find("RegisterRawInputDevices(&rid")
            .expect("必须注册 raw input 设备");
        let fail_pos = body[register_pos..]
            .find("RegisterRawInputDevices 失败")
            .map(|i| register_pos + i)
            .expect("必须存在注册失败日志");
        let destroy_pos = body[register_pos..]
            .find("DestroyWindow(hwnd)")
            .map(|i| register_pos + i)
            .expect("注册失败必须销毁 sink 窗口");
        let reset_pos = body[register_pos..]
            .find("RAW_INPUT_ACTIVE.store(false")
            .map(|i| register_pos + i)
            .unwrap_or(usize::MAX);
        assert!(
            destroy_pos < reset_pos,
            "必须先把 sink 窗口销毁再复位状态标记"
        );
    }

    // 护栏:滚轮事件不得派发到空实现(曾有每次滚轮都跨线程投递一次
    // 空调用,纯浪费)。删除 wheel 派发与空函数后,生产代码区域不应
    // 再出现 adapter 调用或定义。检查范围限定在测试模块(本模块声明)
    // 之前,避免护栏自身字符串字面量误命中。
    #[test]
    fn wheel_event_has_no_empty_dispatch() {
        use crate::services::system::hotkey::test_utils::{source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let prod_end = src
            .find("mod windows_raw_input_tests")
            .unwrap_or(src.len());
        let prod = &src[..prod_end];
        assert!(
            !prod.contains("handle_wheel_event_impl"),
            "生产代码不得存在 wheel 空处理实现——每次滚轮跨线程投递空调用是纯浪费"
        );
    }

    // 护栏:配置修饰键时中键长按必须仍生效——"配置了修饰键就立即触发并
    // return"的短路分支若存在,会把 long_press 模式的中键长按整个跳过。
    // 正确语义:修饰键约束已由 check_modifier_requirement_impl 统一检查,
    // 触发方式判定后直接进入长按启动,不得再有按修饰键配置短路的分支。
    #[test]
    fn middle_button_long_press_not_short_circuited_by_modifier_branch() {
        use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

        let src = strip_line_comments(&source_file("src/services/system/raw_input.rs"));
        let body = fn_body(&src, "handle_middle_button_down_impl");
        // 触发方式判定必须存在
        let long_press_pos = body
            .find("mouse_middle_button_trigger != \"long_press\"")
            .expect("必须存在 long_press 触发判定");
        // 禁止修饰键短路分支:它会在"配了修饰键 + 长按模式"时先于长按返回
        assert!(
            !body.contains("mouse_middle_button_modifier != \"None\""),
            "禁止按修饰键配置短路触发——配置修饰键时中键长按被跳过"
        );
        // long_press 判定之后必须直接进入长按启动逻辑(threshold_ms)
        body[long_press_pos..]
            .find("threshold_ms")
            .expect("long_press 判定后必须直接启动长按计时");
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_raw_input_if_needed() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn enable_quickpaste_keyboard_mode() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn disable_quickpaste_keyboard_mode() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_quickpaste_secondary_key_hold() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn guard_tray_click_region(_x: i32, _y: i32, _width: u32, _height: u32) {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reset_quickpaste_hide_triggered() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn mark_quickpaste_hide_triggered() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn take_quickpaste_hide_triggered() -> bool {
    true
}


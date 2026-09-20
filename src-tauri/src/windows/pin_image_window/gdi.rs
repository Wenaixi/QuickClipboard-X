// 贴图 GDI 分层窗口实现
//
// 目标:把贴图窗口从"每图一个 WebView(约 40-100MB renderer)"换成原生
// GDI 分层窗口,内存占用趋近于零。本文件是核心——窗口类注册、
// CreateWindowExW(WS_EX_LAYERED) 建窗、DIB 位图渲染(UpdateLayeredWindow)、
// 消息循环、HWND 表。
//
// 技术要点(对照 ShareX RegionCaptureLightForm.SelectBitmap 与
// fluor windows_layered.rs 预乘实现核实):
//   - UpdateLayeredWindow + AC_SRC_ALPHA 需要预乘 alpha 的 BGRA 像素,
//     image crate 解码给的是非预乘 RGBA,必须逐像素 a + (a>>7) 修正后
//     (c*ae)>>8 预乘再打包——不是简单拷贝。
//   - WS_EX_LAYERED 窗口不触发 WM_PAINT,所有重绘都走 UpdateLayeredWindow。
//   - 句柄进出必须成对(SelectObject(hOldBitmap) → DeleteObject → DeleteDC
//     → ReleaseDC),任一缺失即 GDI 泄漏。
//   - 窗口类 RegisterClassW 只注册一次(static Once 守卫)。
//
// 本文件只立骨架(注册/渲染/消息入口/HWND 表),交互分支(拖窗/缩放/平移/
// 缩略图/右键菜单)在后续任务接入;护栏测试同时落地,CI 上可反证见红。

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::OnceCell;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, DIB_RGB_COLORS, GetDC,
    ReleaseDC, SelectObject, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, AC_SRC_ALPHA,
    AC_SRC_OVER, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, UpdateLayeredWindow, ULW_ALPHA,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WM_ACTIVATE, WM_CREATE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL,
    WM_MOUSEMOVE, WM_NCHITTEST, WM_RBUTTONUP, WS_POPUP,
};

/// 贴图窗口统一注册的窗口类名
pub(crate) const PIN_IMAGE_WINDOW_CLASS: &str = "QuickClipboardPinImageWindow";

/// 窗口句柄 ↔ 标签映射:GDI 窗口不经 tauri Manager,查找贴图窗口一律走此表。
/// 与 PIN_IMAGE_DATA_MAP(数据表)并存:本表只存 HWND,数据仍在数据表。
static PIN_IMAGE_HWND_MAP: OnceCell<Mutex<HashMap<String, isize>>> = OnceCell::new();

fn lock_hwnd_map() -> std::sync::MutexGuard<'static, HashMap<String, isize>> {
    PIN_IMAGE_HWND_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 每窗口运行时状态:菜单交互需要(e.g. 阴影/锁定/像素级开关、恢复模式、
/// 透明度)。状态跨线程共享由 GDI 层 Mutex 保护;持久化由 gdi_settings 完成。
#[derive(Clone, Debug)]
pub(crate) struct PinWindowState {
    pub shadow: bool,
    pub lock_position: bool,
    pub pixel_render: bool,
    pub opacity: u8,
    pub restore_mode: String,
    pub thumbnail_mode: bool,
}

impl Default for PinWindowState {
    fn default() -> Self {
        Self {
            shadow: false,
            lock_position: false,
            pixel_render: false,
            opacity: 100,
            restore_mode: "follow".to_string(),
            thumbnail_mode: false,
        }
    }
}

/// 全局贴图窗口默认状态(新窗口默认应用;与前端旧 localStorage 默认 8 项
/// 对齐:alwaysOnTop/shadow/lockPosition/pixelRender/opacity 100/thumbnailMode
/// false/thumbnailRestoreMode follow/savedThumbnailPosition null)。由
/// gdi_settings 从磁盘文件加载注入;这里提供默认值保证建窗前状态可用。
static PIN_DEFAULT_STATE: OnceCell<PinWindowState> = OnceCell::new();

/// 取全局默认状态(未初始化时返回内置默认)
pub(crate) fn default_pin_state() -> PinWindowState {
    PIN_DEFAULT_STATE.get().cloned().unwrap_or_default()
}

/// 设定全局默认状态(启动时从持久化文件加载后调用)
pub(crate) fn set_default_pin_state(state: PinWindowState) {
    let _ = PIN_DEFAULT_STATE.set(state);
}

/// 状态位开关(菜单切换用,避免逐个字段 setter)
#[derive(Clone, Copy)]
pub(crate) enum PinStateFlag {
    Shadow,
    LockPosition,
    PixelRender,
    RestoreMode,
}

/// 每窗口状态表:label → PinWindowState,与 HWND 表并行
static PIN_STATE_MAP: OnceCell<Mutex<HashMap<String, PinWindowState>>> = OnceCell::new();

fn lock_state_map() -> std::sync::MutexGuard<'static, HashMap<String, PinWindowState>> {
    PIN_STATE_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 读取窗口状态(缺省用默认值)
pub(crate) fn pin_state(label: &str) -> PinWindowState {
    lock_state_map()
        .get(label)
        .cloned()
        .unwrap_or_else(default_pin_state)
}

/// 写回窗口状态
pub(crate) fn set_pin_state(label: &str, state: PinWindowState) {
    lock_state_map().insert(label.to_string(), state);
}

/// 切换布尔状态位并写回
pub(crate) fn toggle_state_bool(label: &str, flag: PinStateFlag) -> Result<(), String> {
    let mut state = pin_state(label);
    match flag {
        PinStateFlag::Shadow => state.shadow = !state.shadow,
        PinStateFlag::LockPosition => state.lock_position = !state.lock_position,
        PinStateFlag::PixelRender => state.pixel_render = !state.pixel_render,
        PinStateFlag::RestoreMode => unreachable!("RestoreMode 走 set_state_str"),
    }
    set_pin_state(label, state);
    Ok(())
}

/// 设置字符串状态位(恢复模式)
pub(crate) fn set_state_str(label: &str, _flag: PinStateFlag, value: &str) -> Result<(), String> {
    let mut state = pin_state(label);
    state.restore_mode = value.to_string();
    set_pin_state(label, state);
    Ok(())
}

/// 记录 AppHandle 供菜单 SaveAs 使用(仅 UI 线程,静态 OnceCell)
static APP_HANDLE: OnceCell<tauri::AppHandle> = OnceCell::new();

pub(crate) fn init_app_handle(app: tauri::AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub(crate) fn app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.get().cloned()
}

/// 窗口类注册状态:RegisterClassW 只执行一次
static CLASS_REGISTERED: OnceCell<()> = OnceCell::new();

/// 注册贴图窗口类。重复调用安全(OnceCell 保证只注册一次)。
/// 窗口过程固定为 pin_image_window_proc。
pub(crate) fn register_window_class() -> Result<(), String> {
    CLASS_REGISTERED.get_or_try_init(|| unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| format!("获取模块句柄失败: {}", e))?;
        let class_name: Vec<u16> = PIN_IMAGE_WINDOW_CLASS.encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(pin_image_window_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        if atom == 0 {
            return Err("注册贴图窗口类失败".to_string());
        }
        Ok(())
    })
    .map(|_| ())
}

/// 创建贴图 GDI 分层窗口。
/// 返回窗口句柄,并在 HWND 表中登记 label → hwnd。
///
/// 参数:
///   - label:窗口标签("pin-image-N" 或预览固定 "image-preview")
///   - x/y/width/height:窗口物理位置与尺寸(物理像素)
///   - preview:预览模式(穿透 + 无交互)
pub(crate) fn create_gdi_window(
    label: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    preview: bool,
) -> Result<HWND, String> {
    register_window_class()?;

    // 置顶偏好:默认建窗置顶(WS_EX_TOPMOST),菜单可切换置顶(对齐原版
    // alwaysOnTop 默认 false 到 WebView 窗口置顶行为)。全局默认状态
    // shadow/lock_position/pixel_render/opacity/restore_mode 由 gdi_settings
    // 加载注入;置顶是运行时窗口扩展样式,不落状态表。
    let mut ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    if preview {
        ex_style |= WS_EX_TRANSPARENT;
    }

    let class_name: Vec<u16> = PIN_IMAGE_WINDOW_CLASS.encode_utf16().collect();
    let title: Vec<u16> = "贴图".encode_utf16().collect();
    // 建窗同时登记状态:继承全局默认(置顶/阴影/锁定/像素级/透明度/恢复模式)
    lock_state_map()
        .entry(label.to_string())
        .or_insert_with(|| pin_state(label));

    let hwnd = unsafe {
        // GetModuleHandleW 返回 HMODULE,CreateWindowExW 需要 HINSTANCE
        // (两者底层都是 *mut c_void 但属不同新类型,故需显式取 .0 再构造)
        let instance = GetModuleHandleW(None)
            .map_err(|e| format!("获取模块句柄失败: {}", e))?;
        // HWND 参数:CreateWindowExW 期望 Option<HWND>,句柄 0 表示不用父窗
        CreateWindowExW(
            ex_style,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            x,
            y,
            width as i32,
            height as i32,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            Some(std::ptr::null()),
        )
        .map_err(|e| format!("创建贴图窗口失败: {}", e))?
    };

    lock_hwnd_map().insert(label.to_string(), hwnd.0 as isize);
    Ok(hwnd)
}

/// 渲染图片到分层窗口。image crate 解码 → 非预乘 RGBA → 预乘 BGRA → DIB →
/// UpdateLayeredWindow。
///
/// 句柄生命周期严格配对:GetDC → CreateCompatibleDC → CreateDIBSection →
/// SelectObject(dib) → UpdateLayeredWindow → SelectObject(hOldBitmap) →
/// DeleteObject(dib) → DeleteDC → ReleaseDC。
pub(crate) fn render_image(
    hwnd: HWND,
    image_path: &str,
    opacity: u8,
) -> Result<(), String> {
    let decoded = image::ImageReader::open(image_path)
        .map_err(|e| format!("打开贴图文件失败: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("识别贴图格式失败: {}", e))?
        .decode()
        .map_err(|e| format!("解码贴图失败: {}", e))?
        .to_rgba8();

    let (w, h) = (decoded.width() as i32, decoded.height() as i32);
    if w <= 0 || h <= 0 {
        return Err("贴图尺寸无效".to_string());
    }

    let screen_dc = unsafe { GetDC(None) }.map_err(|e| format!("获取屏幕 DC 失败: {}", e))?;
    let mem_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) }
        .map_err(|e| format!("创建内存 DC 失败: {}", e))?;

    // 32bpp 自顶向下 DIB(负高度 = 顶行在内存首行,与 RGBA 行序一致)
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = unsafe { CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|e| format!("创建 DIB 位图失败: {}", e))?;
    if bits.is_null() {
        unsafe {
            DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);
        }
        return Err("创建 DIB 位图失败: DIB 数据指针为空".to_string());
    }

    // 非预乘 RGBA → 预乘 BGRA,逐像素写入 DIB
    let src = decoded.as_raw();
    let dst = unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize) };
    for (d, chunk) in dst.iter_mut().zip(src.chunks_exact(4)) {
        let r = chunk[0] as u32;
        let g = chunk[1] as u32;
        let b = chunk[2] as u32;
        let a = chunk[3] as u32;
        // 预乘:ae = a + (a>>7) 在 0..=255 内加权,(c*ae)>>8 保持 0..=255,
        // α=255 恒等透传、α=0 归零,满足 ULW_ALPHA 对预乘的约束
        let pr = premultiply(r, a);
        let pg = premultiply(g, a);
        let pb = premultiply(b, a);
        *d = (a << 24) | (pr << 16) | (pg << 8) | pb;
    }

    let old = unsafe { SelectObject(mem_dc, HGDIOBJ(dib)) };

    // 渲染目标位置取当前窗口在屏幕上的坐标:渲染需以窗口物理原点为准
    let window_rect = {
        let mut rect = windows::Win32::UI::WindowsAndMessaging::RECT::default();
        let ok = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect)
        };
        if ok.is_err() {
            return Err("读取贴图窗口坐标失败".to_string());
        }
        rect
    };
    let (x, y) = (window_rect.left, window_rect.top);
    let mut dst_pos = POINT { x, y };
    let mut size = SIZE { cx: w, cy: h };
    let mut src_pos = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: opacity,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let result = unsafe {
        UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            Some(&mut dst_pos),
            Some(&mut size),
            Some(mem_dc),
            Some(&mut src_pos),
            windows::Win32::Foundation::COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
    };

    // 句柄进出必须成对:恢复旧位图 → 删 DIB → 删内存 DC → 释放屏幕 DC
    unsafe {
        SelectObject(mem_dc, old).map_err(|e| format!("恢复旧位图失败: {}", e))?;
        DeleteObject(HGDIOBJ(dib)).map_err(|e| format!("删除 DIB 位图失败: {}", e))?;
        DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);
    }

    result.map_err(|e| format!("更新分层窗口失败: {}", e))
}

// 预乘:ae = a + (a>>7) 在 0..=255 内加权,(c*ae)>>8 保持 0..=255,
// α=255 恒等透传、α=0 归零,满足 ULW_ALPHA 对预乘的约束(fluor 实证方案)。
fn premultiply(channel: u32, alpha: u32) -> u32 {
    let ae = alpha + (alpha >> 7);
    ((channel * ae) >> 8) & 0xFF
}

/// 渲染当前状态到窗口:读取状态透明度并重渲染(菜单透明度档/阴影开关用)
pub(crate) fn render_current(label: &str, hwnd: HWND) -> Result<(), String> {
    let state = pin_state(label);
    let path = crate::windows::pin_image_window::pin_image_file_path(label)?;
    render_image(hwnd, &path, state.opacity)
}

/// 切换置顶:用 SetWindowPos 在 TOPMOST/NOTOPMOST 间切换(不抢焦点)
pub(crate) fn toggle_topmost(hwnd: HWND) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOSENDCHANGING};
    let target = if is_topmost(hwnd) { HWND_NOTOPMOST } else { HWND_TOPMOST };
    unsafe {
        SetWindowPos(
            hwnd,
            Some(target),
            0, 0, 0, 0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOSENDCHANGING,
        )
    }
    .map_err(|e| format!("切换置顶失败: {}", e))
}

/// 查询窗口是否置顶
pub(crate) fn is_topmost(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOPMOST};
    // 置顶查询:GetWindowLongPtrW 失败(0 返回 + last_error)按 false 处理,
    // 不 panic 不误报置顶
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    ex & (WS_EX_TOPMOST.0 as isize) != 0
}

/// 查询贴图窗口 state(供菜单勾选/交互复用;缺省默认值)
pub(crate) fn window_is_topmost(label: &str) -> bool {
    find_gdi_window(label).map(is_topmost).unwrap_or(false)
}

/// 贴图窗口过程。交互分支(拖窗/缩放/平移/缩略图/右键菜单)逐任务接入,
/// 本骨架先登记全部相关消息并走默认处理。
unsafe extern "system" fn pin_image_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_ACTIVATE | WM_NCHITTEST | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK
        | WM_MOUSEMOVE | WM_MOUSEWHEEL => DefWindowProcW(hwnd, msg, wparam, lparam),
        // 右键菜单:在光标处弹出原生菜单,选中后分发动作再返回 0
        WM_RBUTTONUP => {
            let Some(label) = label_for_hwnd(hwnd) else {
                return LRESULT(0);
            };
            if let Ok(id) = super::menu::show_pin_menu(hwnd, &label) {
                if id != 0 {
                    let _ = super::menu::handle_pin_menu_action(&label, hwnd, id);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let hwnd_val = hwnd.0 as isize;
            lock_hwnd_map().retain(|_, h| *h != hwnd_val);
            // 窗口销毁时按标签移除状态(能反查则精确移除)
            if let Some(label) = label_for_hwnd(hwnd) {
                lock_state_map().remove(&label);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 按窗口句柄反查标签
fn label_for_hwnd(hwnd: HWND) -> Option<String> {
    let hwnd_val = hwnd.0 as isize;
    lock_hwnd_map()
        .iter()
        .find(|(_, h)| **h == hwnd_val)
        .map(|(label, _)| label.clone())
}

/// 供外部按标签查询窗口句柄(close/save/动画共用)
pub(crate) fn find_gdi_window(label: &str) -> Option<HWND> {
    lock_hwnd_map()
        .get(label)
        .map(|hwnd| HWND(hwnd.clone() as usize as *mut core::ffi::c_void))
}

/// 收集全部贴图窗口句柄(focus.rs 排除表用——GDI 窗口不进 tauri webview_windows)
pub fn collect_pin_image_hwnds() -> Vec<isize> {
    lock_hwnd_map()
        .values()
        .map(|hwnd| *hwnd)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gdi_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/gdi.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图 GDI 源码失败")
    }

    // 窗口类只注册一次:注册状态必须由 OnceCell 守卫(重复调用安全)
    #[test]
    fn window_class_registration_is_guarded_by_once_cell() {
        let src = gdi_source();
        assert!(
            src.contains("static CLASS_REGISTERED: OnceCell<()> = OnceCell::new();"),
            "窗口类注册必须由 OnceCell 守卫,保证只注册一次"
        );
        assert!(
            src.contains("CLASS_REGISTERED.get_or_try_init"),
            "注册入口必须走 get_or_try_init(重复调用安全)"
        );
        assert!(
            src.contains("RegisterClassW(&wc)"),
            "必须调用 RegisterClassW 注册窗口类"
        );
        assert!(
            src.contains("let atom = RegisterClassW(&wc);\n        if atom == 0"),
            "注册失败必须返回错误(atom == 0)"
        );
    }

    // 渲染句柄配对:render_image 必须恢复旧位图后删 DIB/DC/释放屏幕 DC,
    // 任一缺失即 GDI 泄漏。负向断言剥行注释避免字面误命中。
    #[test]
    fn render_image_pairs_every_gdi_handle() {
        let stripped: String = gdi_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub(crate) fn render_image")
            .expect("缺少 render_image");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];

        // 正向:渲染必须包含预乘计算与 UpdateLayeredWindow
        assert!(
            body.contains("UpdateLayeredWindow("),
            "render_image 必须调用 UpdateLayeredWindow"
        );
        assert!(
            body.contains("premultiply("),
            "必须调用预乘函数(非预乘 RGBA 直传会花屏)"
        );
        // 句柄配对:恢复旧位图必须在删除 DIB 之前
        let restore = body.find("SelectObject(mem_dc, old)").expect("缺少恢复旧位图");
        let delete_dib = body.find("DeleteObject(dib)").expect("缺少删除 DIB");
        let delete_dc = body.find("DeleteDC(mem_dc)").expect("缺少删除内存 DC");
        let release = body.find("ReleaseDC(None, screen_dc)").expect("缺少释放屏幕 DC");
        assert!(
            restore < delete_dib && delete_dib < delete_dc && delete_dc < release,
            "句柄释放必须按 恢复旧位图→删DIB→删DC→释放DC 顺序成对"
        );
    }

    // 分层窗口必须 WS_EX_LAYERED 建窗,预览模式必须 WS_EX_TRANSPARENT 穿透
    #[test]
    fn gdi_window_uses_layered_and_preview_transparent_styles() {
        let stripped: String = gdi_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub(crate) fn create_gdi_window")
            .expect("缺少 create_gdi_window");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains("WS_EX_LAYERED"),
            "贴图窗口必须以 WS_EX_LAYERED 创建(UpdateLayeredWindow 前提)"
        );
        assert!(
            body.contains("if preview {\n        ex_style |= WS_EX_TRANSPARENT;"),
            "预览模式必须加 WS_EX_TRANSPARENT 穿透"
        );
        assert!(
            body.contains("CreateWindowExW("),
            "必须用 CreateWindowExW 创建窗口"
        );
    }

    // 消息过程必须登记全部贴图交互消息,且 WM_DESTROY 必须清理 HWND 表
    #[test]
    fn window_proc_handles_interaction_messages_and_cleans_hwnd_map() {
        let stripped: String = gdi_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for msg in [
            "WM_LBUTTONDOWN",
            "WM_LBUTTONUP",
            "WM_LBUTTONDBLCLK",
            "WM_MOUSEMOVE",
            "WM_MOUSEWHEEL",
            "WM_RBUTTONUP",
            "WM_DESTROY",
        ] {
            assert!(
                stripped.contains(msg),
                "窗口过程必须处理 {}",
                msg
            );
        }
        let destroy = stripped.find("WM_DESTROY").expect("缺少 WM_DESTROY 分支");
        let tail = &stripped[destroy..];
        assert!(
            tail.contains("lock_hwnd_map().retain"),
            "WM_DESTROY 必须从 HWND 表移除本窗口"
        );
        assert!(
            tail.contains("lock_state_map().remove"),
            "WM_DESTROY 必须移除本窗口的状态记录"
        );
    }

    // HWND 表 helper 存在(collect_pin_image_hwnds 供 focus.rs 排除表)
    #[test]
    fn hwnd_map_helpers_exist_for_focus_exclusion() {
        let stripped = gdi_source();
        assert!(
            stripped.contains("pub(crate) fn find_gdi_window"),
            "必须提供按标签查询句柄的入口"
        );
        assert!(
            stripped.contains("pub fn collect_pin_image_hwnds"),
            "必须提供收集全部贴图句柄的入口(focus.rs 排除表用),且为 pub 对外可见"
        );
    }
}

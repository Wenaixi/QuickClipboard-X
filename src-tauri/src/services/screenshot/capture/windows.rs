use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Duration;

use windows::core::{HSTRING, Interface};
use windows::Foundation::{Metadata::ApiInformation, TypedEventHandler};
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{POINT, RECT, RPC_E_CHANGED_MODE};
use windows::Win32::Graphics::Gdi::{HMONITOR, MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::{
    GetLayeredWindowAttributes, GetTopWindow, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, GW_HWNDNEXT, GWL_EXSTYLE,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureRect {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

impl CaptureRect {
    pub fn right(self) -> u32 { self.left.saturating_add(self.width) }
    pub fn bottom(self) -> u32 { self.top.saturating_add(self.height) }

    pub fn validate(self, content_width: u32, content_height: u32) -> Result<(), CaptureError> {
        if self.width == 0
            || self.height == 0
            || self.left >= content_width
            || self.top >= content_height
            || self.right() > content_width
            || self.bottom() > content_height
        {
            return Err(CaptureError::InvalidSelection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    fn is_non_empty(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum CaptureError {
    InvalidMonitor,
    InvalidSelection,
    UnsupportedFormat(i32),
    Win32(String),
    FrameUnavailable,
    DeviceUnavailable,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMonitor => write!(f, "显示器句柄无效"),
            Self::InvalidSelection => write!(f, "截图选区无效"),
            Self::UnsupportedFormat(value) => write!(f, "捕获像素格式不支持: {value}"),
            Self::Win32(error) => write!(f, "Windows 捕获失败: {error}"),
            Self::FrameUnavailable => write!(f, "未能取得显示器捕获帧"),
            Self::DeviceUnavailable => write!(f, "无法创建 D3D11 设备"),
        }
    }
}

impl std::error::Error for CaptureError {}

fn win32_error(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Win32(error.to_string())
}

fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice), CaptureError> {
    // WGC 的自由线程帧池要求捕获设备与帧池使用同一个 D3D11 设备。
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .map_err(win32_error)?;
    }
    let device = device.ok_or(CaptureError::DeviceUnavailable)?;
    let context = context.ok_or(CaptureError::DeviceUnavailable)?;
    let dxgi_device: IDXGIDevice = device.cast().map_err(win32_error)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }.map_err(win32_error)?;
    let direct3d_device: IDirect3DDevice = inspectable.cast().map_err(win32_error)?;
    Ok((device, context, direct3d_device))
}

fn capture_item_for_monitor(monitor: HMONITOR) -> Result<GraphicsCaptureItem, CaptureError> {
    if monitor.0.is_null() { return Err(CaptureError::InvalidMonitor); }
    let interop: IGraphicsCaptureItemInterop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(win32_error)?;
    unsafe { interop.CreateForMonitor(monitor) }.map_err(win32_error)
}

fn create_capture_session(monitor: HMONITOR) -> Result<(Direct3D11CaptureFramePool, GraphicsCaptureSession, ID3D11Device, ID3D11DeviceContext, SizeInt32), CaptureError> {
    let item = capture_item_for_monitor(monitor)?;
    let size = item.Size().map_err(win32_error)?;
    if size.Width <= 0 || size.Height <= 0 { return Err(CaptureError::FrameUnavailable); }
    let (device, context, direct3d_device) = create_d3d11_device()?;
    // 自由线程帧池不依赖覆盖窗 UI 线程，首帧回读只发生在用户确认选区后。
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &direct3d_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        SizeInt32 { Width: size.Width, Height: size.Height },
    ).map_err(win32_error)?;
    let session = pool.CreateCaptureSession(&item).map_err(win32_error)?;
    if supports_capture_session_property("IsCursorCaptureEnabled") {
        session.SetIsCursorCaptureEnabled(false).map_err(win32_error)?;
    }
    if supports_capture_session_property("IsBorderRequired") {
        session.SetIsBorderRequired(false).map_err(win32_error)?;
    }
    session.StartCapture().map_err(win32_error)?;
    Ok((pool, session, device, context, size))
}

fn next_frame(pool: &Direct3D11CaptureFramePool) -> Result<Direct3D11CaptureFrame, CaptureError> {
    let ready = std::sync::Arc::new((StdMutex::new(false), Condvar::new()));
    let ready_for_callback = ready.clone();
    let token = pool
        .FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(move |_, _| {
            let (flag, signal) = &*ready_for_callback;
            if let Ok(mut arrived) = flag.lock() {
                *arrived = true;
                signal.notify_one();
            }
            Ok(())
        }))
        .map_err(win32_error)?;

    let frame = (|| {
        if let Ok(frame) = pool.TryGetNextFrame() {
            return Ok(frame);
        }
        let (flag, signal) = &*ready;
        let arrived = flag.lock().map_err(|_| CaptureError::FrameUnavailable)?;
        let (arrived, _) = signal
            .wait_timeout_while(arrived, Duration::from_millis(250), |value| !*value)
            .map_err(|_| CaptureError::FrameUnavailable)?;
        if !*arrived {
            return Err(CaptureError::FrameUnavailable);
        }
        pool.TryGetNextFrame().map_err(win32_error)
    })();
    let _ = pool.RemoveFrameArrived(token);
    frame
}

fn surface_texture(frame: &Direct3D11CaptureFrame) -> Result<ID3D11Texture2D, CaptureError> {
    let surface = frame.Surface().map_err(win32_error)?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(win32_error)?;
    unsafe { access.GetInterface::<ID3D11Texture2D>() }.map_err(win32_error)
}

fn create_staging_texture(device: &ID3D11Device, width: u32, height: u32) -> Result<ID3D11Texture2D, CaptureError> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging)) }.map_err(win32_error)?;
    staging.ok_or(CaptureError::DeviceUnavailable)
}

fn copy_crop_to_staging(
    context: &ID3D11DeviceContext,
    source: &ID3D11Texture2D,
    staging: &ID3D11Texture2D,
    selection: CaptureRect,
) {
    let crop = D3D11_BOX {
        left: selection.left,
        top: selection.top,
        front: 0,
        right: selection.right(),
        bottom: selection.bottom(),
        back: 1,
    };
    unsafe {
        context.CopySubresourceRegion(staging, 0, 0, 0, 0, source, 0, Some(&crop));
    }
}

fn map_rgba(
    context: &ID3D11DeviceContext,
    staging: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.map_err(win32_error)?;
    let row_bytes = width as usize * 4;
    let mut rgba = vec![0u8; row_bytes * height as usize];
    for row in 0..height as usize {
        let source = unsafe { std::slice::from_raw_parts((mapped.pData as *const u8).add(row * mapped.RowPitch as usize), row_bytes) };
        let target = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
        for pixel in 0..width as usize {
            let source_index = pixel * 4;
            let target_index = source_index;
            target[target_index] = source[source_index + 2];
            target[target_index + 1] = source[source_index + 1];
            target[target_index + 2] = source[source_index];
            target[target_index + 3] = source[source_index + 3];
        }
    }
    unsafe { context.Unmap(staging, 0); }
    Ok(rgba)
}

pub fn get_monitor_handle(x: i32, y: i32) -> HMONITOR {
    let pt = POINT { x, y };
    unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) }
}

fn is_capture_candidate(hwnd: windows::Win32::Foundation::HWND, current_process_id: u32) -> bool {
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
        return false;
    }

    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)); }
    if process_id == 0 || process_id == current_process_id {
        return false;
    }

    let extended_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let is_transparent = extended_style & WS_EX_TRANSPARENT.0 != 0;
    let is_non_activatable_tool = extended_style & WS_EX_TOOLWINDOW.0 != 0
        && extended_style & WS_EX_NOACTIVATE.0 != 0;
    // IsWindowVisible 对 layered 窗口只看 WS_VISIBLE 样式位，alpha=0 的完全透明窗口
    // 视觉不可见但会被误选为单击截图目标，需要按 alpha 通道过滤。
    let is_invisible_layered = if extended_style & WS_EX_LAYERED.0 != 0 {
        let mut alpha: u8 = 255;
        let mut flags = windows::Win32::UI::WindowsAndMessaging::LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
        unsafe {
            GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags)).is_ok()
                && alpha == 0
        }
    } else {
        false
    };
    !is_transparent && !is_non_activatable_tool && !is_invisible_layered
}

pub fn find_window_rect_at_point(x: i32, y: i32) -> Option<WindowRect> {
    let current_process_id = std::process::id();
    // GetTopWindow(NULL) 与 GW_HWNDNEXT 是 Win32 明确公开的顶层窗口 Z 序遍历。
    // 从最高层向下取第一个命中的可截图窗口，避免依赖未承诺的枚举顺序。
    let Ok(mut hwnd) = (unsafe { GetTopWindow(None) }) else {
        return None;
    };
    while !hwnd.0.is_null() {
        if is_capture_candidate(hwnd, current_process_id) {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                let rect = WindowRect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                if rect.is_non_empty()
                    && x >= rect.left
                    && x < rect.right
                    && y >= rect.top
                    && y < rect.bottom
                {
                    return Some(rect);
                }
            }
        }
        let Ok(next) = (unsafe { GetWindow(hwnd, GW_HWNDNEXT) }) else {
            break;
        };
        hwnd = next;
    }
    None
}

const GRAPHICS_CAPTURE_SESSION_TYPE: &str = "Windows.Graphics.Capture.GraphicsCaptureSession";

fn supports_capture_session_property(property_name: &str) -> bool {
    ApiInformation::IsPropertyPresent(
        &HSTRING::from(GRAPHICS_CAPTURE_SESSION_TYPE),
        &HSTRING::from(property_name),
    )
    .unwrap_or(false)
}

#[must_use]
pub struct ComInitialization {
    should_uninitialize: bool,
}

impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe { CoUninitialize(); }
        }
    }
}

fn com_initialization_requires_uninitialize(result: windows::core::HRESULT) -> bool {
    result.is_ok()
}

pub fn ensure_com_initialized() -> Result<ComInitialization, CaptureError> {
    unsafe {
        let result = CoInitializeEx(None, COINIT_MULTITHREADED);
        if com_initialization_requires_uninitialize(result) {
            return Ok(ComInitialization { should_uninitialize: true });
        }
        if result == RPC_E_CHANGED_MODE {
            // 当前线程已采用其他 COM 公寓模型；不得解除调用方持有的初始化计数。
            return Ok(ComInitialization { should_uninitialize: false });
        }
        Err(CaptureError::Win32(format!(
            "COM 初始化失败: {}",
            result.to_string()
        )))
    }
}

pub fn capture_monitor(monitor: HMONITOR, selection: CaptureRect) -> Result<CapturedFrame, CaptureError> {
    if selection.width == 0 || selection.height == 0 { return Err(CaptureError::InvalidSelection); }
    let (pool, session, device, context, size) = create_capture_session(monitor)?;
    let initial_width = u32::try_from(size.Width).map_err(|_| CaptureError::FrameUnavailable)?;
    let initial_height = u32::try_from(size.Height).map_err(|_| CaptureError::FrameUnavailable)?;
    selection.validate(initial_width, initial_height)?;
    let result = (|| {
        let frame = next_frame(&pool)?;
        let content = frame.ContentSize().map_err(win32_error)?;
        let content_width = u32::try_from(content.Width).map_err(|_| CaptureError::FrameUnavailable)?;
        let content_height = u32::try_from(content.Height).map_err(|_| CaptureError::FrameUnavailable)?;
        selection.validate(content_width, content_height)?;
        let source = surface_texture(&frame)?;
        let staging = create_staging_texture(&device, selection.width, selection.height)?;
        copy_crop_to_staging(&context, &source, &staging, selection);
        let rgba = map_rgba(&context, &staging, selection.width, selection.height)?;
        let _ = frame.Close();
        Ok(CapturedFrame { width: selection.width, height: selection.height, rgba })
    })();
    let _ = session.Close();
    let _ = pool.Close();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_com_initialization_requires_matching_uninitialization() {
        assert!(com_initialization_requires_uninitialize(windows::core::HRESULT(0)));
        assert!(com_initialization_requires_uninitialize(windows::core::HRESULT(1)));
        assert!(!com_initialization_requires_uninitialize(
            windows::Win32::Foundation::RPC_E_CHANGED_MODE,
        ));
        assert!(!com_initialization_requires_uninitialize(windows::core::HRESULT(0x80004005u32 as i32)));
    }

    #[test]
    fn capture_rect_right_and_bottom_are_saturating() {
        let rect = CaptureRect { left: u32::MAX, top: u32::MAX, width: 4, height: 8 };
        assert_eq!(rect.right(), u32::MAX);
        assert_eq!(rect.bottom(), u32::MAX);
    }

    #[test]
    fn capture_rect_rejects_zero_dimensions_at_public_boundary() {
        let result = capture_monitor(HMONITOR::default(), CaptureRect { left: 0, top: 0, width: 0, height: 10 });
        assert!(matches!(result, Err(CaptureError::InvalidSelection)));
    }

    #[test]
    fn capture_rect_rejects_selection_outside_content_bounds() {
        let rect = CaptureRect { left: 1919, top: 0, width: 2, height: 10 };
        assert!(matches!(rect.validate(1920, 1080), Err(CaptureError::InvalidSelection)));
        assert!(rect.validate(3840, 2160).is_ok());
    }

    #[test]
    fn window_selection_walks_documented_top_level_z_order() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/screenshot/capture/windows.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 Windows 窗口选择源码失败");
        let body_start = source
            .find("fn is_capture_candidate")
            .expect("缺少候选窗口过滤函数");
        let body_end = source[body_start..]
            .find("const GRAPHICS_CAPTURE_SESSION_TYPE")
            .map(|offset| body_start + offset)
            .expect("窗口选择函数后缺少下一模块锚点");
        let body = &source[body_start..body_end];
        assert!(body.contains("GetTopWindow(None)"));
        assert!(body.contains("GetWindow(hwnd, GW_HWNDNEXT)"));
        assert!(!body.contains("EnumWindows("));
        // 完全透明的 layered 窗口必须被过滤，避免单击选窗误选视觉不可见窗口。
        assert!(body.contains("GetLayeredWindowAttributes"));
        assert!(body.contains("alpha == 0"));
    }
}

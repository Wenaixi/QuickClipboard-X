use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Duration;

use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{POINT, RPC_E_CHANGED_MODE};
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
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

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
    session.SetIsCursorCaptureEnabled(false).map_err(win32_error)?;
    session.SetIsBorderRequired(false).map_err(win32_error)?;
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

fn is_acceptable_com_initialization_result(result: windows::core::HRESULT) -> bool {
    result.is_ok() || result == RPC_E_CHANGED_MODE
}

pub fn ensure_com_initialized() -> Result<(), CaptureError> {
    unsafe {
        let result = CoInitializeEx(None, COINIT_MULTITHREADED);
        if is_acceptable_com_initialization_result(result) {
            // 当前线程已采用其他 COM 公寓模型时可继续使用 WGC 捕获。
            Ok(())
        } else {
            Err(CaptureError::Win32(format!(
                "COM 初始化失败: {}",
                result.to_string()
            )))
        }
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
    fn com_initialization_accepts_success_and_changed_apartment_mode_only() {
        assert!(is_acceptable_com_initialization_result(windows::core::HRESULT(0)));
        assert!(is_acceptable_com_initialization_result(
            windows::Win32::Foundation::RPC_E_CHANGED_MODE,
        ));
        assert!(!is_acceptable_com_initialization_result(windows::core::HRESULT(0x80004005u32 as i32)));
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
}
